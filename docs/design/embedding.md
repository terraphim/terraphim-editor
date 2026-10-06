# The embeddable editor

Issue: terraphim/terraphim-editor#77 (a self-contained embeddable editor). Code: `public/js/terraphim-editor.js` (`TeraphimEditor`), `mount_editor` / `unmount_editor` / `start` in `src/lib.rs`, the scoped lookups in `MarkdownEditor.initialize()` (`public/js/editor.js`), the bundle in `scripts/build-dist.sh` and its base stylesheet `public/styles.css`. Tests: `tests/web_embed.rs`.

## What a host page writes

```html
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.2/css/all.min.css">
<link rel="stylesheet" href="terraphim-editor.min.css">
<div id="editor" style="height: 80vh"></div>
<script src="terraphim-editor.min.js"></script>
<script type="module">
  const editor = await TeraphimEditor.create(document.getElementById('editor'), {
    value: '# Hello',
  });
</script>
```

`create()` resolves once the editor is usable: the module is loaded, the markup is rendered into the element and the controller is running, with the Write_On chrome, indicators, the selection menu, the alternatives panel, the Lab, the Blocks view, the Overflow panel and save/open, exactly as on the full page.

## The bundle

`trunk build` (or `trunk build --release`) runs `scripts/build-dist.sh` as a `post_build` hook, which writes next to Trunk's output:

| File | What it is |
|---|---|
| `terraphim-editor.min.js` | the editor scripts in `index.html` order, then the wrapper (classic script) |
| `terraphim-editor.min.css` | `public/styles.css` (the embed base styles), the tokens and every editor stylesheet |
| `terraphim_editor.js` | the wasm-bindgen glue (ES module), a fixed-name copy of Trunk's `terraphim-editor-<hash>.js` |
| `terraphim_editor_bg.wasm` | the module, a fixed-name copy of `terraphim-editor-<hash>_bg.wasm` |
| `example.html` | a working host page |

The hook moved from Trunk's `build` stage to `post_build` because `build` hooks run alongside the asset pipelines, so the hashed wasm-bindgen output is not guaranteed to exist yet; at `post_build` it is in `$TRUNK_STAGING_DIR`. `scripts/release-dist.sh` checks for the two fixed-name files too.

## Decisions

### WebAssembly loading

Trunk builds the module with wasm-bindgen's `web` target: an ES module whose default export `init({ module_or_path })` instantiates the `.wasm`. The wrapper keeps that build (there is one Rust build, no second `--target no-modules` pipeline) and loads it with a dynamic `import()` from the classic bundle script, which every browser the editor supports allows:

