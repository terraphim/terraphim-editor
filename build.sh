#!/bin/bash

# Build the Rust WASM package with wasm-pack
wasm-pack build --target web

# Install npm dependencies if needed
npm install

# Build the package with Vite
npm run build

# Create package directory
rm -rf package
mkdir -p package/dist/{js,css,wasm}

# Copy build artifacts
cp -r dist/* package/dist/
cp -r public/wasm/* package/dist/wasm/
cp -r public/js/config.js package/dist/js/
cp README.md LICENSE package/

# Extract version from Cargo.toml
VERSION=$(grep '^version' Cargo.toml | head -1 | cut -d '"' -f 2)

# Create package.json for npm distribution
cat > package/package.json << EOL
{
  "name": "terraphim-editor",
  "version": "${VERSION}",
  "description": "WebAssembly-based Markdown editor component built with Rust",
  "main": "./dist/js/terraphim-editor.umd.js",
  "module": "./dist/js/terraphim-editor.mjs",
  "unpkg": "./dist/js/terraphim-editor.iife.js",
  "types": "./dist/js/terraphim-editor.d.ts",
  "files": ["dist", "README.md", "LICENSE"],
  "exports": {
    ".": {
      "types": "./dist/js/terraphim-editor.d.ts",
      "import": "./dist/js/terraphim-editor.mjs",
      "require": "./dist/js/terraphim-editor.umd.js",
      "default": "./dist/js/terraphim-editor.iife.js"
    },
    "./style.css": "./dist/css/terraphim-editor.css"
  },
  "peerDependencies": {
    "@shoelace-style/shoelace": "^2.12.0"
  }
}
EOL

# Create example files for each module format
cat > package/example-esm.html << EOL
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Terraphim Editor - ESM Example</title>
  <link rel="stylesheet" href="dist/css/terraphim-editor.css">
  <script type="importmap">
    {
      "imports": {
        "terraphim-editor": "./dist/js/terraphim-editor.mjs"
      }
    }
  </script>
</head>
<body>
  <div id="editor"></div>
  <script type="module">
    import { TeraphimEditor } from 'terraphim-editor';
    
    async function initEditor() {
      const editor = new TeraphimEditor({
        container: document.getElementById('editor'),
        wasmUrl: './dist/wasm/terraphim_editor_bg.wasm'
      });
      
      await editor.initialize();
    }
    
    initEditor().catch(console.error);
  </script>
</body>
</html>
EOL

cat > package/example-umd.html << EOL
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Terraphim Editor - UMD Example</title>
  <link rel="stylesheet" href="dist/css/terraphim-editor.css">
  <script src="dist/js/terraphim-editor.umd.js"></script>
</head>
<body>
  <div id="editor"></div>
  <script>
    async function initEditor() {
      const editor = new TeraphimEditor({
        container: document.getElementById('editor'),
        wasmUrl: './dist/wasm/terraphim_editor_bg.wasm'
      });
      
      await editor.initialize();
    }
    
    initEditor().catch(console.error);
  </script>
</body>
</html>
EOL

cat > package/example-iife.html << EOL
<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>Terraphim Editor - IIFE Example</title>
  <link rel="stylesheet" href="dist/css/terraphim-editor.css">
  <script src="dist/js/terraphim-editor.iife.js"></script>
</head>
<body>
  <div id="editor"></div>
  <script>
    async function initEditor() {
      const editor = new TeraphimEditor({
        container: document.getElementById('editor'),
        wasmUrl: './dist/wasm/terraphim_editor_bg.wasm'
      });
      
      await editor.initialize();
    }
    
    initEditor().catch(console.error);
  </script>
</body>
</html>
EOL

echo "Package built successfully in the 'package' directory"