# Summary: templates/editor.html

## Purpose
Rinja template rendered by `run()` into `#app`.

## Key points
- Toolbar (formatting buttons inserted by JS), the editing surface element with class `te-surface` (contenteditable, replaced the textarea in #4), and the preview pane.
- Variables: `initial_content`, `initial_preview|safe`.
