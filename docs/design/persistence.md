# Save, open, drafts and the Markdown export view

Issues: terraphim/terraphim-editor#76 (save and open store and load real files in the standalone editor) and #73 (the `M↓` control does nothing). Spec: `docs/requirements/alternative-control.md` R-2.5, R-6.5, R-7.2, R-9.1 to R-9.3. Code: `public/js/persistence.js` (`TePersistence`) and `public/css/persistence.css`. Serialisation is unchanged: `editor.saveDocument()` (body plus the trailing `terraphim-alternatives` block, `save_document` in `src/document.rs`), `editor.openDocument(text, name)` and `editor.exportDocument()` (`export_document`).

## Where it plugs in

`MarkdownEditor.initialize()` creates `editor.persistence` last, after the chrome and the panels, and passes the editor's `AbortController` signal; `MarkdownEditor.destroy()` calls `persistence.destroy()`. Script order: after `overflow.js`, before `editor.js` (`index.html`, `scripts/build-dist.sh`, `tests/support/mod.rs`).

The three entry points dispatch a cancelable, bubbling event with `detail.editor` from the chrome root (or the surface when there is no chrome) and run their default action unless a listener calls `preventDefault()`:

| Entry point | Triggers | Event | Default action |
|---|---|---|---|
| `save()` | floppy (Write_On), toolbar Save (plain), Ctrl+S / Cmd+S | `te:save` `{available}` | serialise, keep a draft, write the file |
| `open()` | folder (Write_On), toolbar Open (plain), Ctrl+O / Cmd+O | `te:open` `{available}` | pick a file and open it |
| `showMarkdown()` | `M↓` (Write_On), toolbar Markdown (plain) | `te:markdown` | the export dialog |

Other events: `te:saved` `{text, documentKey}` (from `saveDocument()`, as before), `te:written` `{name, method: 'file' | 'download', text, export?}`, `te:opened` `{name, text}`, `te:dirty-change` `{dirty}`, `te:draft-restored` `{text}`. Hosts that store documents themselves (a server, a VS Code webview) cancel `te:save` / `te:open` and keep using `te:saved` and `editor.openDocument()`.

`chrome.save()` still returns the saved text synchronously (or `null`), so existing callers keep working; the file write is asynchronous and exposed as `persistence.lastWrite` (a promise of `true` when written). Without the WASM document model nothing is saved or opened (there is nothing to serialise annotations with): the events are still dispatched with `available: false`.

## Saving

- **File System Access API** (Chromium: `window.showSaveFilePicker`). The first save asks for a file; the picker is called synchronously inside the click or key press, so it has user activation. The handle is kept per editor and later saves write to it without asking. A file opened through the picker, or dropped in Chromium (`DataTransferItem.getAsFileSystemHandle()`), keeps its handle, so saving writes back to it. After a first save-as, the file name becomes the document key; the Write_On mode is copied to the new key and the draft moves with it. Cancelling the picker leaves the document dirty. If the picker refuses for another reason (no user activation, policy), the save falls back to a download.
- **Fallback** (Firefox, Safari, or `config.fileSystemAccess === false`): a download through a `Blob` and an `<a download>` appended to the page, clicked and removed; the object URL is revoked after 30 s or on `destroy()`.
- **Suggested name** (`teSuggestedFileName`): the open file's name; else the document key when it is a name (not the chrome's `h:` content hash), with `.md` added unless it already ends in `.md`, `.markdown` or `.txt`; else a slug of the first ATX heading; else `untitled.md`.

## Opening

`showOpenFilePicker` (accepting `.md`, `.markdown`, `.txt`), or a hidden `<input type=file>` with the same `accept`. Dropping a file opens it when the drop lands on the editor container, the chrome, the alternatives, Overflow or Blocks panels, or the bare page (the Write_On margins); a drop elsewhere on a host page is left to the host. Only drags whose `types` include `Files` are handled (`dragover` is accepted so Chrome does not navigate to the file), so text drags, the surface's own drop and the Overflow panel's drag and drop are untouched. Anything that is not Markdown or text shows a notice.

The file is read first and only then, if the document is dirty, a modal dialog asks **Save first / Discard changes / Cancel** (Escape cancels). Reading first matters: file pickers consume user activation, so asking before picking would leave the picker without it. "Save first" runs the normal save; if that is cancelled or fails, nothing is opened. The text goes through `editor.openDocument(text, name)`, which restores alternatives, ghosts and overflow and calls `chrome.documentChanged()`; the file name becomes `editor.documentKey`.

## Dirty state

An edit on the surface marks the document dirty at once: a dot on the floppy and on the toolbar Save button (`.te-dirty`, accent colour), the floppy's label becomes "Save document (unsaved changes)", `document.title` gets a `• ` prefix (disable with `config.dirtyTitle: false`) and `te:dirty-change` fires. After `config.autosaveDelay` ms without edits (default 1000) the document is serialised (`save_document` after aligning the model, without dispatching `te:saved`) and compared with the last saved or opened text. That tick also runs after `input`, `keyup` and `pointerup` anywhere on the page, so annotation-only changes (an alternative added in the panel, a ghost, Overflow typing) mark the document dirty, and undoing back to the saved text clears the flag. Per keystroke the cost is a flag and a timer reset; serialising happens only on the idle tick. Opening and restoring a draft do not count as edits. A completed write records the clean state; with the download fallback the download is taken as the save.

