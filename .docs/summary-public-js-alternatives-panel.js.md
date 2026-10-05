# Summary: public/js/alternatives-panel.js

## Purpose
The alternatives side panel (#10, R-4.1 to R-4.6, R-7.3). Design: `docs/design/alternatives-panel.md`; styles: `public/css/alternatives-panel.css`.

## Key points
- `TeAlternativesPanel` (`editor.altPanel`): a left `<aside aria-label="Alternatives">` with cursive Word / Sentence / Paragraph tabs (`role=tablist`, automatic activation) and one `<input>` per alternative; line 0 (the original) read-only, the last line empty for adding.
- Opens from `te:open-panel` (the `●●●` control, toggles, focus in), the `alternatives` selection-menu item (selections) and Ctrl+Shift+A (selection or caret, always cancelled; turns Write_On on in plain mode), and `te:dot` (shows the span, keeps focus, never cancels). Escape closes and returns focus; leaving Write_On closes. Lines commit on Enter, blur, tab change and close (the empty line adds; whitespace-only ignored); destroy() reports uncommitted text as `te:alt-drafts` and never edits.
- Target: the span at the reference point if its kind matches the tab, else a pending word/sentence/paragraph (created with its first alternative in one step; refused if it would overlap a span). Caret moving into another span retargets.
- Operations go through `editor.alternativeOp()` (model first, one undo step): add, edit (empty deletes), delete button, Alt+Up/Down reorder, Up/Down make a line active live via `activate()` (#9 `swapAlternative`). Ctrl+Z/Y in the panel drive the document history.
- Glyphs from `source`: dim dot (original/human), `fa-robot` (ai); active line bright, glyph lavender.
- Text helpers `TeAlternativesPanel.text` (wordAt, sentenceAt, paragraphAt, inferKind). Listeners on the editor's AbortController; `destroy()` removes DOM, listeners, menu item and the dim class.
- Loaded after `blocks.js`, before `editor.js`. Tests: `tests/web_alt_panel.rs`.
