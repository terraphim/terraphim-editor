# Blocks view (issue #19)

The Blocks view shows the open document as a vertical list of top-level
Markdown blocks: headings, paragraphs, lists, code fences, blockquotes,
tables and rules. Each block is rendered with the same Rust converter as the
preview and can be edited on its own. It sits beside the plain editor and
Write_On mode. It is not part of the Write_On specification, and it never
changes how Write_On behaves.

Implementation: `public/js/blocks.js` (`window.BlocksView`),
`public/css/blocks.css`, two small hooks in `public/js/editor.js`
(`MarkdownEditor.initialize()` and `destroy()`) and
`EditorSurface.squashHistory` (one undo step for a block move). Tests:
`tests/web_blocks.rs`, `tests/web_blocks_edit.rs` and
`tests/web_blocks_move.rs`, with fixtures in `tests/fixtures/blocks/`.

## Single source of truth

The view has no copy of the document. The Markdown body held by the
document model (`src/document.rs`, mirrored exactly by the editing surface)
is the only content:

- Blocks are parsed on demand from `editor.surface.getText()`. The annotation
  block (`terraphim-alternatives`) never reaches the surface, so it is never
  shown in a block, and because the model keeps it, it is never lost.
- `BlocksView.parse(text)` returns `{ lead, blocks }`. Each block is
  `{ type, level, start, end, text, sep }`, where `text` is
  `body.slice(start, end)` and `sep` is the whitespace up to the next block.
  `lead` holds leading blank lines. `BlocksView.serialise(model)` joins them
  back together, so body to blocks to body is byte-identical for any input
  by construction. The block type is a label only, so a wrong
  classification can never change the text.
- Committing a block edit works out the smallest changed region with
  `EditorSurface.diff(original, edited)` and applies it with
  `surface.replaceRange(start, end, insert, { source: 'blocks' })`. Typing
  uses the same path, so the surface maps decorations, `mirrorEdit` sends the
  edit to the span model with `apply_edit`, and alternatives and ghosts
  outside the changed region keep their anchors and are saved. The edit is
  also one undo step, and the `input` event re-renders the preview.
- Edits are committed on blur, on Ctrl+Enter or when switching back to the
  text view. They are never applied per keystroke, because `blocks` is not
  a coalesced undo source.

### Parsing rules

The parser is line-based and follows CommonMark closely enough to label
blocks sensibly:

| Block | Starts with | Ends |
|---|---|---|
| code (fenced) | ```` ``` ```` or `~~~` (up to 3 spaces of indent) | a closing fence of the same character that is at least as long, or the end of the text (blank lines and `#` lines inside are part of the code) |
| heading | `#` to `######` and a space, or a paragraph line followed by `===` or `---` (setext) | that line |
| rule | `***`, `---`, `___` (spaced or not) | that line |
| quote | `>` | a blank line (lazy continuation lines are included) |
| list | `-`, `+`, `*`, `1.` or `1)` | a blank line, unless the next content line is indented or is an item with the same marker |
| code (indented) | 4 spaces or a tab, outside a paragraph | the last indented line |
| table | a line with `\|` followed by a delimiter row | a blank line |
| paragraph | anything else | a blank line, or a heading, fence, quote, rule or bullet item |

## Controls and modes

- **Plain mode:** a "View" button group (Text | Blocks) is added to the
  toolbar after the formatting buttons. It uses FontAwesome icons
  (`fa-align-left`, `fa-layer-group`), `aria-pressed` and the primary variant
  for the active view.
- **Write_On mode:** there is no Blocks control. Write_On is the
  distraction-free writing surface defined by the spec, and its toolbar is
  hidden by `write-on.css`. Entering Write_On while Blocks is showing
  switches back to the text surface (with the caret on the focused block).
  The Blocks view comes back when plain mode returns. Calling
  `setView('blocks')` while in Write_On does nothing. `blocks.css` also
  hides the view and its toggle under `body[data-mode="write-on"]` as a
  safeguard.
- **Events:** every switch dispatches a bubbling `te:view-change`
  `{ view: 'text' | 'blocks', editor }` from the blocks container, matching
  the chrome's `te:*` convention.
- **Preference:** the last view is stored per viewer under the localStorage
  key `terraphim-editor:view` (`text` or `blocks`), inside try/catch. It is
  restored when the editor starts (never in Write_On). It is a UI preference
  only. No document content is ever stored there.

### API (`editor.blocks`)

