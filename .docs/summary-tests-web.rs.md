# Summary: tests/web.rs

## Purpose
Browser tests (wasm_bindgen_test, real Chrome, no mocks) for the editing surface and preview.

## Key points
- 28 tests: shortcuts, palette, offset mapping incl. emoji/combining marks, native typing, paste, undo/redo in repeated text, decorations, destroy, debounce.
- Browser tests are split across binaries so each stays within the runner's 20 s budget; helpers live in `tests/support/mod.rs`.
