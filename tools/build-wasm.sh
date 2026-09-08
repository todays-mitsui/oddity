#!/bin/sh
# docs/oddity.wasm を作り直す。
#
# wasm-bindgen は使わない。素の C ABI（src/wasm.rs）なので、
# 出てくるのは import ゼロの単一ファイルになる。
set -eu
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown 2>/dev/null || true
RUSTFLAGS="-C link-arg=-zstack-size=8388608" \
    cargo build --release --target wasm32-unknown-unknown --lib
cp target/wasm32-unknown-unknown/release/oddity.wasm docs/oddity.wasm
ls -l docs/oddity.wasm
