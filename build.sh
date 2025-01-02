#!/bin/bash

# Build the Rust WASM package
wasm-pack build --target web

# Create necessary directories
mkdir -p public/wasm
mkdir -p public/js
mkdir -p public/css
mkdir -p package/js
mkdir -p package/wasm
mkdir -p package/css

# Copy WASM files to public directory for development
cp pkg/terraphim_editor_bg.wasm public/wasm/
cp pkg/terraphim_editor.js public/js/

# Copy config.js to public for development
cp public/js/config.js public/js/

# Install npm dependencies if needed
npm install

# Build the package with Vite
npm run build

# Copy additional files that Vite doesn't handle
cp pkg/terraphim_editor.js package/js/
cp public/js/config.js package/js/

# Copy documentation files
cp README.md package/
cp LICENSE package/

cp public/example.html package/

echo "Package built successfully in the 'package' directory" 