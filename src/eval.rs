//! 評価 (§3, §4, §5)
//!
//! パーレンの中身は平坦な項列のまま保持され、**その括弧を評価する直前に**
//! 一括でグルーピングされる (§4.5)。順位はグルーピング時に、関数は評価時に引かれる。

use crate::env::{Env, DEFAULT_PREC};
use crate::io::IoTable;
use crate::parser::{GroupKind, Item, Items};
use crate::value::{Closure, Value};
use std::rc::Rc;

/// グルーピングの結果。オペレータは常に中置で、位置だけで判別される (§0, §3)
#[derive(Debug)]
pub enum Expr {
    Leaf(Rc<Item>),
    App {
        op: Rc<Item>,
        left: Rc<Expr>,
        right: Rc<Expr>,
    },
}

impl Expr {
    pub fn pos(&self) -> crate::lexer::Pos {
        match self {
            Expr::Leaf(i) => i.pos(),
            Expr::App { op, .. } => op.pos(),
        }
    }
}

/// 遅延オペランドは **構文木と呼び出し側環境の対** として渡される (§5)
pub enum Arg {
    Val(Value),
    Lazy(Rc<Expr>, Env),
}

/// 非局所脱出。catch は無いので必ず最外まで飛ぶ (§8.6)
#[derive(Debug)]
pub enum Flow {
    /// `!!` — 終了コードを設定して強制終了
    Panic(i32),
    /// この言語の静的検査点に引っかかった (§8.1)
    Fatal(String),
}

pub type R<T> = Result<T, Flow>;

fn prec_of_item(item: &Item, env: &Env) -> u32 {
    match item {
        Item::Word(w, _) => env.prec_of(w),
        // 引用トークンと括弧は環境を引かないのでデフォルト順位
        Item::Quoted(_, _) | Item::Group(_, _, _) => DEFAULT_PREC,
    }
}

/// 平坦な項列を木にする。
///
/// **最も弱い演算子のうち、いちばん右で割る**（左結合, §4）。
/// 項数は奇数であること（偶数は宣言なのでここには来ない）。
pub fn group(items: &[Rc<Item>], env: &Env) -> Rc<Expr> {
    debug_assert!(items.len() % 2 == 1 && !items.is_empty());
    if items.len() == 1 {
        return Rc::new(Expr::Leaf(items[0].clone()));
    }

    let mut split = 1usize;
    let mut weakest = prec_of_item(&items[1], env);
    let mut i = 3;
    while i < items.len() {
        let p = prec_of_item(&items[i], env);
        if p <= weakest {
            // 同順位なら右を採る = 左結合
            weakest = p;
            split = i;
        }
        i += 2;
    }

    Rc::new(Expr::App {
        op: items[split].clone(),
        left: group(&items[..split], env),
        right: group(&items[split + 1..], env),
    })
}

pub struct Interp {
    pub io: IoTable,
    /// 残り燃料。0 なら無制限。ループは再帰しかないので、
    /// 止まらないプログラムを止める唯一の手段（ブラウザで走らせるために要る）
    pub fuel: u64,
    /// 呼び出しの深さの上限。ネイティブのスタックは広く取ってあるが、
    /// 落ちるより「深すぎる」と言うほうが親切
    pub max_depth: u32,
    depth: u32,
}

impl Interp {
    pub fn new(io: IoTable) -> Interp {
        Interp { io, fuel: 0, max_depth: 50_000, depth: 0 }
    }

    /// 燃料と深さに上限のある実行（プレイグラウンド用）
    pub fn with_limits(io: IoTable, fuel: u64, max_depth: u32) -> Interp {
        Interp { io, fuel, max_depth, depth: 0 }
    }

    pub fn force(&mut self, a: Arg) -> R<Value> {
        match a {
            Arg::Val(v) => Ok(v),
            Arg::Lazy(e, env) => self.eval_expr(&e, &env),
        }
    }

