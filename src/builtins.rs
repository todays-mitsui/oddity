//! プレリュード演算子の中身 (§8)
//!
//! すべて普通の2引数関数。差はメタデータ (§4.4) だけで、特殊形式は存在しない (§5)。

use crate::env::Env;
use crate::eval::{group, Arg, Expr, Flow, Interp, R};
use crate::io::OpenMode;
use crate::parser::{GroupKind, Item};
use crate::value::{display, value_cmp, value_eq, Builtin, Value};
use std::cmp::Ordering;
use std::rc::Rc;

/// `=` の左辺形 (§8.1)。この言語で唯一の静的検査点
enum Pat {
    Var(Rc<str>),
    Cons(Box<Pat>, Box<Pat>),
}

/// 左オペランドを評価せず、構文木として検査する
fn pattern(e: &Rc<Expr>, env: &Env) -> R<Pat> {
    match &**e {
        // 引用符なしの単一トークン (`1` `2` を含む)
        Expr::Leaf(item) => match &**item {
            Item::Word(w, _) => Ok(Pat::Var(w.clone())),
            Item::Quoted(s, _) => Err(Flow::Fatal(format!(
                "{}: `=` の左辺に引用トークン `\"{}\"` は書けない",
                item.pos(),
                s
            ))),
            // `()` によるグルーピングは透過
            Item::Group(GroupKind::Paren, items, p) => {
                if items.is_empty() || items.len() % 2 == 0 {
                    return Err(Flow::Fatal(format!(
                        "{}: `=` の左辺にできない括弧",
                        p
                    )));
                }
                pattern(&group(items, env), env)
            }
            Item::Group(GroupKind::Brace, _, p) => Err(Flow::Fatal(format!(
                "{}: `=` の左辺に `{{}}` は書けない",
                p
            ))),
        },
        // `,` だけで繋がれたトークンの連鎖
        Expr::App { op, left, right } => match op.as_word() {
            Some(w) if &**w == "," => Ok(Pat::Cons(
                Box::new(pattern(left, env)?),
                Box::new(pattern(right, env)?),
            )),
            _ => Err(Flow::Fatal(format!(
                "{}: `=` の左辺は単一トークンか `,` の連鎖でなければならない",
                op.pos()
            ))),
        },
    }
}

/// パターンは常に末尾寄せの部分マッチ。
/// マッチが失敗するのは **右辺の深さが足りないとき** だけ (§8.1)
fn match_pattern(p: &Pat, v: &Value, out: &mut Vec<(Rc<str>, Value)>) -> bool {
    match p {
        Pat::Var(n) => {
            out.push((n.clone(), v.clone()));
            true
        }
        Pat::Cons(pl, pr) => match v {
            Value::Cons(a, b) => match_pattern(pl, a, out) && match_pattern(pr, b, out),
            _ => false,
        },
    }
}

/// 比較族の規約 (§8.8):
/// 真ならオペランドの一方（右）を返し、偽なら `()` を返す。
/// `()` を受け取った比較は `()` を返す
fn compare(b: Builtin, lv: Value, rv: Value) -> Value {
    if matches!(lv, Value::Unit) || matches!(rv, Value::Unit) {
        return Value::Unit;
    }
    let hit = match b {
        Builtin::Eq => value_eq(&lv, &rv),
        Builtin::Ne => !value_eq(&lv, &rv),
        _ => match value_cmp(&lv, &rv) {
            Some(o) => match b {
                Builtin::Lt => o == Ordering::Less,
                Builtin::Gt => o == Ordering::Greater,
                Builtin::Le => o != Ordering::Greater,
                Builtin::Ge => o != Ordering::Less,
                _ => unreachable!(),
            },
            // 比べようがないものは記号のまま残る
            None => return Value::term(Value::Builtin(b), lv, rv),
        },
    };
    if hit {
        rv
    } else {
        Value::Unit
    }
}

fn arith(b: Builtin, lv: Value, rv: Value) -> Value {
    if let (Value::Num(x), Value::Num(y)) = (&lv, &rv) {
        let r = match b {
            Builtin::Add => x.checked_add(*y),
            Builtin::Sub => x.checked_sub(*y),
            Builtin::Mul => x.checked_mul(*y),
            Builtin::Div => x.checked_div(*y),
            Builtin::Rem => x.checked_rem(*y),
            _ => unreachable!(),
        };
        if let Some(n) = r {
            return Value::Num(n);
        }
    }
    Value::term(Value::Builtin(b), lv, rv)
}

