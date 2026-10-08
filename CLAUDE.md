# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Terraphim Editor is a WebAssembly-based Markdown editor built with Rust and vanilla JavaScript. Trunk orchestrates the build: Rust compiles to WASM for Markdown rendering and for the document model, while a contenteditable surface and the Write_On chrome are plain JavaScript and CSS. It implements the "alternative control" requirements (`docs/requirements/alternative-control.md`): span-anchored alternatives, ghosting and overflow, persisted as a trailing annotation block in the `.md` file.

## Task Tracking and Workflow

- Work is tracked in Gitea at `https://git.terraphim.cloud` (`terraphim/terraphim-editor`) with the `gtr` (gitea-robot) CLI, not GitHub. Source `~/.config/terraphim/gitea.env` for `GITEA_TOKEN`.
- Branch `task/IDX-short-title`; commits end with `Refs #IDX` (or `Fixes #IDX`) and the co-author line; open PRs with `gtr create-pull`.
- Every pull request is reviewed with pi-rust and the `structural-pr-review` skill (read-only tools). An authoring agent's self-review is not the review of record. Verify findings before posting them.
- Commit only when tests are green. Check test coverage after implementation.

## Development Commands

### Development Server
```bash
trunk serve
```
Serves at http://127.0.0.1:8080 with hot reload. Output goes to `target/trunk-dist` (untracked).

### Building

**Development / production Trunk build:**
```bash
trunk build            # or: trunk build --release
```
Both write to `target/trunk-dist`; they never modify the committed `dist/`, so `git status` stays clean.

**Release build (regenerates the committed `dist/` on purpose):**
```bash
scripts/release-dist.sh
```
This runs `trunk build --release --dist dist`, and the build hook adds the embeddable bundle (`terraphim-editor.min.js`, `terraphim-editor.min.css`, `example.html`). Review `git status dist` and commit the result. `.gitignore` ignores `dist/*` but keeps `*.html`, `*.js`, `*.css` and `*.wasm` tracked, so `git add dist` needs no `-f`.

The `scripts/build-dist.sh` hook writes to `$TRUNK_STAGING_DIR`, its first argument, or `target/trunk-dist`; never to `./dist` unprompted. Needs `terser` and `cleancss` on `PATH`.

### Testing

```bash
cargo test --workspace                 # native unit, golden-file and span-model tests
scripts/browser-tests.sh               # browser tests (real Chrome, nothing mocked), one binary at a time, load-guarded
cargo bench --no-run                   # compile the criterion benches
cargo bench                            # run them (native only)
```

The browser tests are split across binaries (`tests/web.rs`, `web_chrome.rs`, `web_persistence.rs`, `web_bench.rs`, plus `tokens.rs`, sharing `tests/support/mod.rs`) because wasm-bindgen-test gives each binary one 20 s budget. Native golden files live in `tests/fixtures/persistence/` (`UPDATE_GOLDENS=1` rewrites them; review the diff).

Run one test: `cargo test <name>`.

#### Browser tests: always through `scripts/browser-tests.sh`

Run the browser binaries locally with `scripts/browser-tests.sh` (all of them, or `scripts/browser-tests.sh web web_chrome`), not with a bare `wasm-pack test`. wasm-bindgen-test gives each binary a fixed 20 s budget. When it runs out, the runner's next WebDriver call gets only the remainder ("execute/sync timed out after 5.0s") and ChromeDriver is killed. Before every binary the script waits for the one-minute load average to drop below `BROWSER_TESTS_MAX_LOAD`: 20 on Linux, and on macOS three quarters of the logical CPUs, at least 4 (8 on an 11-core laptop). While it waits it prints the load, the limit and the busiest processes every 30 s. After `BROWSER_TESTS_LOAD_WAIT` seconds (default 300) it stops, lists the binaries it did not run as NOT RUN and exits 4; it never runs them under load. On CI (`CI`, `GITEA_ACTIONS` or `BROWSER_TESTS_CI`, which `scripts/ci/browser-tests.sh` sets) the guard only warns: one capped wait before the first binary, then every binary runs and the run never exits 4, because the CI runner is fine at loads up to 23. `scripts/browser-tests-load-test.sh` checks the guard against the real load without starting a browser.

