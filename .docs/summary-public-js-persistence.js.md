# Summary: public/js/persistence.js

## Purpose
Save, open, autosave drafts and the Markdown export view (#76, #73). Design notes: `docs/design/persistence.md`.

## Key points
- `TePersistence` (`editor.persistence`), created last in `MarkdownEditor.initialize()` with the editor's AbortController signal; `destroy()` (called from `MarkdownEditor.destroy()`) removes its DOM, timers and object URLs and restores `document.title`.
- Entry points dispatch cancelable bubbling events from the chrome root (or the surface): `save()` -> `te:save`, `open()` -> `te:open`, `showMarkdown()` -> `te:markdown`; a host's `preventDefault()` stops the default action. Triggers: Write_On floppy / folder / `M↓`, the plain toolbar's File group (Open, Save, Markdown; FontAwesome icons), Ctrl+S / Cmd+S and Ctrl+O / Cmd+O on `document`.
- Save: `editor.saveDocument()` (te:saved) then a write: File System Access handle kept per editor (first save asks, later saves silent; files opened by picker or dropped in Chromium keep their handle) or a Blob download through `<a download>` (`config.fileSystemAccess: false`, Firefox, Safari, or no user activation). `te:written {name, method}`; `save()` returns the text synchronously, `lastWrite` is the write promise. First save-as re-keys the document (Write_On mode and draft move with it).
- Open: `showOpenFilePicker` or a hidden `<input type=file>` (.md, .markdown, .txt); file drops on the editor, chrome, panels or bare page (only `Files` drags; text drags untouched). Reads first, then asks Save first / Discard changes / Cancel in a modal `<dialog>` if dirty, then `editor.openDocument(text, name)`; `te:opened`.
- Dirty: surface edits mark dirty at once (dot on save controls, `• ` title prefix, `te:dirty-change`); a debounced tick (`config.autosaveDelay`, default 1000 ms; also after input/keyup/pointerup) compares `save_document()` with the clean text, catching annotation-only changes and undo back to clean.
- Drafts: `localStorage["terraphim-editor:draft:<key>"] = {v, text, savedAt, name}` on the tick, save, pagehide and destroy; a differing draft (newer than the file's lastModified) is offered with a non-blocking Restore / Discard notice. All storage access in try/catch.
- `M↓` dialog: `exportDocument()` in a read-only text area, Copy (Clipboard API, else select + execCommand, else a message) and Download .md (`<name>-export.md`); Escape closes and focus returns.
- Globals: `TePersistence`, `teSuggestedFileName`, `teExportFileName`, `teIsOpenableFile`, `TE_DRAFT_PREFIX`. Tests: `tests/web_files*.rs` (helpers in `tests/support/files.rs`).