`view`, `isActive()`, `setView(view, { persist, focus })`, `toggle()`,
`model` (the last parse), `cards()`, `editBlock(index)` (returns the
textarea), `commitEdit()`, `cancelEdit()`, `insertBlockAfter(index)`
(index -1 inserts at the top), `deleteBlock(index)`,
`moveBlock(index, 'up' | 'down', { focusAction })` (returns whether it
moved), `undo()`, `redo()`,
`keptDrafts()`, `pendingDrafts()`, `draftTarget(draft)`, `applyDraft(id)`
(returns `'replace'` or `'insert'`), `discardDraft(id)`, `destroy()`
(returns the unapplied drafts), and the
static helpers `BlocksView.parse`, `BlocksView.serialise`,
`BlocksView.indexIn` and `BlocksView.planMove`.

## Moving blocks (#58)

Move up and Move down swap a block with its neighbour. They use
`MarkdownEditor.moveRange`, the model-level move from #44, so every span and
ghost inside the block travels with it, keeping its id, alternatives and
active index. There is never a fallback to a deletion plus an insertion,
which would set the ghosts aside.

- **Range.** Block `i` moves as its text plus its trailing separator,
  `[start, next block start)`, or up to the end of the text for the last
  block. Down moves it to the start of block `i + 2` (the end of the text
  when block `i + 1` is the last); up moves it to the start of block `i - 1`.
- **Separators.** A pure move carries each block's trailing separator with
  it, giving `Q.text Q.sep P.text P.sep` for the pair `P, Q`. When the two
  separators differ, whitespace-only edits put them back in place: the gap
  that was between the pair stays between the pair, and the gap after the
  pair stays after it, so the document keeps its trailing text (no trailing
  newline stays no trailing newline). A gap that would separate two blocks
  without a blank line (for example a heading followed by a single newline)
  becomes one blank line. These edits sit at block edges, where an insertion
  is outside every span and ghost. They are ordinary edits, though, so an
  item that covers a separator being repaired (for example a ghost over a
  paragraph and its trailing blank line, which the move carries) would be
  trimmed or grown by them. Before anything changes, `fixUpBlocker` maps
  every span and ghost through the planned move (`mapThroughMove`, the
  model's move rules) and checks each fix-up against it: an insertion
  strictly inside an item, or a deletion overlapping one, refuses the whole
  move with a message naming the item, in the same notice region. When no
  fix-up is needed (equal separators) such an item is simply carried.
- **Checked first.** `BlocksView.planMove` builds the predicted text and
  parses it. It must give the same blocks in the new order (and the same
  leading blank lines); otherwise the move is refused with "it would merge
  with the block next to it" (for example an unclosed code fence moved above
  a paragraph would swallow it).
- **One undo step.** The move and its fix-ups are recorded as separate
  history entries and then folded into one with
  `EditorSurface.squashHistory(count, 'move')`. The merged entry keeps
  every step in order, so undo replays the fix-ups backwards and then the
  inverse move as a move in the model, and redo replays them forwards. Text
  and annotations both return.
- **Refused moves.** The model refuses a move that would split a span or a
  ghost, typically a ghost running across a block boundary. Nothing changes
  (no text, no model change, no history entry) and the model's message,
  which names the item (`moving the text would split "g1"; move whole spans
  and ghosts only`), is shown after "Block not moved:" in the Blocks notice
  region at the top of the view, with a dismiss button. The live region
  (`.te-blocks-notice-region`, `role="status"`, `aria-live="polite"`) is
  always rendered and only the message box inside it (`.te-blocks-notice`)
  is hidden when empty, because a region that appears together with its
  text is not reliably announced. The next successful move clears
  it. Focus stays on the card.
- **Boundaries.** The first block has no Move up button and the last no
  Move down button (the buttons are `hidden` and out of the tab order).
  Alt+ArrowUp on the first block and Alt+ArrowDown on the last are consumed
  and do nothing; `moveBlock` returns false and records nothing.
- **Focus.** Focus follows the moved card. A move from a button keeps focus
  on the same button of the moved card, so it can be pressed again, unless
  that button is now hidden, in which case the card gets focus. After undo
  the focus goes to the block the surface caret lands in, as for every
  other undo.