impl Interp {
    pub fn call_builtin(&mut self, b: Builtin, l: Arg, r: Arg) -> R<Value> {
        use Builtin::*;

        // フラグ付きの4つ。ここだけが Arg をそのまま見る
        match b {
            Assign => return self.op_assign(l, r),
            Guard | Elvis | Unless => {
                let lv = self.force(l)?;
                return match (b, lv.truthy()) {
                    // (b ? t) … b が () でなければ t を評価して返す。ガード
                    (Guard, true) => self.force(r),
                    (Guard, false) => Ok(Value::Unit),
                    // (x : y) … x が () なら y。Elvis
                    (Elvis, true) => Ok(lv),
                    (Elvis, false) => self.force(r),
                    // (x ! y) … x が偽なら y、真なら ()。unless
                    (Unless, true) => Ok(Value::Unit),
                    (Unless, false) => self.force(r),
                    _ => unreachable!(),
                };
            }
            _ => {}
        }

        let lv = self.force(l)?;
        let rv = self.force(r)?;

        Ok(match b {
            // 言語で唯一「値が捨てられる」箇所 (§8.5)
            Seq => rv,

            Eq | Ne | Lt | Gt | Le | Ge => compare(b, lv, rv),
            Add | Sub | Mul | Div | Rem => arith(b, lv, rv),

            Cons => Value::cons(lv, rv),

            // `<<<` は左オペランド（fd）を返すので連鎖できる (§8.4)
            Out => match lv {
                Value::Num(fd) => {
                    let s = display(&rv);
                    self.io.write(fd, &s);
                    lv
                }
                _ => Value::term(Value::Builtin(b), lv, rv),
            },

            // `>>>` は束縛しない。値を返すだけ (§8.4)
            In => match (&lv, &rv) {
                (Value::Num(fd), Value::Unit) => match self.io.read_line(*fd) {
                    Some(s) => Value::sym(&s),
                    None => Value::Unit, // EOF
                },
                (Value::Num(fd), Value::Num(n)) => match self.io.read_chars(*fd, *n) {
                    Some(s) => Value::sym(&s),
                    None => Value::Unit,
                },
                _ => Value::term(Value::Builtin(b), lv, rv),
            },

            Panic => return self.op_panic(lv, rv),

            // `l , r |> op` → 左を uncons して op に渡す (§8.10)
            Uncurry => match &lv {
                Value::Cons(a, c) => {
                    let (a, c) = ((**a).clone(), (**c).clone());
                    return self.apply(rv, Arg::Val(a), Arg::Val(c));
                }
                _ => Value::term(Value::Builtin(b), lv, rv),
            },
            // `f $ l , r` → (l f r)
            Apply => match &rv {
                Value::Cons(a, c) => {
                    let (a, c) = ((**a).clone(), (**c).clone());
                    return self.apply(lv, Arg::Val(a), Arg::Val(c));
                }
                _ => Value::term(Value::Builtin(b), lv, rv),
            },

            // 文字列は連結について閉じている (§8.9)
            Concat => match (&lv, &rv) {
                (Value::Sym(x), Value::Sym(y)) => Value::sym(&format!("{}{}", x, y)),
                _ => Value::term(Value::Builtin(b), lv, rv),
            },

            // `b` の左スパインの要素を `a` に順に積み直す (§8.11)
            Append => rv.spine().into_iter().fold(lv, Value::cons),

            // 先頭から数える。範囲外は () (§8.11)
            Index => match &rv {
                Value::Num(n) => {
                    let sp = lv.spine();
                    if *n < 0 {
                        Value::Unit
                    } else {
                        sp.get(*n as usize).cloned().unwrap_or(Value::Unit)
                    }
                }
                _ => Value::term(Value::Builtin(b), lv, rv),
            },

            // 文字列の n 番目の文字（1文字の文字列を返す）
            CharAt => match (&lv, &rv) {
                (Value::Sym(s), Value::Num(n)) if *n >= 0 => {
                    match s.chars().nth(*n as usize) {
                        Some(c) => Value::sym(&c.to_string()),
                        None => Value::Unit,
                    }
                }
                _ => Value::term(Value::Builtin(b), lv, rv),
            },

            // 文字列から数への唯一の経路。右オペランドは基数 (§8.9)
            Parse => match (&lv, &rv) {
                (Value::Sym(s), Value::Num(radix)) if (2..=36).contains(radix) => {
                    match i64::from_str_radix(s, *radix as u32) {
                        Ok(n) => Value::Num(n),
                        Err(_) => Value::Unit,
                    }
                }
                _ => Value::Unit,
            },

            OpenRead | OpenWrite | OpenRw => {
                let fd = match lv {
                    Value::Unit => None, // 「fd はそちらで決めて」
                    Value::Num(n) => Some(n),
                    _ => return Ok(Value::term(Value::Builtin(b), lv, rv)),
                };
                let mode = match b {
                    OpenRead => OpenMode::Read,
                    OpenWrite => OpenMode::Write,
                    _ => OpenMode::ReadWrite,
                };
                let path = display(&rv);
                match self.io.open(fd, &path, mode) {
                    Some(fd) => Value::Num(fd),
                    None => Value::Unit, // 開けなければ ()
                }
            }

            Assign | Guard | Elvis | Unless => unreachable!(),
        })
    }

    /// `=` — 束縛 (§8.1)
    fn op_assign(&mut self, l: Arg, r: Arg) -> R<Value> {
        let Arg::Lazy(lexpr, lenv) = l else {
            // フラグ付き束縛は第一級ではない。
            // 左が値として届いたので束縛できない (§8.10)
            let _ = self.force(r)?;
            return Ok(Value::Unit);
        };

        // 形を検査してから束縛する。`=` は原子的
        let pat = pattern(&lexpr, &lenv)?;
        let rv = self.force(r)?;

        let mut binds = Vec::new();
        if !match_pattern(&pat, &rv, &mut binds) {
            // マッチ失敗は () を返す。エラーにはしない
            return Ok(Value::Unit);
        }
        for (name, v) in binds {
            // 常に最内フレームに対して束縛する (§6.2)
            lenv.define(name, v);
        }
        // `=` は右辺の値を返す
        Ok(rv)
    }

    /// `!!` — 中断 (§8.6)。両オペランドを評価してから死ぬだけ
    fn op_panic(&mut self, lv: Value, rv: Value) -> R<Value> {
        let msg = display(&rv);
        self.io.write(2, &msg);
        self.io.write(2, "\n");
        self.io.flush(2);
        // POSIX の下位8bit。`256 !! err` はエラーを印字して成功終了する (§9)
        let code = match lv {
            Value::Num(n) => (n & 0xff) as i32,
            _ => 1,
        };
        Err(Flow::Panic(code))
    }
}
