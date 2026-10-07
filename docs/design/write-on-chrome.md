# Write_On mode toggle and corner chrome

Issue: terraphim/terraphim-editor#7 (epic #1). Spec: `docs/requirements/alternative-control.md` R-2.1, R-7.1, R-7.2, R-7.3 and decision 1. Code: `public/js/chrome.js` (`WriteOnChrome`, `countsFor`) and `public/css/write-on.css`. Tokens: `docs/design/tokens.md`.

## Modes

- **Plain (default).** The text and a dim `N words M chars` counter in the top-left corner, nothing more (R-2.1, R-7.1). The legacy formatting toolbar and the split preview are not shown (see "Tools" below), and none of the corner controls is rendered visibly: their wrapper carries the `hidden` attribute.
- **Write_On.** Clicking the counter (a real `<button>` with `aria-pressed`, so Enter and Space also work) sets `data-mode="write-on"` on the editor root (`.te-app`) and, on the full page, on `<body>` (an embedded editor leaves `<body>` alone, issue #77; see `docs/design/embedding.md`). `tokens.css` and `write-on.css` then give a full-bleed dark page and a centred `--te-measure` (70ch) column. The toolbar and the preview pane are hidden (R-7.1), and the corner controls appear. Clicking the counter again returns to plain mode.

## Tools: the toolbar and preview in plain mode (issue #86)

Owner decision (2026-10-07, recorded on #86): plain mode matches the video, where the editor "starts out as a plain simple text editor, nothing more" (0:00-0:11). Issue #7 had kept the legacy toolbar and split preview in plain mode as a recorded deviation; that deviation is gone.

The toolbar and the preview are hidden, not removed:

- The formatting shortcuts (Ctrl+B, Ctrl+I, Ctrl+K, Ctrl+L, Ctrl+H from `config.js`) and the `/` command palette work on the surface without them, and Ctrl+S / Ctrl+O still save and open.
- The preview keeps rendering while hidden, so it is current the moment it is shown.
- The Markdown export dialog is reached through `M↓` in Write_On (one click on the counter away), through the toolbar's Markdown button when the tools are shown, or through `editor.persistence.showMarkdown()`. The corner controls stay inert in plain mode, as before.
- **The way back.** Ctrl+Shift+L, pressed anywhere in the editor, toggles the toolbar and the preview (`chrome.toggleTools()`; `chrome.setTools(on)`, `chrome.toolsVisible()`; on the embed wrapper `setTools(on)` / `toolsVisible()`). `config.tools: true` starts with them shown. The toggle is not persisted. The shortcut reference lists it in its Editor section. It is handled in the capture phase on the editor root and stops there, because the surface's formatting handler ignores Shift and would otherwise read it as Ctrl+L (link).
- The chrome mirrors the state as `data-tools="on" | "off"` on the editor root (removed by `destroy()`), and `write-on.css` hides `.toolbar`, the preview slot and the split divider on `.te-app:not([data-mode="write-on"]):not([data-tools="on"])`, so they are hidden by default. Every rule is scoped to `.te-app`, so a host page's own `.toolbar` is never touched. Every change dispatches `te:tools-change` `{visible}`.
- Write_On is unchanged: it hides the toolbar and the preview whatever `data-tools` says, and the tools state survives a round trip through Write_On.
- The Blocks view's Text | Blocks toggle lives in the toolbar, so hiding the tools switches back to the text surface and Blocks resumes when they are shown, as with Write_On (`docs/design/blocks-view.md`).

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

`MarkdownEditor.initialize()` creates the chrome inside `.editor-container` and passes the editor's `AbortController` signal, so every chrome listener is removed when the editor is destroyed. `MarkdownEditor.destroy()` calls `chrome.destroy()`, which closes the reference, removes the chrome DOM, drops its surface change subscription and removes `data-mode` from the editor root and from `<body>` if the chrome set it.

## Tests

`tests/web.rs` (real Chrome, `wasm-pack test --headless --chrome`, no mocks, with the real stylesheets injected):
plain mode shows only the counter; a click toggles Write_On and shows every corner control in its corner; each control dispatches its event; toggling back hides them all; the mode persists across a re-init for the same document but not for another; key precedence; the reference lists every shortcut; the `countsFor` adapter; `destroy()` removes the chrome DOM and listeners.
