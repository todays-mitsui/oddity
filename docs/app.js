// oddity — プレイグラウンドと位置ハイライト
//
// 動かしているのはインタプリタ本体をそのまま wasm にしたもの（src/wasm.rs）。
// ハイライトも本体のレキサ・パーサが返す役割（src/roles.rs）をそのまま塗っている。
// 「役割は位置だけで決まる」（§0）ので、ハイライタもそれ以上のことは知らない。

const REPO = "https://todays-mitsui.github.io/oddity"; // ← 公開先に合わせて直す

const enc = new TextEncoder();
const dec = new TextDecoder();

let wasm = null;
let wasmBytes = null;

async function boot() {
  const res = await fetch("./oddity.wasm");
  wasmBytes = await res.arrayBuffer();
  wasm = new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), {}).exports;
}

/** トラップした wasm インスタンスは信用できないので作り直す */
function reboot() {
  wasm = new WebAssembly.Instance(new WebAssembly.Module(wasmBytes), {}).exports;
}

function put(s) {
  const b = enc.encode(s);
  const p = wasm.od_alloc(b.length || 1);
  if (b.length) new Uint8Array(wasm.memory.buffer, p, b.length).set(b);
  return [p, b.length];
}

function take(ptr) {
  const dv = new DataView(wasm.memory.buffer);
  const len = dv.getUint32(ptr, true);
  const bytes = new Uint8Array(wasm.memory.buffer, ptr + 4, len).slice();
  wasm.od_free(ptr, 4 + len);
  return new DataView(bytes.buffer);
}

function reader(dv) {
  let o = 0;
  return {
    u32: () => { const v = dv.getUint32(o, true); o += 4; return v; },
    str: () => {
      const n = dv.getUint32(o, true); o += 4;
      const s = dec.decode(new Uint8Array(dv.buffer, o, n));
      o += n;
      return s;
    },
  };
}

/** 括弧の対応が取れていれば、トークンごとの役割を返す */
function analyze(src) {
  if (!wasm) return { terms: 0, err: "", spans: [] };
  const [p, l] = put(src);
  let dv;
  try {
    dv = take(wasm.od_analyze(p, l));
  } finally {
    wasm.od_free(p, l || 1);
  }
  const r = reader(dv);
  const terms = r.u32();
  const err = r.str();
  const n = r.u32();
  const spans = [];
  for (let i = 0; i < n; i++) spans.push([r.u32(), r.u32(), r.u32()]);
  return { terms, err, spans };
}

function run(src, input) {
  if (!wasm) return { code: 1, stdout: "", stderr: "", error: "まだ読み込み中" };
  const [sp, sl] = put(src);
  const [ip, il] = put(input || "");
  let dv;
  try {
    dv = take(wasm.od_run(sp, sl, ip, il));
    wasm.od_free(sp, sl || 1);
    wasm.od_free(ip, il || 1);
  } catch (e) {
    // 深すぎる再帰などで wasm がトラップした場合
    reboot();
    return { code: 1, stdout: "", stderr: "", error: "処理系が力尽きた（" + e.message + "）" };
  }
  const r = reader(dv);
  return { code: r.u32(), stdout: r.str(), stderr: r.str(), error: r.str() };
}

// ---------------------------------------------------------------- ハイライト

const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;" };
const esc = (s) => s.replace(/[&<>]/g, (c) => ESC[c]);

/** トークンとトークンの隙間。括弧・空白・コメントしか来ない */
function gap(t) {
  let out = "", i = 0;
  while (i < t.length) {
    const c = t[i];
    if (c === "(" || c === ")" || c === "{" || c === "}") {
      out += '<span class="bracket">' + c + "</span>";
      i++;
    } else if (c === "/" && t[i + 1] === "/") {
      let j = t.indexOf("\n", i);
      if (j < 0) j = t.length;
      out += '<span class="comment">' + esc(t.slice(i, j)) + "</span>";
      i = j;
    } else {
      let j = i;
      while (j < t.length && !"(){}".includes(t[j]) && !(t[j] === "/" && t[j + 1] === "/")) j++;
      out += esc(t.slice(i, j));
      i = j;
    }
  }
  return out;
}

function highlight(src) {
  const { spans } = analyze(src);
  if (!spans.length) return gap(src);
  const b = enc.encode(src);
  let out = "", pos = 0;
  for (const [start, len, role] of spans) {
    if (start > pos) out += gap(dec.decode(b.subarray(pos, start)));
    out += '<span class="r' + role + '">' + esc(dec.decode(b.subarray(start, start + len))) + "</span>";
    pos = start + len;
  }
  if (pos < b.length) out += gap(dec.decode(b.subarray(pos)));
  return out;
}

/** ページ中の oddity コードを塗る */
function paintStatic() {
  for (const el of document.querySelectorAll("code[data-odd]")) {
    el.innerHTML = highlight(el.textContent);
  }
}

// ---------------------------------------------------------------- 例題

