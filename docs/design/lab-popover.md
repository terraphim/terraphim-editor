# The Lab popover and its marks

Issue: terraphim/terraphim-editor#14 (epic #1). Spec: `docs/requirements/alternative-control.md` R-8.1, R-8.2 and section 10. Code: `src/lab.rs` (WASM bridge), `public/js/lab.js` (`TeLabPopover`), `public/css/lab.css`, and a small hook in `public/js/editor.js`. Engine: `crates/terraphim_lab` (#42). Trim levels (R-8.3 to R-8.5) are issue #15 and are not part of this design; the popover leaves a slot for them.

## Flow

1. In Write_On mode the LAB pill dispatches `te:lab` (`public/js/chrome.js`). The popover toggles open above the pill.
2. Choosing an action calls `editor.alignDocumentModel(api)` (the model body must equal the surface text, as for every other model read), then `wasmBindings.lab_mark(id)`.
3. `lab_mark` reads the open document's body from the session and runs `terraphim_lab::mark(body, &config, action)`. The marks come back unchanged: `{ kind, start, end, score, reason, proposal }` in UTF-16 offsets, which are the surface's own offsets.
4. The popover puts them in the `lab-marks` layer of `editor.decorations` (`TeDecorationRegistry`). It never calls `surface.setDecorations` itself.

## WASM bridge (`src/lab.rs`)

| Function | Returns |
|---|---|
| `lab_actions()` | `[{ id, label }]` in popover order (`LabAction::ALL`, `LabAction::label()`) |
| `lab_mark(action)` | `[{ kind, start, end, score, reason, proposal }]`; throws for an unknown id |

- Ids are the engine's serde names (`typos_and_punctuation`, `weakest_sentences`, `long_sentences`, `convoluted_sentences`, `off_tone`, `hedges_and_filler`), produced and parsed through serde. There is no second table to keep in step.
- The `LabConfig` (`LabConfig::with_defaults()`: the embedded style and typo lists, no role) is built once, in a `thread_local` `OnceCell`, on the first `lab_mark` call. Page load and plain editing pay nothing for it. A config error is kept and thrown on every call, never panicked.
- **Role support is a follow-up.** `LabConfig::with_role()` needs the active role's thesaurus and rolegraph in the browser, and the editor has no role selection yet. Without a role, "Mark the weakest sentences" falls back to hedge density with text-only tie-breaks, as the engine documents.
- Native unit tests cover the core functions, which return `serde_json::Value`. The `JsValue` wrappers only convert.

## Popover

- A non-modal `role="dialog"` labelled by its header: `The Lab guide…` (display face, accent) on the left, `what each idea does` (dim) on the right. It is never `showModal()`, because the writer keeps reading and editing while marks show.
- The six actions are a `role="menu"` of `menuitemradio` buttons. `aria-checked` marks the action whose marks are shown, and a count sits at the right of that row. Each action has a FontAwesome icon.
- Below the menu, a hint line explains what the focused or hovered action does (the "what each idea does" promise).
- Then the trim slot: `<div class="te-lab-trim" data-slot="trim" hidden>`, which issue #15 fills and unhides.
- Then the results, while an action is shown: a legend, the proposed fixes, and Clear marks. Each legend swatch is a live sample (`abc`) drawn with the mark's own class, so the legend always matches the text. If nothing is marked, the legend says `<label>: nothing to mark.`
- Placement: `position: fixed`, horizontally centred on the pill and 10px above it. It is recomputed on open and on window resize.
- While the popover is closed and marks are shown, a small chip beside the pill says what is marked (for example `2 weak sentences`) and offers Clear marks. On narrow screens the chip is hidden; the popover's Clear marks remains.

### Keyboard and focus

- Opening focuses the checked action, or else the first. Up and Down move with wrap-around, and Home and End jump; the menu uses a roving `tabindex`. Enter and Space activate (native buttons). Tab reaches Accept buttons and Clear marks.
- Escape closes and returns focus to the LAB pill. A pointer press outside the popover, the pill and the chip closes without moving focus.
- The pill gets `aria-haspopup="dialog"`, `aria-expanded` and `aria-controls`.
- Re-rendering the surface (which `registry.set` does) restores the surface selection and would pull focus into the text. The popover therefore restores focus to the control the user was on (`keepFocus`). Clearing from the chip hands focus to the pill, since the chip disappears.

## Marks

| Kind (`data-lab-kind`) | Class | Treatment |
|---|---|---|
| `weak_sentence` | `te-lab-mark--weak-sentence` | lavender wash |
| `long_sentence` | `te-lab-mark--long-sentence` | amber wash, 1px amber underline |
| `convoluted_sentence` | `te-lab-mark--convoluted-sentence` | teal wash, double teal underline |
| `typo` | `te-lab-mark--typo` | rose wavy underline |
| `punctuation` | `te-lab-mark--punctuation` | rose wash with a rose baseline (the spans are often a space and a comma) |
| `off_tone` | `te-lab-mark--off-tone` | amber dotted underline |
| `hedge` | `te-lab-mark--hedge` | teal dashed underline |
| `filler` | `te-lab-mark--filler` | dim dashed underline |

