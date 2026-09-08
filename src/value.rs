//! 値 (§7)
//!
//! oddity の値は数・文字列・関数・cons・ユニットの5つ (§7)。
//! 加えて、適用に失敗した式が残る「記号項」がある (付録)。
//! 文字列は `Value::Sym` として持つ。仕様側の呼び名は「文字列」で、`Sym` は内部名。

use crate::env::Env;
use crate::parser::Items;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Seq,        // ;
    Assign,     // =
    Out,        // <<<
    In,         // >>>
    Panic,      // !!
    Guard,      // ?  (&&)
    Elvis,      // :  (||)
    Unless,     // !
    Eq,         // ==
    Ne,         // !=
    Lt,         // <
    Gt,         // >
    Le,         // <=
    Ge,         // >=
    Uncurry,    // |>
    Apply,      // $
    Cons,       // ,
    Add,        // +
    Sub,        // -
    Mul,        // *
    Div,        // /
    Rem,        // %
    Concat,     // ++
    Append,     // <>
    Index,      // #
    CharAt,     // @
    Parse,      // ~
    OpenRead,   // <<-
    OpenWrite,  // ->>
    OpenRw,     // <->
}

impl Builtin {
    pub fn spelling(self) -> &'static str {
        use Builtin::*;
        match self {
            Seq => ";", Assign => "=", Out => "<<<", In => ">>>", Panic => "!!",
            Guard => "?", Elvis => ":", Unless => "!", Eq => "==", Ne => "!=",
            Lt => "<", Gt => ">", Le => "<=", Ge => ">=", Uncurry => "|>",
            Apply => "$", Cons => ",", Add => "+", Sub => "-", Mul => "*",
            Div => "/", Rem => "%", Concat => "++", Append => "<>", Index => "#",
            CharAt => "@", Parse => "~", OpenRead => "<<-", OpenWrite => "->>",
            OpenRw => "<->",
        }
    }
}

/// 2引数関数（ユーザー定義）。
/// 捨てられるフレームの中に自分の名前が束縛されており、それを捕まえている (§2.3)
#[derive(Debug)]
pub struct Closure {
    /// シグネチャの2番目が引用符なしのトークンでなければ名前を持たない。
    /// `{}` の偶数項も同じ経路で無名になる (§2.2)
    pub name: Option<Rc<str>>,
    /// 引数名も同じ。名前でない位置に来た引数は評価だけされて捨てられる
    pub lparam: Option<Rc<str>>,
    pub rparam: Option<Rc<str>>,
    /// 本体は平坦な項列のまま持つ。呼び出しのたびにグルーピングされる (§4.5)
    pub body: Items,
    pub env: Env,
}

/// 束縛が持つレコード (§4.4)。
/// これによって「特殊形式」という概念が消える (§5)
#[derive(Debug)]
pub struct OpRecord {
    pub func: Value,
    pub prec: u32,
    pub lazy_left: bool,
    pub lazy_right: bool,
}

/// 適用に失敗した式。計算できないものは記号のまま残る (付録)
#[derive(Debug)]
pub struct Term {
    pub op: Value,
    pub left: Value,
    pub right: Value,
}

#[derive(Debug, Clone)]
pub enum Value {
    /// 唯一の偽値 (§7.1)
    Unit,
    Num(i64),
    Sym(Rc<str>),
    Cons(Rc<Value>, Rc<Value>),
    Fun(Rc<Closure>),
    Builtin(Builtin),
    Op(Rc<OpRecord>),
    Term(Rc<Term>),
}

impl Value {
    pub fn sym(s: &str) -> Value {
        Value::Sym(Rc::from(s))
    }

    pub fn cons(l: Value, r: Value) -> Value {
        Value::Cons(Rc::new(l), Rc::new(r))
    }

    pub fn term(op: Value, left: Value, right: Value) -> Value {
        Value::Term(Rc::new(Term { op, left, right }))
    }

    /// 偽は `()` のみ。数の 0 も空文字列 `""` も真 (§7.1)
    pub fn truthy(&self) -> bool {
        !matches!(self, Value::Unit)
    }

