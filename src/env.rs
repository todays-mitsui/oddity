//! 環境 (§6)
//!
//! ```text
//! 最内 → ... → トップレベル → プレリュード → 数値フレーム → 自己評価フレーム
//! ```
//!
//! 末尾2枚は表ではなく **関数で解決するフレーム**。
//! 自己評価フレームは全トークンを引き受けて絶対に失敗しないので必ず最後。

use crate::value::{Builtin, OpRecord, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// 未束縛トークンとレコード以外の束縛が持つ順位。いちばん強い側 (§4.1)
pub const DEFAULT_PREC: u32 = 8;

pub struct Scope {
    vars: RefCell<HashMap<Rc<str>, Value>>,
    parent: Option<Env>,
}

#[derive(Clone)]
pub struct Env(Rc<Scope>);

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 環境は循環しうる（クロージャが自分を含むフレームを捕まえている）ので辿らない
        f.write_str("<env>")
    }
}

impl Env {
    pub fn root() -> Env {
        Env(Rc::new(Scope {
            vars: RefCell::new(HashMap::new()),
            parent: None,
        }))
    }

    /// フレームを1枚積む
    pub fn child(&self) -> Env {
        Env(Rc::new(Scope {
            vars: RefCell::new(HashMap::new()),
            parent: Some(self.clone()),
        }))
    }

    /// `=` は常に最内フレームに対して束縛する (§6.2)。
    /// 親フレームへの書き込み手段は存在しない (§6.3)
    pub fn define(&self, name: Rc<str>, value: Value) {
        self.0.vars.borrow_mut().insert(name, value);
    }

    /// 表を持つフレームだけを辿る
    pub fn lookup(&self, name: &str) -> Option<Value> {
        let mut cur = Some(self.clone());
        while let Some(env) = cur {
            if let Some(v) = env.0.vars.borrow().get(name) {
                return Some(v.clone());
            }
            cur = env.0.parent.clone();
        }
        None
    }

    /// オペランド位置での解決。数値フレームと自己評価フレームまで含むので失敗しない。
    ///
    /// レコードは剥がして関数値だけを返す。これが §5 の穴 ——
    /// `f = ?` のように関数値を取り出すと遅延が失われる
    pub fn resolve(&self, name: &str) -> Value {
        match self.lookup(name) {
            Some(Value::Op(r)) => r.func.clone(),
            Some(v) => v,
            None => numeric_frame(name).unwrap_or_else(|| Value::Sym(Rc::from(name))),
        }
    }

    /// オペレータ位置での解決。順位と評価方針まで含めて引く
    pub fn resolve_op(&self, name: &str) -> (Value, u32, bool, bool) {
        match self.lookup(name) {
            Some(Value::Op(r)) => (r.func.clone(), r.prec, r.lazy_left, r.lazy_right),
            Some(v) => (v, DEFAULT_PREC, false, false),
            None => (
                numeric_frame(name).unwrap_or_else(|| Value::Sym(Rc::from(name))),
                DEFAULT_PREC,
                false,
                false,
            ),
        }
    }

    /// グルーピング時に引かれるのは順位だけ。関数は評価時に引かれる (§4.5)
    pub fn prec_of(&self, name: &str) -> u32 {
        match self.lookup(name) {
            Some(Value::Op(r)) => r.prec,
            _ => DEFAULT_PREC,
        }
    }
}

/// 数値フレーム (§6.1)。
/// 数字列を数に解決する。字種の知識はこの1箇所に集まる。差し替え不可 (§12)
pub fn numeric_frame(name: &str) -> Option<Value> {
    let digits = name.strip_prefix('-').unwrap_or(name);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    name.parse::<i64>().ok().map(Value::Num)
}

fn op(env: &Env, name: &str, b: Builtin, prec: u32, lazy_left: bool, lazy_right: bool) {
    env.define(
        Rc::from(name),
        Value::Op(Rc::new(OpRecord {
            func: Value::Builtin(b),
            prec,
            lazy_left,
            lazy_right,
        })),
    );
}

