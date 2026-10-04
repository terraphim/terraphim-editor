# terraphim-editor: "Alternative Control" requirements

Derived from Jason Fried's Write_On demo (x.com/jasonfried/status/2105403067793584590, posted 30 Sept 2026, 6:55 video). Every requirement below cites the evidence it comes from: `[T mm:ss]` is the spoken transcript, `[F mm:ss]` is a video frame. Items marked **inferred** are design decisions the demo does not show and that need confirmation.

Target codebase: `terraphim-editor` (Rust → WASM via Trunk, vanilla JS, Shoelace, `markdown` crate, Rinja templates). Current state: a textarea + live Markdown preview with formatting shortcuts, a `/` command palette and an experimental "Blocks" view. None of the features below exist yet.

## 1. Scope and non-goals

Write_On is "alternative control, not version control" at the word, sentence and paragraph level, plus two supporting ideas: dimming text back without deleting it, and stashing text nearby. The author explicitly does not want AI rewriting his text; AI only proposes alternatives or marks candidates for cutting `[T 5:10–5:20]`.

Out of scope for this spec: collaboration, cloud sync, publishing to X/LinkedIn (the demo has buttons for these but they are incidental to the editing model `[T 3:50–4:05]`).

## 2. Editing model

**R-2.1 Plain mode by default.** The editor opens as a plain text editor with nothing visible except the text and a small word/char count in the top-left corner (`535 words 3115 chars`, dim monospace) `[T 0:00–0:05, F 0:05]`. Clicking the word count toggles "Write_On mode" on, which reveals the alternative indicators and the chrome described in §7 `[T 0:05]`.

**R-2.2 Span-anchored alternatives.** Any contiguous span of text (word, phrase, sentence, headline, paragraph) can own an ordered list of *alternatives*. Index 0 is always the original text as first written; the author's own additions and AI additions follow. Exactly one alternative is *active* and is what the document shows; the others are stored, not rendered `[T 0:35–1:00]`.

**R-2.3 Three granularities.** Alternatives are grouped by Word, Sentence and Paragraph `[F 1:30 side-panel tabs]`. The demo shows all three in use: headline (treated as a sentence-level span) `[T 0:30]`, single words `[T 0:55, 1:55]`, and a whole paragraph `[T 2:00]`.

**R-2.4 In-place cycling.** Hovering the span and pressing ↑/↓ cycles the active alternative in place, so the author sees each candidate in context with surrounding text `[T 0:40–0:50, 1:25–1:35]`. No modal, no diff view.

**R-2.5 Persistence.** The chosen alternative and the full list are kept with the document; the author "leaves it like this for a while" and returns later `[T 1:00, 1:40]`. Alternatives are part of the document model, not transient UI state. **Inferred:** serialise as a sidecar structure alongside the Markdown so plain `.md` export stays clean (see §9).

**R-2.6 Grammar fix-ups on swap.** When the active alternative starts with a vowel sound, a preceding indefinite article is automatically switched between "a" and "an" (`a paperclip` → `an eraser` → `a thumbtack`) `[T 1:50–2:00, F 1:55]`. The rule must be reversible and must not touch text outside the article immediately preceding the span.

**R-2.7 Emptying removes the indicator.** Deleting all non-original alternatives removes the underline and dots; the span reverts to plain text `[T 3:10–3:20]`. Any ghost over that text is unaffected; a ghost no longer keeps an emptied span alive **(decision 2026-10-04: ghost layer)**.

## 3. Inline indicators (the "dots under text")

These are the most distinctive UI elements and must be reproduced precisely.

**R-3.1 Underline.** A span with alternatives gets a thin (≈1px) underline in a muted lavender-grey, drawn just below the baseline across the full width of the span. It is lighter than the text and does not change on hover `[F 0:25 headline, F 0:55 "tension"]`.

**R-3.2 Dot row (word/sentence/headline).** Beneath the underline sits a row of tiny round dots, one per alternative *including the original*. Spacing ≈ one dot diameter. Placement rule:
- For a single word, the row is centred under the word (`tension` with 7 dots `[F 0:55]`, `struggle` with 7 dots `[F 1:45]`).
- For a sentence or headline, the row is right-aligned at the end of the underline (headline with 3 dots `[F 0:25]`, sentence "Shouldn't everything be obvious?" with 3 dots `[F 5:00]`).

