# The alternatives side panel (issue #10)

Spec: `docs/requirements/alternative-control.md` R-4.1 to R-4.6 (the panel), R-7.3 and R-7.4 (menu item and shortcuts), R-2.2 to R-2.7 (spans, active alternative, a/an, emptying), R-3.x (indicators).

Code: `public/js/alternatives-panel.js` (`TeAlternativesPanel`, `editor.altPanel`), `public/css/alternatives-panel.css`, the `// #10` blocks in `public/js/editor.js` (`MarkdownEditor.alternativeOp`, `applyAltChange`, `mirrorAltStep`, plus small `alt` additions to the surface history) and in `src/document.rs` (the `alt_*` exports). Tests: `tests/web_alt_panel.rs` and the `#10` unit tests at the end of `src/document.rs`.

## Opening and closing

| Way in | Target | Focus |
|---|---|---|
| `●●●` in the top-centre chrome (`te:open-panel` { panel: 'alternatives' }) | the span at the caret, else the word, sentence or paragraph there (by tab) | moves to the active line; Escape returns it to the `●●●` control |
| "Alternatives for selection" (selection menu item `alternatives`, offered for a selection) or Ctrl+Shift+A | the selection: the span holding it, or the trimmed selection as a pending span; Ctrl+Shift+A with a collapsed caret: as for `●●●` | moves to the active line (the empty line for a pending span); Escape returns it to the surface with the selection restored |
| A dot click (`te:dot`) | that span | stays where it was; the event is **not** cancelled, so issue #9's default action (jump to that alternative) still happens |

- `●●●` and Ctrl+Shift+A inside the panel toggle it closed. Escape always closes it. Leaving Write_On mode closes it.
- **Write_On only.** The panel, like the indicators, is part of Write_On mode. The `●●●` control only exists there. Ctrl+Shift+A in plain mode is an explicit request for alternatives, so it turns Write_On mode on (through `chrome.setMode`, persisted like a click on the word count) and opens the panel, rather than doing nothing.
- While open, the main text dims (`--te-alt-dim-opacity`, R-4.1) and, on windows at least 48rem wide, the writing column moves right so the panel (fixed to the left edge, `--te-alt-panel-width`: about 330px at 1140px) never covers it; it stays centred when there is room. Below 48rem the panel overlays the text.

## Which span is shown (tabs, R-4.2)

Spans never overlap (crate rule), so at most one span contains the caret.

- The panel resolves around a *reference point*: the caret when it opens, or the start of the span it shows.
- If the span at that point has the active tab's kind, it is shown. Opening on a span switches the tab to its kind.
- Otherwise the tab offers the **word, sentence or paragraph** around the point as a *pending* span: its text is listed as the original line and the empty line invites a first alternative. Ranges: a word is a run of letters, digits, `_`, `'`, `’` and `-`; a sentence ends after `.`, `!` or `?` (plus closing quotes or brackets) followed by whitespace, or at a line break, within its paragraph; a paragraph lies between blank lines. Leading Markdown heading, list and quote markers and surrounding whitespace are trimmed, so a headline is a sentence (R-2.3).
- A pending range that would overlap an existing span is refused with a message naming the tab that shows that span (for example the Sentence tab while the caret is in a word that has alternatives).
- For a selection, the kind is inferred: several sentences or lines are a paragraph; up to four words without closing punctuation are a word (a short phrase, as `SpanKind::Word` allows); anything else is a sentence.
- With the surface focused, moving the caret **into another span** retargets the panel. Moving into plain text does not, so undoing (which puts the caret beside the span) or clicking to read the text keeps the panel where it is.

### Why a pending span rather than an empty one

The issue asks Ctrl+Shift+A to "create a span". A span with only its original is inert (R-2.7): the indicators draw nothing for it, it is saved into the block, and the first keystroke inside it detaches it with a "detached 1 span with 1 alternative" warning. So the panel holds the range as pending and the first alternative typed creates the span **and** that alternative in one model call (`alt_create_span`), one undo step. Undoing that step removes the span again. No inert span is ever created by the panel.

## Lines (R-4.3, R-4.4)

