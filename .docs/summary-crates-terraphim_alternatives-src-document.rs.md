# Summary: crates/terraphim_alternatives/src/document.rs

## Purpose
`Document`: body plus annotations and every edit operation.

## Key points
- `add_span`, `add_alternative`, `set_active` / `cycle_active` (atomic, with the a/an fix-up), `remove_alternative`, `clear_alternatives`, `ghost` / `revive` / `ghost_span` / `ghost_at`, `apply_edit`, `export` (drops ghost ranges and overflow), `counts` (includes ghosted text).
