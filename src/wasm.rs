//! ブラウザ向けの口 (docs/ のプレイグラウンド)
//!
//! wasm-bindgen は使わない。素の C ABI で、長さ前置のバイト列をやり取りする。
//! 返り値は先頭4バイトがペイロード長（LE u32）、その後ろがペイロード。

use crate::env;
use crate::eval::{Flow, Interp};
use crate::io::IoTable;
use crate::parser::{parse, GroupKind};
use crate::roles::roles;
use std::alloc::{alloc, dealloc, Layout};
use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

/// プレイグラウンドの上限。止まらないプログラムでタブを固めないため
const FUEL: u64 = 20_000_000;
const MAX_DEPTH: u32 = 500;
const MAX_OUTPUT: usize = 256 * 1024;

#[no_mangle]
pub extern "C" fn od_alloc(size: usize) -> *mut u8 {
    if size == 0 {
        return std::ptr::null_mut();
    }
    unsafe { alloc(Layout::from_size_align_unchecked(size, 1)) }
}

#[no_mangle]
pub extern "C" fn od_free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() && size != 0 {
        unsafe { dealloc(ptr, Layout::from_size_align_unchecked(size, 1)) }
    }
}

fn slice<'a>(ptr: *const u8, len: usize) -> &'a str {
    if ptr.is_null() || len == 0 {
        return "";
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    std::str::from_utf8(bytes).unwrap_or("")
}

/// 長さ前置にして所有権を JS へ渡す
fn emit(payload: Vec<u8>) -> *mut u8 {
    let total = 4 + payload.len();
    let ptr = od_alloc(total);
    unsafe {
        std::ptr::copy_nonoverlapping((payload.len() as u32).to_le_bytes().as_ptr(), ptr, 4);
        std::ptr::copy_nonoverlapping(payload.as_ptr(), ptr.add(4), payload.len());
    }
    ptr
}

fn put_str(buf: &mut Vec<u8>, s: &str) {
    buf.extend_from_slice(&(s.len() as u32).to_le_bytes());
    buf.extend_from_slice(s.as_bytes());
}

/// 出力を溜める。上限を超えたら黙って捨てる
struct Sink(Rc<RefCell<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut b = self.0.borrow_mut();
        if b.len() < MAX_OUTPUT {
            b.extend_from_slice(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// ソースを走らせる。
/// 返り: [u32 code][str stdout][str stderr][str error]
#[no_mangle]
pub extern "C" fn od_run(
    src: *const u8,
    src_len: usize,
    input: *const u8,
    input_len: usize,
) -> *mut u8 {
    let src = slice(src, src_len);
    let input = slice(input, input_len).as_bytes().to_vec();

    let out = Rc::new(RefCell::new(Vec::new()));
    let err = Rc::new(RefCell::new(Vec::new()));
    let io = IoTable::with_streams(
        Box::new(Sink(out.clone())),
        Box::new(Sink(err.clone())),
        Box::new(std::io::Cursor::new(input)),
    );

    let (code, error) = match parse(src) {
        Err(e) => (1, Some(e)),
        Ok(items) => {
            let toplevel = env::prelude().child();
            let mut it = Interp::with_limits(io, FUEL, MAX_DEPTH);
            let r = it.eval_group(GroupKind::Paren, &items, &toplevel);
            if !matches!(r, Err(Flow::Panic(_))) {
                it.io.flush(1);
            }
            match r {
                Ok(_) => (0, None),
                Err(Flow::Panic(c)) => (c, None),
                Err(Flow::Fatal(m)) => (1, Some(m)),
            }
        }
    };

    let mut buf = Vec::new();
    buf.extend_from_slice(&(code as u32).to_le_bytes());
    put_str(&mut buf, &String::from_utf8_lossy(&out.borrow()));
    put_str(&mut buf, &String::from_utf8_lossy(&err.borrow()));
    put_str(&mut buf, error.as_deref().unwrap_or(""));
    emit(buf)
}

/// 役割を返す。括弧の対応が取れないうちは何も返さない。
/// 返り: [u32 トップレベルの項数][str エラー][u32 n][(u32 byte, u32 len, u32 role) * n]
#[no_mangle]
pub extern "C" fn od_analyze(src: *const u8, src_len: usize) -> *mut u8 {
    let src = slice(src, src_len);
    let mut buf = Vec::new();
    match parse(src) {
        Err(e) => {
            buf.extend_from_slice(&0u32.to_le_bytes());
            put_str(&mut buf, &e);
            buf.extend_from_slice(&0u32.to_le_bytes());
        }
        Ok(items) => {
            buf.extend_from_slice(&(items.len() as u32).to_le_bytes());
            put_str(&mut buf, "");
            let spans = roles(&items);
            buf.extend_from_slice(&(spans.len() as u32).to_le_bytes());
            for s in spans {
                buf.extend_from_slice(&s.byte.to_le_bytes());
                buf.extend_from_slice(&s.len.to_le_bytes());
                buf.extend_from_slice(&(s.role as u32).to_le_bytes());
            }
        }
    }
    emit(buf)
}