- **Drafts.** Moving while a block editor is open commits the editor first,
  as every structural action does: the draft is applied, or kept in the
  drafts notice if its block changed, and is never lost. The index of the
  block to move is mapped through the commit when it added or removed
  blocks before it. A press on a move button does not blur the textarea
  first (its `mousedown` is cancelled while editing), so the click is not
  lost to the re-render. Kept drafts follow their block through a move and
  through its undo and redo: a surface change that carries a move is mapped
  with the model's move rules (inside the moved text it travels with it,
  between the text and the destination it shifts by the moved length,
  elsewhere it stays), never as one big replacement, so "Replace block" and
  "Insert as new paragraph" still target the right block. A move edit is
  never treated as a whole-document replacement, even when it rewrites the
  whole text, and an open editor whose block lies wholly in one zone of the
  move stays open.
- **Alt+Arrow.** The keys are handled on a focused card only. Inside a block
  editor they are left alone (Option+Arrow moves the caret on macOS). The
  text surface's own Alt+ArrowUp / Alt+ArrowDown (cycling alternatives, #9)
  listen on the surface root, which is hidden while the Blocks view shows
  and is not an ancestor of the list, so the two never meet.

## Drafts are never lost

The body can change while a block editor is open: an integration calling
`surface.replaceRange` or `setText`, undo or redo from elsewhere, or
`openDocument` from the open flow. Each surface change event carries the
exact edit `{ start, deletedLength, insertedText }`, and the open editor's
range `[start, start + original.length)` is mapped through it.

- **Edit entirely before the block** (it ends at or before the block start):
  the range shifts by the length change. **Edit entirely after the block:**
  nothing changes. In both cases the textarea stays open with its draft,
  focus and selection, and the other cards are rebuilt around it. The
  editor's card is never detached. While the textarea has focus its
  selection is the document selection, which the surface moves on every
  edit, so the view remembers it on `selectionchange`, `select` and `keyup`
  and puts it back after the rebuild.
- **Edit that overlaps the block, or replaces the whole document**
  (`openDocument`, whose change has source `open`, or `setText`, which
  replaces everything): the editor closes and the draft goes into a
  visible, non-blocking notice (`.te-blocks-drafts`, `role="region"`,
  `aria-live="polite"`). The notice sits above the surface and the blocks,
  so it shows in both views. If the textarea had focus, focus moves to the
  notice. Each kept draft stores its `originalText` (the block text when
  editing began) and a position: the start of the overlapping edit, mapped
  through every later edit, or the block index when the whole document was
  replaced. Each draft offers two buttons:
  - **Replace block**, shown when the block at the draft's position still
    holds exactly `originalText` (for example after undo restored it). It
    replaces that block using the minimal diff, so annotations outside the
    changed text survive.
  - **Insert as new paragraph**, shown otherwise. It inserts the draft as a
    new paragraph after the block at the draft's position, or at the block
    index after a whole-document replacement. Unrelated text is never
    replaced.

  The label is recomputed after every body change, so it always says what
  the button will do. Either action is one undo step. **Discard** removes
  the draft.

  A draft identical to the block's text (nothing typed) is not kept, since
  nothing would be lost.
- **Escape never discards typed text; Discard in the notice does.** Escape
  closes the editor. If the textarea differs from the block's original text
  (or is a non-blank new paragraph), the draft moves into the same notice,
  with the same entry shape and buttons as an overlap draft. The notice's
  `aria-live` region announces it, and focus returns to the card. An
  unchanged draft just closes.
- **Safety net:** a commit checks that the block's original text is still at
  the mapped range. If it is not, the draft goes to the notice instead of
  being applied.

Kept drafts are UI state for the current editor session and are never
written to storage.

### Teardown reports drafts and never edits

`BlocksView.destroy()`, which `MarkdownEditor.destroy()` calls, does not
change the document, because the host may already have saved it. It:

1. collects the pending drafts, which is the same list `pendingDrafts()`
   returns: every kept draft (oldest first), then the open editor's draft if
   it differs from the block's text (or is a non-blank new paragraph). Each
   is `{ value, originalText, anchor, index, isNew }`, where `anchor` is a
   body offset, or `null` after a whole-document replacement (use `index`);
2. if there are any, dispatches one bubbling `te:blocks-drafts` CustomEvent
   from the surface root, with `detail: { editor, drafts }`, before any DOM
   is removed;
3. removes its DOM and returns the array. `MarkdownEditor.destroy()` returns
   it too.

With nothing pending there is no event and the returned array is empty. A
second `destroy()` returns `[]`. A host can call `editor.blocks.pendingDrafts()`
(a read-only copy) to check before tearing down.

## Caret and selection

- **Text to Blocks:** the block that holds the surface selection start
  (its text or the separator after it) gets focus.
