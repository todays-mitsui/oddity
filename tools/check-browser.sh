#!/bin/sh
# 本物のブラウザで docs/ を開き、JS が走ったあとの DOM を検査する。
#
#   ./tools/check-browser.sh
#
# tools/check-site.mjs は DOM を偽装しているので、
# 「id が重複していて getElementById が別の要素を返す」類のバグは捕まえられない。
# こちらは実際に描画させて確かめる。Chrome が無ければ黙って飛ばす。
set -eu
cd "$(dirname "$0")/.."

CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
[ -x "$CHROME" ] || CHROME="$(command -v google-chrome || command -v chromium || true)"
if [ -z "${CHROME:-}" ] || [ ! -x "$CHROME" ]; then
    echo "Chrome が見つからないので飛ばす"
    exit 0
fi

PORT=8791
WORK="$(mktemp -d)"
cleanup() {
    [ -n "${SRV:-}" ] && kill "$SRV" 2>/dev/null || true
    pkill -f "user-data-dir=$WORK" 2>/dev/null || true
    rm -rf "$WORK"
}
trap cleanup EXIT INT TERM

(cd docs && python3 -m http.server "$PORT" >/dev/null 2>&1) &
SRV=$!
sleep 1

# Chrome は dump-dom を吐いたあともしばらく居座るので、
# </html> が出たら打ち切る
dump() {
    out="$2"
    "$CHROME" --headless=new --disable-gpu --no-first-run --no-default-browser-check \
        --disable-background-networking --disable-component-update --disable-sync \
        --disable-extensions --user-data-dir="$WORK/profile" \
        --virtual-time-budget=5000 --dump-dom "http://127.0.0.1:$PORT/$1" \
        > "$out" 2>>"$WORK/chrome.log" &
    ch=$!
    i=0
    while [ $i -lt 200 ]; do
        if grep -q '</html>' "$out" 2>/dev/null; then break; fi
        sleep 0.1
        i=$((i + 1))
    done
    kill $ch 2>/dev/null || true
    wait $ch 2>/dev/null || true
}

dump "" "$WORK/index.html"
dump "spec.html" "$WORK/spec.html"

python3 - "$WORK/index.html" "$WORK/spec.html" <<'PY'
import re, sys, pathlib
fail = 0
def check(label, ok, extra=""):
    global fail
    print(("  ✓ " if ok else "  ✗ ") + label + (("  " + extra) if extra and not ok else ""))
    if not ok: fail += 1

d = pathlib.Path(sys.argv[1]).read_text()
def section(i):
    m = re.search(r'<section id="%s">(.*?)</section>' % i, d, re.S)
    return m.group(1) if m else ""

print("index.html")
check("ページが描画された", len(d) > 5000, f"{len(d)}バイトしかない")
hi = section("highlighting")
check("ハイライトの節が上書きされていない", 'class="legend"' in hi and "fn =" in hi)
play = section("play")
m = re.search(r'<code id="hl">(.*?)</code>', play, re.S)
check("エディタの塗り分けが入っている", bool(m) and '<span class="r' in m.group(1))
r = re.search(r'<div class="result" id="result">(.*?)</div>', d, re.S)
out = re.sub(r'<[^>]+>', '', r.group(1)) if r else ""
check("初期状態で実行されて出力が出ている", out.startswith("Hello, world!"), repr(out))
b = re.search(r'id="badge"[^>]*>(.*?)</span>', d, re.S)
check("項数バッジが出ている", bool(b) and "項 →" in b.group(1), b.group(1) if b else "無い")
blocks = re.findall(r'<code data-odd="?"?>(.*?)</code>', d, re.S)
painted = [x for x in blocks if '<span class="r' in x]
check(f"静的コードブロック {len(blocks)} 個が全部塗られた",
      len(blocks) > 0 and len(blocks) == len(painted), f"{len(painted)}/{len(blocks)}")
check("選択肢が並んでいる", len(re.findall(r'<option>', d)) >= 10)

s = pathlib.Path(sys.argv[2]).read_text()
print("spec.html")
check("仕様が描画された", "<h2" in s and "読み込み中" not in s)
check("表が出ている", s.count("<table") >= 5, f"{s.count('<table')}個")
check("目次ができた", 'id="toc"' in s and 'hidden' not in re.search(r'<div id="toc"[^>]*>', s).group(0))
n = len(re.findall(r'<li><a href="#s\d+"', s))
check(f"目次の項目 {n} 件", n >= 10)
specblocks = re.findall(r'<pre><code[^>]*>(.*?)</code></pre>', s, re.S)
sp = [x for x in specblocks if '<span class="r' in x]
check(f"仕様中のコード {len(sp)}/{len(specblocks)} 個が塗られた", len(sp) > 5, f"{len(sp)}/{len(specblocks)}")
sys.exit(1 if fail else 0)
PY
