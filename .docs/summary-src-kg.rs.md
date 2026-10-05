# Summary: src/kg.rs

## Purpose
Knowledge-graph alternatives (#13): a thin bridge from the open document session to `terraphim_lsp_core` (pinned `=1.21.3`, private registry). No matching, thesaurus, capitalisation or a/an logic of its own.

## Key points
- Thread-local `KgEngine` slot: `kg_load_thesaurus(json)` (bad JSON keeps the previous one), `kg_clear_thesaurus()`.
- Derived KG spans (`kg_spans`, `kg_document_spans`, `document_annotations().kg`): ids `kg-<concept>-<n>`, `source: "kg"`, alternatives = every term of the concept in core order with the text's own form active; single-term concepts and terms overlapping a block span are skipped; never saved.
- Rows are asked of the core once per distinct `(concept, text)`; the list is cached by body, live spans and thesaurus generation (5k words: 1.7 ms fresh, 0.35 ms cached).
- `kg_swap_alternative(id, index)` applies the core's `Replacement.edits` (article plus term) as one contiguous `apply_edit`, returning the `set_active_alternative` shape; refuses edits that would change a block span.
- `kg_lookup_selection(start, end)` and `alt_kg_append(start, end)` (R-8.6, Ctrl+Shift+G): append the term's synonyms to the span over it (created if absent) as `ai` lines with `model: "kg"` via the #10 `AltChange` path (one undo step); hidden and refused inside a larger block span.
- Tests: `tests/kg.rs`, `tests/web_kg*.rs`; bench `benches/kg_bench.rs`; design `docs/design/kg-alternatives.md`.