**R-3.3 Active-dot highlight.** Exactly one dot is filled/bright (lavender); the others are hollow/dim. The first dot represents the original. When the active alternative is not the original, a dot other than the first is lit; the author describes this as "very subtle, but it tells me I'm on a variation" `[T 1:00–1:10, F 1:45 fifth dot lit]`.

**R-3.4 Paragraph indicator.** A paragraph with alternatives shows a thin vertical rule in the left gutter spanning the paragraph's height, with a short vertical column of dots immediately to its left, one dot per alternative, same lit-dot semantics `[F 2:00–2:10]`. No underline is drawn under paragraph text.

**R-3.5 Headline rendering.** Headlines are rendered bold in the same monospace face, same colour as body text, slightly larger; the underline/dots behave as for sentences `[F 0:25]`.

**R-3.6 Hover target.** The hover area for ↑/↓ cycling is the span itself (not the dots); the dots are passive indicators, not buttons. **Inferred:** clicking a dot should jump directly to that alternative.

## 4. Alternatives side panel

**R-4.1 Placement and toggle.** A left-hand panel (~29% of a 1140px window, i.e. ≈330px) opened by a keyboard shortcut or the "●●●" icon in the top-centre chrome `[T 1:10–1:15, F 1:30]`. The main text dims slightly while the panel is open `[F 1:30]`.

**R-4.2 Tabs.** Header row with three labels, Word · Sentence · Paragraph, set in a cursive/handwritten display face; the active tab is lavender, the others cream `[F 1:30]`. Selecting a span of the matching granularity in the document populates the list.

**R-4.3 Alternative list.** One alternative per line, monospace, directly editable. Pressing Enter on a new line adds an alternative (the author types "challenge" and it immediately appears in the document as a candidate) `[T 1:15–1:25, F 1:30]`.

**R-4.4 Provenance marker.** Each line carries a leading glyph:
- small dim dot ("bullet point") = written by the author `[T 3:00–3:05, F 1:30]`;
- small robot/bot glyph ("bot point") = produced by AI `[T 3:05–3:10, F 3:20]`.
The active alternative is shown with brighter/bold text and its glyph tinted lavender `[F 1:30 "challenge", F 3:20 "trade-offs"]`.

**R-4.5 Keyboard navigation.** With the panel open, ↑/↓ moves the active alternative and the document updates live `[T 1:25–1:35]`.

**R-4.6 Undo.** AI-added alternatives can be removed with undo; removing them clears the bot-marked lines and, if none remain, the inline indicator `[T 3:10–3:20]`.

## 5. Ghosting (dim back)

**R-5.1 Ghost.** Select text → right-click → "Ghost it" (Ctrl+/) renders the selection at roughly 10% opacity. The text remains in the document, still readable, still selectable, and still counted **(inferred)**, but visually recedes so the author can judge the paragraph without it `[T 2:15–2:45, F 2:40]`.

**R-5.2 Revive.** Right-click ghosted text → "Revive" restores full opacity `[T 2:45–2:50]`.

**R-5.3 Ghosting is an independent layer, persisted with the document** **(decision 2026-10-04: ghost layer)**. Ghosts are ranges of body text separate from alternative spans: a ghost may fully cover or partially overlap any spans (so a sentence containing a word with alternatives can be ghosted), while ghosts never overlap each other (ghosting over or next to an existing ghost merges them; reviving part of a ghost trims or splits it). Ghosts are elastic: edits and alternative swaps inside a ghost resize it, and deleting all of its text removes it.

## 6. Overflow panel (stash)

**R-6.1 Placement and toggle.** A right-hand panel, same width class as the alternatives panel, opened by the "XYZ" icon in the bottom-right chrome `[T 3:25–3:35, F 3:45]`. Title "Overflow" in the cursive display face, lavender.

**R-6.2 Content.** Free-form text area in the document's monospace face; holds any text (paragraphs, notes, URLs, outlines) `[T 3:35–3:45, F 3:45]`.

**R-6.3 Stash.** Select text in the document → right-click → "Stash this in Overflow" (Ctrl+Shift+X) moves (not copies) the selection to the end of the Overflow panel `[T 3:40–3:50, F 2:40 context menu]`.

**R-6.4 Pull back.** Footer hint reads `ctrl+⏎ or drag into the page to use`: Ctrl+Enter inserts the overflow selection at the document caret; drag-and-drop from the panel into the page does the same `[F 3:45]`. Plain copy/paste also works.

**R-6.5 Persistence.** Overflow content is saved with the document.