    /// 括弧の中身を評価する。項数の奇偶が意味を決める (§2.1)
    pub fn eval_group(&mut self, kind: GroupKind, items: &Items, env: &Env) -> R<Value> {
        match items.len() {
            // ユニット値。`{}` も同じ（空のスコープは何も生まない）
            0 => Ok(Value::Unit),
            // パーサで捕まえ済み
            2 => Err(Flow::Fatal(format!(
                "{}: 2項の括弧は構文エラー",
                items[0].pos()
            ))),
            n if n % 2 == 1 => {
                // 奇数 → 式。`{}` はフレームを1枚積む
                let inner = match kind {
                    GroupKind::Paren => env.clone(),
                    GroupKind::Brace => env.child(),
                };
                self.eval_flat(items, &inner)
            }
            // 4以上の偶数 → 関数宣言
            _ => self.declare(kind, items, env),
        }
    }

    /// 奇数項の平坦な列を、与えられた環境でグルーピングして評価する
    pub fn eval_flat(&mut self, items: &[Rc<Item>], env: &Env) -> R<Value> {
        let e = group(items, env);
        self.eval_expr(&e, env)
    }

    /// シグネチャ3項 + 本体（奇数項）(§2.1, §2.2)
    ///
    /// シグネチャの項が引用符なしのトークンでなくてもエラーにはしない。
    /// `;` を1つ書き忘れたブロックは **静かに** 関数宣言に化ける必要がある (§2.4)
    fn declare(&mut self, kind: GroupKind, items: &Items, env: &Env) -> R<Value> {
        let lparam = items[0].as_word().cloned();
        let name = items[1].as_word().cloned();
        let rparam = items[2].as_word().cloned();
        let body: Items = items[3..].iter().cloned().collect();

        // `()` は現スコープに宣言する。`{}` はフレームを1枚積んでそこに宣言し、
        // フレームは閉じた瞬間に名前が消える —— が、クロージャが捕まえている (§2.3)
        let target = match kind {
            GroupKind::Paren => env.clone(),
            GroupKind::Brace => env.child(),
        };

        let f = Value::Fun(Rc::new(Closure {
            name: name.clone(),
            lparam,
            rparam,
            body,
            env: target.clone(),
        }));
        if let Some(n) = name {
            target.define(n, f.clone());
        }
        Ok(f)
    }

    pub fn eval_item(&mut self, item: &Rc<Item>, env: &Env) -> R<Value> {
        match &**item {
            Item::Word(w, _) => Ok(env.resolve(w)),
            // 引用は環境を引かない。文字列そのものに評価される (§1.3)
            Item::Quoted(s, _) => Ok(Value::Sym(s.clone())),
            Item::Group(k, items, _) => self.eval_group(*k, items, env),
        }
    }

    pub fn eval_expr(&mut self, e: &Rc<Expr>, env: &Env) -> R<Value> {
        match &**e {
            Expr::Leaf(item) => self.eval_item(item, env),
            Expr::App { op, left, right } => {
                if self.fuel > 0 {
                    self.fuel -= 1;
                    if self.fuel == 0 {
                        return Err(Flow::Fatal("実行が長すぎる（燃料切れ）".into()));
                    }
                }
                // 関数と評価方針は評価時に引かれる (§4.5)
                let (func, lazy_left, lazy_right) = self.resolve_operator(op, env)?;
                // オペランドは左→右の順に評価 (§5)
                let l = if lazy_left {
                    Arg::Lazy(left.clone(), env.clone())
                } else {
                    Arg::Val(self.eval_expr(left, env)?)
                };
                let r = if lazy_right {
                    Arg::Lazy(right.clone(), env.clone())
                } else {
                    Arg::Val(self.eval_expr(right, env)?)
                };
                self.apply(func, l, r)
            }
        }
    }

