//! 字句解析 (§1)
//!
//! 空白が唯一の区切り。`(` `)` `{` `}` は全域予約で自己区切り。
//! `"` と `//` は **トークン先頭のときだけ** 特別。

use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
    /// ソース先頭からのバイト位置と、そのトークンのバイト長。
    /// 役割は位置だけで決まるので、ハイライトはこの2つで足りる (§0)
    pub byte: u32,
    pub len: u32,
}

impl Pos {
    /// ソースの先頭
    pub fn start() -> Pos {
        Pos { line: 1, col: 1, byte: 0, len: 0 }
    }
}

impl std::fmt::Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    LParen,
    RParen,
    LBrace,
    RBrace,
    /// 引用符なしのトークン。環境を引く
    Word(Rc<str>),
    /// 引用されたトークン。環境を引かず文字列そのものに評価される (§1.3)
    Quoted(Rc<str>),
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
}

/// 全域予約の自己区切り文字
fn is_bracket(c: char) -> bool {
    matches!(c, '(' | ')' | '{' | '}')
}

struct Lexer {
    src: Vec<char>,
    i: usize,
    line: u32,
    col: u32,
    byte: u32,
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.src.get(self.i).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.src.get(self.i + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.src.get(self.i).copied()?;
        self.i += 1;
        self.byte += c.len_utf8() as u32;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col,
            byte: self.byte,
            len: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.bump();
        }
    }

    /// `//` から行末まで。トークンを1つも生まない (§1.5)
    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    /// 引用。`"` で挟み `\` でエスケープする (§1.3)
    fn quoted(&mut self) -> Result<Tok, String> {
        let open = self.pos();
        self.bump(); // 開き `"`
        let mut buf = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(format!(
                        "{}: 未終端の引用 (開始位置 {})",
                        self.pos(),
                        open
                    ))
                }
                Some('"') => return Ok(Tok::Quoted(Rc::from(buf.as_str()))),
                Some('\\') => match self.bump() {
                    None => {
                        return Err(format!(
                            "{}: 未終端の引用 (開始位置 {})",
                            self.pos(),
                            open
                        ))
                    }
                    Some('n') => buf.push('\n'),
                    Some('t') => buf.push('\t'),
                    Some('r') => buf.push('\r'),
                    Some('0') => buf.push('\0'),
                    Some('e') => buf.push('\u{1b}'),
                    // それ以外は文字そのもの (`\\` `\"` を含む)
                    Some(c) => buf.push(c),
                },
                Some(c) => buf.push(c),
            }
        }
    }

    /// 普通のトークン。空白か括弧に当たるまで。
    /// `"` や `//` が途中に現れても切れない (`x"` `a//b` は合法な識別子)
    fn word(&mut self) -> Tok {
        let mut buf = String::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() || is_bracket(c) {
                break;
            }
            buf.push(c);
            self.bump();
        }
        Tok::Word(Rc::from(buf.as_str()))
    }
}

pub fn lex(src: &str) -> Result<Vec<Token>, String> {
    let mut lx = Lexer {
        src: src.chars().collect(),
        i: 0,
        line: 1,
        col: 1,
        byte: 0,
    };
    let mut out = Vec::new();

    loop {
        lx.skip_whitespace();
        let mut pos = lx.pos();
        let Some(c) = lx.peek() else { break };

        let tok = match c {
            '(' => {
                lx.bump();
                Tok::LParen
            }
            ')' => {
                lx.bump();
                Tok::RParen
            }
            '{' => {
                lx.bump();
                Tok::LBrace
            }
            '}' => {
                lx.bump();
                Tok::RBrace
            }
            // トークン先頭が `"` と `//` の両方に見える場合は `"` が勝つ (§1.2)
            '"' => lx.quoted()?,
            '/' if lx.peek_at(1) == Some('/') => {
                lx.skip_line_comment();
                continue;
            }
            _ => lx.word(),
        };
        pos.len = lx.byte - pos.byte;
        out.push(Token { tok, pos });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn word(s: &str) -> Tok {
        Tok::Word(Rc::from(s))
    }

    fn quoted(s: &str) -> Tok {
        Tok::Quoted(Rc::from(s))
    }

    #[test]
    fn 空白が唯一の区切り() {
        assert_eq!(toks("x + y").len(), 3);
        // `(x+y)` は括弧を除けば1項
        assert_eq!(toks("x+y"), vec![word("x+y")]);
        assert_eq!(toks("1-1"), vec![word("1-1")]);
        assert_eq!(toks("1 - 1").len(), 3);
    }

    #[test]
    fn 括弧は自己区切り() {
        assert_eq!(
            toks("a(b"),
            vec![word("a"), Tok::LParen, word("b")]
        );
        assert_eq!(toks("(x+y)").len(), 3);
    }

    #[test]
    fn 引用符はトークン先頭でだけ特別() {
        assert_eq!(toks(r#"x""#), vec![word("x\"")]);
        assert_eq!(toks(r#""hello world""#), vec![quoted("hello world")]);
        // トークン先頭が `"` と `//` の両方に見える場合は `"` が勝つ
        assert_eq!(toks(r#""a//b""#), vec![quoted("a//b")]);
    }

    #[test]
    fn コメントはトークン先頭でだけ特別() {
        assert_eq!(toks("x//y"), vec![word("x//y")]);
        assert_eq!(toks("x //y"), vec![word("x")]);
        assert_eq!(toks("a//b"), vec![word("a//b")]);
        assert_eq!(toks("x = 1 ; // 説明\n2").len(), 5);
    }

    #[test]
    fn バックスラッシュは文字列モードの中だけ特別() {
        assert_eq!(toks(r"a\b"), vec![word(r"a\b")]);
        assert_eq!(toks(r#""a\"b""#), vec![quoted("a\"b")]);
        assert_eq!(toks(r#""\n""#), vec![quoted("\n")]);
    }

    #[test]
    fn 予約文字を含まない綴りは何でも識別子() {
        assert_eq!(toks("<<<"), vec![word("<<<")]);
        assert_eq!(toks("一"), vec![word("一")]);
        assert_eq!(toks("!!"), vec![word("!!")]);
    }

    #[test]
    fn 未終端の引用はエラー() {
        assert!(lex(r#"" = '"#).is_err());
    }
}
