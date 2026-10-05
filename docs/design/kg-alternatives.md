# Knowledge-graph alternatives (issue #13)

In a knowledge graph every synonym of a concept shares the concept's id, so
the synonyms of a matched word are its alternatives. This note records how
the editor consumes them. Spec: `docs/requirements/alternative-control.md`
(R-2.x, R-3.x, R-4.4, R-8.6, R-8.7, decision 4).

## No logic of our own

All matching, the concept -> synonyms index, capitalisation and the a/an
fix-up come from `terraphim_lsp_core` (terraphim/terraphim-ai#3409), pinned
to `=1.21.3` from the private `terraphim` registry. `src/kg.rs` only adapts
its results to the document session; there is no matcher, thesaurus parser,
case rule or article rule in this repository.

### One copy of the span model

`terraphim_lsp_core` depends on `terraphim_alternatives` 0.1 from the
registry, and the editor has the same crate as a workspace member. The root
`Cargo.toml` patches the registry copy to the member:

```toml
[patch.terraphim]
terraphim_alternatives = { path = "crates/terraphim_alternatives" }
```

The published 0.1.0 is built from the member (their `src/` trees are
identical), so the core and the editor share one annotation-block parser and
one set of a/an rules, and `cargo tree -d` lists no `terraphim_*` crate
twice. When the member's version moves past what the core accepts, publish
it and bump the core first; the patch then keeps applying.

## Loading a thesaurus

`kg_load_thesaurus(json)` (`MarkdownEditor.loadThesaurus`) builds a
`KgEngine` and keeps it in a thread-local slot next to the document session;
a bad thesaurus throws and keeps the previous one. `kg_clear_thesaurus`
forgets it. Loading the active role's thesaurus from the application is a
follow-up; the tests use the committed fixture
`tests/fixtures/kg/thesaurus.json` (the core's writing fixture plus a
`use / employ / utilise` concept for case tests).

## Derived spans

`document_annotations()` gains a `kg` list: one span per KG term in the body
whose concept has two or more terms, in the block span shape plus KG fields:

```json
{ "id": "kg-1-0", "kind": "word", "source": "kg",
  "anchor": { "start": 69, "end": 75, "text": "choice" },
  "active": 1,
  "alts": [{ "text": "decision", "source": "kg" }, { "text": "choice", "source": "kg" }, "..."],
  "conceptId": 1, "nterm": "decision", "description": "decision", "url": null }
```

Decisions:

1. **Derived, never saved.** KG spans are computed from the text on demand
   and never written to the annotation block, which keeps only what the KG
   cannot derive. Save and reopen therefore round-trip a swapped word as
   plain text, and the spans come back from the thesaurus.
2. **Stable concept-order row.** The dots are every term of the concept in
   the core's order (concept name first, then the other synonyms sorted),
   capitalised like the text by the core, with the text's own form at its
   place and lit. The brief suggested "current form first", but a derived
   span has no persisted original, so putting the current form first would
   reorder the row after every swap and make Up/Down bounce between two
   slots. A row that does not move, with a moving lit dot, is what lets
   cycling walk the synonyms and wrap.
3. **Ids survive swaps.** `kg-<concept id>-<n>`, `n` counting earlier
   matches of the same concept in the body. A swap keeps the concept and the
   order even when it rewrites an article before the word, so the hover and
   the indicator follow it. Inserting an earlier instance of the same
   concept renumbers the later ones; that is acceptable for derived spans.
4. **Block spans win.** A KG term that overlaps a live span from the
   annotation block is not offered: the writer's span, with their own
   alternatives, takes precedence. KG synonyms reach such a span only when
   the writer asks (R-8.6, below), and then after the writer's own
   alternatives ("human first after the original").
5. **Single-term concepts** (no synonyms) give no span.
6. **Cost.** `alternatives_for` scans the body, so asking the core once per
   match made a refresh quadratic (about 340 ms natively on 5,000 words with
   1,800 terms). The row depends only on the concept and the exact text, so
   the core is asked once per distinct `(concept, text)` pair, and the whole
   list is cached until the body, the live spans or the thesaurus change.
   `benches/kg_bench.rs` (native, criterion): fresh analysis 1.7 ms, cached
   read 0.35 ms, one swap 1.9 ms on that document.

Only `document_annotations()` carries `kg`; the `annotations` objects
returned by `ghost_range`, `revive_range` and the panel operations keep the
block's own shape (the editor re-reads `annotations()` before drawing
indicators).

## Swapping

`swapAlternative(id, index)` routes `kg-…` ids to `kg_swap_alternative`.
The model goes first: the bridge re-derives the span, asks the core for the
`AlternativeSet`, and applies the chosen `Replacement.edits` (an `a`/`an`
fix-up when the article changes, then the term) as ONE contiguous edit
through `DocumentSession::apply_edit`, returning the same shape as
`set_active_alternative`. The surface applies exactly that edit as one undo
step.

The step is tagged `kg`, not `swap`: `swap` steps replay `set_active` in the
model on undo, which a derived span does not have, and the fallback would be
a full-body re-sync that can disturb block spans. A `kg` step is a plain text
step, so undo and redo mirror its inverse through `apply_edit`, and the KG
spans re-derive from the restored text. The indicators read the tag only to
keep the hover across the swap.

A swap whose edit would change the text of a block span (an article inside
the writer's span) is refused with nothing changed, as are unknown ids and
invalid indices.

## AI alternatives for selection (Ctrl+Shift+G, R-7.3, R-8.6)

The selection menu item `ai-alternatives` ("AI alternatives for selection",
Ctrl+Shift+G) is offered when `kg_lookup_selection` finds a KG term at the
selection (the term covering its start, or a caret just after a word). It
calls `alternativeOp('kg_append', start, end)`: `alt_kg_append` appends the
term's synonyms to the block span exactly over the term, creating a word
span when there is none, as `source: "ai"` with `model: "kg"` and after the
alternatives already there; synonyms already offered are skipped. A KG word
inside a larger block span (a sentence or paragraph span around it) is the
writer's: the lookup returns `null`, so the item is hidden, and the append is
refused with nothing changed. R-8.6 says
the result is appended to the span's list, so these are persisted.

`Source` has no `kg` variant: adding one would change the block schema and
make the workspace member diverge from the published 0.1.0 the core is built
against. Persisted KG lines are therefore `ai` lines whose `model` is `kg`,
and they carry the bot glyph (`fa-robot`) in the panel like any AI line
(R-4.4). The derived JSON, which never enters the model, says `source: "kg"`.

The append is a #10 panel operation (`AltChange`, span before and after), so
it is ONE undo step replayed with `alt_restore`; undo removes a created span
and the KG span is derived again. When the panel is available in Write_On
mode it then opens on the span.

Only the thesaurus provider exists; an LLM provider stays behind the R-8.7
abstraction in the core for a later release (decision 4).

## The panel (#10)

Persisted KG lines appear in the alternatives panel with the robot glyph and
are ordinary editable AI lines. Derived KG spans are not listed in the panel
yet: a dot click on one opens the panel on the text at the caret. Listing a
derived span read-only (choosing a line swaps through `kg_swap_alternative`;
typing a new line creates a block span through `alt_kg_append` plus
`alt_add`) is a follow-up.

## Size

`trunk build --release`: the `_bg.wasm` grows from 1,688,584 bytes (main at
2931d63, with #10 and #12) to 1,819,696 bytes, +131,112 bytes (+7.8%).
`terraphim_automata` 2.1 was already in the bundle through `terraphim_lab`;
the delta is the core itself, its `ConceptIndex` and offset code, and the
new exports.