## Drafts

On the tick, on save, on `pagehide` and on `destroy()`, the serialised document is stored in `localStorage` under `terraphim-editor:draft:<document key>` as `{v: 1, text, savedAt, name}`. The document key is the same as the chrome's: `editor.documentKey`, `config.documentKey`, or `h:` plus the hash of the text the page started with (so the welcome page has a stable key across reloads). Every access is wrapped in `try`/`catch`; when storage is unavailable or full, saving to a file still works.

When an editor starts, or a file is opened, a draft for its key is offered if its text differs from the opened document and, for a file, it is newer than the file's `lastModified`. The offer is a non-blocking notice (a labelled region with a polite live text, Restore and Discard buttons) at the bottom centre; it does not take focus. Restore opens the draft text under the same key and leaves the document dirty (it is not in the file yet); Discard removes the stored draft. While the notice is up the stored draft is only replaced when the document is changed, so a reload before deciding offers it again; Restore uses the copy read when the notice appeared.

Known limit: drafts are keyed by file name, so two different files with the same name share a draft slot (the `lastModified` check stops an older draft being offered for a newer file).

## Markdown export view (`M↓`, issue #73)

A modal `<dialog>` titled "Markdown" showing `editor.exportDocument()` (the active alternatives, no ghosted text, no Overflow, no annotation block, R-9.3) in a read-only text area, with:

- **Copy**: `navigator.clipboard.writeText`; when that is refused, the text is selected and `document.execCommand('copy')` is tried; if that fails too, the status line says the text is selected and to press Ctrl+C (Cmd+C on a Mac).
- **Download .md**: written like a save but always to a new file (a picker in Chromium, else a download), named `<name>-export.md`, so the annotated file is never overwritten by the clean export.

Escape, the close button or a click on the backdrop closes it, and focus returns to where it was (the `M↓` control). The export is taken each time the dialog opens.

**Plain mode.** The plain editor already shows the rendered preview, but the export (the Markdown without annotations and ghosted text) is just as useful there, so the plain toolbar gets a File group with Open, Save and Markdown (FontAwesome icons with screen-reader labels). Save and open therefore work the same in both modes, and Ctrl+S / Ctrl+O work everywhere.

## Shortcuts

Ctrl+S / Cmd+S (save) and Ctrl+O / Cmd+O (open) are handled on `document` and prevent the browser's Save page and Open file. They do not collide with R-7.4 (the config shortcuts are Ctrl+B, I, K, L, H), the R-7.3 selection shortcuts (Ctrl+Shift+A, G, X and Ctrl+/) or undo and redo; Shift and Alt variants are left to the browser. Unlike the formatting shortcuts they accept Cmd, because Cmd+S is what Mac users press.

## Styling

Every colour, radius and font comes from `tokens.css`, which defines its custom properties on `:root`, so the notice and the dialogs look the same in both modes (like the shortcut reference).

## Tests

Headless Chrome defines the picker functions but they throw `SecurityError` without a real gesture, so the browser tests create editors with `fileSystemAccess: false` to cover the download and file-input paths, and use real origin-private (OPFS) file handles for writing back to a kept handle. Downloads are observed with a page-level click listener on the `<a download>` (it records the URL, calls `preventDefault()` so nothing is written to disk, and the test `fetch`es the Blob). Nothing is mocked.

| Binary | Covers |
|---|---|
| `tests/web_files.rs` | file-name logic; the floppy, Ctrl+S, Cmd+S, the toolbar button and `chrome.save()` download exactly `saveDocument()`; cancelling `te:save` |
| `tests/web_files_handles.rs` | open from an OPFS handle, edit, save writes back to it; first save without a gesture falls back to a download; cancelling `te:open` / `te:markdown`; `destroy()` cleanup |
| `tests/web_files_open.rs` | the file input restores spans, ghosts and overflow and open then save gives back the file; `.txt`; non-text files; drops on the surface, page and Overflow panel; text drops untouched; drops outside the editor ignored |
| `tests/web_files_replace.rs` | replacing a dirty document: Save first (and a cancelled save), Discard, Cancel, Escape; a clean document is replaced without asking |
| `tests/web_files_drafts.rs` | the dirty dot, label, title and events through typing, undo, a ghost and save; draft kept on teardown; Restore and Discard after a simulated reload; file newer or older than its draft; identical and broken drafts |
| `tests/web_files_markdown.rs` | the dialog shows exactly `exportDocument()` (the golden export); Copy; Download .md; Escape and focus; one dialog at a time; plain-mode toolbar button; fresh export on reopen |
