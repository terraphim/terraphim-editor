# Terraphim Editor – Deep-Dive Review Scratchpad

## 2025-06-21 – Initial project audit

### High-priority issues discovered

1. **Container ID mismatch**
   * `src/lib.rs::run()` expects an element with id `editor-container` (error string still mentions `app`).
   * `tests/web.rs` creates a container with id `app` – all browser tests will fail.
   * Packaging wrapper `public/js/terraphim-editor.js` dynamically assigns a random id to the passed container, then invokes `run()`, which will **not** find `#editor-container` ⇒ runtime failure in bundled build.

2. **Duplicate / competing rendering layers**
   * Rust/WASM already renders full editor UI via `Rinja` template (`templates/editor.html`).
   * `public/js/terraphim-editor.js` **also** builds its own DOM subtree & later calls the same `run()` again.
   * Leads to double DOM, duplicate listeners, BROKEN separation of concerns.

3. **Markdown preview update strategy**
   * `src/lib.rs::setup_markdown_conversion()` converts on every `input` event; that's fine, but JS wrapper calls `run()` on every keystroke which re-initialises everything – very expensive and probably broken.

4. **Packaging scripts**
   * `vite.config.js` copies `pkg/terraphim_editor.js` & `.wasm` into `public` before every build – OK, but we must ensure `wasm-pack build` ran first.
   * `build.sh` assumes `dist/` exists *and* contains CSS named `style.css`; Vite renames it to `terraphim-editor.css` via `assetFileNames`, but only when css is part of bundle. Need to confirm CSS pipeline or copy from `public/css/terraphim-editor.css`.

5. **Examples**
   * All examples expect global constructor `TeraphimEditor` that works; because of (1) & (2) they currently fail to render.

### Proposed next steps

- [ ] Decide single source of truth for editor UI (Rust vs JS).
- [x] Align container id across Rust code, tests, and JS wrapper.
- [x] Expose a lightweight `render_markdown(input: &str) -> String` func from WASM for JS to use instead of calling `run()`.
- [x] Simplify JS wrapper & use `render_markdown()` for live preview & toolbar actions.
- [x] Fix build.sh CSS handling & verify `package/` examples after `npm pack` (tarball inspected, CSS + wasm present).

### Minor polish

- Update panic message in `run()`