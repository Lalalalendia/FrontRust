# FrontRust Masonry RenderRoot — browser runtime experiment

This experiment builds and runs **upstream Masonry 0.4 source pinned to Xilem revision 271a27a6d4a930f7878d404f9014e3c50a3a9b88**. It uses no product code from Akula or Chaptera.

## Executable gates

1. Native Rust integration test creates genuine `SizedBox`, `Button`, nested `Portal` and transformed `DisclosureButton` widgets, initializes `RenderRoot` without a native window, enables AccessKit, obtains `VisualLayerPlan`, checks scene colors, and resizes.
2. `wasm32-unknown-unknown` release `cdylib` is actually compiled. Node instantiates its generated wasm-bindgen JavaScript glue and verifies scene colors, AccessKit nodes, pointer down/up `ButtonPress` and window resize.
3. Chromium and Firefox load **that same WASM**, replay real Masonry scene layers into a minimal Canvas2D bridge, verify pixels are painted from Rust scene colors, click a Masonry Button and check Rust callback count, and resize the scene.

Use `cargo test --features widgets --manifest-path experiments/masonry-wasm-probe/Cargo.toml` for the native test. The browser CI is at `.github/workflows/masonry-runtime.yml`. The test web app is `experiments/masonry-wasm-probe/demo/index.html`; the real `demo/gen/masonry_probe.js` and `masonry_probe_bg.wasm` generated with pinned wasm-bindgen CLI 0.2.129 must be present.

## Real but limited: transport contract

The bridge uses `VisualLayerPlan::replay_into` and Imaging's retained `Scene`. It replays **ordered semantic commands** via typed scalar WASM exports: solid-color Rect/RoundedRect/BezPath fills and strokes, undashed cap/join/miter attributes, nested nonzero `PushClip/PopClip` scopes, and full six-coefficient affine transforms. Canvas2D reproduces the tested shapes from Rust data, including real rounded corners and rotated open paths. `PathEl` segments are bounded to 512 per shape. Unsupported group compositing, image/gradient brushes, glyphs, complex stroked clips and mismatched open-path caps trigger explicit `mp_skipped_draws`, never an unmarked approximation.

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

## Render fidelity milestone: rounded rectangles and full affine matrices

- Masonry `Geometry::RoundedRect` now preserves *per-corner* radii (clockwise top-left, top-right, bottom-right, bottom-left) through the Rust numeric ABI.
- The host exports both the legacy world-space bounding box and the exact local rectangle, plus all six `kurbo::Affine` coefficients. Canvas2D applies `setTransform` followed by `roundRect`, instead of flattening rotated/skewed coordinates to a bounding box.
- Rust native test checks that real rounded widget geometry reaches the scene; Node verifies corner-radius values and affine exports; Chromium/Firefox sample a real filled pixel and an unfilled rounded-corner pixel. Missing shape data fails these gates.
- R3 supports bounded BezPath segments, solid Stroke and nested Clip. Arbitrary combinations of paths/groups, image resources and gradients still require separate conformance tests.

## R2 ordered paint-stream fidelity: border stroke and viewport clip

The independent demo now creates a real upstream `BorderColor` / `BorderWidth` stroke and a `Portal` that clips a 120px green child to an 80px viewport. The Rust bridge iterates **the original painter command order** and retains `Draw::Fill`, `Draw::Stroke`, `PushClip`, and `PopClip` operations. The browser Canvas2D adapter replays them in exact order with `save` / `clip` / `restore`, preserving affine matrices and per-corner radii. Solid undashed strokes support Miter/Round/Bevel joins, miter limit and applicable cap style; dashed and unsupported compound styles are counted unsupported.

**Failure policy:** unsupported brush/groups/stroked clips are *not drawn as un-clipped approximations*. They increment `mp_skipped_draws`; the R2 browser E2E requires zero skipped draws for its test scene. This is not yet generalized SVG path/gradient/text/glyph compositing or production clipping.

Tests: native Rust checks stroke and clip ops exist; Node confirms real `Draw::Stroke` color/width, clip push/draw/pop ordering, clipped viewport width, and zero skipped draws; Chromium and Firefox inspect red border pixels, green pixels inside a portal and absent pixels outside the clip, and previous button/resize behaviors.

## R2 fix — upstream scrollbar joins

Actual CI diagnostics showed the only unsupported operation was the real Portal
scrollbar's `Stroke::new(edge_width)` using `Join::Round` (not a missing
`PushClip` or `PopClip`). The R2 scene ABI now exports the join mode
(0=Miter,1=Round,2=Bevel) and miter limit. Canvas2D maps them to native
`lineJoin` and `miterLimit`; the demo still rejects dashed strokes
and untested groups or complex clip semantics. Rust/Node/Chromium/Firefox tests assert the
round-join scrollbar outline survives alongside the red miter border,
and that unsupported operation count remains zero.

## R3 (experimental): native BezPath geometry, nested Portals and widget transforms

