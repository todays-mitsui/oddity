//! docs/spec.md から起こした振る舞いのテスト。節番号は仕様に対応する。

use oddity::run_capture;

fn out(src: &str) -> String {
    let o = run_capture(src, "");
    assert!(o.error.is_none(), "予期しないエラー: {:?}", o.error);
    o.stdout
}

fn out_in(src: &str, input: &str) -> String {
    let o = run_capture(src, input);
    assert!(o.error.is_none(), "予期しないエラー: {:?}", o.error);
    o.stdout
}

fn err(src: &str) -> String {
    run_capture(src, "").stderr
}

fn code(src: &str) -> i32 {
    run_capture(src, "").code
}

fn fatal(src: &str) -> String {
    run_capture(src, "")
        .error
        .expect("構文エラーになるはずだった")
}

// ---------------------------------------------------------------- §1 字句

#[test]
fn 空白の有無で意味が変わる() {
    // (x + y) は3項 → 演算
    assert_eq!(out("x = 2 ; y = 3 ; 1 <<< (x + y)"), "5");
    // (x+y) は1項 → `x+y` という名前の変数への参照（未束縛なので自己評価）
    assert_eq!(out("x = 2 ; y = 3 ; 1 <<< (x+y)"), "x+y");
    // 1-1 は1トークン（数ではない）
    assert_eq!(out("1 <<< 1-1"), "1-1");
    assert_eq!(out("1 <<< (1 - 1)"), "0");
}

#[test]
fn 引用はクォートであって型ではない() {
    // "abc" は文字列 abc に評価される。環境を引かない
    assert_eq!(out("abc = 5 ; 1 <<< \"abc\""), "abc");
    assert_eq!(out("abc = 5 ; 1 <<< abc"), "5");
    // 束縛の有無に関わらず綴りが保たれる
    assert_eq!(out("1 <<< \"disk full\""), "disk full");
    // "1" は文字列 1 であって数 1 ではない。数値フレームを通らない
    assert_eq!(out("1 <<< (\"1\" + 1)"), "(1 + 1)");
    assert_eq!(out("1 <<< (1 + 1)"), "2");
}

#[test]
fn コメントは項数に影響しない() {
    assert_eq!(out("x = 1 ; // 説明\n1 <<< x"), "1");
    // x//y は名前、x //y は行の残りが消える (§10)
    assert_eq!(out("1 <<< x//y"), "x//y");
    assert_eq!(out("1 <<< x //y"), "x");
}

// ---------------------------------------------------------------- §2 括弧と偶奇

#[test]
fn 零項はユニット値() {
    assert_eq!(out("1 <<< ()"), "()");
    assert_eq!(out("1 <<< {}"), "()");
}

#[test]
fn 二項は構文エラー() {
    assert!(fatal("(a ;)").contains("2項"));
    assert!(fatal("a b").contains("2項"));
}

#[test]
fn 奇数項は式で偶数項は宣言() {
    // 4項 → 関数宣言。宣言はその関数を返す
    assert_eq!(out("1 <<< (a f b b)"), "<fn f>");
    // 宣言された関数は現スコープから呼べる
    assert_eq!(out("(a f b (a + b)) ; 1 <<< (2 f 3)"), "5");
}

#[test]
fn 波括弧はフレームを一枚積む() {
    // §2.2 の例: 2, 1 の順に印字
    assert_eq!(out("x = 1 ; {x = 2 ; (1 <<< x)} ; (1 <<< x)"), "21");
    // () は環境を汚す
    assert_eq!(out("x = 1 ; (x = 2) ; (1 <<< x)"), "2");
}

#[test]
fn 波括弧の偶数項は無名関数を返す() {
    assert_eq!(out("1 <<< {a f b b}"), "<fn f>");
    // 名前は残らない
    assert_eq!(out("{a f b b} ; 1 <<< f"), "f");
    // () なら名前が残る
    assert_eq!(out("(a f b b) ; 1 <<< f"), "<fn f>");
}

#[test]
fn 再帰に_let_rec_も_y_コンビネータも要らない() {
    // 捨てられるフレームの中に関数自身の名前が束縛されている (§2.3)
    let src = "f = {n fact d (n <= 1) ? 1 : {n * ((n - 1) fact d)}} ; 1 <<< (5 f ())";
    assert_eq!(out(src), "120");
}

// ---------------------------------------------------------------- §4 優先順位

