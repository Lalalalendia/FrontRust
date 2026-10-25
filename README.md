# Rust UI Platform — P0/P1 bounded prototype

A **product-independent**, dependency-free Rust workspace proving the architectural contract between a semantic SSR/DOM page, a Canvas renderer and one Rust/WASM state machine. This is a first vertical slice, **not Masonry in a browser**, not a production SEO stack and not a general UI framework.

## What is actually included

- `rui-core`: typed state, selection, geometry, undo, incremental revision laws (unit tests).
- `rui-wasm`: stable tiny C ABI facade exposing the **same Rust state** to both DOM and Canvas.
- `rui-server`: local Rust SSR reference server with HTML metadata and semantic content; loopback only.
- `web/app.js`: event/Canvas adapter. It owns only transient drag state, *not* document state.
- `web/mock-engine.js`: preview/test fixture only. Not loaded by real Rust server.
- `tests`: mock ABI contract + Chromium browser UI smoke test; **not** Rust/WASM behavioral parity.
- `docs`: inventoried donor sources, dependency/ownership constraints and Masonry WASM proof gates.

## Real Rust/WASM path (requires Rust toolchain)

```sh
rustup toolchain install stable
rustup target add wasm32-unknown-unknown
cargo test --workspace
cargo build --release -p rui-wasm --target wasm32-unknown-unknown
mkdir -p dist
cp target/wasm32-unknown-unknown/release/rui_wasm.wasm dist/ui_wasm.wasm
cargo run -p rui-server
```

Open `http://127.0.0.1:8787`. A `404` for `/assets/ui_wasm.wasm` is a clear failure, not an invisible JS fallback.

The one-command helper `./scripts/build.sh` performs these steps.

## Browser-only visual preview (does not run Rust)

```sh
python3 tests/preview_server.py
```

Open `http://127.0.0.1:8788`. The header explicitly reports `JS mock (tests only)`.

## Tests

```sh
node --check web/app.js
node --check web/mock-engine.js
node tests/mock-abi.test.mjs
cargo test --workspace
cargo build --release -p rui-wasm --target wasm32-unknown-unknown
node tests/wasm-abi.test.mjs # real WASM, not a JS mock
```

GitHub Actions run in `Lalalalendia/FrontRust` on the feature branch and on pull requests. Check the actual CI status; JavaScript mock tests do not prove Rust/WASM behavior.

## Architecture and honest scope

`Rust core -> wasm32 -> browser adapter -> DOM inspector + Canvas.` State, undo and revisions live in `rui-core`; an actual `Masonry` web backend would be a separate **experimental adapter** after compile/input/focus/IME gates.

The SSR HTML contains real semantic text, canonical/OG metadata, JSON-LD and robots behavior. Local HTTP is intentionally `noindex`; a real deployment needs correct site authority, output escaping/schema validation, CSRF/session/auth, caching, framework-level controls and security review. Do not expose this demo server publicly.

This prototype has no distribution license selected (`publish = false` in member crates); confirm licensing before public redistribution. It contains **no copied Akula AGPL implementation**. Rust UI ideas and contracts are implemented independently; licensing and dependency extraction are still a separate gate. 