# Real browser gate

CI launches a local `rui-server` with the **compiled Rust/WASM module** and runs `tests/browser-real-wasm.mjs` in **Chromium and Firefox**.

The test checks server-rendered canonical/robots/JSON-LD, `application/wasm` delivery, absence of the mock-engine, live Canvas selection, no-op redraw invariants, HTML inspector resize and Undo, pointer drag, semantic tree synchronization, keyboard movement, Reset, and responsive/mobile-width pointer mapping. Successful Node Wasm ABI alone is not enough.

Run in an environment with Rust stable, Node 22 and Playwright:

```sh
rustup target add wasm32-unknown-unknown
cargo test --workspace
cargo build --release -p rui-wasm --target wasm32-unknown-unknown
mkdir -p dist
cp target/wasm32-unknown-unknown/release/rui_wasm.wasm dist/ui_wasm.wasm
cargo build -p rui-server
npm install --no-save --no-audit --no-fund playwright@1.56.1
npx playwright install --with-deps chromium firefox
./target/debug/rui-server &
node tests/browser-real-wasm.mjs
```

CI screenshots and failing snapshots are uploaded under `browser-real-wasm-evidence`.

Passing this gate is evidence for the **Canvas2D browser adapter and one Rust/WASM state machine**, not for Masonry widget-tree execution, IME, production SSR, WCAG, or Safari.