## 7. Chrome, menus and shortcuts

**R-7.1 Layout.** Full-bleed dark page; text column ≈ 70 characters wide, centred; generous top margin. No visible toolbar in plain mode `[F 0:05]`.

**R-7.2 Corner controls (Write_On mode on)** `[F 2:35, zoomed crops]`:
- Top-left: `N words M chars`, dim; click toggles Write_On mode (R-2.1).
- Top-centre: "●●●" (three dots, lavender) = alternatives panel toggle; "M↓" boxed glyph = Markdown view/export toggle **(inferred from glyph)**.
- Top-right: keyboard glyph = shortcut reference (also present at top of Overflow panel).
- Bottom-left: save (floppy, lavender), open (folder), post to X, LinkedIn (present but intentionally blocked `[T 3:55–4:05]`).
- Bottom-centre: `LAB` in a dashed-border pill.
- Bottom-right: `XYZ` rounded tag = Overflow panel toggle.

**R-7.3 Context menu on selection** `[F 2:38]`:

| Item | Shortcut |
|---|---|
| Alternatives for selection | Ctrl+Shift+A |
| AI alternatives for selection | Ctrl+Shift+G |
| Ghost it / Revive | Ctrl+/ |
| Stash this in Overflow | Ctrl+Shift+X |

Menu is a dark rounded panel, monospace labels left, dim shortcut text right, hovered row slightly lighter.

**R-7.4 Existing shortcuts** in `editor_config.toml` (bold, italic, code, link, heading) must not collide; Ctrl+/ and the Ctrl+Shift combinations above are free today.

## 8. Lab (AI-assisted editing)

**R-8.1 Lab popover.** `LAB` opens a popover anchored above the bottom-centre control. Header row: `The Lab guide…` left, `what each idea does` right (dim). Then a list of actions, then a row of trim buttons `[F 5:00]`.

**R-8.2 Mark actions** (each marks text, never rewrites it) `[F 5:00, T 4:15–4:35]`:
- Fix punctuation and typos
- Mark the weakest sentences
- Mark sentences that run long
- Mark convoluted sentences
- Mark words that don't fit the tone
- Mark hedges and filler

**R-8.3 Trim levels.** Five bordered buttons: `Original` (selected, lavender border), `Slight trim ~10%`, `Tighten more ~20%`, `Even sharper ~30%`, `Cut in half ~50%` `[F 5:00]`.

**R-8.4 Trim behaviour.** Selecting a level sends the whole document to the model and receives a set of spans to drop. Those spans are rendered using the ghost treatment (R-5.1); nothing is rewritten `[T 4:45–5:20, F 5:35]`. A floating status card appears bottom-centre: bold level name, `535 → 480 words · −10%` in lavender, hint `Faded words would go. Click one to keep it.`, and buttons `Make the cuts` · `Walk through` · `Done` `[F 5:35]`.

**R-8.5 Review.** Clicking a faded span un-ghosts it (keep). `Make the cuts` deletes all still-faded spans; `Walk through` steps through them one at a time **(inferred from label)**; `Done` dismisses the card and leaves the ghosting as is `[T 5:20–5:30]`.

**R-8.6 AI alternatives.** "AI alternatives for selection" appends bot-marked alternatives to the span's list (demo: calls → judgments, decisions, trade-offs, choices, bets) `[T 2:55–3:10, F 3:20]`.

**R-8.7 Provider abstraction.** AI calls go through a pluggable provider. For terraphim-editor the first provider should be local: synonyms and related terms from the active role's thesaurus via `terraphim_automata` (already shipped as WASM), with an LLM provider (Ollama/OpenRouter via terraphim_server `/chat`) as an optional second source. This keeps the "privacy-first, works offline" property of the rest of Terraphim and gives a deterministic baseline the Lab's "mark" actions can use (e.g. hedges/filler as a KG list). **Inferred — Terraphim-specific.**

## 9. Data model and file format

**R-9.1 Document = Markdown + annotations.** Body text stays plain Markdown so existing preview, export and the `markdown` crate pipeline are unchanged. Annotations live in a sidecar (same file as a fenced trailing block, or `<name>.md.alts.json` next to it — decide). **Inferred.**

