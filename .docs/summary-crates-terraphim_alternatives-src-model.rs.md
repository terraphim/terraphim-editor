# Summary: crates/terraphim_alternatives/src/model.rs

## Purpose
Data model and annotation schema v1.

## Key points
- `Span` (id, kind word/sentence/paragraph, anchor, alternatives with source original/human/ai, active index), `Anchor` (UTF-16 start/end, text, optional `before`/`after` context), `Ghost` ranges, `Annotations` (spans, ghosts, overflow).
- Ghosting is an independent layer of ranges that may overlap spans; ghosts never overlap each other (decision 2026-10-04).
