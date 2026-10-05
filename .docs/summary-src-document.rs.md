# Summary: src/document.rs

## Purpose
WASM bridge between the browser editor and `terraphim_alternatives` (#6).

## Key points
- `DocumentSession` (native, unit-tested) owns one `Document`: open, save, export, counts, edit mirroring, body sync, annotations JSON.
- wasm-bindgen exports: `open_document`, `save_document`, `export_document`, `document_counts`, `apply_edit`, `sync_document_body`, `document_body`, `document_annotations`. Offsets are UTF-16.
- Malformed annotation block: body opens, one warning, raw block saved back verbatim.
- Nothing is dropped: spans detached by an edit and items that cannot be re-placed go to `SetAside` and are still saved; they re-attach when their text returns (e.g. after undo).
- Overflow (#12): `document_overflow`, `set_document_overflow`, `stash_document_range` (removal through `apply_edit` plus append, atomic) and `replay_document_overflow` (undo/redo rebased onto panel edits); all refuse while a malformed block is preserved.
