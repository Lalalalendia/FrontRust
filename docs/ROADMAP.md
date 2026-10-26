# First implementation ladder — requirement traceability

## P0 — code inventory / bounds
- [x] Locate Masonry + render/host/output boundaries in Akula live code.
- [x] Confirm Chaptera invalidation source is a domain-coupled Rust crate.
- [x] Identify existing SEO, Pretext, ReScript and accessibility studies and distinguish implementation from research.
- [x] Define license and dependency boundaries for a product-neutral codebase.
- [ ] Audit the rest of relevant repository code and CI histories before extraction; this initial list is selective, not exhaustive.

## P1 — common contract
- [x] Independent `rui-core` source with typed commands and no-renderer revision rules.
- [x] `rui-wasm` adapter source using a single `UiModel` Rust instance.
- [x] `rui-server` SSR/SEO source and browser DOM/Canvas adapter source.
- [ ] Real `cargo test --workspace` run on a Rust-enabled runner.
- [ ] Real `wasm32` build and smoke test without JS mock.
- [ ] Extend model to two+ object identities, event batching, semantic focus projection and per-artifact dependency fingerprints.

## P2 — first integrated browser vertical slice
- [x] HTML field changes update canvas through one API contract (browser mock smoke).
- [x] Pointer move updates HTML inspector; undo returns to previous geometry (browser mock smoke).
- [x] Browser-only screenshot/DOM test with Chromium; explicit mock status.
- [ ] Verify the same tests against **real Rust-generated wasm**.
- [ ] SSR HTTP response test for real compiled `rui-server`.
- [ ] Leptos/Xilem Web DOM experiment and benchmark versus server-generated minimal HTML.
- [ ] Masonry `wasm32` compile + interactive widget gate; see MASONRY-WASM-PROBE.md.

## P3 — performance and correctness
- [ ] p50/p95 input-to-pixel, CPU/RSS and wasm cold start on Chromium/Firefox/Safari.
- [ ] Accessibility tree, screen-reader matrix, real native IME (not synthetic composition events).
- [ ] PreparedText caching, resource scheduler, renderer backend loss and invalidation differential oracle.
- [ ] SEO/canonical/sitemap/cache invalidation regression matrix on realistic content.
- [ ] Mobile and low-end devices, large scene, high DPR, context-loss stress.

## Done criterion for reusable component

A shared module is not adopted just because it has neat abstractions. Require an independent test vector plus usefulness in at least **two distinct product scenarios**, and explicit dependency/license/maintenance ownership. Do not move product-specific complex functionality until this criterion passes.