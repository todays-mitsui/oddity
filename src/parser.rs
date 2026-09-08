//! 構文解析 (§2)
//!
//! 括弧の中身は木にせず **平坦な項の列** として保持する (§2, §4)。
//! ここで行う検査は「2項は構文エラー」だけ。順位は実行時の値なので、
//! どう結合するかはこの段階では決められない。

use crate::lexer::{lex, Pos, Tok, Token};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// `()` — 環境を汚す
    Paren,
    /// `{}` — フレームを1枚積む
    Brace,
}

impl GroupKind {
    fn closer(self) -> &'static str {
        match self {
            GroupKind::Paren => ")",
            GroupKind::Brace => "}",
        }
    }
}

/// 括弧の中身を構成する「項」。
#[derive(Debug, Clone)]
pub enum Item {
    Word(Rc<str>, Pos),
    Quoted(Rc<str>, Pos),
    Group(GroupKind, Items, Pos),
}

pub type Items = Rc<[Rc<Item>]>;

impl Item {
    pub fn pos(&self) -> Pos {
        match self {
            Item::Word(_, p) | Item::Quoted(_, p) | Item::Group(_, _, p) => *p,
        }
    }

    /// 引用符なしの単一トークンか (§8.1 の左辺形で使う)
    pub fn as_word(&self) -> Option<&Rc<str>> {
        match self {
            Item::Word(w, _) => Some(w),
            _ => None,
        }
    }
}

struct Parser {
    toks: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.i)
    }

    /// 閉じ括弧または入力終端まで項を読む
    fn items(&mut self, closing: Option<GroupKind>, open: Pos) -> Result<Items, String> {
        let mut out: Vec<Rc<Item>> = Vec::new();
        loop {
            let Some(t) = self.peek().cloned() else {
                return match closing {
                    None => Ok(out.into()),
                    Some(k) => Err(format!(
                        "{}: `{}` が閉じられていない (開始位置 {})",
                        t_end(&self.toks),
                        k.closer(),
                        open
                    )),
                };
            };
            self.i += 1;
            let item = match t.tok {
                Tok::RParen | Tok::RBrace => {
                    let got = if matches!(t.tok, Tok::RParen) {
                        GroupKind::Paren
                    } else {
                        GroupKind::Brace
                    };
                    return match closing {
                        Some(k) if k == got => Ok(out.into()),
                        Some(k) => Err(format!(
                            "{}: `{}` で閉じるべき括弧が `{}` で閉じられた (開始位置 {})",
                            t.pos,
                            k.closer(),
                            got.closer(),
                            open
                        )),
                        None => Err(format!("{}: 対応する開き括弧がない", t.pos)),
                    };
                }
                Tok::LParen => {
                    let inner = self.items(Some(GroupKind::Paren), t.pos)?;
                    Item::Group(GroupKind::Paren, inner, t.pos)
                }
                Tok::LBrace => {
                    let inner = self.items(Some(GroupKind::Brace), t.pos)?;
                    Item::Group(GroupKind::Brace, inner, t.pos)
                }
                Tok::Word(w) => Item::Word(w, t.pos),
                Tok::Quoted(s) => Item::Quoted(s, t.pos),
            };
            out.push(Rc::new(item));
        }
    }
}

fn t_end(toks: &[Token]) -> Pos {
    toks.last().map(|t| t.pos).unwrap_or_default()
}

/// ファイル全体は暗黙のパーレンで囲まれている (§2.4)
pub fn parse(src: &str) -> Result<Items, String> {
    let toks = lex(src)?;
    let mut p = Parser { toks, i: 0 };
    let items = p.items(None, Pos::start())?;
    check_arity(&items, Pos::start())?;
    Ok(items)
}

/// 2項は構文エラー (§2.1)。
/// 順位に依存しない純粋な項数の検査なので、ここで静的に捕まえられる。
fn check_arity(items: &Items, pos: Pos) -> Result<(), String> {
    if items.len() == 2 {
        return Err(format!(
            "{}: 2項の括弧は構文エラー (シグネチャの3項が取れない)",
            pos
        ));
    }
    for it in items.iter() {
        if let Item::Group(_, inner, p) = &**it {
            check_arity(inner, *p)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 項数は空白で決まる() {
        assert_eq!(parse("(x + y)").unwrap().len(), 1);
        let Item::Group(_, inner, _) = &*parse("(x + y)").unwrap()[0] else {
            panic!()
        };
        assert_eq!(inner.len(), 3);

        let Item::Group(_, inner, _) = &*parse("(x+y)").unwrap()[0] else {
            panic!()
        };
        assert_eq!(inner.len(), 1);
    }

    #[test]
    fn トップレベルは暗黙のパーレン() {
        assert_eq!(parse("a ; b ; c").unwrap().len(), 5);
    }

    #[test]
    fn 二項は構文エラー() {
        assert!(parse("(a ;)").is_err());
        assert!(parse("(a b)").is_err());
        assert!(parse("a b").is_err());
        // 0項と1項は通る
        assert!(parse("()").is_ok());
        assert!(parse("(a)").is_ok());
    }

    #[test]
    fn 括弧の対応が崩れたらエラー() {
        assert!(parse("(a ; b").is_err());
        assert!(parse("{a ; b)").is_err());
        assert!(parse("a ; b)").is_err());
    }

    #[test]
    fn コメントは項数に影響しない() {
        assert_eq!(parse("a ; b // これは消える\n ; c").unwrap().len(), 5);
    }
}