const EXAMPLES = {
  "Hello": '1 <<< "Hello, " <<< "world!" <<< "\\n"',

  "階乗": `// 3項のシグネチャ + 5項の本体 = 8項 → 偶数 → 関数宣言
(n fact d
    (n <= 1) ? 1 : {n * ((n - 1) fact d)}
) ;
1 <<< "10! = " <<< (10 fact ()) <<< "\\n"`,

  "FizzBuzz": `(i fizzbuzz last
    (i > last) ! {
        ( ((i % 15) == 0) ? (1 <<< "FizzBuzz")
        : ( ((i % 3) == 0) ? (1 <<< "Fizz")
          : ( ((i % 5) == 0) ? (1 <<< "Buzz")
            : (1 <<< i)
            )
          )
        ) ;
        (1 <<< "\\n") ;
        ((i + 1) fizzbuzz last)
    }
) ;
(1 fizzbuzz 20)`,

  "リストを畳む": `// isEmpty は要らない。要るのは isPair で、それは分割代入そのもの
(acc sum xs
    (rest , x = xs) ? {((acc + x) sum rest)} : (acc + xs)
) ;
list = 1 , 2 , 3 , 4 , 5 ;
1 <<< list <<< " の合計は " <<< (0 sum list) <<< "\\n"`,

  "標準入力を読む": `// EOF は () を返すので、そのまま失敗の枠に乗る
(n cat d
    (line = 0 >>> ()) ? {
        1 <<< n <<< ": " <<< line <<< "\\n" ;
        ((n + 1) cat d)
    }
) ;
(1 cat ())`,

  "数式処理系": `// 計算できないものは記号のまま残る（事故で手に入った）
1 <<< (x + y) <<< "\\n" ;
1 <<< ((1 + 2) * z) <<< "\\n" ;
1 <<< (+ * /) <<< "\\n" ;

// 表記法のユーザー定義。専用機構はゼロ
一 = 1 ; 二 = 2 ;
1 <<< (一 + 二) <<< "\\n"`,

  "地雷: ; の書き忘れ": `// 3文以上のブロックから ; を1つ落とすと、
// 偶数項になってブロックまるごとが関数宣言に化ける。エラーは出ない。
(1 <<< "a") ; (1 <<< "b") (1 <<< "c")`,

  "地雷: 文法が実行時に変わる": `// 演算子が何をするかは途中で変えられる（関数は評価時に引かれる）
(1 <<< (1 + 2 * 3)) ; * = + ; (1 <<< " → " ) ; (1 <<< (1 + 2 * 3)) ;

// が、どう結合するかは変えられない（順位はグルーピング時に引かれる）
1 <<< "\\n" <<< (1 , 2) ; , = 0 ; 1 <<< " → " <<< (1 , 2)`,

  "地雷: 数を潰す": `// 数値フレームは無限なので = が既存の束縛に当たることは原理的にない。
// 常に「新規作成」＝積み増しになる。実行するたびに倍。
1 = 1 + 1 ; 1 = 1 + 1 ;
2 <<< 1 <<< "\\n" ;

// 一度潰した数字は綴りとしては戻らないが、クォートすればシンボルは取れる
2 <<< "1" <<< "\\n"`,

  "難読化 Hello": `<<< $ (,3, , ,2, |> {<"<< <<"< <<<" <"<"< = ++ ; 1 , "Hello," <"<"< "world!" <"<"< "\\n"})`,

  "難読化 FizzBuzz": `(<< ->> >> << > >> ! (1 <<< << % 15 == 0 ? "FizzBuzz" : (<< % 3 == 0 ? "Fizz") : (<< % 5 == 0 ? "Buzz") : << <<< "\\n" ; << + 1 ->> >>)) ; 1 ->> 20`,
};

// ---------------------------------------------------------------- プレイグラウンド

function setupPlayground() {
  const src = document.getElementById("src");
  const hl = document.getElementById("hl");
  const stdin = document.getElementById("stdin");
  const result = document.getElementById("result");
  const badge = document.getElementById("badge");
  const picker = document.getElementById("picker");
  if (!src) return;

  for (const name of Object.keys(EXAMPLES)) {
    const o = document.createElement("option");
    o.textContent = name;
    picker.append(o);
  }

  function paint() {
    hl.innerHTML = highlight(src.value) + "\n";
    const { terms, err } = analyze(src.value);
    if (err) {
      badge.textContent = err;
      badge.className = "badge err";
      return;
    }
    badge.className = "badge";
    const kind =
      terms === 0 ? "ユニット値" : terms === 2 ? "構文エラー" : terms % 2 ? "式" : "関数宣言";
    badge.textContent = terms + "項 → " + kind;
  }

  function go() {
    const r = run(src.value, stdin.value);
    let html = "";
    if (r.stdout) html += esc(r.stdout);
    if (r.stderr) html += '<span class="stderr">' + esc(r.stderr) + "</span>";
    if (r.error) html += '<span class="stderr">' + esc(r.error) + "</span>\n";
    html += '<span class="meta">[終了コード ' + r.code + "]</span>";
    result.innerHTML = html;
  }

  src.addEventListener("input", paint);
  src.addEventListener("scroll", () => { hl.parentElement.scrollTop = src.scrollTop; });
  document.getElementById("run").addEventListener("click", go);
  src.addEventListener("keydown", (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); go(); }
  });
  picker.addEventListener("change", () => {
    src.value = EXAMPLES[picker.value] || "";
    paint();
    go();
  });

  src.value = EXAMPLES["Hello"];
  paint();
  return go;
}

// ---------------------------------------------------------------- 起動

boot().then(() => {
  paintStatic();
  const go = setupPlayground();
  const btn = document.getElementById("run");
  if (btn) { btn.disabled = false; if (go) go(); }
  for (const a of document.querySelectorAll("a[data-repo]")) a.href = REPO;
  for (const e of document.querySelectorAll("[data-repo-text]")) e.textContent = REPO;
  for (const a of document.querySelectorAll("a[data-repo-spec]")) a.href = REPO + "/blob/main/docs/spec.md";
  document.dispatchEvent(new Event("oddity-ready"));
}).catch((e) => {
  const b = document.getElementById("badge");
  if (b) { b.className = "badge err"; b.textContent = "wasm を読めなかった: " + e.message; }
});

window.oddity = { run, analyze, highlight, EXAMPLES, REPO };
