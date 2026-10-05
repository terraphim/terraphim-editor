#!/bin/bash

# Create dist directory
mkdir -p dist

# Copy and minify JS files
# Order matters: config, then the Write_On chrome and the decoration
# registry/inline indicators, then the editor that instantiates them, then
# the embeddable wrapper.
terser public/js/config.js public/js/chrome.js public/js/indicators.js public/js/editor.js public/js/terraphim-editor.js -o dist/terraphim-editor.min.js

# Copy and minify CSS
# Design tokens before the Write_On layout and chrome that consume them.
cleancss public/styles.css public/css/tokens.css public/css/write-on.css -o dist/terraphim-editor.min.css

# Create example usage file
cat > dist/example.html << EOL
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
