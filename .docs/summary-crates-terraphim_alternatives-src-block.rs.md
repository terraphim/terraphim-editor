# Summary: crates/terraphim_alternatives/src/block.rs

## Purpose
Trailing fenced `terraphim-alternatives` annotation block.

## Key points
- `parse` / `write`, byte-stable; malformed blocks are recoverable (`body + raw_block == source`).
- An empty guard block is written only when needed, so a document ending in an example of the format round-trips unchanged (#26).