**R-9.2 Annotation schema (proposal).**
```json
{
  "version": 1,
  "spans": [
    { "id": "s1", "kind": "word|sentence|paragraph",
      "anchor": { "start": 123, "end": 130, "text": "tension" },
      "active": 4,
      "alts": [
        { "text": "tension",  "source": "original" },
        { "text": "pressure", "source": "human" },
        { "text": "struggle", "source": "ai", "model": "…" }
      ] }
  ],
  "ghosts": [
    { "id": "g1", "anchor": { "start": 100, "end": 140, "text": "…a sentence containing tension…" } }
  ],
  "overflow": "…free text…"
}
```
Spans carry no `ghost` field; ghosting lives in the separate `ghosts` list of non-overlapping ranges, which may overlap spans **(decision 2026-10-04: ghost layer)**.
Anchors must survive edits elsewhere in the document: store `text` and re-anchor by search on load, with `start/end` as a hint. **Inferred.**

**R-9.3 Export.** Plain export emits only the active alternatives, drops ghosted spans **(inferred — ask: should ghosted text export?)**, and omits overflow.

## 10. Visual design tokens

From frames (approximate; sample before finalising): background `#0a0d1c`; body text warm cream `#e8d9c4`; dim/ghost text ≈ body at 10% opacity; accent lavender `#8c86e6` (active tab, lit dot, selected trim button border, save icon, word-count arrow); underline/dot colour ≈ accent at 40%; panels `#11142a` with 1px lighter border; context menu and Lab popover `#171a2e`, 8px radius. Body and UI in a monospace face (looks like JetBrains Mono / IBM Plex Mono); panel titles in a cursive display face. Line height ≈ 1.75.

## 11. Architecture notes for terraphim-editor

- The current `textarea` cannot render underlines, dots, ghosting or per-span hover. Replace it with a `contenteditable` surface or a custom DOM renderer; keep the Markdown→HTML path in Rust. The existing "Blocks" view and `BlocksStore` localStorage pattern can be reused for persisting annotations and overflow.
- Keep the Rust side responsible for: span model, anchor re-attachment, a/an fix-up, Markdown export with alternatives resolved, thesaurus-backed alternatives. Keep JS responsible for: DOM, hover/keys, panels, context menu, drag/drop.
- Tests: Rust unit tests for the span model, re-anchoring and article fix-up; `wasm-pack test --chrome` for cycling, ghost/revive, stash/pull-back; no mocks (per repo rules) — use a fixture thesaurus for the alternatives provider.

## 12. Decisions (answered by Alex, 2026-10-02)

1. **Write_On mode is a toggle**, as in the demo: plain editor by default, and clicking the word count turns it on (R-2.1 stands).
2. **Embedded annotations:** a trailing fenced block in the same `.md` file, not a sidecar. R-9.1 is resolved this way. Consequences: export (R-9.3) and the Markdown preview must strip the block; re-anchoring must ignore the block's own text; LSP clients such as Zed will show the block as raw text (see `zed-plugin-fit.md`).
3. **Ghosted text counts towards words and chars but is not exported.** This confirms R-5.1 and the drop-on-export default in R-9.3.
4. **Thesaurus first, LLM later.** v1 uses `terraphim_automata` synonyms only. The LLM provider stays behind the R-8.7 abstraction for a later release.
5. **The Lab ships in v1 with both parts:** all six mark actions (R-8.2) and all trim levels (R-8.3–R-8.5). With the thesaurus-only provider, the v1 mark and trim heuristics have to be deterministic (KG lists for hedges, filler and tone; length and structure rules for long or convoluted sentences). Weakest-sentence ranking and trim selection without an LLM need a defined heuristic, to be agreed during design.

## Appendix: evidence index

| Time | What it shows |
|---|---|
| 0:05 | Plain mode; word/char count top-left |
| 0:25–0:50 | Headline underline, 3 end dots, ↑/↓ cycling |
| 0:55–1:10 | Word underline with centred 7-dot row; lit non-original dot |
| 1:15–1:35 | Side panel: tabs, list, inline add, live update |
| 1:50–2:00 | a/an auto-fix; paragraph gutter rule + dot column |
| 2:15–2:50 | Ghost it (≈10% opacity) and Revive |
| 2:38 | Context menu with shortcuts |
| 2:55–3:20 | AI alternatives; bullet vs bot provenance markers |
| 3:25–3:50 | Overflow panel; stash; `ctrl+⏎ or drag` hint; XYZ toggle |
| 3:50–4:05 | Save/open/X/LinkedIn corner icons |
| 4:10–5:35 | Lab popover, mark actions, trim levels, trim status card |
