# FrontRust — universal Rust UI platform (experimental)

A **product-independent** Rust UI architecture with two independently tested browser paths: (1) semantic SSR/DOM and Canvas2D sharing the same Rust/WASM state, and (2) an **experimental real Masonry RenderRoot** running in WebAssembly with a deliberately limited Canvas2D scene bridge. Neither path is a production UI framework or production SEO stack.

## What is actually included

- `rui-core`: typed state, selection, geometry, undo, incremental revision laws (unit tests).
- `rui-wasm`: stable tiny C ABI facade exposing the **same Rust state** to both DOM and Canvas.
- `rui-server`: local Rust SSR reference server with HTML metadata and semantic content; loopback only.
- `web/app.js`: event/Canvas adapter. It owns only transient drag state, *not* document state.
- `web/mock-engine.js`: preview/test fixture only. Not loaded by real Rust server.
- `tests`: real WASM ABI, Chromium/Firefox E2E for DOM + Canvas, plus separate mock-fixture tests. E2E without the Rust binary is **not** treated as a pass.
- `experiments/masonry-wasm-probe`: standalone pinned-upstream Xilem/Masonry Cargo workspace, real `RenderRoot`, nested Portals, transformed BezPath, ordered scene export, BrowserHost pointer/resize, generated `wasm-bindgen` glue, Canvas2D demo and real Chromium/Firefox E2E.
- `docs`: inventoried donor sources, dependency/ownership constraints and remaining Masonry Web acceptance gates.

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

GitHub Actions run in `Lalalalendia/FrontRust` on the feature branch and on pull requests. Refer to these concrete proven gates:

- [SSR + DOM/Canvas + Rust/WASM Chrome/Firefox — PASS](https://github.com/Lalalalendia/FrontRust/actions/runs/40190033411)
- [Real Masonry R3: nested clips, rotated BezPath, Rust/WASM/Node/Chromium/Firefox — PASS](https://github.com/Lalalalendia/FrontRust/actions/runs/40201480153)
- [SSR/DOM + Rust/WASM browser checks — PASS on prior code commit](https://github.com/Lalalalendia/FrontRust/actions/runs/40199274590)
- [Masonry wasm32 compatibility matrix](https://github.com/Lalalalendia/FrontRust/actions/runs/40190033421) — Core/widgets PASS, native-host known compile error recorded as non-blocking

## Real Masonry Web proof of concept

The separate experiment has its own pinned Cargo workspace. Build and run the browser demonstrator with a supported Rust toolchain, Node 22, and matching `wasm-bindgen-cli`:

```sh
cargo test --features widgets --manifest-path experiments/masonry-wasm-probe/Cargo.toml
cargo build --release --target wasm32-unknown-unknown --features widgets --manifest-path experiments/masonry-wasm-probe/Cargo.toml
cargo install wasm-bindgen-cli --version 0.2.129 --locked
wasm-bindgen --target web --out-name masonry_probe \\
  --out-dir experiments/masonry-wasm-probe/demo/gen \\
  experiments/masonry-wasm-probe/target/wasm32-unknown-unknown/release/frontrust_masonry_wasm_probe.wasm
node tests/masonry-runtime-abi.mjs
python3 -m http.server 8792 --bind 127.0.0.1 --directory experiments/masonry-wasm-probe/demo
```

Open `http://127.0.0.1:8792/`. The JavaScript/wasm files in `demo/gen` are generated, not committed. The `masonry_winit` native host cannot compile for WASM at the upstream pinned revision; this experimental BrowserHost does **not** use it. Current browser screenshots and compiled WASM are attached to [R3 CI #40201480153](https://github.com/Lalalalendia/FrontRust/actions/runs/40201480153).

## Architecture and honest scope

`Rust core -> wasm32 -> browser adapter -> DOM inspector + Canvas.` State, undo and revisions live in `rui-core`.

The independent Masonry experiment adds `RenderRoot -> VisualLayerPlan -> retained Scene -> ordered semantic paint operations -> Canvas2D`, plus BrowserHost pointer handling and window resize. Actual Rust/WASM exports carry both layout rectangles and bounded `kurbo::BezPath` (MoveTo/LineTo/QuadTo/CurveTo/ClosePath), full affine transforms, solid fill/stroke brush colors, join/cap styles and nested `PushClip/PopClip` scopes. Chromium/Firefox have verified real nested Portals, a rotated open path from `DisclosureButton`, and pixel colors; unlike the main DOM/Canvas prototype, Masonry does **not** yet share its document model.

**Not a complete renderer**: advanced blend/mask groups, arbitrary stroked clips, gradients, images, text glyphs, unbounded curves, and input/accessibility/IME integration are not production-ready. Path Quad/Cubic support is implemented but has not yet had dedicated reference pixel tests. Explicit unsupported-operation counts avoid silently approximating unknown semantics.

The SSR HTML contains real semantic text, canonical/OG metadata, JSON-LD and robots behavior. Local HTTP is intentionally `noindex`; a real deployment needs correct site authority, output escaping/schema validation, CSRF/session/auth, caching, framework-level controls and security review. Do not expose this demo server publicly.

This prototype has no distribution license selected (`publish = false` in member crates); confirm licensing before public redistribution. It contains **no copied Akula AGPL implementation**. Rust UI ideas and contracts are implemented independently; licensing and dependency extraction are still a separate gate. 