#!/bin/bash

# Ensure we're in the project root
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Must be run from project root"
    exit 1
fi

# Clean previous builds
rm -rf package/
mkdir -p package/

# Build the project
echo "Building project..."
trunk build --release

# Create package structure
mkdir -p package/js
mkdir -p package/wasm
mkdir -p package/css

# Copy built files
echo "Packaging files..."
cp dist/*.wasm package/wasm/
cp dist/*.js package/js/
cp dist/*.css package/css/

# Create config.js
cat > package/js/config.js << EOL
window.TerraphimConfig = {
    wasmPath: '../wasm/terraphim_editor_bg.wasm',
    cssPath: '../css/terraphim_editor.css'
};
EOL

# Create package.json for npm distribution
cat > package/package.json << EOL
{
  "name": "terraphim-editor",
  "version": "$(grep '^version' Cargo.toml | cut -d '"' -f 2)",
  "description": "WebAssembly-based Markdown editor built with Rust",
  "files": [
    "js/",
    "wasm/",
    "css/"
  ],
  "main": "js/terraphim_editor.js",
  "types": "js/terraphim_editor.d.ts"
}
EOL

echo "Package created in ./package directory"