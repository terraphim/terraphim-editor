# Ghosts and the selection context menu

Issue: terraphim/terraphim-editor#11 (epic #1). Spec: `docs/requirements/alternative-control.md` R-5.1 to R-5.3, R-7.3, R-7.4, R-9.3 and decisions 3 and 2026-10-04 (ghost layer). Code: `public/js/selection-menu.js` (`TeGhostLayer`, `TeSelectionMenu`), `public/css/selection-menu.css`, the `ghost_range` and `revive_range` exports in `src/document.rs`, and a small hook in `public/js/editor.js`. Tokens: `docs/design/tokens.md`.

## What the user sees

- Select text and right-click (or press the Menu key or Shift+F10): a dark rounded menu opens near the selection. It has monospace labels on the left and dim shortcuts on the right, and the hovered or keyboard-active row is slightly lighter. Today it holds one item, **Ghost it** (Ctrl+/), or **Revive** when the selection is already entirely ghosted.
- Ghosted text recedes to about 10% (`--te-ghost-opacity`). It stays in the document: you can still read, select and edit it, and it is still counted (R-5.1, decision 3). Export drops it (R-9.3). Save keeps it, in the block's `ghosts` list.
- Ctrl+/ toggles the selection with no menu: if the selection is entirely ghosted it is revived, otherwise it is ghosted.
- With a collapsed caret, right-click or Ctrl+/ inside a ghost offers Revive for that whole ghost. R-5.2 says "right-click ghosted text"; right-clicking with no selection selects nothing in most browsers, so this case needs its own handling. With a collapsed caret outside any ghost, the native context menu is left alone and Ctrl+/ does nothing.

## The ghost layer (R-5.3)

The WASM document model owns the ghost rules (`terraphim_alternatives::Document::ghost` / `revive`). The editor does not re-implement them:

| Case | Result (from the model) |
|---|---|
| Ghost a range that covers or partly overlaps spans with alternatives | allowed; spans are untouched (independent layers) |
| Ghost over or touching an existing ghost | merged into one ghost; the first ghost's id is kept |
| Revive a range inside a ghost | the ghost is split; the left part keeps the id, the right part gets a fresh one |
| Revive across one end of a ghost | the ghost is trimmed |
| Revive a range covering a ghost | the ghost is removed |
| Edit before or after a ghost | the ghost moves |
| Edit inside a ghost | the ghost resizes (elastic) |
| Delete all of a ghost's text | the ghost is removed |

### Bridge (`src/document.rs`)

- `DocumentSession::ghost(start, end) -> Result<String, EditError>` and `revive(start, end) -> Result<bool, EditError>` delegate to the crate. Offsets are UTF-16 code units, like every surface offset. On error the model is unchanged.
- `ghost_range(start, end)` returns `{ ok: true, id, annotations }`. `revive_range(start, end)` returns `{ ok: true, changed, annotations }`. Either returns `{ ok: false, error, kind }` on failure, where `kind` is `invalid-range` (reversed, empty, out of bounds, or splitting a surrogate pair), `stale` or `other`. They never throw. `annotations` has the same shape as `document_annotations()`, so the caller can redraw without reading the model a second time.
- Native unit tests cover the result shape, merging of touching and overlapping ghosts, ghosts covering or cutting spans, split, trim and remove on revive, error objects (the session is compared before and after), surrogate pairs, and ghosts following edits through save and reopen with export dropping them.

### Drawing (`TeGhostLayer`, `editor.ghosts`)

