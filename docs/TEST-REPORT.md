# Observed test report — 2026-10-25

**Executed locally in a runtime with Node.js 22, Chromium and Python/Playwright, but NO `rustc`/`cargo`, and NO outbound Rust toolchain download.**

- `node --check web/app.js`: PASS.
- `node --check web/mock-engine.js`: PASS.
- `node tests/mock-abi.test.mjs`: PASS; validates JavaScript **test double**, not Rust state semantics.
- `python3 tests/browser_smoke.py`: PASS, Chromium: HTML→Canvas width, selection without document revision, drag→DOM, keyboard focus scoping and undo; mock injected directly to avoid local HTTP access blocked by browser sandbox.
- `dist/browser-preview.png`: captured, reviewed at 1440 px width.
- `cargo test --workspace`: **NOT EXECUTED** — toolchain absent.
- `cargo build -p rui-wasm --target wasm32-unknown-unknown`: **NOT EXECUTED** — toolchain absent.
- Native Rust SSR HTTP: **NOT EXECUTED** — toolchain absent.
- Browser tests using **actual Rust/WASM**: **NOT EXECUTED** — no wasm build.
- Masonry browser backend: **NOT IMPLEMENTED**.

The screenshot and PASS results establish a coherent browser presentation adapter against a mock ABI only. They do not establish production readiness or Rust/WASM parity.