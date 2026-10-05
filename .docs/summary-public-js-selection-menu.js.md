# Summary: public/js/selection-menu.js

## Purpose
Ghost layer and selection context menu (#11, spec R-5.1 to R-5.3, R-7.3, R-7.4). Design notes: `docs/design/ghosts-and-menu.md`. Styles: `public/css/selection-menu.css`.

## Key points
- `TeGhostLayer` (`editor.ghosts`): `ghost` / `revive` / `toggle` (Ctrl+/) / `targetFor` / `flush`. Merge, trim, split and elastic edits live in the WASM model, reached via `ghost_range` / `revive_range` in `src/document.rs` (`{ ok, id | changed, annotations }` or `{ ok: false, error, kind }`); UTF-16 offsets.
- Draws the `ghosts` layer of `editor.decorations` (class `te-ghost`, no attributes, so indicator `aria-describedby` survives). The fade is on the text colour using `--te-ghost-opacity`, so the selection highlight and indicator underlines stay visible.
- Refresh: immediate after ghost/revive; 120 ms debounce after typing; next animation frame when an edit lands inside a ghost; nothing while the model has no ghosts. Fires `te:ghosts-change`.
- Ghost and revive are not in the text undo history (annotations); Ctrl+/ reverses them.
- `TeSelectionMenu` (`editor.selectionMenu`): ARIA menu on right-click over a selection, the Menu key or Shift+F10; keyboard navigable; closes on outside click, scroll, resize, blur or focus loss. Items plug in via `register({ id, label, key, available, run })` in R-7.3 slot order; unavailable items are hidden. Only Ghost it / Revive is built in.
- Shortcuts match Ctrl exactly (no Cmd alias). Works in plain and Write_On mode.
- Loaded after `indicators.js`, before `editor.js`. Tests: `tests/web_ghosts.rs` and the `src/document.rs` unit tests.
