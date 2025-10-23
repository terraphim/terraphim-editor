# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Documentation
**For detailed architectural context, see `.docs/summary.md`**  
Individual file summaries are available in `.docs/summary-*.md`

## Project Overview
Terraphim Editor is a Rust+WASM markdown editor with minimal dependencies, offering live preview and distributable as an NPM package in multiple formats (ESM, UMD, IIFE).

## Build System Architecture

### Development Workflow (Trunk)
```bash
trunk serve                    # Dev server on http://127.0.0.1:8080
trunk serve --port 3000        # Custom port
trunk build --release          # Production build (dev workflow)
```

**Purpose**: Fast Rust/WASM iteration with live reload

### Distribution Workflow (wasm-pack + Vite)
```bash
./build.sh                     # Full production build → package/
```

**Breakdown**:
1. `wasm-pack build --target web` — Compiles Rust to WASM
2. `npm install` — Installs Vite + plugins
3. `npm run build` — Bundles JS wrapper (ESM/UMD/IIFE)
4. Assembles `package/` directory with npm-ready artifacts

## Testing Commands
```bash
cargo test                     # Rust unit tests
wasm-pack test --chrome        # Browser integration tests (Chrome)
wasm-pack test --firefox       # Browser integration tests (Firefox)
cargo bench                    # Performance benchmarks (Criterion)
```

## Linting & Formatting
```bash
cargo fmt                      # Format Rust code
cargo clippy                   # Lint Rust code
cargo clippy -- -D warnings    # Fail on warnings
```

## Key File Locations

**Rust Sources**:
- `src/lib.rs` — Main WASM library (entry point, markdown conversion)
- `tests/web.rs` — Browser-based integration tests
- `benches/markdown_bench.rs` — Performance benchmarks

**Configuration**:
- `Cargo.toml` — Rust dependencies, dual crate-type (cdylib + rlib)
- `Trunk.toml` — Development server config (port 8080, MIME types)
- `package.json` — NPM metadata, Vite build scripts
- `vite.config.js` — Library bundler config (multi-format output)
- `build.sh` — Production build orchestration

**Templates & UI**:
- `templates/editor.html` — Rinja template for editor layout
- `public/js/config.js` — Editor configuration (shortcuts, commands)
- `public/js/editor.js` — Standalone editor class (used by Trunk)
- `public/js/terraphim-editor.js` — Library wrapper (used by Vite)

## Architecture Highlights

### Dual Build System
- **Development**: Trunk compiles Rust → WASM, serves with live reload
- **Distribution**: wasm-pack + Vite creates multi-format NPM package

### WASM Integration Pattern
1. Rust's `run()` function auto-executes via `#[wasm_bindgen(start)]`
2. Renders Rinja template into `#editor-container`
3. Sets up event listeners for live markdown conversion
4. JS wrapper (`TeraphimEditor` class) provides high-level API

### Data Flow
```
User types → Input event → Rust closure → markdown::to_html_with_options() → set_inner_html() → Preview updates
```

## Common Development Tasks

### Add New Keyboard Shortcut
1. Edit `public/js/config.js` → Add to `shortcuts` array
2. Specify: `name`, `key`, `prefix`, `suffix`, `desc`
3. Restart `trunk serve` to reload

### Add New Toolbar Command
1. Edit `public/js/config.js` → Add to `commands` array
2. Specify: `name`, `icon`, `prefix`, `suffix`
3. Icon must be valid Shoelace icon name

### Modify Markdown Conversion
1. Edit `src/lib.rs` → Update `markdown::to_html_with_options()` call
2. Can customize `Options` struct for different parsing rules
3. Test with `cargo test` and `wasm-pack test --chrome`

### Change UI Layout
1. Edit `templates/editor.html` (Rinja template)
2. Uses Shoelace web components (`sl-*` tags)
3. Inline styles in `<style>` block
4. Rendered by Rust at runtime

### Optimize WASM Binary Size
1. Add to `Cargo.toml`:
   ```toml
   [profile.release]
   lto = true
   codegen-units = 1
   opt-level = "z"
   ```
2. Run `wasm-pack build --target web --release`
3. Consider `wasm-opt` for further optimization

## Development Workflow Guidelines

### Date/Time Handling
- **Always use `jiff`** instead of `chrono` for date/time operations
- `jiff` is the preferred crate for temporal functionality

### Command-Line Constraints (MacOS)
- **NEVER use `timeout` command** - it does not exist on macOS
- Use alternative approaches for time-limited operations:
  - Use `tmux` with monitoring instead of `sleep`
  - Implement timeouts in code (Rust or scripts) rather than shell

