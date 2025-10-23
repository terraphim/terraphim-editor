# File Summary: Trunk.toml

## File Path
`Trunk.toml`

## Purpose
Configuration file for Trunk, a WASM web application bundler specifically designed for Rust+WASM projects. Defines build settings, development server configuration, and static file handling for the terraphim-editor development workflow.

## Key Configuration

### Build Settings
```toml
[build]
target = "index.html"
dist = "dist"
```
- **`target`**: Entry point HTML file that Trunk processes
  - Trunk scans `index.html` for `data-trunk` attributes
  - Automatically compiles referenced Rust/WASM code
  - Injects compiled assets into HTML

- **`dist`**: Output directory for built assets
  - Development builds go here during `trunk serve`
  - Production builds created with `trunk build --release`
  - Contains compiled WASM, JavaScript bindings, and processed HTML

### Development Server
```toml
[serve]
address = "127.0.0.1"
port = 8080
```
- **`address`**: Binds dev server to localhost only
  - Prevents external access during development
  - Security best practice for local development

- **`port`**: Default port for development server
  - Access via `http://127.0.0.1:8080`
  - Can be overridden with `--port` CLI flag

### Static File Headers
```toml
[serve.static_file_headers]
"**/*.js" = { "Content-Type" = "application/javascript" }
"**/*.css" = { "Content-Type" = "text/css" }
```
- **Purpose**: Ensures correct MIME types for static assets
- **JavaScript files**: Explicitly set as `application/javascript`
  - Critical for ES modules to load correctly
  - Prevents MIME type mismatch errors in browser

- **CSS files**: Explicitly set as `text/css`
  - Ensures stylesheets load properly
  - Prevents browser warnings

- **Glob patterns**: `**/*` matches files recursively in all subdirectories

## Integration with Development Workflow

### Trunk Serve (Development Mode)
```bash
trunk serve
```
**What happens**:
1. Compiles Rust code to WASM (`wasm32-unknown-unknown` target)
2. Processes `index.html` and resolves `data-trunk` directives
3. Copies static assets from `public/` directory
4. Starts development server on `127.0.0.1:8080`
5. Watches for file changes and auto-recompiles
6. Provides WebSocket for live reload

### Trunk Build (Production Mode)
```bash
trunk build --release
```
**What happens**:
1. Compiles WASM with optimizations
2. Minifies JavaScript and CSS
3. Generates production-ready assets in `dist/`
4. No dev server started

## How Trunk Processes index.html

### data-trunk Directives
Trunk scans for special attributes in HTML:
```html
<link data-trunk rel="inline" href="public/js/config.js" />
<link data-trunk rel="inline" href="public/js/editor.js" />
```

- **`rel="inline"`**: Inlines file content directly into HTML
- **`rel="copy"`**: Copies file to dist directory
- **`rel="rust"`**: Compiles Rust/WASM (auto-detected)

### Automatic WASM Injection
- Trunk automatically:
  - Detects Rust project via `Cargo.toml`
  - Compiles to WASM
  - Generates JavaScript bindings
  - Injects `<script>` tags into HTML
  - Sets up module initialization

## Development Experience Benefits

### Live Reload
- File watching for Rust, HTML, CSS, JS changes
- Automatic recompilation on save
- Browser auto-refresh via WebSocket
- Fast incremental rebuilds

### Hot Module Replacement
- WASM modules reload without full page refresh (when possible)
- Preserves application state
- Faster iteration cycle

### Error Reporting
- Rust compilation errors displayed in terminal
- Browser console shows WASM errors
- Clear error messages for debugging

## Comparison with Other Tools

### vs wasm-pack
- **Trunk**: Full development server + bundler
- **wasm-pack**: Just WASM compilation + NPM packaging
- **Use case**: Trunk for development, wasm-pack for NPM distribution

### vs Vite
- **Trunk**: Rust-native, WASM-first design
- **Vite**: JavaScript-centric, WASM as plugin
- **Current project**: Uses both (Trunk for dev, Vite for distribution)

## Notable Patterns

### Minimal Configuration
- Only essential settings configured
- Relies on sensible defaults
- Reduces configuration maintenance

### Security by Default
- Localhost-only binding
- No external access during development
- Prevents accidental exposure

## Limitations

### No Custom Build Scripts
- Can't run arbitrary commands during build
- Limited to Trunk's built-in functionality
- `build.sh` script handles complex packaging separately

### Single Entry Point
- Only one `target` HTML file
- Multi-page apps require workarounds
- Not designed for complex SPA routing

### Static Asset Handling
- Assets in `public/` copied automatically
- Limited transformation capabilities
- Complex asset pipelines may need separate tooling

## Integration with Project Build System

### Dual Build System
The project uses two parallel build systems:
1. **Trunk** (for development):
   - `trunk serve` for live development
   - Fast iteration
   - No distribution packaging

2. **Vite + wasm-pack** (for distribution):
   - `build.sh` script orchestrates
   - Creates NPM-ready package
   - Multiple module formats (ESM, UMD, IIFE)

### Why Both?
- Trunk excels at Rust/WASM development
- Vite excels at JavaScript distribution
- Separate tools for separate concerns

## Improvement Opportunities
- Could add `[watch]` configuration to exclude certain paths
- Could configure `[clean]` settings for dist directory management
- Could add `[build.public_url]` for CDN deployment
- Could configure `[serve.ws]` for custom WebSocket settings
- Could add `[serve.no_autoreload]` for specific use cases