- A real `DisclosureButton` paints an open `kurbo::BezPath` in magenta. Its `WidgetTag` is edited through `RenderRoot::edit_widget_with_tag` with a 0.45-radian transform before the scene is captured; JavaScript does not generate or rotate the arrow itself.
- The Rust scene bridge converts bounded `PathEl` elements (MoveTo/LineTo/QuadTo/CurveTo/ClosePath) to numeric WASM exports. The Canvas2D replay builds paths with `moveTo`, `lineTo`, `quadraticCurveTo`, `bezierCurveTo` and `closePath`, preserving the real 6-element affine transform.
- The green child is now wrapped by **two** genuine `Portal` instances, with 65px and 80px viewports. Tests require nesting depth >=2 at the green fill, balanced push/pop commands and zero skipped draws.
- Open path strokes export a round/butt/square line-cap style in addition to the existing join and miter limit. Unsupported differing start/end caps and dashed paths remain fail-closed.
- Node and both browser E2Es check the real Path payload and its nonzero rotation. Chromium/Firefox additionally inspect actual magenta pixels along a computed transformed path segment.
- No claim of SVG-level completeness: advanced composite groups, filled glyphs, image resources, gradient paints, arbitrary stroked clip regions, full DPI/IME/a11y and CSS layout remain out of scope. R3 has passed those tests on [CI #40201480153](https://github.com/Lalalalendia/FrontRust/actions/runs/40201480153), including a real rotated open-line path; QuadTo and CurveTo require dedicated regression pixels before claiming full curve fidelity.

## Current gate and next steps

- [R3: native Rust + WASM release + wasm-bindgen + Node + Chromium/Firefox SUCCESS](https://github.com/Lalalalendia/FrontRust/actions/runs/40201480153). Artifact `masonry-browser-evidence` contains both browser screenshots and `masonry-real-wasm` the compiled binary.
- [PR #1 remains Draft](https://github.com/Lalalalendia/FrontRust/pull/1); no assumption of production readiness or a completed merge.
- R3b: exercise synthetic or widget-generated QuadTo/CurveTo with precise pixel conformance and negative fail-closed cases. R4: focus ownership, browser keyboard dispatch, IME composition and AccessKit→DOM accessibility projection.

## R3b — QuadTo/CurveTo pixel conformance and fail-closed validation

Two separate actual Rust `kurbo::BezPath` strokes are stored through upstream `Canvas::update_scene` and replayed by real Masonry `RenderRoot`: cyan `quad_to((55,0),(102,60))` and amber `curve_to((30,-4),(80,-4),(104,70))`. The `Canvas` occupies the right-hand 120×80 part of a Masonry `Flex::row` beside the existing transformed `DisclosureButton`, within the 320×360 WASM framebuffer (increased to show the full scene).

Native Rust and Node independently assert the numeric `PathEl` payloads and curve colors. Chromium and Firefox compute a quadratic/cubic Bernstein midpoint from the transported control points, sample 5×5 neighborhoods at those transformed coordinates, and verify off-curve pixels do *not* match the stroke colors. Merely exporting a Path opcode does not pass.

The bounded serialization rule is 1–512 `PathEl` per shape; empty, nonfinite or 513-element paths are rejected. Rust unit tests check the boundary and an integration test injects a 514-element path through an actual Masonry `Canvas`, requires a nonzero unsupported counter, and verifies that the invalid shape was not sent to the browser. Browser canvas preflight rejects incomplete/oversized scene data **before** changing the framebuffer; negative browser tests exercise both rejection cases.

This remains a narrowly verified scene subset, not guaranteed SVG, CSS or publisher-accurate curve rasterization across all DPIs. Blend groups, filled glyphs, image/gradient brushes, stroke dashes and color management still require separate evidence.

## R4a — keyboard/focus ownership and browser composition fence (experimental)

- The actual tagged Masonry `Button` is focused via Rust `RenderRoot::focus_on`, not a synthetic canvas click. The scalar WASM exports `mp_focus_button`, `mp_focused_button`, `mp_keyboard_button`, `mp_keyboard_event_count` and demo-local semantic ID `mp_semantic_button_id=1` invoke `RenderRoot::handle_text_event(TextEvent::key_down/key_up)`. `Enter` and `Space` trigger real `ButtonPress` on release; unsupported keys/unfocused events fail closed.
- A single labeled native DOM `<button>` projects the keyboard affordance of the real Rust button independently of the graphics-only `<canvas aria-hidden>`. Its focus/blur synchronizes with Rust; pointer focus is mirrored. A separate text inspector owns browser-native editing. There is NO general AccessKit node-to-DOM projection yet, and semantic ID 1 is a fixed demo contract, not a claim about AccessKit NodeId.
- An explicit browser `compositionstart/compositionend` fence protects scene activation even if focus changes mid-composition or `KeyboardEvent.isComposing` is inconsistent. Native IME insertion stays in a real HTML text input; it is NOT an upstream Masonry text widget.
- Native Rust and Node assert focus gates and real Enter/Space `ButtonPress` through WASM. Chromium/Firefox must prove mouse, Enter, Space, Tab to inspector, local Delete/Enter/Undo, and composition fencing without incrementing Rust key/event counters.
- R4a does NOT certify OS-level Japanese/Russian IME, screen-reader/WCAG, dynamic AccessKit→DOM mapping, modal ownership, roving object tree or shortcut mapping. Those are separate R4b/R4c gates.


## R4b — AccessKit Button/Click gate and DOM activation (experimental)

- Before exposing the tagged Rust button as a DOM control, verify that its **matching WidgetId / AccessKit NodeId** has role `Button` and supports `Action::Click`. An arbitrary nonempty AccessKit update is not enough.
- Treat `TreeUpdate.nodes` as a **delta**, not the complete tree on every redraw; preserve the confirmed node state when an update omits unchanged nodes.
- Route native DOM pointer clicks and `detail=0` accessibility-style activations into actual Masonry `TextEvent`/`ButtonPress`. Suppress duplicate browser default clicks from Enter/Space without suppressing standalone accessibility actions.
- Chromium/Firefox gate: pointer, Enter/Space, Tab and composition fences still pass; a `detail=0` click increments Rust state exactly once. A programmatic `click()` is **not** a screen-reader test.
- Still out of scope: complete AccessKit tree synchronization, accessible-name derivation from Rust, disabled/expanded state, subtree removal, real OS IME and screen-reader interaction. The DOM name remains a demo-host-provided label.
