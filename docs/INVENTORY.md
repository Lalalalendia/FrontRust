# P0 inventory: candidate donors and evidence (2026-10-25)

Status meanings:
- **CODE VERIFIED:** source read from live GitHub connector in this session.
- **NOTION TRACE:** detailed research/CI receipts reported in Notion; current implementation not re-run here.
- **DESIGN:** architecture only; not production code.
- **PROTOTYPE:** original new code in this package, not a migrated dependency.

| Topic | Source and evidence | Actual status | Reuse decision / boundary |
|---|---|---|---|
| Masonry host, native widgets | [Akula Cargo.toml](https://github.com/Lalalalendia/Akula/blob/main/aquilum-app/native/Cargo.toml) pins `masonry` to linebender/xilem rev `271a27...` and `winit=0.31.0-beta.3` | CODE VERIFIED | Reuse architectural lessons. Not a browser build; DO NOT copy AGPL implementation into new independent platform. |
| Masonry layers → CPU renderer | [Akula renderer](https://github.com/Lalalalendia/Akula/blob/main/aquilum-app/native/src/render/renderer/mod.rs) uses `masonry::app::VisualLayerPlan`, `Scene`, replay and Damage Tracker | CODE VERIFIED | Canvas raster backend candidate; direct import requires dependency/API/license inspection. |
| Damage tracking / region shift | [Akula damage](https://github.com/Lalalalendia/Akula/blob/main/aquilum-app/native/src/render/damage/mod.rs), [scroll](https://github.com/Lalalalendia/Akula/blob/main/aquilum-app/native/src/render/scroll.rs) | CODE VERIFIED | Native optimization concepts only; browser may require different present strategy. |
| Output adapter boundary | [Akula output](https://github.com/Lalalalendia/Akula/blob/main/aquilum-app/native/src/output/mod.rs) has `Canvas::with_target` over a pixel buffer; platform-specific DXGI / soft adapters | CODE VERIFIED | Candidate to test Canvas2D/ImageData. It is below widget event/focus/IME layer. |
| Typed incremental artifacts | [rar layout invalidation](https://github.com/HeisLuka/rar/blob/main/crates/chaptera-layout-invalidation/src/lib.rs) `ArtifactReceiptV1`, dependency/output fingerprints, change classification; [Cargo.toml](https://github.com/HeisLuka/rar/blob/main/crates/chaptera-layout-invalidation/Cargo.toml) depends on chaptera-layout-projection | CODE VERIFIED | Good domain-independent *principle*; crate cannot be lifted untouched. Need separate proof of generality. |
| ReScript/Rewatch lessons | [Notion synthesis](https://app.notion.com/p/3e532a84beec812ca447ecf7159b2900) | NOTION TRACE | Move contract-specific invalidation rules, NOT ReScript compiler/toolchain. |
| Prepared text / Pretext | [Notion reference](https://app.notion.com/p/3e532a84beec81d0b983f95b16a484b8), [design](https://app.notion.com/p/3e532a84beec81a688aaff140003149b) | DESIGN + historical product trace | `prepare once / layout many` useful; browser measureText is not exact native typography authority. |
| SEO metadata and invalidation | [NEwBOO SEO audit](https://app.notion.com/p/3de32a84beec8125babafcf8a9456c3d) | NOTION TRACE | Reuse canonical/OG/robots/cache invalidation principles, not Python templates as Rust code. |
| SSR Rust app donor | [bokarust Cargo workspace](https://github.com/calex794/bokarust/blob/main/Cargo.toml) has axum 0.8 | CODE VERIFIED | Existing app is product-specific; new platform must remain independent. Leptos work documented in [editor study](https://app.notion.com/p/3f632a84beec81db9bdec4ef9718ae96), not established by this manifest. |
| Rendering & semantic accessibility | [Web focus architecture](https://app.notion.com/p/3e532a84beec81be9f07e9ff828508b8) cites merged rar#70 and browser receipts | NOTION TRACE | Reuse focus-owner and accessibility-projection *contract*. Real screen reader/IME remain unverified. |
| Browser backend fallback | [Notion WEB-RENDER-FALLBACK-01](https://app.notion.com/p/3e532a84beec8158933ac58b9f078b9d) | DESIGN | Plan only; deterministic `WebGPU > Canvas` is NOT an a priori rule. |
| Browser SEO/renderer prototypes | [Notion WEB-RENDER-01](https://app.notion.com/p/3e432a84beec81f881bae3ad0f3d59db) | NOTION TRACE | Synthetic Chromium/Firefox preflight, not Masonry Web acceptance. |
| New platform PoC | `rui-core`, `rui-wasm`, `rui-server`, `web` within this package | PROTOTYPE | No source copied from existing product repositories. Supports one scene object + shared DOM/Canvas intent. |

## P0 findings

1. **Product independence is not the same as copying source.** Akula's native crate declares `AGPL-3.0-only`; provenance and derivative-work licensing must be reviewed before any code extraction. No extracted Akula source is included here.
2. **P0 proof exists for the *architecture concept*, not the real Masonry browser target.** Rendering a ready pixel buffer into Canvas says nothing about complete Masonry widget execution.
3. **The Rust web stack choice remains open.** Existing Axum is a useful reference, but Leptos versus Xilem Web for the public DOM must be decided against SSR, hydration, footprint and developer experience.
4. **Do not present historical `miy` source as the current core.** Its Notion page calls it a historical boundary and points to current `HeisLuka/rar`.
5. **Do not promote unknown as success.** Cross-browser IME, mobile input, screen readers, native/WASM parity and full app performance are unproven for this new platform.