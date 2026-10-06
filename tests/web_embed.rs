//! Browser tests for the embeddable editor (issue #77): `TeraphimEditor`
//! (`public/js/terraphim-editor.js`) in a bare host container with no
//! template markup, rendering through the real Rust `mount_editor`, with
//! the real editor scripts and stylesheets. Run with
//! `wasm-pack test --headless --chrome --test web_embed`.
//!
//! wasm-bindgen-test has already instantiated the module, so the wrapper is
//! given the real exported functions as `bindings` (the same object the
//! other binaries install on `window.wasmBindings`) instead of fetching the
//! glue and `.wasm`; loading over the network is checked by hand with the
//! bundle's `example.html` (docs/design/embedding.md). Shoelace is not
//! loaded (no network), exactly as in the other binaries.
//!
//! Every test puts a decoy before the host: a container with the same
//! classes and ids as the editor template (`.markdown-input`,
//! `#formatting-toolbar`, `#shortcuts-list`, `.shortcuts-dialog`,
//! `#show-help`, `.markdown-preview`), so any lookup that is not scoped to
//! the editor's own container finds the decoy first. The decoy must come
//! through every scenario byte for byte. Nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// The decoy: template look-alike markup that the editor must never touch.
const DECOY_HTML: &str = r##"<div class="editor-container"><div class="toolbar"><div id="formatting-toolbar"></div><div id="show-help"></div></div><div class="markdown-input te-surface">decoy text</div><div class="markdown-preview"><p>decoy preview</p></div></div><div class="shortcuts-dialog"><div id="shortcuts-list"></div></div>"##;

/// Inject the wrapper (and the bundle's base stylesheet) once per page.
fn load_embed_script(document: &Document) {
    if document
        .query_selector("script[data-te-embed-test]")
        .unwrap()
        .is_some()
    {
        return;
    }
    let style = document.create_element("style").unwrap();
    style.set_attribute("data-te-embed-test", "").unwrap();
    style.set_text_content(Some(BUNDLE_CSS));
    document
        .document_element()
        .unwrap()
        .append_child(&style)
        .unwrap();
    let script = document.create_element("script").unwrap();
    script.set_attribute("data-te-embed-test", "").unwrap();
    script.set_text_content(Some(TERAPHIM_EDITOR_JS));
    document.body().unwrap().append_child(&script).unwrap();
}

/// Await a JavaScript promise that resolves to a string.
async fn js_await_string(src: &str) -> String {
    let promise: Promise = js_eval(src)
        .dyn_into()
        .expect("snippet should return a promise");
    JsFuture::from(promise)
        .await
        .expect("promise should resolve")
        .as_string()
        .expect("promise should resolve to a string")
}

/// Tear down whatever a previous test left, put a decoy and an empty host
/// on the page and create an editor in the host with `value`. The editor is
/// `window.__teEmbed` (the wrapper) and `window.__teEditor` (its
/// MarkdownEditor, for the shared helpers).
async fn fresh_embed(value: &str) {
    fresh_embed_with(value, "{}").await;
}

/// `fresh_embed` with extra editor config: `config_js` is a JavaScript
/// object literal merged into the `config` option.
async fn fresh_embed_with(value: &str, config_js: &str) {
    let document = document();
    let ok = js_string(
        "(() => { if (window.__teEmbed) window.__teEmbed.destroy(); window.__teEmbed = null; \
         if (window.__teEditor && !window.__teEditor.destroyed) window.__teEditor.destroy(); \
         window.__teEditor = null; return 'ok'; })()",
    );
    assert_eq!(ok, "ok");
    while let Some(node) = document
        .query_selector(
            "#app, #te-decoy, #te-host, #te-other, #te-host-mode, #te-host-twin, .command-menu, .te-chrome, .te-selection-menu, .te-blocks, .te-alt-panel, .te-overflow, .te-files-notice, .te-files-input, .te-files-dialog",
        )
        .unwrap()
    {
        node.remove();
    }
    load_editor_scripts(&document);
    load_embed_script(&document);
    let src = format!(
        r##"(() => {{
          teTest.resetWriteOn();
          teTest.resetBlocks();
          teTest.resetDrafts();
          const decoy = document.createElement('div');
          decoy.id = 'te-decoy';
          decoy.innerHTML = {decoy};
          const host = document.createElement('div');
          host.id = 'te-host';
          host.style.height = '400px';
          document.body.append(decoy, host);
          window.__teDecoy = decoy.outerHTML;
          return TeraphimEditor.create(host, {{
            bindings: window.wasmBindings,
            loadShoelace: false,
            value: {value},
            config: Object.assign({{ fileSystemAccess: false, autosaveDelay: 40 }}, {config_js}),
          }}).then((e) => {{
            window.__teEmbed = e;
            window.__teEditor = e.markdownEditor;
            return 'ok';
          }}, (err) => 'create failed: ' + err.message);
        }})()"##,
        decoy = js_string_literal(DECOY_HTML),
        value = js_string_literal(value),
        config_js = config_js,
    );
    assert_eq!(js_await_string(&src).await, "ok");
}