#[test]
fn デフォルト帯は同順位左結合() {
    assert_eq!(out("1 <<< (1 + 2 * 3)"), "9");
    assert_eq!(out("1 <<< (10 - 2 - 3)"), "5");
}

#[test]
fn 順位は束縛の属性で実行時に引かれる() {
    // 検算 (§4.2)
    assert_eq!(out("x = 1 ; y = x + 2 ; 1 <<< y"), "3");
}

#[test]
fn 演算子が何をするかは途中で変えられる() {
    // 関数は評価時に引かれるので、同じブロック内でも即座に効く (§4.5)
    assert_eq!(out("(1 <<< (1 + 2 * 3)) ; * = + ; (1 <<< (1 + 2 * 3))"), "96");
}

#[test]
fn どう結合するかは途中で変えられない() {
    // 順位はグルーピング時に引かれる。効くのは以降に評価される内側の括弧から
    // 同じブロックの残りは無傷 (§4.5)
    assert_eq!(out("(1 <<< 1) ; , = 0 ; (1 <<< 2)"), "12");
    // 内側の括弧では , がデフォルトに落ちて記号項になる (§10)
    assert_eq!(out("(1 <<< (1 , 2)) ; , = 0 ; (1 <<< (1 , 2))"), "(1 , 2)(1 0 2)");
}

// ---------------------------------------------------------------- §6 環境

#[test]
fn 同一フレームなら後勝ち() {
    assert_eq!(out("a = 1 ; a = 2 ; 1 <<< a"), "2");
}

#[test]
fn 内側から外側は書けない() {
    assert_eq!(out("x = 1 ; {x = 2} ; 1 <<< x"), "1");
    // 関数本体にも同じことが効く
    assert_eq!(out("x = 1 ; (a f b (x = 99)) ; (0 f 0) ; 1 <<< x"), "1");
}

#[test]
fn 自己評価フレームは失敗しない() {
    // 未定義エラーが存在しない
    assert_eq!(out("1 <<< nobody-bound-this"), "nobody-bound-this");
}

#[test]
fn 数は遮蔽できるが数値フレームは壊れない() {
    // 1 = 1 + 1 は右辺でプレリュードの 1 を読み、トップレベルに新しい束縛を作る
    assert_eq!(out("1 = 1 + 1 ; 2 <<< 1"), "");
    assert_eq!(err("1 = 1 + 1 ; 2 <<< 1"), "2");
    // 実行するたび倍になる (§10)
    assert_eq!(err("1 = 1 + 1 ; 1 = 1 + 1 ; 2 <<< 1"), "4");
    // un-shadow は部分的にしかできない
    assert_eq!(err("1 = 2 ; 2 <<< \"1\""), "1");
    // 数そのものが必要なら {} の中で生き返らせる
    assert_eq!(out("{1 = 2 ; ()} ; 1 <<< (1 + 1)"), "2");
}

// ---------------------------------------------------------------- §7 リテラル

#[test]
fn 偽は_unit_のみ() {
    assert_eq!(out("1 <<< (0 ? \"真\")"), "真");
    assert_eq!(out("1 <<< (\"\" ? \"真\")"), "真");
    assert_eq!(out("1 <<< (() ? \"真\")"), "()");
}

#[test]
fn 引用はオペレータ位置にも書ける() {
    // 文字列を2引数に適用しようとして失敗し、記号項になる
    assert_eq!(out("1 <<< (a \"foo\" b)"), "(a foo b)");
}

// ---------------------------------------------------------------- §8.1 `=`

#[test]
fn 分割代入() {
    assert_eq!(out("a , b = 1 , 2 ; 1 <<< a <<< b"), "12");
    assert_eq!(out("x = a , b ; 1 <<< x"), "(a , b)");
}

#[test]
fn 左辺形は静的検査点() {
    // 左結合で ((x = y) = 1) になる
    assert!(fatal("x = y = 1").contains("左辺"));
    // 引用トークンは違法
    assert!(fatal("\"hello\" = 5").contains("左辺"));
    assert!(fatal("(a + b) = 5").contains("左辺"));
    // () によるグルーピングは透過
    assert_eq!(out("(a , b) , c = 1 , 2 , 3 ; 1 <<< a <<< b <<< c"), "123");
}

#[test]
fn マッチ失敗は_unit_を返す() {
    // エラーにはしない。リストの終端は予期される失敗
    assert_eq!(out("1 <<< (a , b = 5)"), "()");
    // 原子的。失敗しても部分的な束縛は残らない
    assert_eq!(out("(xs , x = 1) ; 1 <<< xs"), "xs");
}

