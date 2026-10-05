# Summary: Cargo.toml

## Purpose
Workspace and root package manifest for the WASM editor (`terraphim-editor`, `cdylib` + `rlib`).

## Key points
- Workspace members: the root crate and `crates/terraphim_alternatives` (span model); `research/lab-heuristics` stays outside the workspace.
- Runtime deps: wasm-bindgen, web-sys, markdown, rinja, js-sys, serde_json, and `terraphim_alternatives` by path.
- Dev deps: wasm-bindgen-test, wasm-bindgen-futures; `criterion` is a native-only dev dependency (`cfg(not(target_arch = "wasm32"))`) because rayon does not build for wasm32 (#20).
- Benches: `markdown_bench`, `document_bench`.

## Notes
- Terraphim crates (e.g. `terraphim_automata`) come from the private `terraphim` Cargo registry (`registry = "terraphim"`).