/// Problems with the decoy (it must be exactly as it was created).
const DECOY_CHECK_JS: &str = r##"
  if (document.getElementById('te-decoy').outerHTML !== window.__teDecoy) out.push('decoy changed: ' + document.getElementById('te-decoy').innerHTML.slice(0, 200));
"##;

#[wasm_bindgen_test]
async fn test_create_renders_into_a_bare_container_and_stays_inside_it() {
    fresh_embed("# Hello\n\nworld").await;
    let result = run_steps(&[
        r##"
          const E = window.__teEmbed;
          const host = document.getElementById('te-host');
          if (!E.container || E.container.parentNode !== host) return 'container not in the host';
          if (!E.container.classList.contains('te-app')) out.push('container lacks te-app');
          if (ed.root !== E.container) out.push('editor root is not the container');
          for (const el of [ed.input, ed.toolbar, ed.shortcutsList, ed.dialog, ed.helpButton]) {
            if (!E.container.contains(el)) out.push('editor element outside its container: ' + (el && (el.id || el.className)));
          }
          if (ed.toolbar.childElementCount === 0) out.push('no toolbar buttons');
          if (ed.shortcutsList.childElementCount === 0) out.push('no help items');
          const chromes = document.querySelectorAll('.te-chrome');
          if (chromes.length !== 1 || !E.container.contains(chromes[0])) out.push('chrome count/place ' + chromes.length);
          if (document.getElementById('app')) out.push('an #app appeared');
          if (E.getValue() !== '# Hello\n\nworld') out.push('initial value ' + JSON.stringify(E.getValue()));
          if (ed.documentApi().document_body() !== E.getValue()) out.push('model not on the initial value');
          // The initial preview is part of the rendered template.
          if (!E.container.querySelector('.markdown-preview').innerHTML.includes('<h1>Hello</h1>')) out.push('initial preview');
          if (ed.config.standalone !== false) out.push('standalone ' + ed.config.standalone);
        "##,
        r##"
          const E = window.__teEmbed;
          E.setValue('one');
          if (E.getValue() !== 'one') out.push('setValue ' + JSON.stringify(E.getValue()));
          // Native typing at the end of the text.
          s.focus();
          s.setSelectionOffsets(3, 3);
          document.execCommand('insertText', false, ' two');
          if (E.getValue() !== 'one two') out.push('typed ' + JSON.stringify(E.getValue()));
          if (ed.documentApi().document_body() !== 'one two') out.push('model ' + ed.documentApi().document_body());
          // The Rust preview is wired to this container's preview only.
          window.teFlushPreview();
          const preview = E.container.querySelector('.markdown-preview').innerHTML;
          if (preview.trim() !== '<p>one two</p>') out.push('preview ' + JSON.stringify(preview));
          E.setValue('# Title');
          window.teFlushPreview();
          if (!E.container.querySelector('.markdown-preview').innerHTML.includes('<h1>Title</h1>')) out.push('preview after setValue');
          // Toolbar formatting through the container's own button.
          s.setSelectionOffsets(2, 7);
          ed.toolbar.querySelector('sl-button').click();
          if (E.getValue() !== '# **Title**') out.push('bold ' + JSON.stringify(E.getValue()));
        "##,
        DECOY_CHECK_JS,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_module_start_leaves_an_embedding_page_alone() {
    fresh_embed("").await;
    // A host page with its own #app: the module start function must not
    // render the editor into it (the wrapper set window.TE_EMBED).
    let document = document();
    let app = document.create_element("div").unwrap();
    app.set_id("app");
    app.set_inner_html("<p>host app</p>");
    document.body().unwrap().append_child(&app).unwrap();
    terraphim_editor::start().expect("start should succeed on an embedding page");
    assert_eq!(app.inner_html(), "<p>host app</p>");
    // And the full-page bootstrap in editor.js stays out of it too.
    let result = js_string(
        r##"(() => {
          const out = [];
          if (!window.TE_EMBED) out.push('TE_EMBED not set');
          if (document.querySelectorAll('.te-chrome').length !== 1) out.push('chrome count ' + document.querySelectorAll('.te-chrome').length);
          return out.join('; ');
        })()"##,
    );
    app.remove();
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_write_on_toggle_and_scoped_events() {
    fresh_embed("Some words here.").await;
    let result = run_steps(&[
        r##"
          const E = window.__teEmbed;
          T.seen = [];
          T.cb = (e) => T.seen.push(e.detail.mode);
          T.off = E.on('te:mode-change', T.cb);
          const counter = E.container.querySelector('.te-chrome-counter');
          if (!counter) return 'no counter in the container';
          if (!counter.textContent.trim().startsWith('3 words')) out.push('counter ' + counter.textContent.trim());
          if (E.isWriteOn()) out.push('starts in Write_On');
          counter.click();
          if (!E.isWriteOn()) return 'counter click did not turn Write_On on';
          if (E.container.dataset.mode !== 'write-on') out.push('container data-mode ' + E.container.dataset.mode);
          if (JSON.stringify(T.seen) !== '["write-on"]') out.push('events ' + JSON.stringify(T.seen));
          // Write_On layout: no toolbar, no preview pane.
          if (teTest.visible(E.container.querySelector('.toolbar'))) out.push('toolbar visible in Write_On');
          if (teTest.visible(E.container.querySelector('.markdown-preview'))) out.push('preview visible in Write_On');
          for (const name of ['alternatives', 'markdown', 'save', 'open', 'lab', 'overflow']) {
            const el = E.container.querySelector('.te-chrome [data-control="' + name + '"]');
            if (!teTest.visible(el)) out.push('control not visible: ' + name);
          }
        "##,
        r##"
          const E = window.__teEmbed;
          // An event from elsewhere on the page is not this editor's.
          document.getElementById('te-decoy').dispatchEvent(new CustomEvent('te:mode-change', {
            bubbles: true, detail: { editor: {}, mode: 'elsewhere' },
          }));
          if (T.seen.length !== 1) out.push('foreign event delivered ' + JSON.stringify(T.seen));
          E.setWriteOn(false);
          if (E.isWriteOn() || E.container.dataset.mode !== undefined) out.push('setWriteOn(false)');
          if (JSON.stringify(T.seen) !== '["write-on","plain"]') out.push('events ' + JSON.stringify(T.seen));
          T.off();
          E.setWriteOn(true);
          if (T.seen.length !== 2) out.push('listener not removed');
          if (!E.isWriteOn()) out.push('setWriteOn(true)');
          E.setWriteOn(false);
        "##,
        DECOY_CHECK_JS,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_write_on_in_an_embed_leaves_the_host_page_unstyled() {
    fresh_embed("Some words here.").await;
    let result = run_steps(&[
        r##"
          const E = window.__teEmbed;
          const decoy = document.getElementById('te-decoy');
          // Host-owned elements that happen to use the editor's vocabulary:
          // one carrying data-mode="write-on" itself, and a twin without it.
          // The bundle must style neither (they must compute alike) and the
          // embed's toggle must not change them.
          const inner = '<div class="toolbar">toolbar</div><sl-split-panel></sl-split-panel>' +
            '<div class="markdown-input te-surface">surface</div><div class="markdown-preview">preview</div>';
          for (const [id, mode] of [['te-host-mode', 'write-on'], ['te-host-twin', null]]) {
            const el = document.createElement('div');
            el.id = id;
            if (mode) el.dataset.mode = mode;
            el.innerHTML = inner;
            document.body.appendChild(el);
          }
          const styleOf = (el) => {
            const c = getComputedStyle(el);
            return [c.display, c.backgroundColor, c.color, c.fontFamily, c.fontSize, c.minHeight, c.maxWidth, c.paddingTop, c.colorScheme, c.height].join('|');
          };
          T.hostStyles = (id) => {
            const root = document.getElementById(id);
            return [root, ...root.children].map(styleOf).join(' / ');
          };
          if (T.hostStyles('te-host-mode') !== T.hostStyles('te-host-twin')) {
            out.push('bundle styles a host data-mode element: ' + T.hostStyles('te-host-mode') + ' vs ' + T.hostStyles('te-host-twin'));
          }
          // What the host page looks like before Write_On.
          T.snap = () => {
            const b = getComputedStyle(document.body);
            const d = getComputedStyle(decoy);
            return JSON.stringify({
              attrs: Array.from(document.body.attributes).map((a) => a.name + '=' + a.value),
              body: [b.backgroundColor, b.color, b.fontFamily, b.colorScheme, b.minHeight],
              decoy: [d.backgroundColor, d.color, d.fontFamily],
              hostMode: T.hostStyles('te-host-mode'),
              hostTwin: T.hostStyles('te-host-twin'),
            });
          };
          T.before = T.snap();
          T.plainBg = getComputedStyle(E.container).backgroundColor;
          E.setWriteOn(true);
          if (!E.isWriteOn()) return 'Write_On not on';
          if (document.body.dataset.mode !== undefined) out.push('body data-mode set');
          if (T.snap() !== T.before) out.push('host page restyled: ' + T.snap() + ' vs ' + T.before);
          // The editor itself takes the Write_On theme and layout.
          if (E.container.dataset.mode !== 'write-on') out.push('root data-mode');
          if (getComputedStyle(E.container).backgroundColor === T.plainBg) out.push('root not themed');
          if (teTest.visible(E.container.querySelector('.toolbar'))) out.push('toolbar visible');
          if (!teTest.visible(E.container.querySelector('.te-chrome [data-control="overflow"]'))) out.push('corner controls hidden');
        "##,
        // The Overflow panel marks the root, not <body>.
        r##"
          const E = window.__teEmbed;
          if (!ed.overflow.open({ focus: false })) return 'overflow did not open';
          if (E.container.dataset.teOverflow !== 'open') out.push('root not marked');
          if (document.body.dataset.teOverflow !== undefined) out.push('body marked');
          ed.overflow.close({ restoreFocus: false });
          if (E.container.dataset.teOverflow !== undefined) out.push('root still marked');
          E.setWriteOn(false);
          if (E.container.dataset.mode !== undefined) out.push('root mode left');
          if (T.snap() !== T.before) out.push('host page changed after plain');
          document.getElementById('te-host-mode').remove();
          document.getElementById('te-host-twin').remove();
        "##,
        DECOY_CHECK_JS,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_config_values_render_as_text_never_markup() {
    // A host passes config through the embed API: names, descriptions and
    // keys must land as text, and icon names must be plain icon names.
    fresh_embed_with(
        "x",
        r##"{
          shortcuts: [
            { name: '"><img src=x onerror="window.__teXss=1">', key: '<img src=x onerror="window.__teXss=2">', prefix: '*', suffix: '*', desc: '<img src=x onerror="window.__teXss=3">desc' },
            { name: 'type-bold', key: 'ctrl+b', prefix: '**', suffix: '**', desc: 'Bold' },
          ],
          commands: [
            { name: '<img src=x onerror="window.__teXss=4">cmd', icon: 'x" onmouseover="window.__teXss=5', prefix: '', suffix: '' },
            { name: 'Bold', icon: 'type-bold', prefix: '**', suffix: '**' },
          ],
        }"##,
    )
    .await;
    // Give any injected handler (an image error event) time to run.
    sleep(50).await;
    let result = js_string(
        r##"(() => {
          const out = [];
          const E = window.__teEmbed;
          const ed = E.markdownEditor;
          const scopes = [E.container, ed.commandMenu];
          for (const scope of scopes) {
            if (scope.querySelector('img')) out.push('an <img> was parsed in ' + scope.className);
          }
          if (window.__teXss !== undefined) out.push('script ran: ' + window.__teXss);
          const items = ed.shortcutsList.querySelectorAll('.shortcut-item');
          if (items.length !== 2) out.push('help items ' + items.length);
          const desc = items[0].querySelector('.shortcut-desc');
          if (!desc || desc.textContent !== '<img src=x onerror="window.__teXss=3">desc') out.push('desc ' + (desc && desc.textContent));
          const badge = items[0].querySelector('sl-badge');
          if (!badge || badge.textContent !== '<img src=x onerror="window.__teXss=2">') out.push('key badge ' + (badge && badge.textContent));
          // Bad icon names are dropped, good ones kept.
          const icons = Array.from(ed.toolbar.querySelectorAll('sl-icon')).map((i) => i.getAttribute('name'));
          if (JSON.stringify(icons.slice(0, 2)) !== '[null,"type-bold"]') out.push('toolbar icons ' + JSON.stringify(icons));
          const tip = ed.toolbar.querySelector('sl-tooltip');
          if (tip.getAttribute('content') !== '<img src=x onerror="window.__teXss=2">') out.push('tooltip content');
          const cmds = Array.from(ed.commandMenu.querySelectorAll('.command-item'));
          if (cmds.length !== 2) out.push('commands ' + cmds.length);
          if (cmds[0].textContent.trim() !== '<img src=x onerror="window.__teXss=4">cmd') out.push('command name ' + cmds[0].textContent);
          const cmdIcons = cmds.map((c) => c.querySelector('sl-icon').getAttribute('name'));
          if (JSON.stringify(cmdIcons) !== '[null,"type-bold"]') out.push('command icons ' + JSON.stringify(cmdIcons));
          if (cmds[0].querySelector('[onmouseover]')) out.push('attribute injected');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_files_dropped_on_the_host_page_are_left_to_the_host() {
    fresh_embed("unchanged").await;
    let result = js_await_string(
        r##"(() => {
          const E = window.__teEmbed;
          const drop = (target, name) => {
            const dt = new DataTransfer();
            dt.items.add(new File(['# ' + name], name, { type: 'text/markdown' }));
            const ev = new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt });
            target.dispatchEvent(ev);
            return ev.defaultPrevented;
          };
          const out = [];
          // The bare host page and the decoy are not the editor's.
          if (drop(document.body, 'page.md')) out.push('drop on the page taken');
          if (drop(document.getElementById('te-decoy'), 'decoy.md')) out.push('drop on the decoy taken');
          if (E.getValue() !== 'unchanged') out.push('value ' + JSON.stringify(E.getValue()));
          // The editor's own surface is.
          if (!drop(E.markdownEditor.input, 'mine.md')) out.push('drop on the surface ignored');
          return Promise.resolve(E.persistence.lastOpen).then(() => {
            if (E.getValue() !== '# mine.md') out.push('dropped file not opened: ' + JSON.stringify(E.getValue()));
            if (document.getElementById('te-decoy').outerHTML !== window.__teDecoy) out.push('decoy changed');
            return out.join('; ');
          });
        })()"##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_alternative_through_the_panel_then_save_export_and_reopen() {
    fresh_embed("").await;
    let result = run_steps(&[
        // Ctrl+Shift+A on a selection: Write_On, the panel opens on the word.
        r##"
          const E = window.__teEmbed;
          E.setValue('Pass me a paperclip. Drop this.');
          s.focus();
          s.setSelectionOffsets(10, 19);
          if (!teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true })) out.push('Ctrl+Shift+A not taken');
          const P = ed.altPanel;
          if (!P.isOpen()) return 'panel not open';
          if (!E.container.contains(P.root)) out.push('panel outside the container');
          if (!E.container.classList.contains('te-alt-open')) out.push('container not dimmed');
          if (!E.isWriteOn()) out.push('Write_On not on');
        "##,
        // Type an alternative on the new line; then make it active.
        r##"
          const E = window.__teEmbed;
          const P = ed.altPanel;
          const inputs = () => Array.from(P.list.querySelectorAll('.te-alt-input'));
          const input = inputs()[inputs().length - 1];
          input.focus();
          input.value = 'eraser';
          teTest.key(input, 'Enter');
          const spans = ed.annotations().spans;
          if (spans.length !== 1) return 'spans ' + JSON.stringify(spans);
          if (JSON.stringify(spans[0].alts.map((a) => a.text)) !== '["paperclip","eraser"]') out.push('alts ' + JSON.stringify(spans[0].alts));
          teTest.key(inputs()[inputs().length - 1], 'ArrowUp');
          if (E.getValue() !== 'Pass me an eraser. Drop this.') out.push('active alternative ' + JSON.stringify(E.getValue()));
        "##,
        // saveDocument(): the body plus the annotation block, and te:saved.
        r##"
          const E = window.__teEmbed;
          const saved = [];
          const off = E.on('te:saved', (e) => saved.push(e.detail.text));
          const text = E.saveDocument();
          off();
          if (!text.startsWith('Pass me an eraser. Drop this.')) out.push('saved body ' + JSON.stringify(text.slice(0, 40)));
          if (!text.includes('paperclip')) out.push('annotation block missing the original');
          if (saved.length !== 1 || saved[0] !== text) out.push('te:saved ' + saved.length);
          const exported = E.exportDocument();
          if (exported.trim() !== 'Pass me an eraser. Drop this.') out.push('export ' + JSON.stringify(exported));
          T.text = text;
        "##,
        // Reopen the saved text: the alternative comes back.
        r##"
          const E = window.__teEmbed;
          E.setValue('something else');
          E.openDocument(T.text, 'embed.md');
          if (E.getValue() !== 'Pass me an eraser. Drop this.') out.push('reopened ' + JSON.stringify(E.getValue()));
          const spans = ed.annotations().spans;
          if (spans.length !== 1 || spans[0].alts.length !== 2) out.push('reopened spans ' + JSON.stringify(spans));
          if (ed.documentKey !== 'embed.md') out.push('document key ' + ed.documentKey);
          if (!E.persistence || E.persistence.standalone !== false) out.push('persistence not scoped');
          E.setWriteOn(false);
        "##,
        DECOY_CHECK_JS,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_single_instance_destroy_and_legacy_initialize() {
    fresh_embed("keep").await;
    // A second editor while one is alive is refused and leaves its element
    // untouched.
    let refused = js_await_string(
        r##"(() => {
          const other = document.createElement('div');
          other.id = 'te-other';
          document.body.appendChild(other);
          return TeraphimEditor.create(other, { bindings: window.wasmBindings, loadShoelace: false })
            .then(() => 'second editor created', (err) => err.message + '|' + other.childNodes.length);
        })()"##,
    )
    .await;
    assert!(
        refused.contains("one editor per page") && refused.ends_with("|0"),
        "{refused}"
    );
    let result = js_string(&format!(
        r##"(() => {{
          const out = [];
          const E = window.__teEmbed;
          if (E.getValue() !== 'keep') out.push('first editor disturbed');
          E.setWriteOn(true);
          const s = E.markdownEditor.surface;
          s.focus();
          s.setSelectionOffsets(0, 4);
          E.destroy();
          E.destroy();
          const host = document.getElementById('te-host');
          if (host.childNodes.length !== 0) out.push('host not empty: ' + host.innerHTML.slice(0, 120));
          // Nothing the editor created is left anywhere (the decoy's own
          // te-surface class excepted).
          const left = Array.from(document.querySelectorAll('[class^="te-"], [class*=" te-"], .command-menu, .terraphim-editor-container'))
            .filter((el) => !el.closest('#te-decoy'));
          if (left.length) out.push('left behind: ' + left.map((el) => el.className).join(', '));
          if (document.body.dataset.mode !== undefined) out.push('body mode ' + document.body.dataset.mode);
          if (TeraphimEditor.active !== null) out.push('still active');
          try {{ E.getValue(); out.push('getValue after destroy'); }} catch (e) {{}}
          {decoy}
          return out.join('; ');
        }})()"##,
        decoy = DECOY_CHECK_JS.trim()
    ));
    assert_eq!(result, "");
    // After destroy() a new editor can be created, through the earlier
    // `new TeraphimEditor(el).initialize()` API too.
    let legacy = js_await_string(
        r##"(() => {
          const host = document.getElementById('te-host');
          const L = new TeraphimEditor(host, { bindings: window.wasmBindings, loadShoelace: false });
          return L.initialize().then((same) => {
            const out = [];
            if (same !== L) out.push('initialize did not resolve to the editor');
            if (L.getValue() !== '') out.push('value ' + JSON.stringify(L.getValue()));
            L.setValue('legacy');
            if (L.markdownEditor.documentApi().document_body() !== 'legacy') out.push('model');
            L.destroy();
            if (host.childNodes.length !== 0) out.push('not cleaned');
            return out.join('; ');
          }, (err) => 'initialize failed: ' + err.message);
        })()"##,
    )
    .await;
    assert_eq!(legacy, "");
}