- Every item also has `te-lab-mark`, an `aria-describedby` pointing at a visually hidden description (`Lab: weak sentence. <engine reason>`, plus `Proposed: "<fix>"` where there is one), and `data-lab-kind`. Only allow-listed attributes are used (`aria-*`, `data-*`).
- Marks use tints and underlines, **never opacity**. Opacity is the ghost treatment (R-5.1, "this text would go"), and a mark must not look like a cut. A browser test checks that the computed opacity is 1.
- Sentence washes use `box-decoration-break: clone`, so wrapped lines each get the tint. Nothing changes line height.
- The hues (`--te-lab-amber`, `--te-lab-rose`, `--te-lab-teal` and the washes) are defined once in `lab.css` and use the base tokens otherwise. Only one action's marks show at a time, so at most two treatments are on screen together.
- Item `data` holds `{ kind, reason, proposal, text, score }`. The registry keeps it with each item at live offsets, so the popover needs no parallel list.

### One action at a time

Running an action replaces the previous action's marks; running the same action again recomputes them. Mixing several actions' marks would make the text unreadable, and the engine output for one action is what the writer asked for.

### Transience (decision)

- **Edits:** the registry maps every layer through every surface edit (`EditorSurface.mapRanges`). Marks after the edit move, and a mark the edit lands inside is dropped. That is the definition of "clears on the next edit that touches it". An insertion exactly at a mark's edge leaves the mark alone, matching every other decoration.
- **No re-run on edit:** marks are not recomputed after edits. Weakest-sentence ranking is document-wide, so a re-run while typing would move marks under the writer's hands, and would cost a full analysis per pause. Choosing the action again refreshes it.
- **No `registry.set` in a change listener:** the popover never calls it from inside a surface change listener, because that would force a surface render mid-input or mid-undo. It only refreshes its own legend and proposals list, which touches no surface DOM.
- **Mode change:** leaving Write_On closes the popover and clears the marks.
- **Clear marks** (popover or chip) clears the layer.

## Proposals (alternatives, not rewrites)

- Typo and punctuation marks carry a `proposal`. The popover lists them as `from → to` with spaces shown as `␣`, so ` ,` → `,` is readable. Each has an **Accept** button whose accessible name is `Accept: replace "…" with "…"`.
- Accept looks the mark up at its live offsets and checks that the text under it is still exactly the marked text, refusing otherwise. It then calls `surface.replaceRange(start, end, proposal, { source: 'lab' })`.
  - Exactly the proposal is applied, as **one undo step**: only `typing` and `delete` coalesce in the surface history.
  - The edit lands inside the mark, so that mark drops. The other marks move.
  - Focus stays on the next Accept button, or on Clear marks.
- Undo restores the original text. Nothing is ever applied without an explicit Accept.
- Accepting dispatches `te:lab-accept` `{ editor, start, end, text, proposal }`. Running or clearing dispatches `te:lab-marks` `{ editor, action, count }`.

## Lifecycle

- `MarkdownEditor.initialize()` creates the popover inside the `TeDecorationRegistry` block, after the indicators, with the editor's `AbortController` signal. Every listener (document `te:lab`, `te:mode-change` and `pointerdown`, window `resize`, the popover's own controls) uses the popover's controller, which aborts with the editor's.
- `destroy()` clears the layer while the surface is still alive (the editor destroys the Lab before the registry and the surface). It also removes the popover, the chip and the descriptions, removes the pill's ARIA attributes and unsubscribes from surface changes.
- Without the WASM Lab API (`lab_mark` and `lab_actions` missing from `window.wasmBindings`), the popover still opens, with no actions and a note explaining that the WebAssembly document model did not load.

## Performance and size

- Release build, first action including the one-off `LabConfig` build: about 15 ms on the 19-paragraph fixture. A repeat run: about 1.6 ms.
- Release `_bg.wasm` (`trunk build --release`): 971,919 → 1,334,390 bytes (+362,471, +37.3%); gzip -9: 307,379 → 412,030 bytes (+104,651, +34.0%). The growth comes from the engine's dependency graph through `terraphim_automata` (`regex` with its Unicode tables, `aho-corasick`, `fst`, `bincode`, `sha2`, `terraphim-markdown-parser`). Follow-up options:
  - feature-gate those dependencies in `terraphim_automata` for the matcher-only path;
  - load the Lab as a second WASM module on first use of the LAB pill.

## Tests

- `src/lab.rs` (native): actions in popover order with the spec labels; id round trip and rejection; marks equal the engine's output with UTF-16 offsets; protected text is skipped; an unknown action is an error, not a panic; the session body is read and left unchanged; the browser fixture exercises every action outside its heading and fenced block.
- `tests/web_lab.rs` (browser, real scripts, CSS and bindings, fixture `tests/fixtures/lab/lab.md`), three tests:
  1. Header, labels and placement; for each action, the layer equals the engine's `[start, end, kind]`, with nothing protected marked, the right classes, an accessible reason, no opacity, unchanged text and an untouched undo history; Clear marks removes everything.
  2. The proposal list (spaces shown); Accept applies exactly the proposal, once, as one undo step with source `lab`; undo restores; a missing mark is refused; an edit inside a mark drops it, the later marks move by the edit and the legend follows.
  3. Roving keyboard navigation with wrap, Home and End; the hint; Escape returns focus to the pill and `aria-expanded` follows; focus stays in the popover when an action runs; the chip clears and hands focus to the pill; an outside press closes; plain mode closes and clears; destroy removes the DOM, the marks, the pill attributes and the listeners.
