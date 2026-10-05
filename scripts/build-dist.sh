#!/bin/bash

# Writes the embeddable bundle (min.js, min.css, example.html).
#
# Output directory, in order of precedence:
#   1. first argument:       scripts/build-dist.sh some/dir
#   2. $TRUNK_STAGING_DIR:   set by Trunk for build hooks, so the files ship
#                            with whichever dist Trunk is producing
#   3. target/trunk-dist:    manual runs; never the committed ./dist
# The committed release copy is regenerated only by scripts/release-dist.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

OUT="${1:-${TRUNK_STAGING_DIR:-target/trunk-dist}}"
mkdir -p "$OUT"

# Copy and minify JS files
# Order matters: config, then the Write_On chrome and the decoration
# registry/inline indicators, the ghost layer and selection menu, then the
# editor that instantiates them, then the embeddable wrapper.
terser public/js/config.js public/js/chrome.js public/js/indicators.js public/js/selection-menu.js public/js/editor.js public/js/terraphim-editor.js -o "$OUT/terraphim-editor.min.js"

# Copy and minify CSS
# Design tokens before the Write_On layout and chrome that consume them.
cleancss public/styles.css public/css/tokens.css public/css/write-on.css public/css/selection-menu.css -o "$OUT/terraphim-editor.min.css"

# Create example usage file
cat > "$OUT/example.html" << EOL
<!DOCTYPE html>
<html>
<head>
  <title>Terraphim Editor Example</title>
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.2/css/all.min.css" integrity="sha512-SnH5WK+bZxgPHs44uWIX+LLJAJ9/2PkPKZ5QiAj6Ta86w+fsb2TkcmfRyVX3pBnMFcV7oQPJkl9QevSCWr3W6A==" crossorigin="anonymous" referrerpolicy="no-referrer">
  <link rel="stylesheet" href="terraphim-editor.min.css">
</head>
<body>
  <div id="editor"></div>
  <script src="terraphim-editor.min.js"></script>
  <script>
    document.addEventListener('DOMContentLoaded', async () => {
      const editor = new TeraphimEditor(document.getElementById('editor'));
      await editor.initialize();
    });
  </script>
</body>
</html>
EOL
