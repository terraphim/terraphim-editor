# Summary: crates/terraphim_alternatives/src/reanchor.rs

## Purpose
Re-anchoring saved spans and ghosts after external edits.

## Key points
- All occurrence search goes through a `terraphim_automata` 2.1 `CompiledMatcher` (`MatchOverlap::All`, blank patterns allowed, positions without per-match allocation) (#30, #35).
- Rules: stored position first, then a single uncontested match; otherwise `Missing`, `Ambiguous` or `SearchFailed`. Context-based matching is planned (#36).