- One `<input type="text">` per alternative. Inputs were chosen over contenteditable lines: they give native, accessible single-line editing with no Markdown or rich-text side effects, and the panel handles Enter, the arrows and Escape itself.
- Line 0 is the original and is **read-only** (the crate never adds, removes or rewrites index 0; it stays "the text as first written", R-2.2).
- A line is committed on Enter or when it loses focus: changed text edits the alternative, **empty text deletes it**. Each line also has a delete button (shown on hover or focus; not in the tab order, so keyboard users empty the line instead).
- The last line is always empty. Enter there adds the typed text, which appears at once in the document as a new dot (R-4.3); focus stays on the empty line for the next one. Adding text the span already has changes nothing and says so in the status line.
- Glyph from the `source` field: a small dim dot (`fa-circle`) for `original` and `human`, a robot (`fa-robot`) for `ai`. The active line is brighter and bold, and its glyph is lavender (R-4.4). Each input's accessible name states its position, provenance and whether it is active.
- A status line (`role="status"`) reports the span ("Word: 3 alternatives, 2 of 3 active"), the pending prompt, or why a change was refused.

## Keyboard (R-4.5)

| Key | In a line |
|---|---|
| ArrowUp / ArrowDown | commit the line, make the previous / next line the active alternative (live document update), focus follows; ArrowDown on the last alternative goes to the empty line, ArrowUp from the empty line activates the last alternative. No wrap-around (the list reads top to bottom). |
| Alt+ArrowUp / Alt+ArrowDown | move the focused line up or down (reorder); never above the original; the active alternative stays active |
| Enter | empty line: add; other lines: commit and go to the empty line |
| Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z, Ctrl+Y | undo / redo the document, when the focused line has no uncommitted typing (otherwise the input's own undo) |
| Escape | discard uncommitted typing in the line, close, return focus |
| Ctrl+Shift+A | close |

Tabs are a `role="tablist"` with automatic activation: Left/Right (wrapping), Home/End, ArrowDown moves into the list. The panel is an `<aside aria-label="Alternatives">`; the tab panel is labelled by the active tab; the `●●●` control gets `aria-expanded` and `aria-controls`.

## Model first, one undo step

Every operation runs in the WASM model first, through `MarkdownEditor.alternativeOp(name, ...args)`, which calls `wasmBindings.alt_<name>`:

| Export | Does | Text change |
|---|---|---|
| `alt_create_span(kind, start, end, text, source)` | span plus its first alternative | none |
| `alt_add(span, text, source)` | append (returns `null` if the text exists) | none |
| `alt_edit(span, index, text)` | rewrite a line (not the original; empty or duplicate text refused) | only if it is the active line, through `Document::set_active` so the a/an fix-up applies |
| `alt_remove(span, index)` | delete a line; deleting the active one shows the original; deleting the last one removes the span (R-2.7) | only if it was active |
| `alt_move(span, from, to)` | reorder (indices past the original) | none |
| `alt_restore(span, target_json, start, deletedLength, inserted)` | replay (undo/redo) | the recorded edit |

A refused operation throws and changes nothing on either side (each export snapshots the session and restores it on error). A successful one returns `{ span, index, before, after, edit, ...outcome }`: the span before and after (or `null`), and the single contiguous body edit (`body_edit`: common prefix and suffix, character-safe, UTF-16 offsets), if any.

`applyAltChange` applies that to the surface as **one** history step without mirroring it through `apply_edit` (`suppressModelSync`, as `moveRange` does): the body edit, or an empty edit at the span start when only annotations changed. The step carries `alt: { span, before, after }`:

- `replaceRange` copies `options.alt` onto the edit; `record` gives a step with `alt` its own entry even when the text is unchanged, and never coalesces it; `invertStep` swaps `before` and `after`; `replaySteps` copies it back.
- `mapRanges` leaves every range alone for an empty edit, so an annotation-only step does not drop a decoration that spans the span start (a ghost over the sentence, for example).
- On undo/redo, `mirrorEdit` sees `alt` and calls `mirrorAltStep`, which runs `alt_restore`: take the span out of the model, apply the recorded body edit as an ordinary edit (other anchors, ghosts and set-aside hints follow it), put the target snapshot back verbatim, refresh every anchor's context. If the model refuses, or its body differs from the surface text afterwards, the body is re-synced from the surface (`sync_document_body`), so model and surface never diverge.

Replaying snapshots rather than re-running operations makes undo exact for every operation, including the R-2.7 removal of an emptied span (undo puts the whole span back) and edits of the active line (undo restores the text, the article and the alternative).

### Making a line active: issue #9

Up/Down (and `setActiveIndex`) go through issue #9's `MarkdownEditor.swapAlternative(spanId, index)`: model first (`set_active_alternative`, with the a/an fix-up), one `swap` history step that undo and redo replay in the model. `TeAlternativesPanel.activate()` is the one call site; the panel re-reads the model afterwards. The panel has no swap path of its own.

## Undo of AI lines (R-4.6)

Provenance is the `source` field. There is no AI provider yet (#13), so AI lines come from files (or `editor.altPanel.addAlternative(text, { source: 'ai' })`). Undoing an addition removes that line whatever its source; undoing the creation of a span removes the span, so its indicator clears. When #13 lands, "AI alternatives for selection" should add its lines through `alt_add(span, text, 'ai')` (or `alt_create_span` for a span without alternatives) inside **one** step per request, so a single undo removes every bot-marked line it added and, if nothing else remains, the span and its indicator (R-4.6). That needs a batch variant of `alternativeOp` (several `alt_add` calls recorded as one step with the first `before` and the last `after`), which belongs with #13.

## Ctrl+Shift+A and the browser (R-7.4)

Ctrl+Shift+A is tab search in Chrome (and the add-ons manager in Firefox), noted in [ghosts-and-menu.md](ghosts-and-menu.md). Handling:

- The menu item is available for a non-empty selection, where the selection menu runs it and cancels the key. With a collapsed caret the panel's own keydown listener on the surface handles Ctrl+Shift+A and cancels it, so the key is always `preventDefault`ed while focus is in the editor and the document model is available. Inside the panel the panel cancels it itself (and closes). Keeping the item to selections means a right-click on a bare caret still gets the browser's own menu (the selection menu opens only when it has an item to offer).
- `teShortcutMatches` requires Ctrl, not Cmd, so on macOS Chrome's Cmd+Shift+A tab search is never involved.
- Unverified: whether Chrome on Windows and Linux lets a page cancel Ctrl+Shift+A. If the browser reserves it, the menu item and the `●●●` control still open the panel.
- No editor shortcut collides: `config.js` binds Ctrl+B/I/K/L/H (matched ignoring Shift, so a future `ctrl+a` there would also fire on Ctrl+Shift+A; keep it out).

## Lifecycle

Listeners (`te:open-panel`, `te:dot`, `te:mode-change` and `selectionchange` on `document`, the panel's own DOM events, the surface change subscription) use the panel's AbortController, which follows the editor's. `destroy()` (called by `MarkdownEditor.destroy()`) aborts them, cancels pending frames, unregisters the menu item, removes the dim class and the panel element.

While open, the panel re-reads the model on the next animation frame after a surface change (typing, undo, redo), so its list follows the document; while closed it reads nothing.

## Tests

- Native (`src/document.rs`, `#10` tests): `body_edit` (minimal, UTF-16, never splitting a character); create as one operation and its refusals (same as the original, empty, overlapping) never leaving an inert span; add (AI provenance kept, duplicates a no-op), edit inactive and active (with a/an), move (active follows), remove (active falls back to the original); removing the last alternative removes the span and undo restores it, also through save and reopen; replay moves other spans and ghosts and refuses stale or mismatched snapshots and edits that do not fit, changing nothing. Every operation is checked with undo and redo through `alt_restore` against the exact states around it.
- Browser (`tests/web_alt_panel.rs`, real Chrome, real scripts, no mocks): Ctrl+Shift+A on a selection in plain mode (Write_On turns on, pending span, no span until the first alternative); Enter adds live (dots appear); Up/Down with live document updates and the a/an fix-up; undo and redo of each operation with model and surface compared; edit active and inactive lines; Alt+Up reorder; delete by button and by emptying, the span and indicator removed at zero and restored by undo; Escape returns focus; the `●●●` control with ARIA; provenance glyphs and colours from a file's `source` fields; an AI addition undone; Ctrl+Z inside the panel and a draft keeping its own undo; tabs by keyboard, the overlap refusal, a pending sentence created and swapped; save and reopen; a dot click opening the panel without taking focus or cancelling `te:dot`; toggling with Ctrl+Shift+A and the control; leaving Write_On mode; the text helpers; `destroy()` cleanup.
