# Terraphim Editor - Comprehensive Project Summary

**Last Updated**: 2025-10-23  
**Version**: 0.1.0

## 1. Project Overview

### Core Purpose
Terraphim Editor is a WebAssembly-based Markdown editor that leverages Rust's performance for markdown-to-HTML conversion while providing a modern, web-component-based UI. It demonstrates WASM integration patterns and serves as a distributable NPM package supporting multiple module formats.

### Technology Stack
- **Core Language**: Rust 2021 edition
- **Web Runtime**: WebAssembly (wasm32-unknown-unknown target)
- **UI Framework**: Shoelace web components (peer dependency)
- **Template Engine**: Rinja (Jinja2-like, compile-time templates)
- **Markdown Parser**: markdown crate (1.0.0-alpha.21)
- **Build Tools**: 
  - Development: Trunk (WASM-first bundler)
  - Distribution: wasm-pack + Vite (multi-format library bundler)
- **Testing**: wasm-bindgen-test (browser tests), Criterion (benchmarks)

### Key Features
1. **Live Markdown Preview**: Real-time conversion as you type
2. **WASM Performance**: Near-native speed markdown rendering
3. **Minimal Dependencies**: Pure Rust + Shoelace, no heavy JS frameworks
4. **Multi-Format Distribution**: ESM, UMD, and IIFE bundles
5. **Keyboard Shortcuts**: Ctrl+B (bold), Ctrl+I (italic), etc.
6. **Toolbar Commands**: Visual formatting buttons with Shoelace icons
7. **Resizable Split Pane**: Adjustable editor/preview layout

## 2. Architecture Analysis

### Dual Build System Architecture

The project employs two complementary build systems optimized for different use cases:

#### Development Workflow (Trunk-based)
```
Rust Code (src/lib.rs)
    ↓ (rustc + wasm-bindgen)
WASM Module
    ↓ (Trunk processes index.html)
Development Server (127.0.0.1:8080)
    ├─ Live Reload
    ├─ Auto-recompilation
    └─ Static Asset Serving
```

**Purpose**: Fast iteration, hot reloading, Rust-centric DX  
**Command**: `trunk serve`  
**Output**: `dist/` (ephemeral, for dev server only)

#### Distribution Workflow (wasm-pack + Vite)
```
Rust Code
    ↓ (wasm-pack build --target web)
pkg/
    ├─ terraphim_editor_bg.wasm
    ├─ terraphim_editor.js (bindings)
    └─ package.json
    ↓ (npm install; npm run build)
Vite Bundles (via vite.config.js)
    ├─ ESM (.mjs)
    ├─ UMD (.umd.js)
    └─ IIFE (.iife.js)
    ↓ (build.sh orchestrates)
package/
    ├─ dist/ (all formats + WASM + CSS)
    ├─ package.json (npm-ready)
    ├─ examples (ESM/UMD/IIFE demos)
    └─ README.md + LICENSE
```

**Purpose**: NPM publishing, multi-format support, CDN-ready  
**Command**: `./build.sh`  
**Output**: `package/` (ready for `npm publish`)

### Component Architecture

#### Rust/WASM Layer (`src/lib.rs`)
- **`run()` function**: Auto-executes on WASM load via `#[wasm_bindgen(start)]`
  - Initializes panic hook for better error messages
  - Renders Rinja template into `#editor-container`
  - Sets up event listeners for live markdown conversion
  
- **`render_markdown()` function**: Public API for JS-callable conversion
  - Takes markdown string
  - Returns HTML string or error
  - Used by JS wrapper for custom toolbar integration

- **`EditorTemplate` struct**: Rinja template data
  - `initial_content`: Welcome markdown
  - `initial_preview`: Pre-rendered HTML
  - Rendered to `templates/editor.html`