#[test]
fn パターンは末尾寄せの部分マッチ() {
    let src = "rest , y , z = 1 , 2 , 3 , 4 ; 1 <<< rest <<< \"|\" <<< y <<< \"|\" <<< z";
    assert_eq!(out(src), "(1 , 2)|3|4");
}

#[test]
fn 代入は右辺の値を返す() {
    assert_eq!(out("1 <<< (x = 41 + 1)"), "42");
}

// ---------------------------------------------------------------- §8.2 `,`

#[test]
fn 左ネストは見えない() {
    assert_eq!(out("1 <<< ((1 , 2) , 3)"), "(1 , 2 , 3)");
    assert_eq!(out("1 <<< (1 , (2 , 3))"), "(1 , (2 , 3))");
}

#[test]
fn マッチが判定を兼ねる() {
    // isEmpty は不要。必要なのは isPair であり、それは分割代入そのもの
    assert_eq!(out("list = 1 , 2 ; 1 <<< ((xs , x = list) ? \"pair\" : \"基底\")"), "pair");
    assert_eq!(out("list = 1 ; 1 <<< ((xs , x = list) ? \"pair\" : \"基底\")"), "基底");
}

// ---------------------------------------------------------------- §8.3 `?` `:` `!`

#[test]
fn 三項演算子は二項演算子二つ() {
    assert_eq!(out("1 <<< (1 ? \"t\" : \"f\")"), "t");
    assert_eq!(out("1 <<< (() ? \"t\" : \"f\")"), "f");
}

#[test]
fn 短絡する() {
    assert_eq!(out("() ? (1 <<< \"評価された\") ; 1 <<< \"終わり\""), "終わり");
    assert_eq!(out("1 : (1 <<< \"評価された\") ; 1 <<< \"終わり\""), "終わり");
}

#[test]
fn 三つ目の短絡演算子は_unless() {
    assert_eq!(out("1 <<< (() ! \"y\")"), "y");
    assert_eq!(out("1 <<< (1 ! \"y\")"), "()");
    // 否定はその退化した使い方にすぎない
    assert_eq!(out("1 <<< (() ! 1)"), "1");
}

#[test]
fn 論理演算子はエイリアス() {
    // || は : と、&& は ? と完全に同一
    assert_eq!(out("1 <<< (() || \"既定値\")"), "既定値");
    assert_eq!(out("1 <<< (1 && \"次\")"), "次");
}

#[test]
fn 真の枝が_unit_だと_elvis_が誤発火する() {
    // §10 の地雷そのもの
    assert_eq!(out("1 <<< (1 ? () : 1)"), "1");
    // cons で包めば逃げられる (§8.3)。
    // 箱の中身は右側なので、先頭から数える `#`（§8.11）では 1 番目
    let not = "(_ not b ((b ? (() , ())) : (() , 1)) # 1)";
    assert_eq!(out(&format!("{} ; 1 <<< (() not 1) <<< \"|\" <<< (() not ())", not)), "()|1");
}

// ---------------------------------------------------------------- §8.4 入出力

#[test]
fn 印字は連鎖できて改行は付かない() {
    assert_eq!(out("1 <<< \"Hello, \" <<< \"world\" <<< \"\\n\""), "Hello, world\n");
    // <<< は左オペランド（fd）を返す
    assert_eq!(out("1 <<< (1 <<< \"a\")"), "a1");
}

#[test]
fn 標準エラーは_fd_2() {
    let o = run_capture("2 <<< \"エラー\" ; 1 <<< \"通常\"", "");
    assert_eq!(o.stderr, "エラー");
    assert_eq!(o.stdout, "通常");
}

#[test]
fn fd_番号も単なる変数() {
    // 2 = 5 で標準エラーが消える (§10)
    let o = run_capture("2 = 5 ; 2 <<< \"消える\" ; 1 <<< \"見える\"", "");
    assert_eq!(o.stderr, "");
    assert_eq!(o.stdout, "見える");
}

#[test]
fn 標準入力を読む() {
    assert_eq!(out_in("1 <<< (0 >>> ())", "hello\nworld\n"), "hello");
    assert_eq!(out_in("1 <<< (0 >>> 3)", "hello"), "hel");
    // n に足りずに EOF なら読めた分を返す
    assert_eq!(out_in("1 <<< (0 >>> 100)", "hi"), "hi");
    // 0文字読みは空文字列。真なので EOF と区別できる
    assert_eq!(out_in("1 <<< ((0 >>> 0) ? \"真\")", ""), "真");
    // EOF は () を返す
    assert_eq!(out_in("1 <<< (0 >>> ())", ""), "()");
    assert_eq!(out_in("((0 >>> ()) : {1 <<< \"終了\"})", ""), "終了");
}

