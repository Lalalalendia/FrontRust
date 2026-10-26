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


## R4c — live AccessKit state/lifecycle projection (experimental)

- Export real `WidgetId → AccessKit NodeId` as scalar `u64`, preserved as JS `bigint` / decimal DOM `data-accesskit-id`, never as an imprecise JS number.
- A tagged Button's real `TreeUpdate` drives its `disabled` state; updates are **deltas**, so unchanged state is retained. `TreeUpdate.focus` is exported for a focus-identity gate.
- An actual `Flex` child can be disabled, removed (replaced with an inert spacer) and recreated via `RenderRoot::edit_widget_with_tag`. A removed node loses its DOM control and a recreated Rust widget gets a new ID. The DOM focus is moved to the inspector when a focused Rust control is disabled/removed.
- Native Rust, real Node/WASM ABI, Chromium and Firefox must test the full lifecycle plus preserved pointer/key/IME-fence and R3b paint behavior; no mock establishes acceptance.
- **Boundary:** this is a dynamically synchronized **single tagged Button** projection, not a general AccessKit-to-DOM tree. Labels still belong to the demo HTML host; arbitrary roles, parent/child order, screen-reader AT, OS IME and WCAG remain unverified.


## R4d — real Masonry TextInput + bounded browser Unicode/IME bridge

- A second genuine `RenderRoot` owns upstream `TextInput` wrapping a tagged editable `TextArea<true>`. This isolates unsupported glyph drawing from the bounded R3b Canvas2D serializer: text is displayed by a native accessible HTML input, but the accepted text model is Rust.
- JS→WASM strings cross through **pinned wasm-bindgen 0.2.129**, never raw pointers or lossy numeric character codes. Rust rejects operations without text focus, over 4096 UTF-8 bytes, or full-value replacement during active composition.
- DOM `input` events reconcile a bounded full-value text snapshot with the real `TextArea::reset_text`. Browser composition start/update/end is delivered to `RenderRoot::handle_text_event(TextEvent::Ime(Preedit/Commit/Disabled))`. **Upstream Parley `PlainEditor::text()` intentionally omits the preedit span** even while the real TextArea handles composition, so the demo's append-only preview is displayed from separately tracked transient preedit input. `TextAction::Changed` is NOT automatically durable: preedit can emit it. Only explicit accepted commit/replacement updates the committed text read from `TextArea`.
- Native Rust, real Node/WASM, Chromium and Firefox acceptance gates verify UTF-8 Cyrillic, Japanese preedit/commit/cancel, emoji, focus isolation, commit-once counter, and the preserved R3b/R4c scene checks. Synthetic composition events prove only the event-routing contract; native browser text input proves normal Unicode typing, NOT operating-system Japanese IME.
- **Explicitly unsupported:** cursor/selection parity for arbitrary mid-string edits (Masonry `reset_text` resets the caret), generic AccessKit text subtree, rich text, glyph renderer parity, actual operating-system IME/screen-reader certification. Treat this as an experimental text ingress gate, not production text-editing.


### R4d JS glue correction (String ABI)

The old `#[unsafe(no_mangle)]` scalar exports are callable directly on `WebAssembly.Exports`, but `#[wasm_bindgen]` functions accepting or returning `String` **must** be called through the generated `masonry_probe.js` named exports. `initWasm()` alone returns raw Wasm exports; a raw String result is a `[pointer, length]` pair, *not* a JavaScript string. The browser and Node adapters merge the generated wrappers for String signatures over the legacy scalar exports. Both adapters must enforce that the generated wrapper exists. The first R4d CI exposed this at the Node UTF-8 gate, so no R4d browser success may be claimed before a rerun.


## R4e — Unicode-safe selection/splice (bounded, pending CI)