- **Blocks to Text:** if a block is being edited, the edit is committed and
  the textarea selection is mapped to the body (`block.start` plus the
  textarea offsets). Otherwise the caret goes to the start of the focused
  block. The surface is shown and focused, and the selection is set
  explicitly rather than trusting the DOM selection inside the hidden
  surface.
- **Undo:** the surface history is the only history. In the Blocks view,
  Ctrl+Z and Ctrl+Shift+Z or Ctrl+Y call `surface.undo()` and
  `surface.redo()`, and the cards re-render with focus on the block the
  caret landed in. A block edit is one undo step in either view.

## Accessibility

- The list has `role="list"`, each card has `role="listitem"`, and the cards
  use a roving tabindex: one card is in the tab order. Each card's
  `aria-label` names its type and position and lists the keys ("Paragraph,
  block 2 of 5. Enter to edit, Shift+Enter to add a paragraph below, Delete
  to remove, Tab for actions."), and `aria-keyshortcuts` lists them too.
- Keyboard: ArrowUp and ArrowDown move between blocks, Home and End jump to
  the first and last, Enter or F2 edits, Shift+Enter adds a paragraph below,
  Alt+ArrowUp and Alt+ArrowDown move the block up and down, Ctrl+Enter
  commits, Escape closes the editor, and Delete removes the block (which can
  be undone).
- Every action is reachable by Tab. The per-card action buttons (edit, move
  up, move down, add below, delete) follow the roving tabindex: only the
  focused card's visible buttons are in the tab order, so the order is card,
  edit, move up, move down, add below, delete, then "Add paragraph" (no move
  up on the first block, no move down on the last). Escape on an action
  button returns to its card. Each button carries `aria-keyshortcuts`
  (`Enter F2`, `Alt+ArrowUp`, `Alt+ArrowDown`, `Shift+Enter`, `Delete`) and
  shows its key in its label. The buttons become visible when the card
  is hovered or has focus inside it.
- Focus is always visible: a focused card gets a 2px `--te-color-accent`
  outline, and so do the action buttons and the draft notice buttons when
  focused from the keyboard.
- Transitions are switched off under `prefers-reduced-motion`.

## Lifecycle

`MarkdownEditor.initialize()` creates the view after the Write_On chrome, so
the view can see the current mode, and passes the editor's AbortController
signal. Every listener (toolbar buttons, list keydown, click, focus,
textarea input and blur, and the document-level `te:mode-change`) uses that
signal. `surface.onChange` returns an unsubscribe function, which
`destroy()` calls. `destroy()` also removes the container, the drafts notice
and the toolbar group and shows the surface again. The constructor is cheap: it builds the
toggle and an empty hidden container, and parses only when the view is
shown, because every editor instance (including those in other test
binaries) creates one.

## Reused from and dropped from `wip/blocks-view`

The old branch was written for the textarea editor. Its "blocks" were LLM
pipeline steps (generation, critic, parallel and validator blocks with
prompts and model settings), not Markdown blocks.

Reused:

- The interaction pattern: a Text | Blocks toggle in the toolbar and a card
  list with per-card edit and remove actions.
- The idea of keeping something in localStorage, now limited to the
  per-viewer view preference.

Dropped:

- **The block model and `BlocksStore`** (a localStorage copy of the blocks).
  A second copy of the content would drift from the body and the
  annotations. The body is now the only source.
- **Import from Text, Export to Text and the Auto Sync switch.** The view
  edits the canonical body directly through `replaceRange`, so there is
  nothing to import, export or keep in sync. Every edit already lands in the
  body, the preview and the model.
- **`plugins/flow.js` (`EventBus`, `CommandActions`, a second slash menu and
  a second `MarkdownEditor`).** The editor already has its own command
  palette, and the chrome already defines the `te:*` CustomEvent
  convention.
- **Move up and down as delete plus insert.** On the old branch this
  reordered pipeline steps in the store. On the body, deleting text and
  inserting it elsewhere releases the ghosts inside it (as cut and paste
  does): a test showed `g1` missing from the saved document after a swap.
  Moving came back in #58 on top of the model-level move (#44); see "Moving
  blocks".
- **The LLM pipeline builder itself** (generation, critic, parallel and
  validator steps) is a separate feature, tracked as issue #43.
- **The `criterion` change** on that branch is obsolete; issue #20 handled
  it.

## Follow-ups

- **Moving blocks.** Done: #44 added the model move and #58 the Blocks
  controls (see "Moving blocks"). Moving a block more than one place at a
  time (drag and drop) is not implemented.
- **Inline indicators (#8).** When the decoration registry lands, cards
  could show the alternatives and ghosts inside each block.
