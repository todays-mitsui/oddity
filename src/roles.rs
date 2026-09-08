//! 位置から決まる役割 (§0, §2.1)
//!
//! この言語では字種が役割を決めないので、ハイライトに使える情報は
//! **括弧の中で何番目か** だけである。順位（＝どう結合するか）は実行時の値なので
//! 出せない（§4.3）が、役割そのものは括弧の対応が取れた時点で確定する。

use crate::parser::{Item, Items};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Role {
    /// 奇数項の括弧の、奇数番目（0起算で偶数）
    Operand = 0,
    /// 奇数項の括弧の、偶数番目（0起算で奇数）
    Operator = 1,
    /// 偶数項の括弧のシグネチャ1番目と3番目
    Param = 2,
    /// 偶数項の括弧のシグネチャ2番目。ここが関数名として潰される
    Name = 3,
    /// 2項の括弧。構文エラー
    Error = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub byte: u32,
    pub len: u32,
    pub role: Role,
}

/// 括弧の入れ子を歩いて、トークンに役割を振る。
/// 括弧そのものには振らない（役割を持つのは項だけ）
pub fn roles(items: &Items) -> Vec<Span> {
    let mut out = Vec::new();
    walk(items, &mut out);
    out.sort_by_key(|s| s.byte);
    out
}

fn walk(items: &Items, out: &mut Vec<Span>) {
    let n = items.len();
    match n {
        0 => {}
        2 => {
            // 奇偶判定の前段で特別扱いされる (§2.1)
            for it in items.iter() {
                push(it, Role::Error, out);
            }
        }
        _ if n % 2 == 1 => {
            for (i, it) in items.iter().enumerate() {
                push(it, by_index(i), out);
            }
        }
        _ => {
            // 4以上の偶数 → シグネチャ3項 + 本体（奇数項）
            push(&items[0], Role::Param, out);
            push(&items[1], Role::Name, out);
            push(&items[2], Role::Param, out);
            for (i, it) in items[3..].iter().enumerate() {
                push(it, by_index(i), out);
            }
        }
    }
    for it in items.iter() {
        if let Item::Group(_, inner, _) = &**it {
            walk(inner, out);
        }
    }
}

fn by_index(i: usize) -> Role {
    if i % 2 == 0 {
        Role::Operand
    } else {
        Role::Operator
    }
}

fn push(item: &Item, role: Role, out: &mut Vec<Span>) {
    // 括弧は役割を持たない。中身が持つ
    if matches!(item, Item::Group(_, _, _)) {
        return;
    }
    let p = item.pos();
    out.push(Span { byte: p.byte, len: p.len, role });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn r(src: &str) -> Vec<(String, Role)> {
        let items = parse(src).unwrap();
        roles(&items)
            .into_iter()
            .map(|s| {
                let t = src[s.byte as usize..(s.byte + s.len) as usize].to_string();
                (t, s.role)
            })
            .collect()
    }

    #[test]
    fn 奇数項は位置で役割が決まる() {
        assert_eq!(
            r("x + y"),
            vec![
                ("x".into(), Role::Operand),
                ("+".into(), Role::Operator),
                ("y".into(), Role::Operand),
            ]
        );
    }

    #[test]
    fn 偶数項はシグネチャと本体に割れる() {
        assert_eq!(
            r("(n fact d n * 1)"),
            vec![
                ("n".into(), Role::Param),
                ("fact".into(), Role::Name),
                ("d".into(), Role::Param),
                ("n".into(), Role::Operand),
                ("*".into(), Role::Operator),
                ("1".into(), Role::Operand),
            ]
        );
    }

    #[test]
    fn 二文字以上の演算子に空白を入れると名前が潰れるのが見える() {
        // §10 の地雷。`=` が Name として塗られる
        assert_eq!(
            r("a = = b"),
            vec![
                ("a".into(), Role::Param),
                ("=".into(), Role::Name),
                ("=".into(), Role::Param),
                ("b".into(), Role::Operand),
            ]
        );
    }

    #[test]
    fn 括弧は役割を持たず中身が持つ() {
        assert_eq!(
            r("1 <<< (x + y)"),
            vec![
                ("1".into(), Role::Operand),
                ("<<<".into(), Role::Operator),
                ("x".into(), Role::Operand),
                ("+".into(), Role::Operator),
                ("y".into(), Role::Operand),
            ]
        );
    }

    #[test]
    fn マルチバイトでもバイト位置が合う() {
        let src = "一 + 二";
        let got = r(src);
        assert_eq!(got[0].0, "一");
        assert_eq!(got[2].0, "二");
    }
}