- **New real TextArea path:** `TextArea::select_byte_range` sets the native Rust editor's UTF-8 byte-range selection. `TextEvent::ClipboardPaste` then replaces that selection, emitting a real `TextAction`. No full-value `reset_text` is used for normal browser `input` changes.
- **Browser mapping:** native `HTMLInputElement.selectionStart/End` are UTF-16 code units. They are validated against surrogate-pair splits and encoded as UTF-8 byte offsets. `beforeinput` captures selected ranges when available; a Unicode-scalar prefix/suffix diff handles `fill`, Backspace/Delete and other unmatched input events. After mutation, the browser caret/selection is copied to the Rust TextArea.
- **Fail-closed gates:** reject out-of-range, reversed or mid-UTF-8-scalar indices, edits while composing, unfocused edits, final text above 4096 bytes, missing Masonry `TextAction`, or mismatches between expected and actual Rust text. The real scene `ButtonPress` remains isolated.
- **IME race fence:** `compositionstart` syncs the current Rust selection; preedit remains transient, and an `insertFromComposition` `input` event after explicit `compositionend` must not write the same confirmed text twice. This is NOT yet evidence of full system-IME event ordering or selection replacement across all platforms.
- Native Rust, Node WASM and Playwright Chromium/Firefox test mid-string insertions, selected emoji deletion, selected Japanese/Cyrillic replacement, Unicode boundary refusal and unchanged graphical gates. Arbitrary undo, multi-range selection, keyboard dead keys, OS-native Japanese IME, accessibility text offsets and Canvas glyph transport are out of scope.


### R4e prerequisite: deterministic font-backed selection (layout only)

