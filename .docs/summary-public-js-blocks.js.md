# Summary: public/js/blocks.js

## Purpose
Blocks view (#19): a third view of the open document, alongside plain and Write_On, showing the Markdown body as editable top-level blocks.

## Key points
- `BlocksView.parse` / `BlocksView.serialise`: lossless split of the body into contiguous blocks (heading, paragraph, list, code, quote, table, rule); body -> blocks -> body is byte-identical by construction.
- No content copy: blocks are parsed on demand from the surface text, and block edits are applied as minimal diffs through `surface.replaceRange(..., { source: 'blocks' })`, so the span model, undo and preview see ordinary edits.
- Text | Blocks toolbar toggle in plain mode; no control in Write_On (entering Write_On switches to Text; Blocks resumes in plain mode). Fires `te:view-change`; the view preference (only) is in localStorage `terraphim-editor:view`.
- Keyboard navigable (roving tabindex, Enter to edit, Escape/Ctrl+Enter, Delete, undo/redo); listeners use the editor's AbortController; `destroy()` removes its DOM. No move up/down until the model can move ranges (#44). Design: `docs/design/blocks-view.md`.
