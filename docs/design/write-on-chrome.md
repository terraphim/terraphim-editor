# Write_On mode toggle and corner chrome

Issue: terraphim/terraphim-editor#7 (epic #1). Spec: `docs/requirements/alternative-control.md` R-2.1, R-7.1, R-7.2, R-7.3 and decision 1. Code: `public/js/chrome.js` (`WriteOnChrome`, `countsFor`) and `public/css/write-on.css`. Tokens: `docs/design/tokens.md`.

## Modes

- **Plain (default).** The existing editor (toolbar, split preview), plus a dim `N words M chars` counter in the top-left corner. None of the corner controls is rendered visibly: their wrapper carries the `hidden` attribute.
- **Write_On.** Clicking the counter (a real `<button>` with `aria-pressed`, so Enter and Space also work) sets `<body data-mode="write-on">`. `tokens.css` and `write-on.css` then give a full-bleed dark page and a centred `--te-measure` (70ch) column. The toolbar and the preview pane are hidden (R-7.1), and the corner controls appear. Clicking the counter again returns to plain mode.

Spec note: R-7.1 and R-2.1 describe plain mode as text plus counter only, without a toolbar. Issue #7 keeps the existing plain editor as it is and hides the toolbar only in Write_On mode, and the merged browser tests depend on the plain toolbar being there. Removing the plain-mode toolbar is a separate decision.

## Corner controls (R-7.2)

| Corner | Control | Event (bubbles from `.te-chrome`, listen on `document`) |
|---|---|---|
| top-left | `N words M chars` | `te:mode-change` `{mode}` |
| top-centre | `●●●` (lavender) | `te:open-panel` `{panel: 'alternatives'}` |
| top-centre | `M↓` (boxed) | `te:markdown` (cancelable); if not cancelled, the Markdown export dialog (issue #73) |
| top-right | keyboard (FontAwesome `fa-keyboard`) | opens the shortcut reference, then `te:shortcuts` |
| bottom-left | save (`fa-floppy-disk`, lavender) | `te:save` (cancelable); if not cancelled, `editor.saveDocument()` and the file is written (issue #76) |
| bottom-left | open (`fa-folder-open`) | `te:open` (cancelable); if not cancelled, a file is picked and opened (issue #76) |
| bottom-centre | `LAB` dashed pill | `te:lab` |
| bottom-right | `XYZ` tag | `te:overflow` |

Every event has `detail.editor`, the `MarkdownEditor`. The X and LinkedIn buttons are out of scope. Features that arrive later (alternatives panel, Markdown view, Lab, Overflow) should listen for their event.

The default actions of `te:save`, `te:open` and `te:markdown` belong to `editor.persistence` (`public/js/persistence.js`, issues #76 and #73; see `docs/design/persistence.md`), which dispatches the events from the chrome root itself. A host page that stores documents calls `preventDefault()` and does its own thing: for example it reads a file, calls `editor.openDocument(text, name)` (which calls `editor.chrome.documentChanged()`), and writes the `text` of `te:saved`. The shortcut reference's Editor section adds Ctrl+S (save) and Ctrl+O (open) when `persistence.js` is loaded.

## Counter

`countsFor(editor)` returns `editor.counts()` when that method exists (issue #6, where the counts include ghosted text). Otherwise it counts `editor.surface.getText()`: words are runs of non-whitespace, and chars are Unicode code points. The counter refreshes on every surface change, typing included.

## Toggle persistence: document identity rule

The mode is stored in `localStorage` under `terraphim-editor:write-on:<document key>`. The value is `"1"` for Write_On and `"0"` for plain. A missing value means plain. The document key is the first of these that is a non-empty string:

1. `editor.documentKey`: the save key or file name, set by persistence when available;
2. `editor.config.documentKey`;
3. `h:` followed by the FNV-1a 32-bit hash (8 hex digits) of the surface text at the moment the chrome is created, or when `chrome.documentChanged()` is called.

Every `localStorage` access is wrapped in `try`/`catch`. If storage is unavailable (private mode, full quota), the toggle still works but is not remembered. Keeping the mode inside the document's annotation block is out of scope.

## Shortcut reference (R-7.3)

The reference is a native `<dialog>` opened with `showModal()`, so Esc closes it and focus stays inside. It lists every shortcut from `public/js/config.js` (`EditorConfig.shortcuts`, the real source; `editor_config.toml` is unused), the four R-7.3 selection shortcuts (Ctrl+Shift+A, Ctrl+Shift+G, Ctrl+/, Ctrl+Shift+X) and `/` for the command palette. A config can replace the selection list with `writeOnShortcuts`.

## Lifecycle

`MarkdownEditor.initialize()` creates the chrome inside `.editor-container` and passes the editor's `AbortController` signal, so every chrome listener is removed when the editor is destroyed. `MarkdownEditor.destroy()` calls `chrome.destroy()`, which closes the reference, removes the chrome DOM, drops its surface change subscription and removes `data-mode` from `<body>` if the chrome set it.

## Tests

`tests/web.rs` (real Chrome, `wasm-pack test --headless --chrome`, no mocks, with the real stylesheets injected):
plain mode shows only the counter; a click toggles Write_On and shows every corner control in its corner; each control dispatches its event; toggling back hides them all; the mode persists across a re-init for the same document but not for another; key precedence; the reference lists every shortcut; the `countsFor` adapter; `destroy()` removes the chrome DOM and listeners.
