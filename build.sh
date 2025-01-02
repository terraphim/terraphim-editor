#!/bin/bash

# Build the Rust WASM package
wasm-pack build --target web

# Install npm dependencies if needed
npm install

# Build the package with Vite
npm run build

# Copy documentation and example files
mkdir -p package
cp README.md package/
cp LICENSE package/
cp public/example-esm.html package/
cp public/example-umd.html package/
cp public/example-iife.html package/
# Create package.json for npm distribution
cat > package/package.json << EOL
{
  "name": "terraphim-editor",
  "version": "$(grep '^version' Cargo.toml | cut -d '"' -f 2)",
  "description": "WebAssembly-based Markdown editor component built with Rust",
  "files": [
    "js/",
    "wasm/",
    "css/"
  ],
  "main": "js/terraphim_editor.js",
  "types": "js/terraphim_editor.d.ts"
}
EOL

echo "Package built successfully in the 'package' directory"