# Summary: scripts/build-dist.sh

## Purpose
Trunk build hook that bundles the distribution files.

## Key points
- Writes to its first argument, else `$TRUNK_STAGING_DIR`, else `target/trunk-dist`; never `./dist`.
- Bundles `config.js`, `chrome.js`, `editor.js`, `terraphim-editor.js` into `terraphim-editor.min.js`, the CSS into `terraphim-editor.min.css`, and an `example.html` linking FontAwesome with SRI.