#### JavaScript Wrapper Layer (`public/js/terraphim-editor.js`)
- **`TeraphimEditor` class**: High-level API for library consumers
  - Constructor takes `{ container, config, wasmUrl }`
  - `async initialize()`: Loads WASM, renders template, applies config
  - `postSetup()`: Adds custom toolbar commands, injects initial content
  - `destroy()`: Cleanup method

#### Configuration Layer (`public/js/config.js`)
- **`EditorConfig` object**: Centralized settings
  - Keyboard shortcuts (ctrl+b, ctrl+i, etc.)
  - Toolbar commands (headings, formatting)
  - Initial markdown content
  - UI styling (split position, dimensions)

#### UI Layer (`templates/editor.html` + Shoelace)
- Split-pane layout with resizable divider
- Toolbar with Shoelace button group
- Monospace textarea for markdown input
- Preview pane with rendered HTML
- Keyboard shortcut help dialog

### Data Flow

```
User Types in Textarea
    ↓ (input event)
JavaScript Event Listener (setup_markdown_conversion in Rust)
    ↓ (calls)
markdown::to_html_with_options()
    ↓ (returns HTML string)
set_inner_html() on Preview Div
    ↓ (result)
Live Preview Updated
```

**Performance**: Synchronous, no debouncing (< 50ms for complex documents per benchmarks)

## 3. Build System Deep Dive

### Development Build (trunk serve)
1. **Compilation**: Rust → WASM (debug mode, fast compile)
2. **Template Processing**: Inlines `public/js/config.js` and `public/js/editor.js` via `data-trunk` directives
3. **Asset Serving**: Static files from `public/` served at root
4. **Live Reload**: WebSocket-based file watching
5. **Error Reporting**: Rust compiler errors in terminal, WASM errors in browser console

### Production Build (build.sh)
1. **`wasm-pack build --target web`**
   - Compiles Rust with release optimizations
   - Generates `pkg/` with WASM binary and JS bindings
   
2. **`npm install`**
   - Installs Vite and plugins (wasm, top-level-await)
   
3. **`npm run build`** (invokes Vite)
   - Bundles `public/js/terraphim-editor.js` in 3 formats
   - Copies WASM to output (via vite.config.js pre-build script)
   - Processes CSS assets
   - Generates sourcemaps
   
4. **Package Assembly**
   - Creates `package/` directory structure
   - Copies dist artifacts, WASM, CSS, README, LICENSE
   - Extracts version from `Cargo.toml`
   - Generates npm-compatible `package.json`
   - Creates example HTML files for each module format

### Build Artifacts

**Development (`dist/` via Trunk)**:
- Single-page app bundle
- Unoptimized WASM
- Inlined JavaScript
- Not suitable for distribution

**Production (`package/` via build.sh)**:
```
package/
├── dist/
│   ├── js/
│   │   ├── terraphim-editor.mjs       (ESM for bundlers)
│   │   ├── terraphim-editor.umd.js    (Universal module)
│   │   ├── terraphim-editor.iife.js   (Browser global)
│   │   ├── terraphim-editor.d.ts      (TypeScript types)
│   │   └── config.js                   (Configuration)
│   ├── css/
│   │   └── terraphim-editor.css        (Component styles)
│   └── wasm/
│       └── terraphim_editor_bg.wasm    (Compiled Rust)
├── package.json                         (NPM manifest)
├── README.md
├── LICENSE
└── example-{esm,umd,iife}.html         (Integration examples)
```

## 4. Core Functionality Analysis

### Markdown Conversion Engine
- **Library**: `markdown` crate (CommonMark-compliant)
- **Configuration**: `Options::default()` (extensible for custom rules)
- **Features Supported**:
  - Headings (H1-H6)
  - Text formatting (bold, italic, strikethrough)
  - Lists (ordered, unordered, nested)
  - Code blocks (fenced, with syntax highlighting hints)
  - Blockquotes
  - Links and images
  - Tables
  - Horizontal rules

### Live Preview System
- **Trigger**: Input event on textarea
- **Handler**: Rust closure with `.forget()` (persists for lifetime of page)
- **Conversion**: Synchronous, blocking
- **Update**: Direct `.set_inner_html()` on preview div
- **Performance**: < 50ms average for complex documents (per benchmarks)

