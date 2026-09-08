//! examples/ 以下が動くことを確かめる

use std::path::Path;

fn run_example(name: &str, input: &str) -> oddity::Outcome {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name);
    let src = std::fs::read_to_string(&path).expect("例題が読めない");
    let o = oddity::run_capture(&src, input);
    assert!(o.error.is_none(), "{}: {:?}", name, o.error);
    o
}

#[test]
fn hello() {
    assert_eq!(run_example("hello.odd", "").stdout, "Hello,world!\n");
}

#[test]
fn greet() {
    // 手で触られる例題なので、綴りではなく振る舞いだけを見る
    let o = run_example("greet.odd", "Mitsui\n");
    assert!(o.stdout.starts_with("What's your name? : "), "{:?}", o.stdout);
    assert!(o.stdout.contains("Mitsui"), "{:?}", o.stdout);
    assert_eq!(o.code, 0);
}

#[test]
fn fact() {
    let o = run_example("fact.odd", "");
    assert_eq!(o.stdout, "10! = 3628800\n");
    assert_eq!(o.code, 0);
}

#[test]
fn fizzbuzz() {
    let o = run_example("fizzbuzz.odd", "");
    let lines: Vec<&str> = o.stdout.lines().collect();
    assert_eq!(lines.len(), 20);
    assert_eq!(lines[0], "1");
    assert_eq!(lines[2], "Fizz");
    assert_eq!(lines[4], "Buzz");
    assert_eq!(lines[14], "FizzBuzz");
}

/// 難読化版が元と1バイトも違わない出力になること
fn assert_same(base: &str, obfuscated: &str, input: &str) {
    let a = run_example(base, input);
    let b = run_example(obfuscated, input);
    assert_eq!(a.stdout, b.stdout, "{} の stdout", obfuscated);
    assert_eq!(a.stderr, b.stderr, "{} の stderr", obfuscated);
    assert_eq!(b.code, 0);
}

#[test]
fn 難読化版() {
    assert_same("hello.odd", "hello_.odd", "");
    assert_same("greet.odd", "greet_.odd", "Mitsui\n");
    assert_same("fact.odd", "fact_.odd", "");
    assert_same("fizzbuzz.odd", "fizzbuzz_.odd", "");
    assert_same("sum.odd", "sum_.odd", "");
    assert_same("cat.odd", "cat_.odd", "alpha\nbravo\n");
}

#[test]
fn 難読化版_symbolic() {
    // これだけは出力自体が記号なので、難読化すると出力も変わる
    let o = run_example("symbolic_.odd", "");
    assert_eq!(o.stdout, "3\n(<< + >>)\n(3 * <->)\n(+ * /)\n");
    assert_eq!(o.stderr, "2 / 1\n");
}

#[test]
fn sum() {
    let o = run_example("sum.odd", "");
    assert_eq!(o.stdout, "(1 , 2 , 3 , 4 , 5) の合計は 15\n42\n");
}

#[test]
fn cat() {
    let o = run_example("cat.odd", "alpha\nbravo\n");
    assert_eq!(o.stdout, "1: alpha\n2: bravo\n");
}

#[test]
fn symbolic() {
    let o = run_example("symbolic.odd", "");
    assert_eq!(o.stdout, "3\n(x + y)\n(3 * z)\n");
    assert_eq!(o.stderr, "2 / 1\n");
}
