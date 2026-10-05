# Summary: public/js/overflow.js

## Purpose
Overflow panel (#12, R-6.1 to R-6.5, R-7.3): stash, pull back, drag into the page.

## Key points
- `TeOverflowPanel` (`editor.overflow`): right-hand `<aside role="complementary">` labelled by a cursive lavender "Overflow" title, a monospace text area, footer hint `ctrl+⏎ or drag into the page to use`, keyboard glyph (opens the chrome shortcut reference) and close button (FontAwesome).
- Toggled by the XYZ control (`te:overflow`); sets `aria-expanded`/`aria-controls` on it and `data-te-overflow="open"` on `<body>`. Escape closes and returns focus to where it came from (XYZ) or the page.
- Write_On only (panel UI and the stash item/shortcut); the overflow data is document state and is kept and saved in both modes.
- Panel typing is written to the model (`set_document_overflow`) after a 250 ms debounce, on blur and before any model read (`alignDocumentModel` calls `flush()`); `te:overflow-change` is dispatched for storage owners.
- Stash: selection menu item `stash` (Ctrl+Shift+X) calls `MarkdownEditor.stashRange`: a move and ONE undo step restoring text and overflow together (overflow undo rebased onto later panel edits). Spans in the stashed text detach and ghosts are set aside (normal edit rules), re-attaching on undo. Dragging a page selection onto the panel also stashes.
- Pull back is a copy: Ctrl+Enter inserts the panel selection (or current line) at the document caret (end of a document selection), one undo step, focus stays in the panel. Dragging panel text into the page drops it at the drop point (copy-only drag effect).
- Refused while a malformed block is preserved (its save would drop the overflow): the panel is read-only with a note.
- `destroy()` removes the DOM, the menu item, the body attribute and all listeners. Tests: `tests/web_overflow.rs`; bridge tests in `src/document.rs`.