#[test]
fn 読んだものは文字列なので数にはパースが要る() {
    assert_eq!(out_in("1 <<< ((0 >>> ()) + 1)", "41\n"), "(41 + 1)");
    assert_eq!(out_in("1 <<< (((0 >>> ()) ~ 10) + 1)", "41\n"), "42");
}

// ---------------------------------------------------------------- §8.6 `!!`

#[test]
fn 中断は終了コードを設定する() {
    let o = run_capture("1 !! \"disk full\"", "");
    assert_eq!(o.code, 1);
    assert_eq!(o.stderr, "disk full\n");
    // 未束縛トークンが自己評価するので引用符なしでメッセージが書ける
    assert_eq!(run_capture("2 !! disk-full", "").stderr, "disk-full\n");
    // catch は無い。必ず最外まで飛ぶ
    assert_eq!(code("(a f b (1 !! \"死\")) ; (0 f 0) ; 1 <<< \"ここには来ない\""), 1);
    assert_eq!(out("(a f b (1 !! \"死\")) ; (0 f 0) ; 1 <<< \"ここには来ない\""), "");
}

#[test]
fn 下位八ビットにより二五六は成功終了する() {
    assert_eq!(code("256 !! err"), 0);
}

#[test]
fn 昇格は一行で書ける() {
    // unwrap-or-die (§8.7)
    let o = run_capture("(a , b = 5) : {1 !! \"not a pair\"}", "");
    assert_eq!(o.code, 1);
    assert_eq!(o.stderr, "not a pair\n");
    assert_eq!(run_capture("(a , b = 1 , 2) : {1 !! \"not a pair\"} ; 0", "").code, 0);
}

// ---------------------------------------------------------------- §8.8 比較

#[test]
fn 比較は真ならオペランドを返す() {
    assert_eq!(out("1 <<< (5 == 5)"), "5");
    assert_eq!(out("1 <<< (5 == 6)"), "()");
    // 0 が真だからこそ成立する
    assert_eq!(out("1 <<< ((0 == 0) ? \"真\")"), "真");
}

#[test]
fn 連鎖比較が無料で出てくる() {
    assert_eq!(out("1 <<< (1 < 2 < 3)"), "3");
    assert_eq!(out("1 <<< (3 < 2 < 1)"), "()");
    // () を受け取った比較は () を返す
    assert_eq!(out("1 <<< (5 < 1 < 100)"), "()");
}

#[test]
fn unit_は自分自身と等しくない() {
    assert_eq!(out("1 <<< (() == ())"), "()");
    assert_eq!(out("1 <<< (5 != ())"), "()");
}

// ---------------------------------------------------------------- §8.9 文字列演算

#[test]
fn 文字列は連結について閉じている() {
    assert_eq!(out("1 <<< (\"hello\" ++ \" \" ++ \"world\")"), "hello world");
}

#[test]
fn 文字列から数への経路は存在しない() {
    assert_eq!(out("1 <<< ((\"1\" ++ \"2\") + 1)"), "(12 + 1)");
    assert_eq!(out("1 <<< ((\"1\" ++ \"2\") ~ 10 + 1)"), "13");
}

#[test]
fn パースは右オペランドが基数() {
    assert_eq!(out("1 <<< (\"42\" ~ 10)"), "42");
    assert_eq!(out("1 <<< (\"2a\" ~ 16)"), "42");
    assert_eq!(out("1 <<< (\"xyz\" ~ 10)"), "()");
    assert_eq!(out("1 <<< (\"42\" ~ ())"), "()");
}

#[test]
fn 文字列と数は印字が同一で区別できない() {
    assert_eq!(out("1 <<< \"1\""), "1");
    assert_eq!(out("1 <<< 1"), "1");
    assert_eq!(out("1 <<< (\"1\" == 1)"), "()");
    // 空文字列は等値判定の内側に留まれる
    assert_eq!(out("1 <<< ((\"\" == \"\") ? \"真\")"), "真");
}

// ---------------------------------------------------------------- §8.10 適用

