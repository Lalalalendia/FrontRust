# FrontRust Masonry RenderRoot — browser runtime experiment

This experiment builds and runs **upstream Masonry 0.4 source pinned to Xilem revision 271a27a6d4a930f7878d404f9014e3c50a3a9b88**. It uses no product code from Akula or Chaptera.

## Executable gates

1. Native Rust integration test creates two widgets (a plain colored `SizedBox` and a second colored `SizedBox` inside a real Masonry `Button`), initializes `RenderRoot` without a native window, enables AccessKit, obtains `VisualLayerPlan`, checks scene colors, and resizes.
2. `wasm32-unknown-unknown` release `cdylib` is actually compiled. Node instantiates its generated wasm-bindgen JavaScript glue and verifies scene colors, AccessKit nodes, pointer down/up `ButtonPress` and window resize.
3. Chromium and Firefox load **that same WASM**, replay real Masonry scene layers into a minimal Canvas2D bridge, verify pixels are painted from Rust scene colors, click a Masonry Button and check Rust callback count, and resize the scene.

Use `cargo test --features widgets --manifest-path experiments/masonry-wasm-probe/Cargo.toml` for the native test. The browser CI is at `.github/workflows/masonry-runtime.yml`. The test web app is `experiments/masonry-wasm-probe/demo/index.html`; the real `demo/gen/masonry_probe.js` and `masonry_probe_bg.wasm` generated with pinned wasm-bindgen CLI 0.2.129 must be present.

## Real but limited: transport contract

The bridge uses `VisualLayerPlan::replay_into` and Imaging's retained `Scene`. It transports **only solid Rect/RoundedRect fills** via typed numeric WASM exports. It deliberately does not support every Drawing primitive, transform, clip or layer effect, text, gradient, stroke, image or OffscreenCanvas. Rounded corners are approximated with rectangles; those omissions are counted where applicable. There is no silent replacement of the Rust widget tree with JavaScript widgets.

It is **not production-ready Masonry Web**. Real clipboard/IME, keyboard, screen-reader DOM/AccessKit projection, high-DPI, generalized graphics, resource scheduling and error/capability fallback are separate acceptance gates. The browser's semantic HTML remains authoritative for SEO.

## Compatibility

The standard `masonry_core` and `masonry` crates compile under wasm32. The native `masonry_winit` driver does not compile on that target in the pinned revision (upstream [#1848](https://github.com/linebender/xilem/issues/1848) documents the `texture_render` issue). This experiment avoids `masonry_winit` entirely.

## wasm-bindgen runtime setup

The upstream UI dependencies import wasm-bindgen symbols (`__wbindgen_placeholder__`). Compiled `cdylib` WASM **cannot** be instantiated with an empty imports object. After building the Rust target, generate the matching version of glue:

```sh
cargo install wasm-bindgen-cli --version 0.2.129 --locked
wasm-bindgen --target web --out-name masonry_probe \\
  --out-dir experiments/masonry-wasm-probe/demo/gen \\
  experiments/masonry-wasm-probe/target/wasm32-unknown-unknown/release/frontrust_masonry_wasm_probe.wasm
node tests/masonry-runtime-abi.mjs
```

The browser uses the generated ES module; the Node test supplies the generated WASM bytes explicitly. The `gen` directory is generated only in CI/local builds, not committed. Missing or mismatched glue must fail the test, not silently fall back to mock behavior.
