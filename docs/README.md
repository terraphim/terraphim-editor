# Terraphim Editor Documentation

> Your one-stop reference for using, developing, and publishing **terraphim-editor**.

---

## 1  Project Structure Overview

```
terraphim-editor/
│  Cargo.toml            — Rust crate manifest (WASM library)
│  Trunk.toml            — Trunk build configuration
│  build.sh              — End-to-end packaging script (wasm-pack ➜ vite ➜ npm)
│  vite.config.js        — Front-end bundler configuration (ESM/UMD/IIFE builds)
│  README.md             — High-level project description (quick start)
│
├─ src/                  — Rust source (WASM entry + helpers)
│   └─ lib.rs            — `run()` initialises UI, `render_markdown()` helper
│
├─ templates/            — HTML fragments rendered by Rinja (Rust side)
│   └─ editor.html       — Editor UI template injected at runtime
│
├─ public/               — Static assets copied verbatim by Trunk/Vite
│   ├─ css/…             — Editor stylesheet
│   ├─ js/…              — Thin JS wrapper + config
│   └─ wasm/…            — WASM binary copied for dev-server
│
├─ pkg/                  — wasm-pack output (auto-generated, source of truth for WASM bindings)
├─ dist/                 — Vite production bundles (auto-generated)
├─ package/              — Final npm-ready directory with dist/, examples, licence, etc.
│
└─ tests/ & benches/     — Browser + criterion benchmarks
```

---

## 2  Using the Editor in Your Project

### 2.1 Install from npm (recommended)
```bash
npm install terraphim-editor
# or
pnpm add terraphim-editor
```

### 2.2 Choose a Build Flavour
* **ESM** (modern bundlers)
* **UMD** (Node, classic script loaders)
* **IIFE** (drop-in `<script>` tag)

Example (ESM):
```html
<script type="importmap">
  {
    "imports": {
      "terraphim-editor": "./node_modules/terraphim-editor/dist/js/terraphim-editor.mjs"
    }
  }
</script>

<div id="editor"></div>
<script type="module">
  import { TeraphimEditor } from 'terraphim-editor';
  new TeraphimEditor({
    container: document.getElementById('editor'),
    wasmUrl: '/node_modules/terraphim-editor/dist/wasm/terraphim_editor_bg.wasm'
  }).initialize();
</script>
```

> **Note**: Point `wasmUrl` to the bundled WASM file *relative to* your site.

---

## 3  Local Development Workflow

### 3.1 Prerequisites
* Rust + `wasm32-unknown-unknown` target
* [Trunk](https://trunkrs.dev) (`cargo install trunk`)
* Node ≥18 & npm/pnpm/yarn

### 3.2 Start Dev Server (live-reload)
```bash
# One-shot helper that ensures WASM + JS outputs exist
trunk serve
# visit http://127.0.0.1:8080
```
Trunk watches Rust & template changes and rebuilds on the fly. Vite handles JS/TS bundling.

### 3.3 Running Tests
```bash
# Rust unit tests
cargo test

# Browser WASM tests
wasm-pack test --chrome
```

### 3.4 Benchmarks
```bash
cargo bench            # criterion
```

---

## 4  Building & Packaging

### 4.1 Generate Production Assets
```bash
./build.sh   # orchestrates wasm-pack ➜ vite ➜ npm package preparation
```
Outputs:
* `dist/` – minified JS bundles + source maps
* `package/` – npm-publishable directory with CSS, JS bundles, WASM, examples

### 4.2 Create .tgz for Inspection
```bash
cd package && npm pack
```
The tarball should contain:
* `dist/css/terraphim-editor.css`
* `dist/js/terraphim-editor.{mjs,umd.js,iife.js}` (+ maps)
* `dist/wasm/terraphim_editor_bg.wasm`
* Example HTML files
* Licence & README

---

## 5  Publishing to npm
1. Make sure you're in a **clean git state** and on the correct version tag.
2. Bump version in `Cargo.toml` (and optionally `package.json` template inside `build.sh`).
3. Run `./build.sh`.
4. `cd package`
5. `npm publish --access public`

> The script auto-generates a fresh `package.json` with the right version and export map.

---

## 6  Contributing Guidelines

### 6.1 Branch & Commit
* Use feature branches: `feature/…`, `fix/…`
* Write clear, concise commit messages (present tense).  
  Example: `fix: handle empty markdown input gracefully`.

### 6.2 Code Style & Principles
* Follow Rust & JavaScript idioms.  
  (See `.editorconfig` if present.)
* Async Rust via **tokio** (see `user_rules`).
* Keep public APIs minimal; document all exports with `rustdoc` & JSDoc.

### 6.3 Pull Request Checklist
- [ ] `cargo test` & `wasm-pack test` pass
- [ ] `npm run build` succeeds
- [ ] Benchmarks unaffected (or updated)
- [ ] Docs & examples updated

---

## 7  FAQ

**Q: The preview doesn't update!**  
A: Ensure the WASM file is served from `wasmUrl`, and that `render_markdown()` is called on `input`.

**Q: Can I style the editor?**  
A: Yes – override variables in `public/css/terraphim-editor.css` or extend with your own stylesheet.

---

## 8  Useful Commands Reference
| Purpose | Command |
|---|---|
| build & package | `./build.sh` |
| dev server | `trunk serve` |
| unit tests | `cargo test` |
| browser tests | `wasm-pack test --chrome` |
| benchmark | `cargo bench` |
| generate .tgz | `cd package && npm pack` |
| publish | `cd package && npm publish --access public` |

---

Happy hacking! 🎉 