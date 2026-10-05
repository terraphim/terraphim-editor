# Summary: crates/terraphim_alternatives/src/context.rs

## Purpose
Anchor context capture and comparison for context re-anchoring (#36, decision 2026-10-05).

## Key points
- Each anchor may store `before` / `after`: up to `CONTEXT_UNITS` (32) UTF-16 units of surrounding text, walking whole characters (never splitting a surrogate pair), trimmed to a word boundary unless that leaves fewer than 16 units; `""` means the anchor touched that document edge, `None` means no context stored (older files).
- Captured on creation (spans, ghosts, merges, splits, resizes), refreshed near every body change, after re-anchoring and on `write`; stale and set-aside anchors keep their stored context.
- Comparison ignores carriage returns, so CRLF/LF conversion keeps agreeing.
- Used by `reanchor.rs` rule 2: a side agrees with at least half of the stored context and at least 8 units (or exact distance from the same edge); the unique best qualifying candidate is placed, otherwise Missing / Ambiguous.
