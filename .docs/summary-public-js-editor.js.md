# Summary: public/js/editor.js

## Purpose
Browser editor controller.

## Key points
- `EditorSurface`: contenteditable `plaintext-only` surface with UTF-16 offset mapping, decorations (`setDecorations`), its own undo/redo that replays recorded edits, edit location from `beforeinput` target ranges with fallbacks, and `destroy()` via an AbortController.
- `MarkdownEditor`: toolbar, shortcuts, `/` command palette, document API (`openDocument(source, name)`, `saveDocument`, `exportDocument`, `counts`, `annotations`, `onWarning`), mirrors every surface edit into the Rust model, shows `.te-warning` notices.
- Degrades to a plain Markdown editor when `window.wasmBindings` is unavailable.