- **URLs.** `glueUrl` and `wasmUrl` default to `terraphim_editor.js` and `terraphim_editor_bg.wasm` resolved against the bundle script's own URL (`document.currentScript.src`, captured when the script runs, because it is `null` later), so a host that serves the four files from one directory passes nothing. Explicit options resolve against the page, like any URL in it. The glue is always given `wasmUrl` explicitly: its built-in default is the hyphenated `terraphim-editor_bg.wasm`, which the bundle does not ship. Serving the files from another origin needs CORS on both (an ES module import and a `fetch` of the `.wasm`), and the `.wasm` should be served as `application/wasm` for streaming compilation.
- **Once per page.** `TeraphimEditor.loadModule()` caches the promise, so destroying an editor and creating another reuses the instantiated module; a failed load is forgotten so it can be retried.
- **Already loaded.** `bindings` takes any object with the module's exports and skips loading. The browser tests use it (wasm-bindgen-test has already instantiated the module), and a host that bundles the glue itself can do the same.
- **The module start function.** `#[wasm_bindgen(start)]` used to be `run()`, which rendered into `#app` and failed without one; a failing start function rejects `init()`, so the module could not load on a host page at all. It is now `start()`: it does nothing when the page declared itself an embedding host (`window.TE_EMBED`, set by the wrapper as soon as its script runs) or has no `#app`, and otherwise calls `run()`. A host page with its own `#app` is therefore never taken over. `run()` is still exported and still renders the full page into `#app`.
- **`window.wasmBindings`.** The editor scripts get the module from `config.bindings` (`MarkdownEditor.documentApi()`, the Blocks view's renderer) and fall back to `window.wasmBindings`, which Trunk sets on the full page. The wrapper passes `bindings` in the config and also sets the global when nothing else has, for console use and older code.

### One editor per page

The document session (`with_session` in `src/document.rs`) and the live preview state (`PREVIEW` in `src/lib.rs`) are `thread_local` singletons in the module, and the module is instantiated once per page. Two editors would share one span model: an edit in one would be mirrored into the other's document. Keying the session per editor would mean threading an editor id through every exported function and every call site in the scripts, which is a large refactor for a case nobody has asked for yet. So the limit is enforced rather than left to chance: `TeraphimEditor.create()` rejects with "TeraphimEditor supports one editor per page; destroy() the existing editor first" while another is alive, without touching its element. `destroy()` then `create()` works (the session is reopened with the new value). Making the session per editor is the way to lift the limit; nothing else in the wrapper assumes one instance.

Write_On never restyles the host page. The chrome sets `data-mode="write-on"` on the editor's root (`editor.root`, the `.te-app` element), and the Write_On rules in `tokens.css`, `write-on.css`, `blocks.css`, `alternatives-panel.css` and `overflow.css` are scoped to that attribute (`[data-mode="write-on"] ...` for descendants, `.te-app[data-mode="write-on"]` for the root's own layout, with an `#app.te-app` variant that outranks the full page's inline `#app` rule). Only the full page (root `#app`, or `config.standalone`, `chrome.pageMode`) also puts the attribute on `<body>`, so the whole page goes dark there exactly as before; the Overflow panel's `data-te-overflow` follows the same rule. An embedded editor themes its own container only. Its corner controls are still `position: fixed` to the viewport, so give the editor most of the page. `destroy()` removes the attributes. Because the editor's menus and dialogs appended to `<body>` (selection menu, persistence notice and dialogs) carry their own colours from the `:root` tokens, they look the same either way.

### Rendering and scoping

- **Markup.** `mount_editor(root, initial)` renders the Rinja template (`templates/editor.html`) into `root`, opens `initial` as the document, adds the `te-app` class and wires the Rust preview to the surface and preview inside `root` (`root.query_selector`, not `document`). `run()` is `mount_editor(#app, welcome text)`. The wrapper renders into a `.terraphim-editor-container` it appends to the host element, so the element's own attributes and other children are left alone and `destroy()` removes exactly what was added. `unmount_editor()` cancels a pending preview render and drops the module's references to the removed nodes.
- **Controller.** `MarkdownEditor.initialize(root)` looks up the surface, toolbar, help list, help dialog and help button with `root.querySelector`; with no argument it uses `#app` when the page has one (the full page and the existing tests), otherwise the document. It records `editor.root`, which the alternatives panel mounts into (it used to look for `closest('#app')`) and the chrome mirrors `data-mode` onto. The Write_On layout rule that sized `#app` now keys on `.te-app`.
- **Things already per editor.** The chrome, Lab, trim card, Overflow panel and Blocks view mount inside the editor's `.editor-container`; the selection menu, the slash command menu and the persistence notice, file input and dialogs are appended to `<body>` (so they are not clipped) and are tracked and removed by `destroy()`. Every `te:*` listener on `document` already ignores events whose `detail.editor` is another editor.
- **Full-page bootstrap.** The `DOMContentLoaded` start-up at the end of `editor.js` polled the whole document for the template's selectors and would have started a second, standalone controller on an embedded editor's markup as soon as the wrapper rendered it. It now runs only on a page with `#app` and no `window.TE_EMBED`, and looks inside `#app` only.
- **Dropped files.** A file dropped on the bare page (`<body>`) opens only when `standalone` is true; an embedded editor takes drops on its own container, chrome and panels and leaves the host page's to the host.

### Wrapper API

`TeraphimEditor.create(element, options)` resolves to an object with:

| Member | |
|---|---|
| `getValue()` / `setValue(markdown)` | the body text; `setValue` is one undo step and the preview follows |
| `openDocument(source, name)` | open `.md` with its annotation block; `name` becomes the document key |
| `saveDocument()` | the `.md` text (body plus annotation block); dispatches `te:saved`. Storing it is the host's job, or call `persistence.save()` for the file / download flow |
| `exportDocument()` | clean Markdown |
| `isWriteOn()` / `setWriteOn(on)` | Write_On mode |
| `on(type, fn)` / `off(type, fn)` | this editor's `te:*` events (from its container, or with `detail.editor` set to it); `on` returns an unsubscribe function |
| `persistence` | `TePersistence` (save, open, drafts, `markClean()`) |
| `markdownEditor` | the `MarkdownEditor` (surface, chrome, panels) |
| `container` | the `.terraphim-editor-container` |
| `destroy()` | remove everything the editor added, on the page and in the module; returns unapplied drafts (see `MarkdownEditor.destroy()`) |

Options: `value` (default empty), `wasmUrl`, `glueUrl`, `bindings`, `standalone` (default `false`: Ctrl+S / Ctrl+O act only while focus is in the editor, see `docs/design/persistence.md`), `dirtyTitle` (default: `standalone`, so an embed does not prefix the host's `document.title`), `loadShoelace` (default `true`) and `config` (merged over `window.EditorConfig`: `shortcuts`, `commands`, `documentKey`, `fileSystemAccess`, `autosaveDelay`).

`new TeraphimEditor(element, options).initialize()` still works: `initialize()` is an alias of `mount()`, which now also loads the module and renders the markup.

### Shoelace and FontAwesome

Shoelace still comes from the jsDelivr CDN (pinned to 2.12.0, as on the full page): the wrapper adds the light theme stylesheet when the page has none, calls Shoelace's `setBasePath` (it cannot infer one without a `<script src>` pointing at Shoelace, and its icons would 404) and imports the components the template uses. `loadShoelace: false` skips this for a page that loads Shoelace itself. FontAwesome (used by the chrome and panels) is not bundled; the host page links its stylesheet, as `example.html` and `index.html` do.

### Styles

`public/styles.css` is only in the bundle. It used to style the host page's `<body>` (margin, padding, font) and size `#app` to the viewport; every rule is now scoped to the editor (`.terraphim-editor-container`, `.te-app ...`, the command menu), and it carries the editing surface and preview sizing that the full page keeps inline in `index.html`. The host gives the element a height and the editor fills it (`min-height: 20rem`).

### Configuration values are text

The toolbar, the help list and the slash command palette are built with `createElement` and `textContent` (`teConfigIcon`, `teTextElement` in `editor.js`), because the embed API exposes the shortcut and command configuration to host pages (`options.config`). Icon names must match `/^[a-z0-9-]+$/` before they become an `<sl-icon name>`; anything else is dropped. The other `innerHTML` uses in `public/js` are a static template (the custom formatting dialog) and the Blocks view's rendered Markdown, which comes from the same Rust converter as the preview (the markdown crate's defaults escape raw HTML and drop dangerous link protocols).

## Tests

`tests/web_embed.rs` (real DOM, the real scripts and stylesheets, the real Rust `mount_editor`; nothing mocked). Every test puts a decoy before the host: a container with the template's classes and ids, so an unscoped `document.querySelector` finds the decoy first; the decoy must be byte-for-byte unchanged afterwards.

| Test | Covers |
|---|---|
| `test_create_renders_into_a_bare_container_and_stays_inside_it` | rendering into an empty element; every looked-up element, the chrome and the toolbar buttons inside the container; initial value, model and preview; `setValue`/`getValue`; native typing; the Rust preview updates the container's preview only; a toolbar button formats |
| `test_module_start_leaves_an_embedding_page_alone` | `start()` does not render into a host page's own `#app`; the `editor.js` bootstrap starts no second controller |
| `test_write_on_toggle_and_scoped_events` | the counter toggles Write_On (container `data-mode`, toolbar and preview hidden, corner controls shown); `on`/`off`; an event from elsewhere is not delivered; `setWriteOn` |
| `test_write_on_in_an_embed_leaves_the_host_page_unstyled` | Write_On on an embed leaves `<body>`'s attributes and the computed background, colour, font, colour scheme and min-height of `<body>` and of a host element unchanged while the container is themed; the Overflow panel marks the root, not `<body>` |
| `test_config_values_render_as_text_never_markup` | shortcut names, descriptions and keys and command names with `<img onerror>` payloads land as text (no `<img>` parsed, nothing runs); icon names that are not `/^[a-z0-9-]+$/` are dropped, valid ones kept |
| `test_files_dropped_on_the_host_page_are_left_to_the_host` | drops on the page and the decoy are ignored; a drop on the surface opens the file |
| `test_alternative_through_the_panel_then_save_export_and_reopen` | Ctrl+Shift+A opens the panel inside the container; an alternative typed and made active; `saveDocument()` (with `te:saved` through `on`), `exportDocument()`, `openDocument()` restores the alternative; persistence is not standalone |
| `test_single_instance_destroy_and_legacy_initialize` | a second `create()` is refused and leaves its element empty; `destroy()` (twice) empties the host and leaves no `te-` node, no body mode; `new TeraphimEditor(el).initialize()` after it |

Network loading (the dynamic `import()` of the glue and the `.wasm` fetch) cannot run inside wasm-bindgen-test, so it is checked by hand: `trunk build --release`, copy `target/trunk-dist` to a scratch directory, serve it with `python3 -m http.server` and load `example.html` in headless Chrome (`--screenshot`). The result is `docs/images/embed-example.png`.
