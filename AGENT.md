# AGENT.md

This file provides guidance to AI agents when working with code in this repository.

## Documentation Reference

### Primary Context Source
**Before making architectural changes, consult `.docs/summary.md`**

This comprehensive document (658 lines, 13 sections) provides deep analysis of:
1. Project Overview (tech stack, features, purpose)
2. Architecture Analysis (dual build system, component structure)
3. Build System Deep Dive (Trunk vs. wasm-pack+Vite)
4. Core Functionality Analysis (markdown conversion, live preview)
5. Security Analysis (XSS risks, WASM sandboxing)
6. Testing Strategy (unit, integration, benchmarks)
7. Performance Considerations (WASM benefits, optimization opportunities)
8. Development Workflow (prerequisites, common commands)
9. Distribution & Integration (NPM package, module formats)
10. Business Value & Use Cases (target applications)
11. Technical Debt & Improvements (critical issues, missing features)
12. File Organization & Conventions (structure, naming)
13. Cross-Reference Map (data flows, dependencies)

### File-Specific Context
**Individual summaries available in `.docs/summary-<path>.md`**

Currently documented files:
- **Rust Sources**: `summary-src-lib-rs.md`, `summary-tests-web-rs.md`, `summary-benches-markdown-bench-rs.md`
- **Configuration**: `summary-cargo-toml.md`, `summary-trunk-toml.md`, `summary-package-json.md`, `summary-vite-config-js.md`, `summary-build-sh.md`
- **Templates**: `summary-templates-editor-html.md`
- **JavaScript**: `summary-public-js-config-js.md`

## Required Pre-Development Steps

### 1. Review Architecture
Read `.docs/summary.md` sections 2-3 to understand:
- Dual build system (Trunk for dev, wasm-pack+Vite for distribution)
- Component architecture (Rust/WASM layer, JS wrapper, config, UI)
- Data flow (user input → Rust conversion → DOM update)

### 2. Identify Impact Scope
Determine which layer your change affects:
- **Rust/WASM** (`src/lib.rs`): Markdown conversion logic, template rendering
- **JavaScript Wrapper** (`public/js/terraphim-editor.js`): Library API, initialization
- **Configuration** (`public/js/config.js`): Shortcuts, commands, initial content
- **UI Template** (`templates/editor.html`): Layout, Shoelace components
- **Build System**: Cargo.toml, Trunk.toml, package.json, vite.config.js, build.sh

### 3. Check for Technical Debt
Before implementing features, review `.docs/summary.md` section 11:
- **Critical Issues**: XSS vulnerability, memory leaks, error handling gaps
- **Code Quality**: Hard-coded selectors, inconsistent error handling
- **Missing Features**: Undo/redo, accessibility, mobile responsiveness

### 4. Verify Testing Strategy
Plan testing approach based on `.docs/summary.md` section 6:
- **Unit Tests**: `cargo test` for Rust logic
- **Browser Tests**: `wasm-pack test --chrome` for integration
- **Benchmarks**: `cargo bench` for performance validation
- **Test Gaps**: No accessibility, cross-browser, edge case, or visual regression tests

## Development Guidelines

### Architecture Principles
1. **Separation of Concerns**: Rust handles computation, JS handles UI orchestration
2. **Minimal Dependencies**: Only add essential crates/packages
3. **Configuration Over Code**: Centralize settings in `config.js`
4. **Performance First**: Target < 50ms markdown conversion
5. **Security Conscious**: Be aware of XSS risks (see `.docs/summary.md` section 5)

### Code Quality Standards
- **Rust**: Use `cargo fmt` and `cargo clippy` before committing
- **Error Handling**: Prefer `Result<T, JsValue>` for WASM-compatible errors
- **Memory Safety**: Avoid `.forget()` on closures (creates leak), use proper cleanup
- **Testing**: Write tests for new functionality (unit + browser integration)
- **Documentation**: Update `.docs/` summaries when making architectural changes

### Build System Awareness
- **Development**: Use `trunk serve` for fast iteration
- **Distribution**: Run `./build.sh` to verify full build pipeline
- **Version Sync**: Version in `Cargo.toml` is source of truth (propagated to NPM)
- **Module Formats**: Ensure changes work with ESM, UMD, and IIFE outputs

## Common Tasks Reference

### Adding Features

**New Keyboard Shortcut**:
1. Edit `public/js/config.js` → `shortcuts` array
2. Test in `trunk serve` environment
3. Document in README if user-facing

