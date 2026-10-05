# Summary: src/document.rs

## Purpose
WASM bridge between the browser editor and `terraphim_alternatives` (#6).

## Key points
- `DocumentSession` (native, unit-tested) owns one `Document`: open, save, export, counts, edit mirroring, body sync, annotations JSON.
- wasm-bindgen exports: `open_document`, `save_document`, `export_document`, `document_counts`, `apply_edit`, `sync_document_body`, `document_body`, `document_annotations`. Offsets are UTF-16.
- Malformed annotation block: body opens, one warning, raw block saved back verbatim.
- Nothing is dropped: spans detached by an edit and items that cannot be re-placed go to `SetAside` and are still saved; they re-attach when their text returns (e.g. after undo).
- #10 block (alternatives panel): `alt_create_span`, `alt_add`, `alt_edit`, `alt_remove`, `alt_move` run atomically (session snapshot restored on error) and return `AltChange` { span, index, before, after, edit } with the single body edit (`body_edit`); `alt_restore` replays a recorded step (span out, edit applied as an ordinary edit, snapshot back, context refreshed) for undo/redo.
- Overflow (#12): `document_overflow`, `set_document_overflow`, `stash_document_range` (removal through `apply_edit` plus append, atomic) and `replay_document_overflow` (undo/redo by recorded position; `applied: false` when the stashed text is no longer intact there); all refuse while a malformed block is preserved.
