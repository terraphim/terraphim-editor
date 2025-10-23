# File Summary: package.json

## File Path
`package.json`

## Purpose
JavaScript package manifest for building and distributing the editor as a consumable library via Vite. Defines build scripts, output formats (ESM/UMD/IIFE), and peer dependencies for UI components.

## Key Fields
- `name`: `terraphim-editor`
- `version`: `1.0.0`
- `type`: `module` (enables ESM semantics)
- `files`: Publish whitelist: `dist`, `README.md`, `LICENSE`
- Entry Points:
  - `main`: `./dist/js/terraphim-editor.umd.js` (Common entry for Node/UMD)
  - `module`: `./dist/js/terraphim-editor.mjs` (ESM import)
  - `unpkg`: `./dist/js/terraphim-editor.iife.js` (CDN/global)
  - `types`: `./dist/js/terraphim-editor.d.ts` (TypeScript types)
- `exports` map:
  - `.`: Conditional exports for `types`/`import`/`require`/`default`
  - `./style.css`: Exposes CSS entry `./dist/css/terraphim-editor.css`

## Scripts
- `dev`: `vite` (starts Vite dev server; used for iterating on JS wrapper and examples)
- `build`: `vite build` (produces library bundles in `dist/`)
- `preview`: `vite preview`
- `prepublishOnly`: `npm run build` (ensures bundles are built before publishing)

## Dev Dependencies
- `vite` `^5.0.10` — bundler/build tool
- `vite-plugin-wasm` `^3.3.0` — supports importing WASM modules
- `vite-plugin-top-level-await` `^1.4.1` — enables top-level await usage for WASM init

## Peer Dependencies
- `@shoelace-style/shoelace` `^2.12.0` — UI component library used by examples and editor toolbar
  - Declared as peer to avoid bundling and to let host app control version

## Role in Build System
- Complements Rust/WASM build by bundling JS wrapper (`public/js/terraphim-editor.js`) into multiple formats
- Works alongside `build.sh` to assemble npm-ready package structure in `package/`
- Not responsible for compiling Rust to WASM (handled by wasm-pack/Trunk)

## Output Artifacts
- `dist/js/terraphim-editor.{mjs,umd.js,iife.js}` — library builds
- `dist/js/terraphim-editor.d.ts` — types
- `dist/css/terraphim-editor.css` — styles via asset pipeline
- `dist/wasm/*` — WASM copied via rollup asset config

## Integration Notes
- Consumers can import as ESM, require as UMD, or load via IIFE on CDN
- CSS exposed as `terraphim-editor/style.css` for straightforward theming
- Requires Shoelace to be provided by consumer (peer dependency)
