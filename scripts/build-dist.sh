#!/bin/bash

# Create dist directory
mkdir -p dist

# Copy and minify JS files
terser public/js/config.js public/js/editor.js public/js/terraphim-editor.js -o dist/terraphim-editor.min.js

# Copy and minify CSS
cleancss public/styles.css -o dist/terraphim-editor.min.css

# Create example usage file
cat > dist/example.html << EOL
<!DOCTYPE html>
<html>
<head>
  <title>Terraphim Editor Example</title>
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