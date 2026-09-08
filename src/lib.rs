//! oddity — 奇数項なら式、偶数項なら宣言。項数の奇偶が意味を決める言語。

pub mod builtins;
pub mod env;
pub mod eval;
pub mod io;
pub mod lexer;
pub mod parser;
pub mod roles;
pub mod value;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use eval::{Flow, Interp};
use io::IoTable;
use parser::GroupKind;
use std::cell::RefCell;
use std::rc::Rc;

/// ソースを走らせる。ファイル全体は暗黙のパーレン (§2.4)
pub fn run(src: &str, io: IoTable) -> (i32, Option<String>, IoTable) {
    let items = match parser::parse(src) {
        Ok(i) => i,
        Err(e) => return (1, Some(e), io),
    };
    let toplevel = env::prelude().child();
    let mut it = Interp::new(io);
    let r = it.eval_group(GroupKind::Paren, &items, &toplevel);
    // `!!` で死ぬときは stdout を流さない —— §10 最後の地雷
    if !matches!(r, Err(Flow::Panic(_))) {
        it.io.flush(1);
    }
    match r {
        // プログラム全体の評価値は捨てられる。既定で 0 で終了し、
        // それ以外の終了コードを返す手段は `!!` だけ (§9)
        Ok(_) => (0, None, it.io),
        Err(Flow::Panic(c)) => (c, None, it.io),
        Err(Flow::Fatal(msg)) => (1, Some(msg), it.io),
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

struct SharedBuf(Rc<RefCell<Vec<u8>>>);

impl std::io::Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// テスト用。標準入出力を捕まえて走らせる
pub fn run_capture(src: &str, input: &str) -> Outcome {
    let out = Rc::new(RefCell::new(Vec::new()));
    let err = Rc::new(RefCell::new(Vec::new()));
    let io = IoTable::with_streams(
        Box::new(SharedBuf(out.clone())),
        Box::new(SharedBuf(err.clone())),
        Box::new(std::io::Cursor::new(input.as_bytes().to_vec())),
    );
    let (code, error, _io) = run(src, io);
    let stdout = String::from_utf8_lossy(&out.borrow()).into_owned();
    let stderr = String::from_utf8_lossy(&err.borrow()).into_owned();
    Outcome {
        code,
        stdout,
        stderr,
        error,
    }
}