- The layer owns the `ghosts` layer of the decoration registry (`editor.decorations`, issue #8). It never calls `surface.setDecorations` directly. Items have the class `te-ghost` and **no attributes**: where a ghost and an indicator share text, the surface renders one span with both classes. Attributes from the later decoration win on that span, so a ghost attribute could overwrite the indicator's `aria-describedby`.
- **Fade via the text colour.** `.te-ghost { color: color-mix(in srgb, currentColor calc(var(--te-ghost-opacity) * 100%), transparent) }`. `opacity` would also fade the selection highlight, which makes ghosted text effectively unselectable, and it would fade the indicator underline. With the colour approach the `::selection` background and the underline (which sets its own `text-decoration-color`) stay at full strength. The indicator dots are in an overlay and are not affected either. A `@supports` fallback uses `opacity` where `color-mix` is missing.
- **Refresh.**
  - After `ghost`/`revive` the layer redraws at once from the returned annotations.
  - Typing outside a ghost needs no model read. The registry moves the decoration with the text, and a trailing debounce (120 ms, as for the indicators) confirms it against the model.
  - An edit inside a ghost makes the surface drop that decoration (`EditorSurface.mapRanges`) while the model resizes the ghost. The layer notices that the layer shrank and redraws on the next animation frame, so the ghost does not flash at full strength.
  - When the model holds no ghosts at all (live or set aside), no edit can create one, so typing reads nothing. In particular, plain mode with no ghosts costs nothing per keystroke.
  - `flush()` reads the model now. `openDocument()` calls it.
- Each change dispatches a bubbling `te:ghosts-change` event from the surface with `detail: { editor, action, start, end, id, changed, ghosts }`. Ghost state is not text, so no `input` event fires. Whoever owns storage or autosave should listen for this event.

### Undo

Ghosts are annotations, not text. Ghost and revive are therefore **not** entries in the surface's text undo history, and Ctrl+Z does not undo a ghost. Ctrl+/ (or the menu) is the reversal. Putting annotation changes into the text history would make Ctrl+Z after a ghost appear to do nothing to the text, and would need the model to snapshot and restore its ghost layer, which it does not offer.

Text undo and redo still move ghosts, because the model mirrors every replayed edit. One consequence of the elastic rule: undoing an edit that deleted a ghost's whole text brings the text back without the ghost, and redo does not restore it either.

## The selection context menu (R-7.3)

### Plug-in items

Items are registered and shown in the R-7.3 order (`TeSelectionMenu.SLOTS`: `alternatives`, `ai-alternatives`, `ghost`, `stash`). Any other id follows in registration order. An item that is not implemented, or whose `available(ctx)` returns false, is **not rendered at all**: items are hidden, never disabled. Only `ghost` is built in. Issues #10, #13 and #12 add theirs:

```js
editor.selectionMenu.register({
  id: 'alternatives',                     // R-7.3 slot
  label: 'Alternatives for selection',    // or (ctx) => string
  key: 'ctrl+shift+a',                    // shown dim on the right, and handled on the surface
  available: (ctx) => !ctx.collapsed,     // ctx: { editor, start, end, collapsed, text, selection }
  run: (ctx) => { /* ... */ },
});
```

`register` returns an unregister function. An item's shortcut works on the surface whenever the item would be available, and also inside the open menu.

### Behaviour

- **Opening.** A `contextmenu` event over the surface opens the menu at the pointer. The Menu key or Shift+F10 opens it under the end of the selection. When no item is available nothing opens and the native menu appears. On Windows the Menu key sends both keydown and `contextmenu`; the second event is swallowed while the menu is open.
- **Position.** `position: fixed` and clamped to the viewport. It flips above the point when there is no room below.
- **Keyboard.** ArrowDown and ArrowUp move between items and wrap. Home and End jump to the first and last item. Enter or Space runs the active item. Escape or Tab closes the menu and returns focus and the original selection to the surface. Focus moves to the first item on open (roving focus, `preventScroll`).
- **Running an item.** The menu closes, the surface gets focus and the selection captured at open back, and then the item runs. This order matters: the surface restores the selection after a decoration re-render only while it is focused.
- **Closing.** The menu also closes on a mousedown or touch outside it, on any scroll (a capture listener, so scrolling containers count), on resize, on window blur, and when focus moves elsewhere.
- **ARIA.** The menu has `role="menu"` with `aria-label="Selection actions"` and `aria-orientation="vertical"`. Items have `role="menuitem"` and `aria-keyshortcuts` (for example `Control+/`); the visible shortcut text is `aria-hidden`. While the menu is open the surface carries `aria-controls`. The active row also gets a 1px accent focus ring, so keyboard focus is visible beyond the subtle hover colour.
- **Lifecycle.** Listeners use the editor's AbortController signal, plus one controller per open menu that is aborted on close. `MarkdownEditor.destroy()` destroys the menu (removing its element) and the ghost layer (clearing its decorations) before the indicators, the registry and the surface.

### Modes

The menu and ghosting work in **both plain and Write_On mode**. The spec describes the menu in Write_On, but ghosting is document state that is saved, exported and counted, so it must not disappear when the mode changes. The tokens are defined on `:root`, so the menu keeps the same dark rounded look in plain mode. In plain mode the faded text is the body colour at 10% on white, which is faint, but that is what the spec asks for.

## Shortcut collisions (R-7.4)

| Shortcut | In this editor | Notes |
|---|---|---|
| Ctrl+/ | Ghost it / Revive (implemented) | `config.js` and `editor_config.toml` bind only Ctrl+B/I/K/L/H. The command palette opens on `/` only without Ctrl. The surface takes only Ctrl/Cmd+Z/Y. No browser binding was found for Ctrl+/ in page content. Matched by `key === '/'`, or by `code === 'Slash'` without Shift. Shift is ignored when the `/` key itself arrives, because some layouts (for example German, Shift+7) need Shift to type `/`. |
| Cmd+/ | not handled | Ctrl is not aliased to Cmd: Safari binds Cmd+/ to the status bar, and macOS browsers bind Cmd+Shift+A and Cmd+Shift+G. A browser test checks that Cmd+/ is left alone. |
| Ctrl+Shift+A, Ctrl+Shift+G, Ctrl+Shift+X | hidden until #10, #13, #12 register them | `MarkdownEditor.setupShortcuts` matches config shortcuts as `ctrl+<key>` and **ignores Shift**. A future `ctrl+a`, `ctrl+g` or `ctrl+x` entry in `config.js` would therefore also fire on the R-7.3 Ctrl+Shift item; avoid those keys or make that matcher Shift-aware. |
| Menu key, Shift+F10 | opens the menu when an item is available | otherwise the browser's own menu |

Known browser-level risks for the hidden items. These are noted for #10, #12 and #13 and have not been checked here: Ctrl+Shift+A is tab search in Chrome and the add-ons manager in Firefox; Ctrl+Shift+G is "find previous" in several browsers; Ctrl+Shift+X switches text direction in Firefox text fields on Windows and Linux. Pages can usually cancel these with `preventDefault` on keydown, but browser-reserved shortcuts cannot be cancelled. Verify each item when it lands.

## Tests

- Native unit tests (`src/document.rs`): see "Bridge" above.
- Browser tests (`tests/web_ghosts.rs`, real scripts, stylesheets and WASM exports; nothing is mocked):
  - ghost and revive through the menu (roles, labels, tokens, focus, position) and through Ctrl+/;
  - collapsed-caret behaviour inside and outside a ghost;
  - a ghost over a sentence containing a word with alternatives (shared span, faded colour, underline colour and `aria-describedby` kept, dots shown);
  - a partial revive splitting a ghost, and merging it back;
  - the change event and the error object;
  - save and reopen, and export without the ghosted text;
  - typing before a ghost (decoration moved at once with no model read, then one debounced confirmation) and inside it (redrawn on the next frame);
  - keyboard navigation with a second item registered through the plug-in API, and items hidden when unavailable;
  - outside click and scroll closing the menu, Cmd+/ ignored, Ctrl+B unaffected;
  - `destroy()` cleanup.
