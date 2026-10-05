//! Browser tests for the Write_On mode toggle and corner chrome (issue #7).
//! Run with `wasm-pack test --headless --chrome`. Real scripts, real DOM,
//! real stylesheets; nothing is mocked. See `tests/web.rs` for why the
//! browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Write_On mode toggle and corner chrome (issue #7)
// ---------------------------------------------------------------------------

/// The corner controls of R-7.2 (the counter is separate: always visible).
const CHROME_CONTROLS_JS: &str =
    "['alternatives', 'markdown', 'shortcuts', 'save', 'open', 'lab', 'overflow']";

#[wasm_bindgen_test]
fn test_plain_mode_shows_only_the_counter() {
    let _document = fresh_full_editor();
    let result = js_string(&format!(
        r##"(() => {{
          const out = [];
          const counter = document.querySelector('.te-chrome-counter');
          if (!teTest.visible(counter)) out.push('counter not visible');
          if (counter.getAttribute('aria-pressed') !== 'false') out.push('aria-pressed ' + counter.getAttribute('aria-pressed'));
          if (document.body.dataset.mode !== undefined) out.push('body mode ' + document.body.dataset.mode);
          for (const name of {CHROME_CONTROLS_JS}) {{
            const el = teTest.control(name);
            if (!el) out.push('missing control ' + name);
            else if (teTest.visible(el)) out.push('control visible in plain mode: ' + name);
          }}
          if (teTest.visible(document.querySelector('.te-chrome-shortcuts'))) out.push('reference visible');
          // Hidden corner controls must not act in plain mode, even when
          // clicked programmatically.
          const types = ['te:open-panel', 'te:markdown', 'te:shortcuts', 'te:save', 'te:open', 'te:lab', 'te:overflow'];
          const fired = [];
          const onAny = (e) => fired.push(e.type);
          for (const t of types) document.addEventListener(t, onAny);
          for (const name of {CHROME_CONTROLS_JS}) {{
            const el = teTest.control(name);
            if (el) el.click();
          }}
          for (const t of types) document.removeEventListener(t, onAny);
          if (fired.length) out.push('plain-mode clicks dispatched ' + fired.join(','));
          if (document.body.dataset.mode !== undefined) out.push('plain-mode click changed mode');
          if (document.querySelector('.te-chrome-shortcuts')?.open) out.push('plain-mode click opened reference');
          // The plain editor itself is unchanged.
          if (!teTest.visible(document.querySelector('.toolbar'))) out.push('toolbar hidden in plain mode');
          // Counter text follows the surface: whitespace-separated words,
          // Unicode code points for chars.
          const s = teTest.surface();
          s.setText('one two  three\nfour');
          const t1 = counter.textContent.trim();
          if (!t1.startsWith('4 words 19 chars')) out.push('count ' + JSON.stringify(t1));
          s.setText('a \u{{1F600}}');
          const t2 = counter.textContent.trim();
          if (!t2.startsWith('2 words 3 chars')) out.push('unicode count ' + JSON.stringify(t2));
          s.setText('');
          const t3 = counter.textContent.trim();
          if (!t3.startsWith('0 words 0 chars')) out.push('empty count ' + JSON.stringify(t3));
          // Native typing updates it too.
          s.focus();
          document.execCommand('insertText', false, 'hi');
          if (!counter.textContent.trim().startsWith('1 word 2 chars')) out.push('typed count ' + JSON.stringify(counter.textContent));
          return out.join('; ');
        }})()"##
    ));
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_counter_click_toggles_write_on_and_every_corner_control() {
    let _document = fresh_full_editor();
    let result = js_string(&format!(
        r##"(() => {{
          const out = [];
          const modes = [];
          const onMode = (e) => modes.push(e.detail.mode);
          document.addEventListener('te:mode-change', onMode);
          const counter = document.querySelector('.te-chrome-counter');
          counter.click();
          if (document.body.dataset.mode !== 'write-on') out.push('body mode ' + document.body.dataset.mode);
          if (counter.getAttribute('aria-pressed') !== 'true') out.push('aria-pressed not true');
          if (!teTest.chrome().isWriteOn()) out.push('chrome not in write-on');
          if (!teTest.visible(counter)) out.push('counter hidden in write-on');
          const want = {{
            alternatives: ['te-chrome-top-centre', '●●●'],
            markdown: ['te-chrome-top-centre', 'M↓'],
            shortcuts: ['te-chrome-top-right', ''],
            save: ['te-chrome-bottom-left', ''],
            open: ['te-chrome-bottom-left', ''],
            lab: ['te-chrome-bottom-centre', 'LAB'],
            overflow: ['te-chrome-bottom-right', 'XYZ'],
          }};
          for (const name of {CHROME_CONTROLS_JS}) {{
            const el = teTest.control(name);
            if (!teTest.visible(el)) {{ out.push('control not visible: ' + name); continue; }}
            if (!el.parentElement.classList.contains(want[name][0])) out.push(name + ' in ' + el.parentElement.className);
            if (el.textContent.trim() !== want[name][1]) out.push(name + ' text ' + JSON.stringify(el.textContent));
            if (!el.getAttribute('aria-label')) out.push(name + ' has no label');
          }}
          // Icon controls use FontAwesome classes, never emoji.
          if (!teTest.control('save').querySelector('i.fa-floppy-disk')) out.push('save icon');
          if (!teTest.control('open').querySelector('i.fa-folder-open')) out.push('open icon');
          if (!teTest.control('shortcuts').querySelector('i.fa-keyboard')) out.push('keyboard icon');
          // Corners are placed where R-7.2 says.
          const r = (n) => teTest.control(n).getBoundingClientRect();
          // Fixed positioning is relative to the viewport without scrollbars.
          const w = document.documentElement.clientWidth, h = document.documentElement.clientHeight;
          const mid = (a, b) => (a.left + b.right) / 2;
          if (!(r('alternatives').top < h / 2 && Math.abs(mid(r('alternatives'), r('markdown')) - w / 2) < 4)) out.push('top-centre placement');
          if (!(r('shortcuts').left > w / 2 && r('shortcuts').top < h / 2)) out.push('top-right placement');
          if (!(r('save').right < w / 2 && r('save').top > h / 2)) out.push('bottom-left placement');
          if (!(r('lab').top > h / 2 && Math.abs(mid(r('lab'), r('lab')) - w / 2) < 4)) out.push('bottom-centre placement');
          if (!(r('overflow').left > w / 2 && r('overflow').top > h / 2)) out.push('bottom-right placement');
          // R-7.1: no toolbar, no preview pane, centred dark column.
          if (teTest.visible(document.querySelector('.toolbar'))) out.push('toolbar visible in write-on');
          if (teTest.visible(document.querySelector('.markdown-preview'))) out.push('preview visible in write-on');
          if (getComputedStyle(document.body).backgroundColor !== 'rgb(10, 13, 28)') out.push('bg ' + getComputedStyle(document.body).backgroundColor);
          const sr = teTest.root().getBoundingClientRect();
          const gutter = document.documentElement.clientWidth;
          if (Math.abs(sr.left - (gutter - sr.right)) > 2) out.push('surface not centred ' + sr.left + ' ' + (gutter - sr.right));
          if (getComputedStyle(teTest.root()).maxWidth === 'none') out.push('surface has no measure');
          // Each control dispatches its documented event from the chrome.
          const seen = [];
          const types = ['te:open-panel', 'te:markdown', 'te:shortcuts', 'te:save', 'te:open', 'te:lab', 'te:overflow'];
          const rec = (e) => seen.push(e.type + (e.detail.panel ? ':' + e.detail.panel : '') + (e.detail.editor === window.__teEditor ? '' : '!editor'));
          types.forEach((t) => document.addEventListener(t, rec));
          for (const name of {CHROME_CONTROLS_JS}) teTest.control(name).click();
          document.querySelector('.te-chrome-shortcuts').close();
          types.forEach((t) => document.removeEventListener(t, rec));
          const wantSeen = 'te:open-panel:alternatives,te:markdown,te:shortcuts,te:save,te:open,te:lab,te:overflow';
          if (seen.join(',') !== wantSeen) out.push('events ' + seen.join(','));
          // Clicking the counter again returns to plain mode.
          counter.click();
          if (document.body.dataset.mode !== undefined) out.push('body mode after toggle back ' + document.body.dataset.mode);
          if (counter.getAttribute('aria-pressed') !== 'false') out.push('aria-pressed not false');
          for (const name of {CHROME_CONTROLS_JS}) if (teTest.visible(teTest.control(name))) out.push('still visible: ' + name);
          if (!teTest.visible(document.querySelector('.toolbar'))) out.push('toolbar not restored');
          if (!teTest.visible(document.querySelector('.markdown-preview'))) out.push('preview not restored');
          document.removeEventListener('te:mode-change', onMode);
          if (modes.join(',') !== 'write-on,plain') out.push('mode events ' + modes.join(','));
          return out.join('; ');
        }})()"##
    ));
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_write_on_mode_persists_per_document() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const reinit = (config) => {
            window.__teEditor.destroy();
            const ed = new MarkdownEditor(config || window.EditorConfig);
            ed.initialize();
            window.__teEditor = ed;
            return ed;
          };
          const input = () => document.querySelector('.markdown-input');
          const counter = () => document.querySelector('.te-chrome-counter');
          const docA = 'Document A\n\nsome words';
          const docB = 'Document B\n\nother words';
          input().textContent = docA;
          let ed = reinit();
          if (ed.chrome.isWriteOn()) out.push('A started in write-on');
          const keyA = ed.chrome.storageKey();
          if (!keyA.startsWith(window.WRITE_ON_STORAGE_PREFIX + 'h:')) out.push('hash key ' + keyA);
          counter().click();
          if (localStorage.getItem(keyA) !== '1') out.push('not stored ' + localStorage.getItem(keyA));
          // Same document after a re-init: Write_On is restored.
          input().textContent = docA;
          ed = reinit();
          if (!ed.chrome.isWriteOn()) out.push('A not restored after re-init');
          if (document.body.dataset.mode !== 'write-on') out.push('body mode not restored');
          if (ed.chrome.storageKey() !== keyA) out.push('A key changed');
          // Another document is not affected.
          input().textContent = docB;
          ed = reinit();
          if (ed.chrome.isWriteOn()) out.push('B inherited write-on');
          if (document.body.dataset.mode !== undefined) out.push('B body mode ' + document.body.dataset.mode);
          if (ed.chrome.storageKey() === keyA) out.push('B shares the A key');
          // Loading a different document re-keys via documentChanged().
          ed.surface.setText(docA);
          ed.chrome.documentChanged();
          if (!ed.chrome.isWriteOn()) out.push('documentChanged did not apply A');
          // Toggling A back to plain is stored too.
          counter().click();
          if (localStorage.getItem(keyA) !== '0') out.push('plain not stored');
          input().textContent = docA;
          ed = reinit();
          if (ed.chrome.isWriteOn()) out.push('A plain not restored');
          // A save key or file name takes precedence over the content hash.
          const named = Object.assign({}, window.EditorConfig, { documentKey: 'notes.md' });
          ed = reinit(named);
          if (ed.chrome.storageKey() !== window.WRITE_ON_STORAGE_PREFIX + 'notes.md') out.push('named key ' + ed.chrome.storageKey());
          counter().click();
          ed.surface.setText('completely different text');
          ed = reinit(named);
          if (!ed.chrome.isWriteOn()) out.push('named document not restored despite new text');
          ed.documentKey = 'saved-key';
          ed.chrome.documentChanged();
          if (ed.chrome.storageKey() !== window.WRITE_ON_STORAGE_PREFIX + 'saved-key') out.push('editor.documentKey precedence');
          if (ed.chrome.isWriteOn()) out.push('saved-key inherited write-on');
          // Leave a clean plain editor for later tests.
          teTest.resetWriteOn();
          reinit();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_shortcut_reference_lists_every_shortcut() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          document.querySelector('.te-chrome-counter').click();
          teTest.control('shortcuts').click();
          const dialog = document.querySelector('.te-chrome-shortcuts');
          if (!dialog.open) out.push('reference not open');
          if (!teTest.visible(dialog)) out.push('reference not visible');
          const rows = Array.from(dialog.querySelectorAll('.te-chrome-shortcut')).map((r) => ({
            desc: r.querySelector('dt').textContent.trim(),
            kbd: r.querySelector('kbd').textContent.trim().toLowerCase(),
          }));
          const has = (kbd, desc) => rows.some((r) => r.kbd === kbd.toLowerCase() && r.desc === desc);
          // Every shortcut from public/js/config.js.
          for (const s of window.EditorConfig.shortcuts) {
            if (!has(s.key, s.desc)) out.push('missing config shortcut ' + s.key + ' ' + s.desc);
          }
          // Every selection shortcut from R-7.3 (fixed expectations).
          const r73 = [
            ['Ctrl+Shift+A', 'Alternatives for selection'],
            ['Ctrl+Shift+G', 'AI alternatives for selection'],
            ['Ctrl+/', 'Ghost it / Revive'],
            ['Ctrl+Shift+X', 'Stash this in Overflow'],
          ];
          for (const [kbd, desc] of r73) {
            if (!has(kbd, desc)) out.push('missing R-7.3 shortcut ' + kbd);
            if (!Array.from(dialog.querySelectorAll('kbd')).some((k) => k.textContent.trim() === kbd)) out.push('not displayed as ' + kbd);
          }
          // Close button and leaving Write_On both close it.
          dialog.querySelector('.te-chrome-shortcuts-close').click();
          if (dialog.open) out.push('close button did not close');
          teTest.control('shortcuts').click();
          document.querySelector('.te-chrome-counter').click();
          if (dialog.open) out.push('reference open after leaving write-on');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_counts_adapter_falls_back_to_surface_text() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          // The editor has counts() (issue #6, tested against the Rust model in
          // test_chrome_follows_opened_documents...); an object without it,
          // here just the real surface, makes countsFor read the surface text.
          if (typeof ed.counts !== 'function') out.push('editor.counts missing');
          ed.surface.setText('three short words');
          const c = countsFor({ surface: ed.surface });
          if (c.words !== 3 || c.chars !== 17) out.push('counts ' + JSON.stringify(c));
          const text = document.querySelector('.te-chrome-counter').textContent.trim();
          if (!text.startsWith('3 words 17 chars')) out.push('counter ' + JSON.stringify(text));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_destroy_removes_chrome_dom_and_listeners() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const a = window.__teEditor;
          const counter = document.querySelector('.te-chrome-counter');
          const lab = teTest.control('lab');
          counter.click();
          teTest.control('shortcuts').click();
          const dialog = document.querySelector('.te-chrome-shortcuts');
          a.destroy();
          a.destroy(); // idempotent
          if (!a.chrome.destroyed) out.push('chrome not marked destroyed');
          if (document.querySelectorAll('.te-chrome').length !== 0) out.push('chrome DOM left behind');
          if (dialog.open) out.push('reference left open');
          if (document.body.dataset.mode !== undefined) out.push('body mode left ' + document.body.dataset.mode);
          // Click listeners on the removed nodes were aborted with the editor.
          let events = 0;
          const count = () => { events += 1; };
          counter.addEventListener('te:mode-change', count);
          lab.addEventListener('te:lab', count);
          counter.click();
          lab.click();
          if (events !== 0) out.push('destroyed chrome still handles clicks: ' + events);
          if (document.body.dataset.mode !== undefined) out.push('destroyed counter toggled mode');
          // A new editor gets exactly one chrome.
          const b = new MarkdownEditor(window.EditorConfig);
          b.initialize();
          window.__teEditor = b;
          if (document.querySelectorAll('.te-chrome').length !== 1) out.push('chrome count ' + document.querySelectorAll('.te-chrome').length);
          if (document.querySelectorAll('.te-chrome-shortcuts').length !== 1) out.push('reference count');
          // chrome.destroy() itself drops its change subscription.
          const n = b.surface.changeListeners.size;
          const extra = new WriteOnChrome(b, { signal: b.abortController.signal });
          if (b.surface.changeListeners.size !== n + 1) out.push('chrome did not subscribe');
          extra.destroy();
          if (b.surface.changeListeners.size !== n) out.push('chrome left its change listener');
          if (document.querySelectorAll('.te-chrome').length !== 1) out.push('extra chrome DOM left');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
