# FrontRust — bounded Masonry WASM compatibility probe

Pinned upstream Xilem revision: `271a27a6d4a930f7878d404f9014e3c50a3a9b88`, matching Akula. This is a separate Cargo workspace and copies no Akula/Chaptera code.

## Independent compile tests

- Core: `cargo check --target wasm32-unknown-unknown --manifest-path experiments/masonry-wasm-probe/Cargo.toml`
- Widgets: `cargo check --target wasm32-unknown-unknown --features widgets --manifest-path experiments/masonry-wasm-probe/Cargo.toml`
- Native host: `cargo check --target wasm32-unknown-unknown --features native-host --manifest-path experiments/masonry-wasm-probe/Cargo.toml`

The CI matrix captures actual output for all three. A native-host failure does not invalidate a successful Core/Widgets gate. See [linebender/xilem#1848](https://github.com/linebender/xilem/issues/1848) for a reported `masonry_winit` / `masonry_imaging::texture_render` gating error in another revision.

## Not a browser renderer yet

These checks prove only dependency/type availability on wasm32. They do NOT prove a live widget tree, `RenderRoot` event dispatch, browser host, scene rendering, IME, clipboard, or accessibility. Those are subsequent gates; the current Canvas2D + Rust/WASM product adapter remains separate.

[Notion architecture](https://app.notion.com/p/40332a84beec81a6a577e2741c64b24a)

## Native RenderRoot headless experiment

The `headless-native` CI matrix member runs a real two-widget `Flex` tree of colored `SizedBox` nodes through `RenderRoot::new`, `redraw()`, `WindowEvent::Resize`, and a second scene redraw. This tests the real engine without a native window or system fonts. The same path also type-checks in the `widgets` wasm32 job, but it is **not yet executed inside a browser**.