### UI Interaction Patterns
- **Keyboard Shortcuts**: Event listener on textarea, `ctrl+key` combinations
- **Toolbar Buttons**: Click handlers that wrap selected text with markdown syntax
- **Text Wrapping**: Preserves selection, adds prefix/suffix, maintains cursor position
- **Command Palette**: Slash (/) key triggers inline formatting menu (incomplete feature)
- **Split Pane**: Shoelace component with drag-to-resize

## 5. Security Analysis

### Input Handling
⚠️ **XSS Vulnerability**: No input sanitization before `.set_inner_html()`
- **Risk**: Malicious markdown could inject `<script>` tags
- **Mitigation**: Should use DOMPurify or similar sanitizer
- **Current State**: Trusts markdown library's output implicitly

### DOM Manipulation
- Direct innerHTML updates (fast but risky)
- No Content Security Policy (CSP) headers configured
- Shoelace components use Shadow DOM (provides some isolation)

### WASM Security
✅ **Sandboxed Execution**: WASM runs in browser's sandbox
- No file system access
- No network access (unless via browser APIs)
- No access to OS-level resources
- Memory safe (Rust's ownership system)

### Dependency Security
- 4 production dependencies (minimal attack surface)
- Alpha version of markdown crate (needs monitoring)
- Should run `cargo audit` regularly
- Shoelace is peer dependency (consumer controls version)

## 6. Testing Strategy

### Unit Tests (`src/lib.rs::tests`)
- **Scope**: Markdown conversion logic
- **Coverage**: Basic conversion, template rendering
- **Execution**: `cargo test`
- **Environment**: Native Rust (not WASM)

### Browser Integration Tests (`tests/web.rs`)
- **Framework**: wasm-bindgen-test
- **Scope**: Editor initialization, DOM manipulation, event handling
- **Test Cases**:
  1. Editor elements render correctly
  2. Markdown conversion works end-to-end
  3. Performance < 50ms per conversion (100 iterations)
- **Execution**: `wasm-pack test --chrome`
- **Environment**: Real browser (Chrome, Firefox, Safari)

### Performance Benchmarks (`benches/markdown_bench.rs`)
- **Framework**: Criterion
- **Scope**: Isolated markdown conversion performance
- **Metrics**: Mean time, throughput, standard deviation, outliers
- **Execution**: `cargo bench`
- **Output**: Console summary + HTML report in `target/criterion/`
- **Purpose**: Baseline performance, regression detection

### Test Coverage Gaps
- ❌ No accessibility testing (ARIA labels, keyboard nav)
- ❌ No cross-browser compatibility validation
- ❌ No error handling path tests
- ❌ No edge case testing (empty input, huge documents, malformed markdown)
- ❌ No visual regression testing
- ❌ No memory leak detection

## 7. Performance Considerations

### WASM Advantages
- **Near-native Speed**: Markdown parsing in compiled Rust
- **Predictable Performance**: No GC pauses
- **Small Bundle Size**: ~100KB WASM (vs. ~500KB+ for JS markdown libraries)
- **Parallel Potential**: Could use Web Workers for large documents (not currently implemented)

### Performance Bottlenecks
1. **No Debouncing**: Converts on every keystroke (acceptable for documents < 10KB)
2. **Synchronous Rendering**: Blocks UI thread during conversion
3. **Full Re-render**: No incremental/diff-based updates
4. **DOM Manipulation**: innerHTML replacement is fast but could use virtual DOM

### Optimization Opportunities
- Add debouncing (e.g., 150ms delay after last keystroke)
- Use `requestAnimationFrame()` for smoother updates
- Implement incremental parsing (only re-parse changed sections)
- Use Web Workers for documents > 100KB
- Add WASM memory pooling
- Enable WASM SIMD for faster parsing
- Use Rust's Release Profile optimizations:
  ```toml
  [profile.release]
  lto = true
  codegen-units = 1
  opt-level = "z"  # Optimize for size
  ```

## 8. Development Workflow

### Prerequisites
```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Add WASM target
rustup target add wasm32-unknown-unknown

# 3. Install Trunk
cargo install trunk

# 4. Install wasm-pack (for distribution builds)
cargo install wasm-pack
```

### Common Commands

**Development**:
```bash
trunk serve                    # Start dev server on http://127.0.0.1:8080
trunk serve --port 3000        # Custom port
trunk serve --open             # Open browser automatically
```

**Testing**:
```bash
cargo test                                  # Run Rust unit tests
wasm-pack test --chrome                    # Run browser tests (Chrome)
wasm-pack test --firefox                   # Run browser tests (Firefox)
cargo bench                                 # Run performance benchmarks
```

**Building**:
```bash
trunk build --release          # Production build (dev workflow)
./build.sh                     # NPM package build (distribution)
npm run build                  # Vite-only build (if WASM already compiled)
```

**Linting & Formatting**:
```bash
cargo fmt                      # Format Rust code
cargo clippy                   # Lint Rust code
cargo clippy -- -D warnings    # Fail on warnings
```

### Debugging

**Rust/WASM Debugging**:
- `console_error_panic_hook` provides stack traces in browser console
- Use `web_sys::console::log_1()` for debug logging
- Browser DevTools → Sources → WASM modules (limited debugging)

**JavaScript Debugging**:
- Standard browser DevTools
- Sourcemaps enabled (via Vite config)
- Console logging in wrapper code

**Performance Profiling**:
- Chrome DevTools → Performance tab
- `cargo bench` for isolated Rust performance
- Browser Performance API in `tests/web.rs::bench_markdown_conversion_in_browser`

## 9. Distribution & Integration

### NPM Package Structure
Generated by `build.sh`, ready for publishing:
```json
{
  "name": "terraphim-editor",
  "version": "0.1.0",  // Synced from Cargo.toml
  "main": "./dist/js/terraphim-editor.umd.js",
  "module": "./dist/js/terraphim-editor.mjs",
  "unpkg": "./dist/js/terraphim-editor.iife.js",
  "exports": {
    ".": {
      "types": "./dist/js/terraphim-editor.d.ts",
      "import": "./dist/js/terraphim-editor.mjs",
      "require": "./dist/js/terraphim-editor.umd.js"
    },
    "./style.css": "./dist/css/terraphim-editor.css"
  },
  "peerDependencies": {
    "@shoelace-style/shoelace": "^2.12.0"
  }
}
```

### Integration Examples

**ESM (Modern Bundlers)**:
```javascript
import { TeraphimEditor } from 'terraphim-editor';
import 'terraphim-editor/style.css';

const editor = new TeraphimEditor({
  container: document.getElementById('editor'),
  wasmUrl: '/path/to/terraphim_editor_bg.wasm'
});
await editor.initialize();
```

**UMD (Node.js / RequireJS)**:
```javascript
const { TeraphimEditor } = require('terraphim-editor');
// Works in CommonJS environments
```

**IIFE (Direct Browser)**:
```html
<script src="terraphim-editor.iife.js"></script>
<script>
  const editor = new TeraphimEditor({...});
  editor.initialize();
</script>
```

### CDN Usage
```html
<script src="https://unpkg.com/terraphim-editor"></script>
<link rel="stylesheet" href="https://unpkg.com/terraphim-editor/style.css">
```

## 10. Business Value & Use Cases

### Target Use Cases
1. **Embedded Markdown Editors**: Add markdown editing to existing web apps
2. **Documentation Platforms**: Real-time markdown preview for docs
3. **CMS Systems**: Lightweight, performant content editing
4. **Note-Taking Apps**: Fast, offline-capable markdown editing
5. **Educational Tools**: Teach markdown syntax with live feedback

### Performance Benefits
- **Fast Initial Load**: WASM compiles faster than parsing large JS libraries
- **Smooth Typing Experience**: Conversion latency < 50ms
- **Scales to Large Documents**: Near-native performance vs. JavaScript alternatives
- **Offline-First**: WASM cached by browser, no network dependency

### Extensibility Points
- **Custom Markdown Options**: Configure markdown crate via `Options`
- **Custom Toolbar Commands**: Add via `config.commands` array
- **Custom Keyboard Shortcuts**: Add via `config.shortcuts` array
- **Custom Styling**: Override CSS variables, Shoelace theme customization
- **WASM Function Exports**: Extend `src/lib.rs` with new `#[wasm_bindgen]` functions

## 11. Technical Debt & Improvements

### Critical Issues
1. **XSS Vulnerability**: No input sanitization (see Security Analysis)
2. **Memory Leak**: Event handler closure leaked with `.forget()` - no cleanup
3. **Error Handling**: No user-facing error messages when conversion fails
4. **Command Palette**: Incomplete implementation (shows menu, but integration partial)

### Code Quality Issues
- Hard-coded CSS selector strings (should be constants)
- No TypeScript definitions (only placeholder `.d.ts`)
- Inconsistent error handling (some use `?`, some use `.expect()`)
- No logging framework (uses raw `console::log`)

### Missing Features
- ✓ Keyboard shortcuts (implemented)
- ✓ Toolbar commands (implemented)
- ⚠️ Command palette (partially implemented)
- ❌ Undo/redo
- ❌ Find/replace
- ❌ Spell check
- ❌ Export to PDF/HTML
- ❌ Markdown syntax highlighting in editor
- ❌ Collaborative editing
- ❌ Dark mode
- ❌ Mobile responsiveness
- ❌ Accessibility (ARIA labels, screen reader support)

### Performance Improvements
- Add input debouncing
- Implement incremental rendering
- Use Web Workers for large documents
- Add virtual scrolling for preview pane
- Optimize WASM binary size with LTO

### Testing Improvements
- Add E2E tests (Playwright, Cypress)
- Add visual regression tests
- Add accessibility audit (axe-core)
- Add cross-browser CI testing
- Add memory leak detection
- Add load testing (handle 1MB+ documents)

## 12. File Organization & Conventions

### Directory Structure
```
terraphim-editor/
├── src/                    # Rust source code
│   └── lib.rs             # Main library (WASM entry point)
├── tests/                  # Browser integration tests
│   └── web.rs
├── benches/                # Performance benchmarks
│   └── markdown_bench.rs
├── templates/              # Rinja templates
│   └── editor.html
├── public/                 # Static assets (dev + examples)
│   ├── js/
│   │   ├── config.js      # Editor configuration
│   │   ├── editor.js      # Standalone editor class
│   │   ├── terraphim-editor.js  # Library wrapper
│   │   └── terraphim_editor.js  # Generated WASM bindings
│   ├── css/
│   │   ├── terraphim-editor.css  # Component styles
│   │   └── styles.css     # Example page styles
│   ├── wasm/
│   │   └── terraphim_editor_bg.wasm  # Compiled WASM
│   └── example-*.html     # Integration examples
├── dist/                   # Build output (Trunk + Vite)
├── pkg/                    # wasm-pack output
├── package/                # NPM-ready distribution
├── Cargo.toml              # Rust project config
├── Trunk.toml              # Trunk build config
├── package.json            # NPM project config
├── vite.config.js          # Vite bundler config
├── build.sh                # Production build script
└── index.html              # Trunk development entry point
```

### Naming Conventions
- **Rust**: snake_case for functions/variables, PascalCase for types
- **JavaScript**: camelCase for functions/variables, PascalCase for classes
- **Files**: kebab-case for multi-word files
- **Components**: Prefixed with `terraphim-` for disambiguation
- **Build Artifacts**: Predictable naming (`.mjs`, `.umd.js`, `.iife.js`)

### Code Organization Principles
1. **Separation of Concerns**: Rust handles computation, JS handles UI orchestration
2. **Configuration Over Code**: `config.js` centralizes settings
3. **Dual Build Targets**: Development (Trunk) vs. Distribution (wasm-pack + Vite)
4. **Minimal Dependencies**: Only essential crates/packages
5. **Example-Driven**: Each module format has working example

## 13. Cross-Reference Map

### Key Interaction Flows

#### Editor Initialization Flow
```
index.html
  → Loads config.js (EditorConfig global)
  → Loads Shoelace components
  → Trunk inlines editor.js
  → WASM module loads
    → src/lib.rs::run() auto-executes
      → Renders templates/editor.html via Rinja
        → Inserts into #editor-container
      → Calls setup_markdown_conversion()
        → Attaches input event listener
  → editor.js::MarkdownEditor initializes
    → Sets up toolbar buttons
    → Sets up keyboard shortcuts
    → Sets up command palette
```

#### Markdown Conversion Flow
```
User types in textarea
  → Input event fires
    → Rust closure (setup_markdown_conversion)
      → Reads textarea.value
      → Calls markdown::to_html_with_options()
        → Returns HTML string
      → Calls preview.set_inner_html(html)
  → Preview pane updates
```

#### Build Pipeline Flow
```
Rust Code (src/lib.rs)
  ↓
wasm-pack build
  ↓
pkg/ artifacts
  ↓ (vite.config.js copies to public/)
public/wasm/ + public/js/
  ↓
npm run build (Vite)
  ↓
dist/ (ESM, UMD, IIFE bundles)
  ↓
build.sh assembles
  ↓
package/ (NPM-ready)
```

### File Dependencies

**Core Dependencies**:
- `src/lib.rs` → `templates/editor.html` (Rinja rendering)
- `src/lib.rs` → `Cargo.toml` (dependencies)
- `index.html` → `Trunk.toml` (build config)
- `index.html` → `public/js/config.js` (editor config)
- `index.html` → `public/js/editor.js` (UI logic)

**Build Dependencies**:
- `vite.config.js` → `package.json` (dependencies)
- `build.sh` → `Cargo.toml` (version extraction)
- `build.sh` → `pkg/` (WASM artifacts from wasm-pack)
- `build.sh` → `dist/` (Vite output)

**Test Dependencies**:
- `tests/web.rs` → `src/lib.rs` (editor module)
- `benches/markdown_bench.rs` → `markdown` crate (direct)

### API Surface

**Rust → JavaScript**:
- `run()`: Auto-initialization function
- `render_markdown(input: &str) -> Result<String, JsValue>`: Public conversion API

**JavaScript → Rust**:
- Not applicable (one-way binding, JS calls Rust but not vice versa)

**JavaScript Public API**:
- `class TeraphimEditor`: Main library interface
  - `constructor({ container, config, wasmUrl })`
  - `async initialize()`: Loads WASM and renders editor
  - `destroy()`: Cleanup method

**Configuration API**:
- `window.EditorConfig`: Global configuration object
  - `shortcuts`: Array of keyboard shortcut definitions
  - `commands`: Array of toolbar command definitions
  - `initialContent`: Default markdown text
  - `styles`: Layout configuration

---

## Related Documentation

For detailed information on specific files, see individual summaries in `.docs/summary-*.md`:

- **Rust Sources**: `summary-src-lib-rs.md`, `summary-tests-web-rs.md`, `summary-benches-markdown-bench-rs.md`
- **Configuration**: `summary-cargo-toml.md`, `summary-trunk-toml.md`, `summary-package-json.md`, `summary-vite-config-js.md`, `summary-build-sh.md`
- **Templates**: `summary-templates-editor-html.md`
- **JavaScript**: `summary-public-js-config-js.md` (others to be added)

---

**Maintained by**: Project contributors  
**Update Frequency**: On major architectural changes or new feature additions  
**Feedback**: Create an issue or submit a PR to improve this documentation
