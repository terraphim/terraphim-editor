# Alternative synonyms in the knowledge graph

User requirement, 11 October 2026: reuse existing Terraphim crates and track alternatives as knowledge-graph synonyms. This supplements the existing 41 video requirements; it does not silently change their acceptance status.

## Existing implementation and missing direction

`src/kg.rs` consumes the pinned `terraphim_lsp_core` engine. Its matcher and concept-to-synonym index already come from `terraphim_automata`. Known KG synonyms can be cycled and appended to a document span. `terraphim_alternatives` owns durable ordered alternatives, source/model provenance and anchors. `terraphim_lab` reexports the existing `terraphim_types` and automata types. `terraphim_rolegraph_browser` already wraps the actual `terraphim_rolegraph::RoleGraph`, including validated checkpoints and document indexing.

The reverse direction is **not implemented or accepted**: creating a human/provider alternative currently persists an annotation but does not register that new synonym in the knowledge graph. The r10 VSIX therefore does not yet satisfy this additional requirement. An annotation array or a JSON field labelled synonyms is not evidence of KG registration.

## Required shared behavior

- Reuse `terraphim_types::Thesaurus`/`NormalizedTerm`, `terraphim_automata::ConceptIndex` and the existing RoleGraph. Do not implement a parallel synonym dictionary, parser, matcher or graph per host.
- Track original, human and accepted provider alternatives as members of one concept. Preserve original text, ordering, granularity, span identity and each alternative's source/model provenance. Keep exact display text separately from the existing normalized lookup key.
- Scope authored synonym groups to a document and span. The same normalized phrase can occur in unrelated contexts; it must not silently unify their concepts or overwrite a role's thesaurus. Existing KG concept IDs remain distinct from authored document IDs. Explicit promotion into a shared role needs a defined conflict policy.
- Derive registration from the **committed** document source. A proposed edit, rejected ACK, stale provider response or unaccepted AI suggestion must not publish synonyms. Browser commits and host exact-output acknowledgements must follow the same rule.
- Save/reopen, native undo/redo, alternative edit/removal and external source changes must reconstruct exactly the current group membership. An obsolete synonym must not survive only in a worker cache. A ghost remains an independent visibility layer and must not delete its synonym group.
- Use existing validated RoleGraph persistence/checkpoints where graph state is stored. Keep document annotation provenance sufficient to reconstruct document-scoped synonyms; avoid independently authoritative copies that can diverge.
- Put the domain contract in shared Terraphim Rust code used by browser/WASM and LSP. VS Code, Sublime, Obsidian and Logseq adapters own host transactions, not synonym semantics. The Zed fork stays separate.

## Actual-output acceptance

Against real Terraphim crates, create an alternative through the document command, acknowledge the actual output, query the resulting KG and prove both forms resolve to the same scoped concept. Round-trip the graph and annotated Markdown, reopen and query again. Repeat for human and accepted provider alternatives, Unicode/case normalization, sentence/paragraph text, duplicates and two unrelated spans/documents sharing a phrase. Verify rejection, stale ACK and external conflicts publish nothing. Verify edit/remove and native undo/redo restore exact membership and preserve provenance. Keep input fixtures separate from resulting graph/checkpoint/query outputs.

All these checks remain pending. Existing KG-read tests, compilation and the earlier 41-row ledger do not prove reverse registration.