    /// リストとは cons の **左スパイン** のことでしかない (§8.2)。
    /// 終端が無いので、cons でない値は1要素のリストになる
    pub fn spine(&self) -> Vec<Value> {
        let mut out = Vec::new();
        let mut cur = self.clone();
        loop {
            match cur {
                Value::Cons(l, r) => {
                    out.push((*r).clone());
                    cur = (*l).clone();
                }
                v => {
                    out.push(v);
                    break;
                }
            }
        }
        out.reverse();
        out
    }
}

/// 内容比較 (§8.9)。文字列は不変で、内容は綴りそのもの
pub fn value_eq(a: &Value, b: &Value) -> bool {
    use Value::*;
    match (a, b) {
        (Unit, Unit) => true,
        (Num(x), Num(y)) => x == y,
        (Sym(x), Sym(y)) => x == y,
        (Cons(a1, a2), Cons(b1, b2)) => value_eq(a1, b1) && value_eq(a2, b2),
        (Fun(x), Fun(y)) => Rc::ptr_eq(x, y),
        (Builtin(x), Builtin(y)) => x == y,
        (Op(x), Op(y)) => Rc::ptr_eq(x, y),
        (Term(x), Term(y)) => {
            value_eq(&x.op, &y.op)
                && value_eq(&x.left, &y.left)
                && value_eq(&x.right, &y.right)
        }
        _ => false,
    }
}

/// `<` の族が使う順序。数どうし・文字列どうしでしか定義されない
pub fn value_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => Some(x.cmp(y)),
        (Value::Sym(x), Value::Sym(y)) => Some(x.as_ref().cmp(y.as_ref())),
        _ => None,
    }
}

/// 印字形。文字列は綴りそのものが出るので、数 1 と文字列 `1` は
/// 印字が同一で区別できない (§8.9)
pub fn display(v: &Value) -> String {
    let mut s = String::new();
    write_value(&mut s, v);
    s
}

fn write_value(out: &mut String, v: &Value) {
    match v {
        Value::Unit => out.push_str("()"),
        Value::Num(n) => out.push_str(&n.to_string()),
        Value::Sym(s) => out.push_str(s),
        Value::Cons(_, _) => {
            out.push('(');
            for (i, e) in v.spine().iter().enumerate() {
                if i > 0 {
                    out.push_str(" , ");
                }
                write_value(out, e);
            }
            out.push(')');
        }
        Value::Fun(c) => match &c.name {
            Some(n) => {
                out.push_str("<fn ");
                out.push_str(n);
                out.push('>');
            }
            None => out.push_str("<fn>"),
        },
        // 組み込みは綴りそのもので出る。おかげで記号項がソースの見た目に戻る (付録)
        Value::Builtin(b) => out.push_str(b.spelling()),
        Value::Op(r) => write_value(out, &r.func),
        Value::Term(t) => {
            out.push('(');
            write_value(out, &t.left);
            out.push(' ');
            write_value(out, &t.op);
            out.push(' ');
            write_value(out, &t.right);
            out.push(')');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 空リストは存在せず一要素のリストは値そのもの() {
        assert_eq!(Value::Num(1).spine().len(), 1);
    }

    #[test]
    fn 左ネストは見えない() {
        // ((1 , 2) , 3) ≡ (1 , 2 , 3)
        let a = Value::cons(Value::cons(Value::Num(1), Value::Num(2)), Value::Num(3));
        assert_eq!(a.spine().len(), 3);
        assert_eq!(display(&a), "(1 , 2 , 3)");

        // 右ネストだけが構造として残る
        let b = Value::cons(Value::Num(1), Value::cons(Value::Num(2), Value::Num(3)));
        assert_eq!(b.spine().len(), 2);
        assert_eq!(display(&b), "(1 , (2 , 3))");
    }

    #[test]
    fn 偽は_unit_のみ() {
        assert!(!Value::Unit.truthy());
        assert!(Value::Num(0).truthy());
        assert!(Value::sym("").truthy());
    }
}
