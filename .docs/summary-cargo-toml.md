# File Summary: Cargo.toml

## File Path
`Cargo.toml`

## Purpose
Rust package manifest defining project metadata, dependencies, build configuration, and WebAssembly compilation settings for the terraphim-editor WASM library.

## Key Configuration

### Package Metadata
- **Name**: `terraphim-editor`
- **Version**: `0.1.0`
- **Edition**: `2021` (Rust 2021 edition features)

### Library Configuration
```toml
[lib]
crate-type = ["cdylib", "rlib"]
```
- **`cdylib`**: Creates dynamic library for WebAssembly target
  - Essential for WASM compilation
  - Produces `.wasm` file for browser consumption
- **`rlib`**: Creates Rust library for native compilation
  - Enables `cargo test` and `cargo bench`
  - Allows linking in Rust-native contexts

## Dependencies

### Core WASM Dependencies
**`wasm-bindgen = "0.2.89"`**
- JavaScript/Rust FFI bindings
- Enables exporting Rust functions to JavaScript
- Provides `#[wasm_bindgen]` attribute macro
- Handles type conversions between Rust and JS

**`web-sys = { version = "0.3.66", features = [...] }`**
- Browser API bindings
- Feature flags enable specific web APIs:
  - **DOM APIs**: `Document`, `Element`, `HtmlElement`, `Node`, `Text`
  - **Form Elements**: `HtmlTextAreaElement`, `HtmlDivElement`
  - **Events**: `InputEvent`
  - **Window APIs**: `Window`, `Performance`
  - **Console**: `console` for logging

**`console_error_panic_hook = "0.1.7"`**
- Better panic messages in browser console
- Essential for debugging WASM code
- Converts Rust panics to JavaScript errors

### Functional Dependencies
**`markdown = "1.0.0-alpha.21"`**
- Core markdown-to-HTML conversion
- Alpha version indicates active development
- Provides `to_html_with_options()` API
- Configurable via `Options` struct

**`rinja = { version = "0.3.5" }`**
- Template rendering engine (Jinja2-like syntax)
- Compile-time template parsing
- Used for rendering `templates/editor.html`
- Type-safe template rendering

## Development Dependencies

**`wasm-bindgen-test = "0.3.39"`**
- WASM-specific test framework
- Enables browser-based integration tests
- Required for `tests/web.rs`

**`criterion = "0.5"`**
- Statistical benchmarking framework
- Used in `benches/markdown_bench.rs`
- Provides performance regression detection

## Benchmark Configuration

```toml
[[bench]]
name = "markdown_bench"
harness = false
```
- **`harness = false`**: Disables default benchmark harness
- Allows using Criterion framework
- Creates separate benchmark binary

## Build Implications

### WASM Target Build
```bash
cargo build --target wasm32-unknown-unknown --release
```
- Uses `cdylib` crate type
- Produces `terraphim_editor.wasm`
- Requires `wasm-bindgen` CLI for final processing

### Native Build
```bash
cargo build
```
- Uses `rlib` crate type
- Enables running tests and benchmarks
- Standard Rust compilation

### Feature Considerations
- No feature flags defined
- All web-sys features explicitly listed
- Prevents unnecessary code bloat
- Optimizes WASM binary size

## Dependency Version Strategy

### Stable vs Alpha
- **Stable**: wasm-bindgen, web-sys, console_error_panic_hook (production-ready)
- **Alpha**: markdown crate (1.0.0-alpha.21)
  - Potential breaking changes in future updates
  - May need migration to stable 1.0 when available

### Version Pinning
- Exact versions not pinned (allows patch updates)
- Uses caret requirements (`^`)
- Balances stability with security updates

## Integration with Build Tools

### wasm-pack Integration
- Respects `cdylib` crate type
- Generates JavaScript bindings automatically
- Creates `pkg/` directory with bundled artifacts

### Trunk Integration
- Trunk compiles WASM automatically
- Uses `Cargo.toml` for dependency resolution
- Serves compiled WASM during development

### Vite Integration
- Vite uses pre-built WASM from `pkg/` directory
- Cargo.toml not directly used by Vite
- Build pipeline: Cargo → wasm-pack → Vite

## Notable Patterns

### Dual Compilation Targets
- Single `Cargo.toml` supports both WASM and native
- `cdylib` + `rlib` enables testing native performance
- Allows benchmarking without WASM overhead

### Minimal Dependencies
- Only 6 dependencies (4 production, 2 dev)
- Reduces WASM bundle size
- Faster compilation times
- Smaller attack surface

## Security Considerations

### Dependency Audit
- All dependencies from crates.io
- Should run `cargo audit` regularly
- Alpha dependencies require extra scrutiny

### WASM Sandboxing
- cdylib restricts capabilities to browser sandbox
- No file system access
- No network access (unless via browser APIs)
- No process spawning

## Improvement Opportunities
- Consider pinning markdown crate to specific alpha version
- Add `[profile.release]` optimizations for WASM size reduction
- Could add `wasm-opt` configuration
- Consider adding `web-sys` features incrementally as needed
- Add `[package.metadata.wasm-pack]` configuration section
- Consider LTO (Link Time Optimization) for smaller WASM binaries
