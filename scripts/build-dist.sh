#!/bin/bash

# Writes the embeddable bundle (issue #77): terraphim-editor.min.js,
# terraphim-editor.min.css, the WebAssembly module under fixed names
# (terraphim_editor.js, terraphim_editor_bg.wasm) and example.html.
#
# Output directory, in order of precedence:
#   1. first argument:       scripts/build-dist.sh some/dir
#   2. $TRUNK_STAGING_DIR:   set by Trunk for build hooks, so the files ship
#                            with whichever dist Trunk is producing
#   3. target/trunk-dist:    manual runs; never the committed ./dist
# The committed release copy is regenerated only by scripts/release-dist.sh.
#
# Trunk runs this as a post_build hook (Trunk.toml), after the Rust
# pipeline has written the hashed wasm-bindgen output
# (terraphim-editor-<hash>.js and terraphim-editor-<hash>_bg.wasm) into the
# staging directory; those are copied to the fixed names the embed wrapper
# loads by default. A manual run with no Trunk output in the directory
# writes the bundle without the module and says so.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-${TRUNK_STAGING_DIR:-target/trunk-dist}}"
mkdir -p "$OUT"

# Copy and minify JS files
# Order matters: config, then the Write_On chrome, the decoration
# registry/inline indicators, the ghost layer and selection menu, the trim
# levels and the Lab popover that hosts them, the Blocks view, the
# alternatives panel and the Overflow panel, save/open and the Markdown
# export view, then the editor that
# instantiates them, then the embeddable wrapper.
terser public/js/config.js public/js/chrome.js public/js/indicators.js public/js/selection-menu.js public/js/trim.js public/js/lab.js public/js/blocks.js public/js/alternatives-panel.js public/js/overflow.js public/js/persistence.js public/js/editor.js public/js/terraphim-editor.js -o "$OUT/terraphim-editor.min.js"

# Copy and minify CSS
# The embed base styles (scoped to the editor), then design tokens before
# the Write_On layout, chrome, selection menu, Lab popover, trim card,
# Blocks view, alternatives panel, Overflow panel and the save/open notice
# and dialogs that consume them.
cleancss public/styles.css public/css/tokens.css public/css/write-on.css public/css/selection-menu.css public/css/lab.css public/css/trim.css public/css/blocks.css public/css/alternatives-panel.css public/css/overflow.css public/css/persistence.css -o "$OUT/terraphim-editor.min.css"

# The WebAssembly module under fixed names (Trunk hashes its own copies).
# The newest hashed pair wins should an old one still be lying around.
shopt -s nullglob
wasm_files=("$OUT"/terraphim-editor-*_bg.wasm)
if [ "${#wasm_files[@]}" -gt 0 ]; then
  wasm="$(ls -t "${wasm_files[@]}" | head -n 1)"
  glue="${wasm%_bg.wasm}.js"
  if [ ! -f "$glue" ]; then
    echo "build-dist: $glue missing next to $wasm" >&2
    exit 1
  fi
  cp "$wasm" "$OUT/terraphim_editor_bg.wasm"
  cp "$glue" "$OUT/terraphim_editor.js"
else
  echo "build-dist: no terraphim-editor-*_bg.wasm in $OUT (run trunk build); bundle written without the module" >&2
fi
shopt -u nullglob

# Create example usage file
cat > "$OUT/example.html" << 'EOL'
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Terraphim Editor embed example</title>
  <!-- The editor's chrome uses FontAwesome icons; the host page loads them. -->
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.2/css/all.min.css" integrity="sha512-SnH5WK+bZxgPHs44uWIX+LLJAJ9/2PkPKZ5QiAj6Ta86w+fsb2TkcmfRyVX3pBnMFcV7oQPJkl9QevSCWr3W6A==" crossorigin="anonymous" referrerpolicy="no-referrer">
  <link rel="stylesheet" href="terraphim-editor.min.css">
  <style>
    body { margin: 0; font-family: system-ui, sans-serif; }
    /* The word counter (the Write_On toggle) sits in the viewport's top-left
       corner, as on the full page; leave it a 20px band. */
    header { padding: 1.5rem 1.5rem 0.75rem; border-bottom: 1px solid #ddd; }
    header h1 { margin: 0; font-size: 1rem; }
    /* Give the host element a height; the editor fills it. */
    #editor { height: calc(100vh - 4.25rem); padding: 1rem 1rem 0; box-sizing: border-box; }
  </style>
</head>
<body>
  <header><h1>A host page with one container</h1></header>
  <div id="editor"></div>
  <!-- terraphim_editor.js and terraphim_editor_bg.wasm are found next to this script. -->
  <script src="terraphim-editor.min.js"></script>
  <script type="module">
    const editor = await TeraphimEditor.create(document.getElementById('editor'), {
      value: '# Hello from an embedded editor\n\nType here. Click the word counter for Write_On mode.\n',
    });
    window.editor = editor;
  </script>
</body>
</html>
EOL
