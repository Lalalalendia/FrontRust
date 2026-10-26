#!/usr/bin/env sh
set -eu
cargo test --workspace
rustup target add wasm32-unknown-unknown
cargo build --release -p rui-wasm --target wasm32-unknown-unknown
mkdir -p dist
cp target/wasm32-unknown-unknown/release/rui_wasm.wasm dist/ui_wasm.wasm
node tests/wasm-abi.test.mjs
cargo run -p rui-server