The evidence (issue #110): on the CI runner, over 25 runs (37746–37790, 1,158 binary runs, load average 2.1–23.2), no binary used more than 3.24 s of its 20 s budget:

| Binary | Test time max | Test time median | Wall median (with build) |
|---|---|---|---|
| web_bench | 3.24 s | 2.40 s | 7 s |
| web_bench_logged | 1.41 s | 1.15 s | 5 s |
| web_chrome | 1.29 s | 0.15 s | 4 s |
| every other binary | ≤ 0.91 s | ≤ 0.83 s | 4–5 s |

The 17–21 s runs, timeouts and SIGKILLs seen on a shared laptop came from other processes, typically several agents' headless Chrome sessions pushing the load to 10–17. In that state even an untouched binary went from 5 s to 21 s. Do not split binaries or raise timeouts for local slowness; read the per-binary times (`<bin>: ok (Ns wall, finished in Xs, load L)`) from the CI job log first.

Coverage: `cargo llvm-cov --workspace` measures the native tests. The Rust that runs only in the browser (the `#[wasm_bindgen]` exports) and `public/js` are measured by `bash scripts/ci/wasm-coverage.sh [suite...]`: wasm-bindgen-test coverage on nightly for the Rust, V8 precise coverage over the DevTools protocol for the JavaScript. It needs a nightly with the wasm32 target and llvm-tools, and a clang of the same LLVM major version (`brew install llvm@N`). Output goes to `target/wasm-coverage/` (`summary.txt`, `js-summary.txt`, `coverage.lcov`). It does not gate CI.

### Linting and Formatting
```bash
cargo fmt --all -- --check
cargo clippy --workspace
```

### Private registry
`terraphim_automata` and `terraphim_types` come from the private `terraphim` Cargo registry (`sparse+https://git.terraphim.cloud/api/packages/terraphim/cargo/`, configured in `~/.cargo/config.toml`). Cargo cannot resolve them without it. If cargo refuses to run in a nested worktree, run from an rsync copy under the scratchpad directory.

## Architecture

### Workspace
- `terraphim-editor` (root): WASM crate and `rlib`.
- `crates/terraphim_alternatives`: the span model, plain Rust (edition 2024), no `web-sys`. Spans, alternatives, the ghost layer, overflow, UTF-16 offsets, re-anchoring (through `terraphim_automata::CompiledMatcher`) and the trailing `terraphim-alternatives` fenced JSON block (schema version 1).
- `research/lab-heuristics`: deliberately not a workspace member; deterministic weakest-sentence ranking and trim selection prototype (issue #3). Run with `cargo run --manifest-path research/lab-heuristics/Cargo.toml`.

### Rust layer
- `src/lib.rs`: `run()` (`#[wasm_bindgen(start)]`), Rinja template render, `render_markdown`, and the debounced preview (`flush_preview`, `set_preview_delay`, `preview_delay`, `preview_pending`, `preview_render_count`; default 120 ms trailing).
- `src/document.rs`: persistence bridge. `DocumentSession` (native, unit-testable) holds one open document; thin `#[wasm_bindgen]` wrappers (`open_document`, `save_document`, `export_document`, `apply_edit`, `sync_document_body`, `document_counts`, `document_body`, `document_annotations`) reach JavaScript as `window.wasmBindings.<name>`. Offsets are UTF-16 code units. Malformed blocks are preserved verbatim; detached or unresolved annotations are set aside and saved, never dropped.
- `src/kg.rs`: knowledge-graph alternatives (#13) over `terraphim_lsp_core` (exact registry version; `terraphim_alternatives` is patched to the workspace member so there is one copy). Derived KG spans (`document_annotations().kg`, never saved), `kg_swap_alternative`, `kg_lookup_selection`, `alt_kg_append` (Ctrl+Shift+G, R-8.6). No matching or synonym logic here. See `docs/design/kg-alternatives.md`.
- `src/providers.rs`: the pluggable alternatives provider seam (#83, R-8.7): `AlternativesProvider` trait, ordered per-thread registry, `append_from_providers`, and the `alt_append_alternatives` / `alternatives_provider_*` exports behind `editor.registerAlternativesProvider`. See `docs/design/provider-seam.md`.
- `templates/editor.html`: Rinja template (`{{ initial_content }}`, `{{ initial_preview|safe }}`).

### JavaScript layer (`public/js/`)
- `editor.js`: `EditorSurface` (contenteditable `plaintext-only` span-aware surface: UTF-16 offsets, decorations, its own undo/redo history, canonical DOM so `textContent` is the document) and `MarkdownEditor` (toolbar, shortcuts, `/` command palette, help dialog, document API wrappers `openDocument`/`saveDocument`/`exportDocument`/`counts`/`annotations`).
- `chrome.js`: Write_On mode toggle and corner chrome; dispatches `te:*` CustomEvents (see `docs/design/write-on-chrome.md`).
- `config.js`: `EditorConfig` with shortcuts, commands and initial content. `editor_config.toml` is not read by the code.
- `terraphim-editor.js`: embeddable `TeraphimEditor` wrapper used by the minified bundle.

### CSS (`public/css/`)
- `tokens.css`: Write_On design tokens (`--te-*`), documented in `docs/design/tokens.md`. Consume tokens; do not hard-code colours.
- `write-on.css`: layout and corner chrome scoped to `body[data-mode="write-on"]`.

### Data flow
1. Trunk serves `index.html`; the WASM module loads and `run()` executes.
2. `run()` opens the welcome text in the document session and renders the Rinja template into `#app`.
3. `MarkdownEditor` creates the `EditorSurface` and the chrome after the DOM is ready.
4. Typing fires a capture-phase `input` that normalises the DOM; the target-phase handler schedules the debounced Rust preview from `textContent`; edits are mirrored into the span model through `apply_edit`.
5. Save writes `body + annotation block`; export returns the clean Markdown body.

### Key decisions
- WASM for Markdown and the document model: near-native text processing, shared Rust code with the native tests.
- Contenteditable surface rather than a textarea, so spans can carry decorations.
- Shoelace (CDN, pinned to v2.12.0) for UI components; no npm dependencies. FontAwesome icons are used in the Write_On chrome.
- Trunk for the WASM build; dev output is separated from the committed release copy.

## Project Structure

```
terraphim-editor/
├── src/                       # lib.rs (preview, entry), document.rs (bridge)
├── crates/terraphim_alternatives/   # span model, block format, re-anchoring
├── research/lab-heuristics/   # standalone research prototype (not in workspace)
├── templates/editor.html      # Rinja template
├── public/js/                 # editor.js, chrome.js, config.js, ...
├── public/css/                # tokens.css, write-on.css
├── tests/                     # native golden tests, split browser tests, support/
├── benches/                   # markdown_bench.rs, document_bench.rs
├── docs/{requirements,design,research}/
├── scripts/                   # build-dist.sh (hook), release-dist.sh
├── dist/                      # committed release copy (scripts/release-dist.sh)
├── .docs/                     # per-file summaries and summary.md
├── index.html, Trunk.toml, Cargo.toml
```

## Working with Templates

Rinja (Jinja2-like) templates live in `templates/`, are referenced with `#[template(path = "editor.html")]`, and take variables from struct fields. Use `|safe` for pre-rendered HTML.

## WebAssembly Considerations

- Target is `wasm32-unknown-unknown`; `crate-type = ["cdylib", "rlib"]`.
- Use `wasm-bindgen` for exports and `web-sys` for browser APIs; closures need `.forget()` or a held handle to outlive the Rust scope.
- Browser tests use `wasm_bindgen_test` with `wasm_bindgen_test_configure!(run_in_browser)`.
- criterion does not compile for wasm32 (rayon), so it is a non-wasm dev-dependency only.

## Configuration System

Editor behaviour is driven by `public/js/config.js`.

**Adding a keyboard shortcut:**
```javascript
shortcuts: [
  { name: "icon-name", key: "ctrl+x", prefix: "~~", suffix: "~~", desc: "Description" }
]
```

**Adding a command palette item:**
```javascript
commands: [
  { name: 'Display Name', icon: 'shoelace-icon', prefix: '**', suffix: '**' }
]
```

## Common Development Tasks

- **New formatting feature:** add the shortcut (and optionally the command) to `public/js/config.js`, then try it with `trunk serve`.
- **Template change:** edit `templates/editor.html`; keep `EditorTemplate` fields in sync.
- **Markdown rendering change:** edit `src/lib.rs`; update unit tests there and `tests/web.rs`; run `cargo test --workspace` and `wasm-pack test --headless --chrome`.
- **Span model or block format change:** edit `crates/terraphim_alternatives`; update its tests and the persistence fixtures; keep `docs/requirements/alternative-control.md` consistent.
- **Performance:** browser typing-latency benchmark in `tests/web_bench.rs`; native `cargo bench`. Target under 50 ms average conversion in the browser.

## Dependencies Management

- Update with `cargo update`; audit with `cargo audit`.
- JavaScript: Shoelace from CDN (v2.12.0); no npm dependencies. Build tools: `trunk`, `wasm-pack`, and `terser`/`cleancss` for the bundle hook.

## Troubleshooting

- **"No window found":** the WASM module must load after the DOM; check the console.
- **Template not rendering:** verify the template path, Rinja syntax and struct fields.
- **Shortcuts not working:** `EditorConfig` must load before `editor.js`; ensure the surface has focus.
- **WASM tests failing:** Chrome must be installed; use `--headless`; look at the console without it. A loaded host can approach the 20 s per-binary budget.
- **`trunk build` changes `dist/`:** it should not; confirm `Trunk.toml` has `dist = "target/trunk-dist"`.

## File Naming Conventions

- Rust: `snake_case.rs`; JavaScript: `kebab-case.js` or `camelCase.js`; templates `kebab-case.html`; config `PascalCase.toml` or lowercase.

## Important Notes

- Never use mocks in tests.
- Always commit changes and keep Gitea issues updated with progress (`gtr`).
- Use tmux for background tasks instead of sleep.
- Never use the `timeout` command (it does not exist on macOS).
- British English, no emoji.
- Keep `.docs/` summaries updated when making significant changes.
