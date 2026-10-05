# Summary: src/lib.rs

## Purpose
WASM entry point and preview pipeline.

## Key points
- `run()` sets the panic hook, opens the welcome text in the document session, renders the template and wires the preview.
- Preview is debounced (default 120 ms trailing) so whole-document Markdown conversion stays off the keystroke path (#28).
- Exports: `render_markdown`, `flush_preview`, `set_preview_delay`, `preview_delay`, `preview_pending`, `preview_render_count`, plus the document bridge from `src/document.rs`.

## Notes
- Thread-local preview state; no `RefCell` borrow is held across DOM calls; `run()` cancels a stale pending render.
