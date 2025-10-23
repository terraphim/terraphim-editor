# File Summary: build.sh

## File Path
`build.sh`

## Purpose
Production build orchestration script that coordinates the full build pipeline: compiling Rust to WASM, bundling JavaScript, and assembling an NPM-ready package with examples for all module formats.

## Build Pipeline

### Step 1: WASM Compilation
```bash
wasm-pack build --target web
```
- Compiles Rust to WASM for web target
- Generates `pkg/` directory with:
  - `terraphim_editor_bg.wasm` (compiled module)
  - `terraphim_editor.js` (JavaScript bindings)
  - `package.json` (auto-generated)

### Step 2: NPM Dependencies
```bash
npm install
```
- Installs Vite and plugins
- Required for JavaScript bundling step

### Step 3: Vite Build
```bash
npm run build
```
- Runs `vite build` (configured in `vite.config.js`)
- Produces multi-format library bundles in `dist/`
- Creates ESM, UMD, and IIFE variants

### Step 4: Package Assembly
```bash
rm -rf package
mkdir -p package/dist/{js,css,wasm}
```
- Cleans previous package output
- Creates directory structure for npm package

### Step 5: Artifact Copying
```bash
cp -r dist/* package/dist/
cp -r public/wasm/* package/dist/wasm/
cp -r public/js/config.js package/dist/js/
cp -r public/css/terraphim-editor.css package/dist/css/
cp README.md LICENSE package/
```
- Consolidates all build artifacts into `package/`
- Includes WASM binaries, JS bundles, CSS, and metadata files

### Step 6: Package.json Generation
```bash
VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d '"' -f 2)
cat > package/package.json << EOL
...
EOL
```
- Extracts version from `Cargo.toml` (single source of truth)
- Generates npm-compatible `package.json`
- Defines entry points for all module formats
- Specifies exports map and peer dependencies

### Step 7: Example File Generation
Creates three example HTML files demonstrating different module formats:
- `example-esm.html` — ES module with import map
- `example-umd.html` — Universal module with global variable
- `example-iife.html` — Immediately-invoked function expression

Each example shows:
- How to include the library
- How to initialize the editor
- Container setup
- WASM URL configuration

## Output Structure
```
package/
├── dist/
│   ├── js/
│   │   ├── terraphim-editor.mjs (ESM)
│   │   ├── terraphim-editor.umd.js (UMD)
│   │   ├── terraphim-editor.iife.js (IIFE)
│   │   ├── terraphim-editor.d.ts (TypeScript)
│   │   └── config.js
│   ├── css/
│   │   └── terraphim-editor.css
│   └── wasm/
│       └── terraphim_editor_bg.wasm
├── package.json
├── README.md
├── LICENSE
└── example-*.html (3 files)
```

## Key Features

### Version Synchronization
- Single version source in `Cargo.toml`
- Automatically propagated to npm package
- Prevents version mismatch between Rust and JS

### Module Format Support
- **ESM**: Modern bundlers (Webpack, Rollup, Vite)
- **UMD**: Node.js, RequireJS, browser globals
- **IIFE**: Direct CDN/script tag usage

### Complete Examples
- Each module format has working example
- Shows integration best practices
- Demonstrates async initialization pattern

## Integration Notes
- Run after code changes to prepare for publishing
- Output in `package/` ready for `npm publish`
- Examples can be tested directly in browser

## Error Handling
- No explicit error checking (relies on command failures)
- Commands fail fast (bash default behavior)
- Should add `set -e` for explicit fail-fast

## Improvement Opportunities
- Add `set -e` for strict error handling
- Add `set -x` option for debugging
- Validate `pkg/` existence before copying
- Add checksum/integrity verification
- Generate TypeScript declaration files
- Add build artifact size reporting
- Implement incremental builds (skip unchanged steps)
