//! 入出力 (§8.4)
//!
//! fd は単なる数であり、所有権を持たない。close は存在せず、
//! プロセス終了だけが閉じる契機である。

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, LineWriter, Write};

pub struct FdEntry {
    pub reader: Option<Box<dyn BufRead>>,
    pub writer: Option<Box<dyn Write>>,
}

pub enum OpenMode {
    Read,
    Write,
    ReadWrite,
}

pub struct IoTable {
    fds: HashMap<i64, FdEntry>,
}

impl Default for IoTable {
    fn default() -> Self {
        Self::new()
    }
}

impl IoTable {
    pub fn new() -> IoTable {
        let mut fds = HashMap::new();
        fds.insert(
            0,
            FdEntry {
                reader: Some(Box::new(BufReader::new(std::io::stdin()))),
                writer: None,
            },
        );
        fds.insert(
            1,
            FdEntry {
                reader: None,
                writer: Some(Box::new(std::io::stdout())),
            },
        );
        fds.insert(
            2,
            FdEntry {
                reader: None,
                writer: Some(Box::new(std::io::stderr())),
            },
        );
        IoTable { fds }
    }

    /// テスト用。stdout/stderr を差し替える
    pub fn with_streams(out: Box<dyn Write>, err: Box<dyn Write>, input: Box<dyn BufRead>) -> IoTable {
        let mut fds = HashMap::new();
        fds.insert(0, FdEntry { reader: Some(input), writer: None });
        fds.insert(
            1,
            FdEntry {
                // 本物の stdout と同じ行バッファにしておく
                reader: None,
                writer: Some(Box::new(LineWriter::new(out))),
            },
        );
        fds.insert(2, FdEntry { reader: None, writer: Some(err) });
        IoTable { fds }
    }

    /// 書けなければ黙って捨てる。`2 = 5` で標準エラーが消えるのと同じ経路 (§10)
    pub fn write(&mut self, fd: i64, s: &str) {
        if let Some(e) = self.fds.get_mut(&fd) {
            if let Some(w) = e.writer.as_mut() {
                let _ = w.write_all(s.as_bytes());
            }
        }
    }

    pub fn flush(&mut self, fd: i64) {
        if let Some(e) = self.fds.get_mut(&fd) {
            if let Some(w) = e.writer.as_mut() {
                let _ = w.flush();
            }
        }
    }

    /// 読み取りは、それまでに書かれたものを追い越さない (§8.4)。
    /// ブロックする前に書き込み側の fd をすべて掃き出す
    fn flush_all(&mut self) {
        for e in self.fds.values_mut() {
            if let Some(w) = e.writer.as_mut() {
                let _ = w.flush();
            }
        }
    }

    /// n 文字読む。EOF に達していれば None、
    /// n に足りずに EOF なら読めた分を返す (§8.4)
    pub fn read_chars(&mut self, fd: i64, n: i64) -> Option<String> {
        self.flush_all();
        let e = self.fds.get_mut(&fd)?;
        let r = e.reader.as_mut()?;
        if n <= 0 {
            // 0文字読み。空文字列 `""` は真なので EOF と区別できる
            return Some(String::new());
        }
        let mut out = String::new();
        for _ in 0..n {
            match read_char(r.as_mut()) {
                Some(c) => out.push(c),
                None => break,
            }
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// 1行読む。改行は含めない。EOF なら None
    pub fn read_line(&mut self, fd: i64) -> Option<String> {
        self.flush_all();
        let e = self.fds.get_mut(&fd)?;
        let r = e.reader.as_mut()?;
        let mut buf = String::new();
        match r.read_line(&mut buf) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                if buf.ends_with('\n') {
                    buf.pop();
                    if buf.ends_with('\r') {
                        buf.pop();
                    }
                }
                Some(buf)
            }
        }
    }

    /// fopen は fd を返すだけ。開けなければ None (§8.4)
    pub fn open(&mut self, fd: Option<i64>, path: &str, mode: OpenMode) -> Option<i64> {
        let entry = match mode {
            OpenMode::Read => {
                let f = File::open(path).ok()?;
                FdEntry {
                    reader: Some(Box::new(BufReader::new(f))),
                    writer: None,
                }
            }
            OpenMode::Write => {
                let f = OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(path)
                    .ok()?;
                FdEntry {
                    reader: None,
                    writer: Some(Box::new(f)),
                }
            }
            OpenMode::ReadWrite => {
                let f = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false) // 読み書きは切り詰めない
                    .open(path)
                    .ok()?;
                let g = f.try_clone().ok()?;
                FdEntry {
                    reader: Some(Box::new(BufReader::new(f))),
                    writer: Some(Box::new(g)),
                }
            }
        };
        let fd = fd.unwrap_or_else(|| self.alloc_fd());
        self.fds.insert(fd, entry);
        Some(fd)
    }

    fn alloc_fd(&self) -> i64 {
        let mut fd = 3;
        while self.fds.contains_key(&fd) {
            fd += 1;
        }
        fd
    }
}

fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        _ => 1,
    }
}

fn read_char(r: &mut dyn BufRead) -> Option<char> {
    let mut buf = [0u8; 4];
    match r.read(&mut buf[0..1]) {
        Ok(0) | Err(_) => return None,
        Ok(_) => {}
    }
    let len = utf8_len(buf[0]);
    if len > 1 && r.read_exact(&mut buf[1..len]).is_err() {
        return Some('\u{fffd}');
    }
    match std::str::from_utf8(&buf[..len]) {
        Ok(s) => s.chars().next(),
        Err(_) => Some('\u{fffd}'),
    }
}