    fn resolve_operator(&mut self, item: &Rc<Item>, env: &Env) -> R<(Value, bool, bool)> {
        match &**item {
            Item::Word(w, _) => {
                let (f, _, ll, lr) = env.resolve_op(w);
                Ok((f, ll, lr))
            }
            // オペレータ位置に書かれた引用や括弧はフラグを持たない
            Item::Quoted(s, _) => Ok((Value::Sym(s.clone()), false, false)),
            Item::Group(k, items, _) => Ok((self.eval_group(*k, items, env)?, false, false)),
        }
    }

    /// すべての関数は2引数 (§3)
    pub fn apply(&mut self, f: Value, l: Arg, r: Arg) -> R<Value> {
        match f {
            Value::Builtin(b) => self.call_builtin(b, l, r),
            Value::Op(rec) => self.apply(rec.func.clone(), l, r),
            Value::Fun(c) => {
                let lv = self.force(l)?;
                let rv = self.force(r)?;
                self.depth += 1;
                if self.depth > self.max_depth {
                    self.depth -= 1;
                    return Err(Flow::Fatal(format!(
                        "再帰が深すぎる（{}段）",
                        self.max_depth
                    )));
                }
                let frame = c.env.child();
                if let Some(n) = &c.lparam {
                    frame.define(n.clone(), lv);
                }
                if let Some(n) = &c.rparam {
                    frame.define(n.clone(), rv);
                }
                // 本体は呼び出しのたびにグルーピングされる (§4.5)
                let out = self.eval_flat(&c.body, &frame);
                self.depth -= 1;
                out
            }
            // 関数でないものを2引数に適用しようとして失敗し、静かに記号項になる (付録)
            other => {
                let lv = self.force(l)?;
                let rv = self.force(r)?;
                Ok(Value::term(other, lv, rv))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::prelude;
    use crate::parser::parse;

    fn show(src: &str) -> String {
        let items = parse(src).unwrap();
        let env = prelude().child();
        let e = group(&items, &env);
        render(&e)
    }

    fn render(e: &Expr) -> String {
        match e {
            Expr::Leaf(i) => match &**i {
                Item::Word(w, _) => w.to_string(),
                Item::Quoted(s, _) => format!("\"{}\"", s),
                Item::Group(_, items, _) => {
                    let env = prelude().child();
                    if items.is_empty() {
                        "()".into()
                    } else if items.len() % 2 == 1 {
                        format!("[{}]", render(&group(items, &env)))
                    } else {
                        "[decl]".into()
                    }
                }
            },
            Expr::App { op, left, right } => {
                let o = match &**op {
                    Item::Word(w, _) => w.to_string(),
                    Item::Quoted(s, _) => format!("\"{}\"", s),
                    Item::Group(_, _, _) => "[..]".into(),
                };
                format!("({} {} {})", render(left), o, render(right))
            }
        }
    }

    #[test]
    fn 検算() {
        // §4.2
        assert_eq!(
            show("x = 1 ; y = x + 2 ; 1 <<< y"),
            "(((x = 1) ; (y = (x + 2))) ; (1 <<< y))"
        );
    }

    #[test]
    fn デフォルト帯は同順位左結合() {
        // §4.1: 1 + 2 * 3 は 9
        assert_eq!(show("1 + 2 * 3"), "((1 + 2) * 3)");
        assert_eq!(show("list # n + 1"), "((list # n) + 1)");
    }

    #[test]
    fn 三項演算子は二項演算子二つ() {
        // §8.3
        assert_eq!(show("b ? t : f"), "((b ? t) : f)");
    }

    #[test]
    fn カンマは代入より強い() {
        // §4.3: 逆だと分割代入が消える
        assert_eq!(show("a , b = 1 , 2"), "((a , b) = (1 , 2))");
    }

    #[test]
    fn 印字は比較より弱い() {
        // §4.1
        assert_eq!(show("1 <<< a == b"), "(1 <<< (a == b))");
        assert_eq!(show("1 <<< a , b"), "(1 <<< (a , b))");
    }

    #[test]
    fn 未束縛トークンはデフォルト順位() {
        assert_eq!(show("1 foo 2 ; 3"), "((1 foo 2) ; 3)");
    }
}
