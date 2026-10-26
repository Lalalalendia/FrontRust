# P1 — Masonry browser target falsification gate

The PoC intentionally **does not claim Masonry runs in a browser**. This file specifies the minimum experiment before investing in its adapter.

## Inputs

- Linebender Xilem source at exact revision chosen for experiment. Akula uses `271a27a6d4a930f7878d404f9014e3c50a3a9b88` as of this audit; upstream may differ.
- Rust stable with `wasm32-unknown-unknown`, modern Chromium + Firefox (+ Safari if available).
- One minimal 2-widget Masonry tree, then basic text input and a nested scroll view.
- Existing known wasm-target breakage tracked at [linebender/xilem#1848](https://github.com/linebender/xilem/issues/1848); check current issue state rather than assuming unresolved.

## Ordered tests

1. Build `masonry_core` and minimal widget subset for wasm32; retain exact compiler diagnostics and dependency graph. **Do not** start with the entire Akula binary.
2. Implement `BrowserHost` around RenderRoot: browser events, pointer capture, DPI, focus, resize and requestAnimationFrame; no WebView.
3. Send a real recorded `Scene/VisualLayerPlan` through a browser renderer. First use a simple Canvas2D pixel transport, then compare GPU path on representative workloads.
4. Test keyboard navigation, clipboard, native IME composition (Russian/Japanese), focus escape/return, accessibility semantics and mobile touch.
5. Compare frame time, memory, first useful paint and input latency against a DOM baseline. Record unsupported APIs as capability failures.
6. Decide whether to upstream a narrowly scoped browser adapter, maintain an isolated wrapper, or stop the fork. Changes to core Masonry must be justified individually.

## Pass / fail criteria

PASS requires genuine widget event execution plus correct output, not merely `cargo check` or showing a bitmap. Failure is acceptable when documented; it should not block the DOM/SSR subsystem.

## No-go conditions

- Dependence on non-portable Akula product modules or AGPL code copied without approval.
- Reimplementing text editing, accessibility and CSS for ordinary public pages merely to obtain visual uniformity.
- Silent fallback from failed WASM to a JS implementation claimed to be shared Rust state.