# Synonym-to-search demo

User decisions, 2026-10-11: create a new synonym in the editor, share its graph, search with that synonym, then compare synthesis off/on. Project brief uses risk/concern and related decisions. Deliver one focused60–90second story with natural British female narration. This supersedes the previous existing-shared-graph-only demo arc. Existing publication authorization applies; no additional phase permission is required.

## User journey

1. A short public project brief states a concrete risk and the team's decision.
2. Add Concern as an alternative to Risk, swap both ways, retain Risk in saved prose.
3. Open Knowledge graph: visible Risk ↔ Concern plus the exact synonyms:: source. Download the document graph and clean Markdown.
4. Run terraphim-grep concern against that downloaded graph and text. The corpus contains no literal concern; the real result must show Risk and the decision.
5. Repeat with --answer. Show actual answer and source evidence, not a scripted substitute. Close with the benefit: choose your wording, keep the meaning searchable.

## Implementation

Reuse SynonymGroup's existing synonyms:: parser, Thesaurus, NormalizedTerm and ConceptIndex. Add Document::export_synonym_thesaurus; assign distinct exported IDs and refuse overlapping normalized terms across different groups. Export attached committed document groups only, without changing the active shared role. Reject preserved malformed annotation blocks. Typed comma/multiline synonyms stay exact.

Expose through DocumentSession and WASM; add MarkdownEditor.exportKnowledgeGraph. Add a Knowledge graph dialog in existing file controls and from Markdown export. Show a readable concept relationship and exact directive source; offer JSON graph download. Empty/conflicting graph gets explanatory output and no downloadable success artifact.

Tests: actual parser/types; multiple independent concepts; collision refusal; save/reopen, removal, undo/redo and detached groups; browser UI downloads exact graph and document; actual grep alias retrieval with clean corpus and baseline without synonym, plus same corpus/graph synthesis comparison. No input fixture is counted as generated output.

Files: crates/terraphim_alternatives/src/synonyms.rs + lib.rs, src/document.rs + lib.rs, public/js/editor.js + persistence.js, shared-crate and integration/browser tests. Record under demos/knowledge-work and verification receipts. Website update and media admission follow reviewed bytes. Zed fork remains separate.

## Presentation

One clear sentence per scene; cursor visible; large readable word changes and results. No unrelated controls, integrity hashes or acceptance bookkeeping on screen. Approximately20s wording,20s graph/export,20s search,15s synthesis. Voice aligned to visible outcomes, no speech time stretching. Supporting evidence is separate from the story.
