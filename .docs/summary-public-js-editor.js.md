# Summary: public/js/editor.js

## Purpose
Browser editor controller.

## Key points
- `EditorSurface`: contenteditable `plaintext-only` surface with UTF-16 offset mapping, decorations (`setDecorations`), its own undo/redo that replays recorded edits, edit location from `beforeinput` target ranges with fallbacks, and `destroy()` via an AbortController.
- `MarkdownEditor`: toolbar, shortcuts, `/` command palette, document API (`openDocument(source, name)`, `saveDocument`, `exportDocument`, `counts`, `annotations`, `onWarning`), mirrors every surface edit into the Rust model, shows `.te-warning` notices.
- Degrades to a plain Markdown editor when `window.wasmBindings` is unavailable.
- #10: `alternativeOp(name, ...args)` runs an `alt_*` model operation and `applyAltChange` applies its edit (or an empty edit) as one history step carrying `alt: { span, before, after }`; `record` gives such steps their own entry, `invertStep` swaps before/after, `mirrorEdit` replays them with `alt_restore` (`mirrorAltStep`, re-syncing on mismatch); `mapRanges` keeps ranges for empty edits. Creates `editor.altPanel`.
- Overflow (#12): history steps may carry `overflow: { before, after }` (replayed by `mirrorOverflow` via `replay_document_overflow`); `stashRange(start, end)` stashes model-first as one undo step; `editor.overflow` is the `TeOverflowPanel`.