#[test]
fn 適用演算子() {
    assert_eq!(out("1 <<< (1 , 2 |> +)"), "3");
    assert_eq!(out("1 <<< (+ $ 1 , 2)"), "3");
    // 関数を計算結果として渡せる
    assert_eq!(out("op = + ; 1 <<< (1 , 2 |> op)"), "3");
    // 擬似的な3引数呼び出し
    assert_eq!(out("(p f q ((p # 0) + (p # 1) + q)) ; 1 <<< (1 , 2 , 3 |> f)"), "6");
}

#[test]
fn フラグ付き束縛は第一級ではない() {
    // = は左が値として届くので束縛できない
    assert_eq!(out("(x , 1 |> =) ; 1 <<< x"), "x");
    // ? は短絡しない。両オペランドが評価済み
    assert_eq!(out("() , (1 <<< \"副作用\") |> ? ; 1 <<< \"|終わり\""), "副作用|終わり");
}

// ---------------------------------------------------------------- §8.11 添字と連結

#[test]
fn 添字は先頭から数える() {
    assert_eq!(out("1 <<< ((10 , 20 , 30) # 0)"), "10");
    assert_eq!(out("1 <<< ((10 , 20 , 30) # 2)"), "30");
    // 範囲外は ()
    assert_eq!(out("1 <<< ((10 , 20) # 9)"), "()");
    // 1要素のリスト（＝値そのもの）に対する # 0 はその値を返す
    assert_eq!(out("1 <<< (42 # 0)"), "42");
}

#[test]
fn 文字の添字は別演算子() {
    assert_eq!(out("1 <<< (\"hello\" @ 1)"), "e");
    assert_eq!(out("1 <<< (\"hello\" @ 99)"), "()");
}

#[test]
fn 連結は半群() {
    assert_eq!(out("1 <<< ((1 , 2) <> (3 , 4))"), "(1 , 2 , 3 , 4)");
    // 結合的
    assert_eq!(
        out("1 <<< (((1 , 2) <> (3 , 4)) <> (5))"),
        out("1 <<< ((1 , 2) <> ((3 , 4) <> (5)))")
    );
}

// ---------------------------------------------------------------- §9 全ては式

#[test]
fn 評価値は捨てられ既定で成功終了する() {
    // プログラムの評価値がどれであっても終了コードには漏れない
    assert_eq!(code("処理"), 0);
    assert_eq!(code("1 <<< \"x\""), 0); // 印字で終わっても fd 1 は漏れない
    assert_eq!(code("42"), 0);
    assert_eq!(code("()"), 0);
    assert_eq!(code("0 = 1"), 0);
}

#[test]
fn 終了コードを設定する手段は中断だけ() {
    assert_eq!(code("1 <<< \"x\" ; 3 !! \"err\""), 3);
    // `!!` の左オペランドも単なるトークン。`1 = 0` で全 panic が成功終了に化ける (§10)
    let o = run_capture("1 = 0 ; 1 !! \"err\"", "");
    assert_eq!(o.code, 0);
    assert_eq!(o.stderr, "err\n");
}

// ---------------------------------------------------------------- §10 地雷

#[test]
fn セミコロンの書き忘れでブロックが静かに関数宣言に化ける() {
    // 3文以上は黙って化ける。エラーは出ない
    let o = run_capture("(1 <<< \"a\") ; (1 <<< \"b\") (1 <<< \"c\")", "");
    assert!(o.error.is_none());
    assert_eq!(o.stdout, "");
    // 2文だけのブロックは守られる（2項＝構文エラー）
    assert!(fatal("(1 <<< \"a\") (1 <<< \"b\")").contains("2項"));
}

#[test]
fn ブロック最後の文をコメントアウトするとセミコロンが余る() {
    // シグネチャ `a ; b`、本体 `;` の関数が宣言され、逐次実行が壊れる (§1.5)
    let o = run_capture("((1 <<< \"a\") ; (1 <<< \"b\") ; ) ; (1 <<< \"c\")", "");
    assert!(o.error.is_none());
    // a と b はシグネチャなので評価されない
    assert_eq!(o.stdout, "c");
}

#[test]
fn 二文字以上の演算子は空白を入れると宣言に化ける() {
    // シグネチャの2番目が関数名として潰される
    assert_eq!(out("(a = = b) ; 1 <<< \"生きてる\""), "生きてる");
    assert_eq!(out("1 <<< (a = = b)"), "<fn =>");
}