### Testing Philosophy
- **Never use mocks in tests** - prefer real implementations and integration tests
- Write tests that validate actual behavior, not mocked responses
- Use browser integration tests (`wasm-pack test`) for WASM validation

### Error Detection & Code Quality
- **Use IDE diagnostics** to find and fix errors before committing
- Run `cargo clippy -- -D warnings` to catch issues early
- Leverage rust-analyzer or similar LSP for real-time feedback
- **Always check test coverage** after implementation
  - Run tests: `cargo test`, `wasm-pack test --chrome`
  - Validate coverage meets project standards

### Task & Issue Management
- **Keep track of all tasks in GitHub issues** using `gh` tool
- Create issues for features, bugs, and improvements:
  ```bash
  gh issue create --title "Add feature X" --body "Description"
  gh issue list --state open
  ```
- **Commit every change** and keep GitHub issues updated:
  ```bash
  git commit -m "feat: implement X (fixes #123)"
  gh issue comment 123 --body "Implemented in commit abc123"
  gh issue close 123
  ```
- Reference issue numbers in commit messages for traceability

### Background Task Management with tmux
- **Use `tmux` to spin off background tasks** instead of blocking commands
- **Use `tmux` instead of `sleep`** to continue working while processes run
- Monitor long-running processes in separate panes:
  ```bash
  # Start new tmux session with descriptive name
  tmux new -s editor-build
  
  # Split panes for parallel work
  tmux split-window -h
  
  # Run build in one pane, work in another
  # Pane 1: ./build.sh
  # Pane 2: Continue development
  
  # Attach to existing session
  tmux attach -t editor-build
  
  # Read log output from background pane
  tmux capture-pane -t editor-build:0.1 -p
  ```
- Example workflow:
  1. Start `trunk serve` in tmux pane 1
  2. Run tests in tmux pane 2
  3. Continue coding in tmux pane 3
  4. Switch between panes to check output without interrupting work

## Important Notes

### Dependencies
- Minimal production deps: wasm-bindgen, web-sys, markdown, rinja
- Shoelace is peer dependency (consumer controls version)
- Alpha markdown crate (may have breaking changes)
- **Use `jiff` for date/time** (not chrono)

### Security Considerations
⚠️ **XSS Risk**: No input sanitization before `.set_inner_html()`
- Should add DOMPurify or equivalent
- See `.docs/summary.md` Section 5 for details

### Performance
- Target: < 50ms markdown conversion (validated by benchmarks)
- No debouncing (acceptable for documents < 10KB)
- Synchronous rendering (blocks UI thread)

### Browser Compatibility
- Requires WebAssembly support (all modern browsers)
- Shoelace components use Shadow DOM
- Tested in Chrome, Firefox (via wasm-pack test)

## Debugging

**Rust/WASM**:
- Panics logged to browser console via `console_error_panic_hook`
- Use `web_sys::console::log_1()` for debug logging
- DevTools → Sources → WASM modules (limited inspection)

**JavaScript**:
- Standard DevTools, sourcemaps enabled (Vite)
- Console logging in wrapper code

**Performance**:
- Chrome DevTools → Performance tab
- `cargo bench` for isolated Rust benchmarks
- Browser Performance API in integration tests

## Project Structure

```
terraphim-editor/
├── src/                    # Rust source (WASM entry point)
├── tests/                  # Browser integration tests
├── benches/                # Performance benchmarks
├── templates/              # Rinja templates
├── public/                 # Static assets + examples
│   ├── js/                # JavaScript (config, editor, wrapper)
│   ├── css/               # Styles
│   └── wasm/              # Compiled WASM (from pkg/)
├── dist/                   # Build output (Trunk/Vite)
├── pkg/                    # wasm-pack artifacts
├── package/                # NPM-ready distribution
├── .docs/                  # Architecture documentation
├── Cargo.toml              # Rust config
├── Trunk.toml              # Dev server config
├── package.json            # NPM config
├── vite.config.js          # Bundler config
├── build.sh                # Production build script
└── index.html              # Development entry point
```

## Additional Resources
- **Comprehensive Summary**: `.docs/summary.md` (13 sections, deep architectural analysis)
- **File-Specific Docs**: `.docs/summary-*.md` (individual file summaries)
- **README.md**: User-facing project overview
- **Shoelace Docs**: https://shoelace.style/
- **wasm-bindgen Guide**: https://rustwasm.github.io/wasm-bindgen/
- **Trunk Docs**: https://trunkrs.dev/