The first R4e native/compatibility run [#40221693042](https://github.com/Lalalalendia/FrontRust/actions/runs/40221693042) failed: replacing UTF-8 `1..5` in `A🌍B日本C` produced `A🌍B日本C🐱` instead of `A🐱B日本C`. The text `RenderRoot` had `use_system_fonts: false`, `test_font: None`, so Parley had no glyph clusters to map byte ranges to actual cursors and snapped selection to the end. This is a real precondition of the upstream editor, not an invalid UTF-8 range.

The bounded test/runtime now supplies an **in-house, minimal, placeholder-only layout font**, stored as hex text in `fixtures/r4e-layout-font.hex`, via `RenderRootOptions.test_font`. It exists only to make cursor shaping deterministic; all glyph outlines are identical boxes and must never be used for actual rendering or claimed as glyph fidelity. Production needs real font loading, fallback/shaping coverage for Cyrillic/CJK/emoji, and a separate browser-glyph parity gate. The R3b Canvas serializer remains unchanged.


### R4e browser Unicode insertion portability (test boundary)

On `4f7da7d0`, native Rust, Node/WASM and Chromium passed, but Firefox E2E timed out when `Playwright.keyboard.type('猫')` was used to replace an existing selected `X` in the middle of the string. The failure screenshot showed the character appended instead of replacing the selection. `keyboard.type` is a keyboard-event simulator and cannot reliably emulate a physical CJK key across Firefox and Chromium. The E2E now asserts the native DOM selection and uses `Playwright.keyboard.insertText('猫')`, which invokes the browser protocol text-insertion path and produces a real DOM input event. It is NOT a real OS Japanese/Chinese IME test and does not settle production IME, undo or cursor fidelity. The byte-range splice and test expectations remain unchanged.


## R4e2 — selected IME transactions and cancellation (experimental)

- Preserve the last validated DOM UTF-16 selection before `compositionstart`; some browsers collapse the DOM caret as IME begins. Convert that **original** range into UTF-8 byte offsets and pass it to `mp_editor_begin_composition` in Rust. Explicit caret movement, ordinary input and blur invalidate stale selections. The browser never assumes `compositionend` is a simple append.
- The actual Masonry `TextArea` processes `TextEvent::Ime(Preedit)` and `TextEvent::Ime(Commit)`. A separate Rust snapshot (`editor_committed`) is the only durable accepted text while composing. Preview substitutes preedit into the selected source range; `TextAction::Changed` during preedit is NOT a durable commit.
- The bounded `editor_commit_selection` path restores the original text after `Ime::Disabled` if Parley replaced selected text with transient preedit, reapplies the original UTF-8 selection through `TextArea::select_byte_range`, then issues a genuine `Ime::Commit` and verifies the **exact** resulting TextArea text and an upstream `TextAction`. It rejects stale/duplicate commit, malformed ranges, oversized content and no focus. This recovery can reset the editor's native undo history; it is NOT production-grade IME/undo handling.
- `editor_cancel` and focus loss restore the original committed document, rather than silently deleting a replaced selection. Native Rust, actual Node/WASM and Chromium/Firefox tests now require selected X → 猫, retained selected X after cancel/blur, and a late duplicate `compositionend` plus `insertFromComposition` input to leave both content and commit counter unchanged.
- A browser-protocol `keyboard.insertText` test, plus deliberately dispatched composition lifecycle tests, cannot prove real OS Japanese/Chinese IME behavior, grapheme-aware caret fidelity, text glyph rendering, multi-range selections or screen-reader operation. R4e2 requires independent device/OS tests before any production claim.


### R4e2 DOM-selection snapshot correction

[Runtime CI #40330514109](https://github.com/Lalalalendia/FrontRust/actions/runs/40330514109) on `17717dab` had 9/9 native tests PASS and Node real WASM PASS, but Chromium timed out at line 468 when *waiting for a cached `select` event* after canceling composition and setting an unchanged DOM selection. No browser `select` event is guaranteed for a no-op range assignment; this was an invalid timing assumption in the test and diagnostics, **not evidence of an invalid Rust selection**. The non-mutating snapshot now reads current `HTMLInputElement.selectionStart/End` when the editor is focused and its value matches the committed Rust text; it falls back to the recorded range only while browser composition is in flight. The test asserts the actual DOM range immediately, and `compositionstart` independently captures and submits the current range to the genuine Rust TextArea. Future acceptance still requires full Chromium AND Firefox PASS.


### R4e2 Firefox independent-fill accounting

On [Runtime #40335492754](https://github.com/Lalalalendia/FrontRust/actions/runs/40335492754), native Rust 9/9, real Node/WASM and Chromium R4e2 all passed. Firefox reached the selected-cancel test but failed on commit count `8 !== 7` at line 464. That comparison included the **independent Playwright `editor.fill('Привет X日本')` operation** between the earlier selected-commit snapshot and the canceled composition. Input sequences differ between Chromium and Firefox; attributing every intervening commit to cancellation is not a valid transaction assertion. The corrected test snapshots `editorCommits` immediately after fill and before canceled composition, requires `cancel` and `blur` to leave that value unchanged, and separately keeps the immediate late-`input`/duplicate-`compositionend` assertion after the original commit. It does NOT loosen text integrity or duplicate-commit protection. R4e2 remains unverified until the new final browser run passes.


### R4e2: Chromium stale DOM value after synthetic compositionend

[Runtime #40336789335](https://github.com/Lalalalendia/FrontRust/actions/runs/40336789335) on `6b75a9d6`: native 9/9 PASS; real Node/WASM R4e2 PASS; compatibility/core PASS; Chromium E2E FAIL at `tests/masonry-browser-real-wasm.mjs:449`. The previously accepted selected IME commit `Привет 猫日本` reverted to `Привет X日本` after duplicate `compositionend` and a late `input(insertFromComposition)`. The HTML input's value had not been synchronously reconciled with the accepted Rust commit, leaving a race before the scheduled Canvas/DOM paint. Browser input without a fresh `beforeinput` could consequently overwrite Rust's accepted document.

The bounded adapter now synchronously projects `mp_editor_committed_text` to the DOM at `compositionend` and fences same-frame input echoes without a fresh `beforeinput`. The guard clears on new beforeinput/blur/compositionstart or the next paint, so normal later edits can proceed. A browser E2E requires synchronous DOM/Rust agreement and deliberately reproduces an outdated input value followed by an `input(insertText)` echo after a completed selected composition. This tests only the bridge ordering; native OS IME behaviour, real undo/redo and full text rendering are still unverified.


## R4f — bounded Rust TextArea undo/redo without full-document reset (pending CI)

- **Native editor:** `editor_replace` and all ordinary `editor_splice` operations route through upstream `TextArea::select_byte_range` + `RenderRoot::handle_text_event(TextEvent::ClipboardPaste)`; `TextArea::reset_text` is no longer invoked in BrowserHost text mutation or IME recovery.
- **IME rollback:** `Ime::Disabled` clears transient preedit in Parley but may also remove an originally selected range. Rust now computes one UTF-8 scalar-safe contiguous diff from the actual `TextArea::text()` back to the last accepted canonical document and applies a compensating **real TextArea splice**. That internal recovery does not create an undo transaction. `Ime::Commit` then applies the originally selected range and records exactly one accepted transaction. Cancel/blur preserve the prior selected text and do not add history.
- **App-level history:** bounded `Vec<EditorEdit>` (64 edits max), recording byte offsets and replaced/inserted UTF-8 strings for accepted edits only. A new edit clears redo. `mp_editor_undo` and `mp_editor_redo` apply inverse edits to the same live TextArea through real event routing with journal recording muted. Invalid current text/range, unfocused editor and active preedit fail closed. Oldest records are discarded after the 64-entry limit.
- **Browser:** `Ctrl/Meta+Z`, `Ctrl/Meta+Shift+Z`, and `Ctrl/Meta+Y` are routed to Rust when the native input owns focus; browser-native history shortcuts are prevented so Rust and DOM cannot diverge. The UI displays the two journal depths. Browser/Node/native tests cover IME-selected replacement X→猫, undo X, redo 猫, commit-count invariance, redo invalidation by a new edit, cancellation without a new history entry and focus/IME fences.
- **Not a production editor:** The pinned upstream Parley TextArea has no exposed native undo stack on this integration path; this journal is application-owned and deliberately simple. It does not group ordinary keystrokes or restore arbitrary directional selections, and it does not prove OS text input methods, native IME event-order races, spelling/autocorrect, multi-range grapheme editing, CJK font fallback or screen-reader actions. The synthetic layout font is still not suitable for rendered text. R4f must pass native Rust, Node/WASM, Chromium and Firefox on the same SHA before it can be marked Verified bounded.


## R5a — actual Masonry glyph outlines to Canvas2D (pending CI)

**Purpose.** Close the missing graphics-transport link without claiming full typography. A genuine upstream `masonry::widgets::Label("Привет")` in the existing RenderRoot uses Parley/fontique to shape a real `imaging::record::Draw::GlyphRun`. Rust uses the glyph IDs, positions and font blob provided by **that recorded run**. Pinned `skrifa 0.40.0` decodes each outline into `BezPath`, and the existing bounded scene protocol transports its Move/Line/Quad/Curve/Close commands to Canvas2D, applying each run/glyph transform plus y-up→y-down coordinate reflection. Browser JS does NOT call `fillText`, infer characters from Unicode, or re-shape the string.

The scene renderer accepts only solid monochrome `Style::Fill(Fill::NonZero)` glyph runs, unhinted outlines, no variation coordinates, no synthetic embolden/glyph transform/brush transform, no non-default compositing, valid finite glyph positions and font size, fonts <=2 MB, <=32 glyphs/run, <=512 path commands/glyph, <=128 total geometry shapes and <=256 scene operations. Empty space outlines may generate no draw operation. Unsupported operations fail closed and increment `skipped_draws`, which stops Canvas painting under the production-fidelity gate; they are not silently replaced by boxes, `fillText` or arbitrary geometry.

**Tests.** Native Rust requires a real `GlyphRun`, >=6 Cyrillic outlined glyphs, valid finite Kurbo paths and zero skipped operations before and after resize. Real Node/WASM validates exported counts and packed RGBA path geometry. Chromium and Firefox assert the glyph path color, scene counts and actual colored Canvas pixels and retain all R3b–R4f tests.

**Font provenance and boundary.** The scene Label now uses a **real, renamed Lato Regular Cyrillic subset** (six characters needed for `Привет`; 1,480-byte TTF). Source: Debian `fonts-lato`, upstream Łukasz Dziedzic, SIL OFL-1.1; internal family renamed to `FrontRust Demo Subset` to avoid use of reserved original family name for a modified subset. License and SHA256 provenance live under `fixtures/`. This produces actual distinct Cyrillic contours rather than placeholder rectangles. **The editor TextArea still uses the synthetic R4e layout fixture** for Unicode selection coverage. Therefore R5a proves a small genuine glyph rendering pipeline, **not full typography**. It lacks full font provisioning and coverage/fallback for CJK/emoji, hinted/variable/color glyphs, rich text and broad glyph visual baselines. Do not mark fonts production-ready.