**New Toolbar Command**:
1. Edit `public/js/config.js` → `commands` array
2. Use valid Shoelace icon names
3. Test text wrapping behavior

**New Markdown Feature**:
1. Check if `markdown` crate supports it
2. Update `src/lib.rs` conversion options if needed
3. Add tests in `tests/web.rs`
4. Benchmark performance impact with `cargo bench`

### Fixing Bugs

**Security Issues** (High Priority):
1. Review `.docs/summary.md` section 5 (Security Analysis)
2. For XSS: Add input sanitization before `.set_inner_html()`
3. Test with malicious markdown inputs

**Memory Leaks**:
1. Search for `.forget()` in `src/lib.rs`
2. Implement proper cleanup mechanism
3. Test with long-running sessions

**Performance Issues**:
1. Run `cargo bench` to establish baseline
2. Implement optimization (see `.docs/summary.md` section 7)
3. Re-benchmark to validate improvement
4. Ensure still < 50ms conversion time

### Refactoring

**Before Refactoring**:
1. Read relevant file summary in `.docs/summary-*.md`
2. Understand integration points and dependencies
3. Plan backwards-compatible changes when possible

**After Refactoring**:
1. Update affected `.docs/summary-*.md` files
2. Run full test suite (`cargo test`, `wasm-pack test --chrome`, `cargo bench`)
3. Verify both build systems work (`trunk serve`, `./build.sh`)
4. Update `.docs/summary.md` if architecture changed

## Critical Warnings

### Do Not
- ❌ Remove or modify the dual build system without deep analysis
- ❌ Change `Cargo.toml` crate-type (breaks both dev and dist workflows)
- ❌ Bundle Shoelace directly (it's a peer dependency)
- ❌ Add heavy JavaScript frameworks (defeats WASM performance benefits)
- ❌ Use `.innerHTML` without sanitization (XSS risk)
- ❌ Modify version in `package.json` directly (sync from `Cargo.toml`)

### Always
- ✅ Test in both Trunk and Vite builds
- ✅ Validate all 3 module formats (ESM, UMD, IIFE) work
- ✅ Run benchmarks after performance-related changes
- ✅ Update documentation when making architectural changes
- ✅ Consider mobile/accessibility when adding UI features
- ✅ Check browser compatibility for new Web APIs

## Integration Points Reference

### Rust ↔ JavaScript Boundary
- `#[wasm_bindgen(start)]` on `run()` — Auto-execution
- `#[wasm_bindgen]` on `render_markdown()` — Public API
- `JsValue` for error propagation
- See `.docs/summary-src-lib-rs.md` for details

### Build Pipeline
```
src/lib.rs
  → cargo/rustc → WASM binary
  → wasm-pack → pkg/ (bindings + .wasm)
  → Vite → dist/ (ESM/UMD/IIFE)
  → build.sh → package/ (NPM-ready)
```

### Runtime Initialization
```
index.html
  → Loads config.js (EditorConfig)
  → Loads Shoelace components
  → WASM module loads
    → run() executes
      → Renders template
      → Sets up events
  → editor.js initializes
    → Toolbar/shortcuts setup
```

See `.docs/summary.md` section 13 for complete cross-reference map.

## Performance Targets
- **Markdown Conversion**: < 50ms (validated by benchmarks)
- **WASM Binary Size**: ~100KB (can optimize with LTO)
- **Initial Load**: Fast (WASM compiles/caches quickly)
- **Memory**: Minimal footprint (no large dependencies)

## Testing Checklist

Before submitting changes:
- [ ] `cargo fmt` — Rust formatting
- [ ] `cargo clippy -- -D warnings` — Rust linting
- [ ] `cargo test` — Unit tests pass
- [ ] `wasm-pack test --chrome` — Browser tests pass
- [ ] `cargo bench` — Benchmarks run (check for regressions)
- [ ] `trunk serve` — Dev build works
- [ ] `./build.sh` — Production build succeeds
- [ ] Test ESM, UMD, IIFE examples in `package/`
- [ ] Update `.docs/` if architecture changed
- [ ] Update WARP.md if commands/workflow changed

## Additional Context

**Shoelace Components**: https://shoelace.style/  
**wasm-bindgen Guide**: https://rustwasm.github.io/wasm-bindgen/  
**Trunk Documentation**: https://trunkrs.dev/  
**Vite Library Mode**: https://vitejs.dev/guide/build.html#library-mode

---

**Remember**: This project demonstrates best practices for Rust+WASM integration. Maintain the clean architecture, minimal dependencies, and performance-first approach when making changes.
