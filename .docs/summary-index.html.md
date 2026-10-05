# Summary: index.html

## Purpose
Trunk entry page.

## Key points
- Loads Shoelace from the CDN and FontAwesome 6.5.2 from cdnjs with an SRI hash.
- Trunk links: the Rust crate, `public/styles.css`, `public/css/tokens.css`, `public/css/write-on.css`.
- Inline scripts in order: `config.js`, `chrome.js`, `editor.js`; `window.wasmBindings` exposes the Rust exports.