#[test]
fn 中断を印字に差し替えると全_panic_が化ける() {
    // クロージャはフレームを捕まえているので、あとから書いた1行が
    // 既存の全 panic を遡って無効化する
    let o = run_capture("(a f b (1 !! \"boom\")) ; !! = <<< ; (0 f 0) ; 1 <<< \"|生きてる\" ; 0", "");
    assert_eq!(o.code, 0);
    assert_eq!(o.stdout, "boom|生きてる");
    assert_eq!(o.stderr, "");
}

#[test]
fn 波括弧の中での差し替えは呼び出し先に届かない() {
    // {} はレキシカルなので (§10)
    let o = run_capture("(a f b (1 !! \"boom\")) ; {!! = <<< ; (0 f 0)} ; 0", "");
    assert_eq!(o.code, 1);
    assert_eq!(o.stderr, "boom\n");
}

#[test]
fn tee_ができない() {
    // (f (1 <<< x) d) は x ではなく fd 番号を渡す (§8.4)
    assert_eq!(out("(a f b a) ; 1 <<< ((1 <<< \"x\") f ())"), "x1");
    // デバッグ印字には ; と一時束縛が要る
    assert_eq!(out("(a f b a) ; 1 <<< ((t = 40 + 2 ; 2 <<< t ; t) f ())"), "42");
}

// ---------------------------------------------------------------- §8.4 バッファ

mod バッファ {
    use oddity::io::IoTable;
    use std::cell::RefCell;
    use std::io::{BufRead, Cursor, Read, Write};
    use std::rc::Rc;

    /// 読み取りが起きた瞬間の stdout の中身を覗く入力
    struct Probe {
        out: Rc<RefCell<Vec<u8>>>,
        snapshot: Rc<RefCell<Option<String>>>,
        data: Cursor<Vec<u8>>,
    }

    impl Probe {
        fn snap(&mut self) {
            let mut s = self.snapshot.borrow_mut();
            if s.is_none() {
                *s = Some(String::from_utf8_lossy(&self.out.borrow()).into_owned());
            }
        }
    }

    impl Read for Probe {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.snap();
            self.data.read(buf)
        }
    }

    impl BufRead for Probe {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            self.snap();
            self.data.fill_buf()
        }
        fn consume(&mut self, n: usize) {
            self.data.consume(n)
        }
    }

    struct Shared(Rc<RefCell<Vec<u8>>>);

    impl Write for Shared {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// 読み取りの瞬間に stdout に出ていたものを返す
    fn seen_at_read(src: &str, input: &str) -> String {
        let out = Rc::new(RefCell::new(Vec::new()));
        let err = Rc::new(RefCell::new(Vec::new()));
        let snapshot = Rc::new(RefCell::new(None));
        let io = IoTable::with_streams(
            Box::new(Shared(out.clone())),
            Box::new(Shared(err)),
            Box::new(Probe {
                out: out.clone(),
                snapshot: snapshot.clone(),
                data: Cursor::new(input.as_bytes().to_vec()),
            }),
        );
        let (_, e, _) = oddity::run(src, io);
        assert!(e.is_none(), "予期しないエラー: {:?}", e);
        let s = snapshot.borrow().clone();
        s.expect("読み取りが起きなかった")
    }

    #[test]
    fn 読み取りは書き込みを追い越さない() {
        // 改行で終わらないプロンプトでも、ブロックする前に掃き出される
        let src = "1 <<< \"name? : \" ; n = (0 >>> ()) ; 1 <<< \"Hi, \" <<< n ; 0";
        assert_eq!(seen_at_read(src, "Mitsui\n"), "name? : ");
        // n 文字読みでも同じ
        let src = "1 <<< \"? \" ; n = (0 >>> 2) ; 1 <<< n ; 0";
        assert_eq!(seen_at_read(src, "ab"), "? ");
    }

    #[test]
    fn 中断は_stdout_を掃き出さない() {
        // §10 最後の地雷は残っている。読み取りが絡まないので掃き出しは走らない
        let o = oddity::run_capture("1 <<< \"progress\" ; 1 !! \"err\"", "");
        assert_eq!(o.stdout, "");
        assert_eq!(o.stderr, "err\n");
        assert_eq!(o.code, 1);
        // 行が閉じていれば行バッファが吐くので、地雷は「行の途中」でだけ踏む
        let o = oddity::run_capture("1 <<< \"progress\\n\" ; 1 !! \"err\"", "");
        assert_eq!(o.stdout, "progress\n");
    }
}
