use oddity::io::IoTable;
use std::process::ExitCode;

const USAGE: &str = "\
oddity — 項数の奇偶が意味を決める言語

usage:
    oddity <file.od>        ファイルを走らせる
    oddity -e <source>      ソースを直接走らせる
    oddity --ast <file.od>  項の入れ子だけを表示する（結合は実行時に決まるので出せない）
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let (src, name) = match args.as_slice() {
        [] => {
            eprint!("{}", USAGE);
            return ExitCode::from(2);
        }
        [f] if f == "-h" || f == "--help" => {
            print!("{}", USAGE);
            return ExitCode::SUCCESS;
        }
        [e, src] if e == "-e" => (src.clone(), "<-e>".to_string()),
        [a, f] if a == "--ast" => match std::fs::read_to_string(f) {
            Ok(s) => return dump_ast(&s, f),
            Err(e) => {
                eprintln!("oddity: {}: {}", f, e);
                return ExitCode::from(2);
            }
        },
        [f] => match std::fs::read_to_string(f) {
            Ok(s) => (s, f.clone()),
            Err(e) => {
                eprintln!("oddity: {}: {}", f, e);
                return ExitCode::from(2);
            }
        },
        _ => {
            eprint!("{}", USAGE);
            return ExitCode::from(2);
        }
    };

    // 再帰がループの唯一の手段なので、スタックを広く取る
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (code, err, _io) = oddity::run(&src, IoTable::new());
            if let Some(msg) = err {
                eprintln!("{}: {}", name, msg);
            }
            code
        })
        .expect("スレッドを起こせなかった");

    let code = handle.join().unwrap_or(1);
    ExitCode::from((code & 0xff) as u8)
}

fn dump_ast(src: &str, name: &str) -> ExitCode {
    match oddity::parser::parse(src) {
        Ok(items) => {
            print_items(&items, 0);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {}", name, e);
            ExitCode::from(1)
        }
    }
}

fn print_items(items: &oddity::parser::Items, depth: usize) {
    use oddity::parser::{GroupKind, Item};
    let pad = "  ".repeat(depth);
    let kind = match items.len() {
        0 => "ユニット",
        2 => "構文エラー",
        n if n % 2 == 1 => "式",
        _ => "関数宣言",
    };
    println!("{}[{}項 → {}]", pad, items.len(), kind);
    for it in items.iter() {
        match &**it {
            Item::Word(w, p) => println!("{}  {} ({})", pad, w, p),
            Item::Quoted(s, p) => println!("{}  \"{}\" ({})", pad, s, p),
            Item::Group(k, inner, p) => {
                let b = match k {
                    GroupKind::Paren => "()",
                    GroupKind::Brace => "{}",
                };
                println!("{}  {} ({})", pad, b, p);
                print_items(inner, depth + 2);
            }
        }
    }
}
