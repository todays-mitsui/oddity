// docs/index.html に載せた出力が、本当にその出力になるか確かめる。
//
//   node tools/check-site.mjs
//
// app.js を（DOM を最小限に偽装して）そのまま読み込むので、
// wasm の口・ハイライト・実行のすべてが本番と同じ経路を通る。

import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { createRequire } from "node:module";

const root = new URL("../docs/", import.meta.url);

// --- DOM の偽装
const noop = () => {};
globalThis.window = globalThis;
globalThis.Event = class { constructor(t) { this.type = t; } };
globalThis.document = {
  querySelectorAll: () => [],
  getElementById: () => null,
  addEventListener: noop,
  dispatchEvent: noop,
  createElement: () => ({ append: noop }),
};
globalThis.fetch = async (p) => {
  const buf = readFileSync(new URL(p.replace(/^\.\//, ""), root));
  return { ok: true, arrayBuffer: async () => buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength), text: async () => buf.toString() };
};

await import(pathToFileURL(new URL("app.js", root).pathname));
await new Promise((r) => setTimeout(r, 200)); // boot() を待つ

let fail = 0;
const { run, analyze, highlight } = globalThis.oddity;

// docs/oddity.wasm はビルド成果物なので、置いていかれていないか見ておく
{
  const { statSync, readdirSync } = await import("node:fs");
  const wasm = statSync(new URL("oddity.wasm", root)).mtimeMs;
  const srcDir = new URL("../src/", import.meta.url);
  const newest = Math.max(...readdirSync(srcDir).map((f) => statSync(new URL(f, srcDir)).mtimeMs));
  if (newest > wasm) console.log("! docs/oddity.wasm が src より古い。tools/build-wasm.sh を走らせること");
}

// --- id の重複と、app.js が触る id が本当にあるか
// （id が重複していると getElementById が別物を返して、まったく違う場所が書き換わる）
{
  const appjs = readFileSync(new URL("app.js", root), "utf8");
  const wanted = [...appjs.matchAll(/getElementById\("([^"]+)"\)/g)].map((m) => m[1]);
  for (const page of ["index.html", "spec.html"]) {
    const src = readFileSync(new URL(page, root), "utf8");
    const ids = [...src.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);
    for (const id of new Set(ids)) {
      if (ids.filter((x) => x === id).length > 1) {
        console.log(`✗ ${page}: id="${id}" が重複している`);
        fail++;
      }
    }
  }
  const index = readFileSync(new URL("index.html", root), "utf8");
  const indexIds = [...index.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]);
  for (const id of new Set(wanted)) {
    if (id === "src" || indexIds.includes(id)) continue;
    console.log(`✗ app.js が getElementById("${id}") を呼ぶが index.html に無い`);
    fail++;
  }
}

const html = readFileSync(new URL("index.html", root), "utf8");
const unesc = (s) => s.replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");

const blocks = [...html.matchAll(
  /<pre><code data-odd>([\s\S]*?)<\/code><\/pre>(?:\s*<div class="out">([\s\S]*?)<\/div>)?/g
)];

let checked = 0, painted = 0;
for (const [, rawSrc, rawOut] of blocks) {
  const src = unesc(rawSrc);

  // 1. ハイライトできること（括弧の対応が取れていること）
  const { err, spans } = analyze(src);
  if (err) { console.log("✗ 解析できない:", err, "\n  ", src.split("\n")[0]); fail++; continue; }
  if (spans.length) painted++;
  const h = highlight(src);
  const stripped = h.replace(/<[^>]*>/g, "");
  if (unesc(stripped) !== src) {
    console.log("✗ ハイライトで文字が落ちた:\n  ", JSON.stringify(src), "\n  ", JSON.stringify(unesc(stripped)));
    fail++;
  }

  // 2. 載せた出力が本当にその出力か（散文の注釈は飛ばす）
  if (rawOut === undefined) continue;
  const want = unesc(rawOut);
  if (want.includes("（")) continue;
  checked++;
  const r = run(src, "");
  const got = r.stdout + r.stderr;
  if (got !== want) {
    console.log("✗ 出力が違う:\n  コード:", JSON.stringify(src), "\n  期待:", JSON.stringify(want), "\n  実際:", JSON.stringify(got));
    fail++;
  }
}

// 3. プレイグラウンドの例題が全部走ること
let exFail = 0;
for (const [name, src] of Object.entries(globalThis.oddity.EXAMPLES)) {
  const { err } = analyze(src);
  const r = run(src, "alpha\nbravo\n");
  if (err || r.error) { console.log("✗ 例題", name, ":", err || r.error); exFail++; }
}
fail += exFail;

// 4. 仕様が marked で描画できること
const md = readFileSync(new URL("spec.md", root), "utf8");
const require = createRequire(import.meta.url);
const { marked } = require("../docs/vendor/marked.min.js");
const rendered = marked.parse(md);
if (!/<table/.test(rendered) || !/<h2/.test(rendered)) { console.log("✗ 仕様の描画が怪しい"); fail++; }

console.log(`例題 ${Object.keys(globalThis.oddity.EXAMPLES).length} 個 / 仕様 ${(rendered.match(/<h2/g)||[]).length} 節`);
console.log(`コードブロック ${blocks.length} 個 / うちハイライト ${painted} 個 / 出力照合 ${checked} 個 / 不一致 ${fail} 件`);
process.exit(fail ? 1 : 0);