/// プレリュード (§8)。すべて普通の2引数関数で、差はメタデータだけ。
///
/// 弱い順:
/// `; < = < <<< >>> !! < ? : ! || && < == != < > <= >= < |> $ < , < デフォルト`
pub fn prelude() -> Env {
    use Builtin::*;
    let e = Env::root();

    //                                    順位  左遅延  右遅延
    op(&e, ";", Seq, 1, false, false);
    op(&e, "=", Assign, 2, true, false); // 左は木として検査するだけ
    op(&e, "<<<", Out, 3, false, false);
    op(&e, ">>>", In, 3, false, false);
    op(&e, "!!", Panic, 3, false, false);
    op(&e, "?", Guard, 4, false, true);
    op(&e, ":", Elvis, 4, false, true);
    op(&e, "!", Unless, 4, false, true);
    op(&e, "&&", Guard, 4, false, true); // `&&` は `?` と完全に同一
    op(&e, "||", Elvis, 4, false, true); // `||` は `:` と完全に同一
    op(&e, "==", Eq, 5, false, false);
    op(&e, "!=", Ne, 5, false, false);
    op(&e, "<", Lt, 5, false, false);
    op(&e, ">", Gt, 5, false, false);
    op(&e, "<=", Le, 5, false, false);
    op(&e, ">=", Ge, 5, false, false);
    op(&e, "|>", Uncurry, 6, false, false);
    op(&e, "$", Apply, 6, false, false);
    op(&e, ",", Cons, 7, false, false);

    // デフォルト帯。順位はいちばん強い側で未束縛トークンと同じ
    for (name, b) in [
        ("+", Add), ("-", Sub), ("*", Mul), ("/", Div), ("%", Rem),
        ("++", Concat), ("<>", Append), ("#", Index), ("@", CharAt),
        ("~", Parse), ("<<-", OpenRead), ("->>", OpenWrite), ("<->", OpenRw),
    ] {
        op(&e, name, b, DEFAULT_PREC, false, false);
    }

    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 数値フレームは数字列を解決する() {
        assert!(matches!(numeric_frame("42"), Some(Value::Num(42))));
        assert!(matches!(numeric_frame("-7"), Some(Value::Num(-7))));
        assert!(numeric_frame("1-1").is_none());
        assert!(numeric_frame("").is_none());
        assert!(numeric_frame("-").is_none());
        assert!(numeric_frame("x").is_none());
    }

    #[test]
    fn 自己評価フレームは失敗しない() {
        let e = prelude().child();
        assert!(matches!(e.resolve("<<<<<"), Value::Sym(_)));
        assert!(matches!(e.resolve("hello"), Value::Sym(_)));
    }

    #[test]
    fn 未束縛トークンはデフォルト順位() {
        let e = prelude().child();
        assert_eq!(e.prec_of("hello"), DEFAULT_PREC);
        assert_eq!(e.prec_of("+"), DEFAULT_PREC);
        assert_eq!(e.prec_of(";"), 1);
        assert!(e.prec_of(";") < e.prec_of("="));
        assert!(e.prec_of("=") < e.prec_of("<<<"));
        assert!(e.prec_of("<<<") < e.prec_of("?"));
        assert!(e.prec_of("?") < e.prec_of("=="));
        assert!(e.prec_of("==") < e.prec_of("|>"));
        assert!(e.prec_of("|>") < e.prec_of(","));
        assert!(e.prec_of(",") < e.prec_of("+"));
    }

    #[test]
    fn レコード以外の束縛はデフォルトに落ちる() {
        let e = prelude().child();
        e.define(Rc::from(","), Value::Num(0));
        assert_eq!(e.prec_of(","), DEFAULT_PREC);
    }

    #[test]
    fn 内側から外側は書けない() {
        let outer = Env::root();
        outer.define(Rc::from("x"), Value::Num(1));
        let inner = outer.child();
        inner.define(Rc::from("x"), Value::Num(2));
        assert!(matches!(outer.lookup("x"), Some(Value::Num(1))));
        assert!(matches!(inner.lookup("x"), Some(Value::Num(2))));
    }
}
