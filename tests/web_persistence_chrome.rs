//! Browser tests for persistence (issue #6) together with the Write_On
//! chrome (issue #7): the chrome following opened documents, counts from the
//! model, save, and the warning's styling. Run with
//! `wasm-pack test --headless --chrome`. Real scripts, the real exported
//! document API and real fixtures; nothing is mocked. Set-aside annotations
//! re-attaching across undo, reopen and re-sync are in
//! `web_persistence_reattach.rs`. See `tests/web.rs` for why the browser
//! tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

// ---------------------------------------------------------------------------
// Persistence with the Write_On chrome (issues #6 and #7 together).
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn test_chrome_follows_opened_documents_and_counts_from_the_model() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          teTest.resetWriteOn();
          const ed = window.__teEditor;
          const out = [];
          const counter = () => document.querySelector('.te-chrome-counter-text').textContent.trim();
          const fmt = (c) => c.words + ' ' + (c.words === 1 ? 'word' : 'words') + ' ' + c.chars + ' ' + (c.chars === 1 ? 'char' : 'chars');

          // Document A: the full fixture, with two ghosts.
          ed.openDocument(teFixtures.full.md, 'a.md');
          if (ed.chrome.storageKey() !== window.WRITE_ON_STORAGE_PREFIX + 'a.md') out.push('key A ' + ed.chrome.storageKey());
          if (ed.chrome.isWriteOn()) out.push('A started in write-on');
          const model = window.wasmBindings.document_counts();
          if (counter() !== fmt(model)) out.push('counter ' + counter() + ' vs model ' + fmt(model));
          const exportWords = ed.exportDocument().split(/\s+/).filter(Boolean).length;
          if (!(model.words > exportWords)) out.push('ghosted words not counted');
          // Typing updates the counter from the model.
          const s = ed.surface;
          s.focus();
          s.setSelectionOffsets(s.getText().length);
          document.execCommand('insertText', false, ' more words');
          const after = window.wasmBindings.document_counts();
          if (after.words !== model.words + 2) out.push('model words after typing ' + after.words);
          if (counter() !== fmt(after)) out.push('counter after typing ' + counter());

          // Toggle Write_On for A, then open B: B is plain.
          document.querySelector('.te-chrome-counter').click();
          if (!ed.chrome.isWriteOn()) out.push('toggle failed');
          ed.openDocument(teFixtures.plain.md, 'b.md');
          if (ed.chrome.isWriteOn()) out.push('B inherited write-on');
          if (document.body.dataset.mode === 'write-on') out.push('body still write-on for B');
          if (counter() !== fmt(window.wasmBindings.document_counts())) out.push('counter B ' + counter());
          // Back to A: Write_On comes back; then B again: plain.
          ed.openDocument(teFixtures.full.md, 'a.md');
          if (!ed.chrome.isWriteOn()) out.push('A write-on not restored');
          ed.openDocument(teFixtures.plain.md, 'b.md');
          if (ed.chrome.isWriteOn()) out.push('B not plain on return');
          teTest.resetWriteOn();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_chrome_save_surfaces_the_saved_document() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          teTest.resetWriteOn();
          const ed = window.__teEditor;
          const out = [];
          ed.openDocument(teFixtures.full.md, 'saved.md');
          document.querySelector('.te-chrome-counter').click();
          const saved = [];
          const onSaved = (e) => saved.push(e.detail);
          document.addEventListener('te:saved', onSaved);
          teTest.control('save').click();
          if (saved.length !== 1) out.push(saved.length + ' te:saved events');
          else {
            if (saved[0].text !== ed.saveDocument()) out.push('event text differs from saveDocument');
            if (!saved[0].text.includes('```terraphim-alternatives')) out.push('saved text has no block');
            if (saved[0].documentKey !== 'saved.md') out.push('documentKey ' + saved[0].documentKey);
            if (saved[0].editor !== ed) out.push('detail.editor');
          }
          saved.length = 0;
          // chrome.save() returns the text too.
          const returned = ed.chrome.save();
          if (typeof returned !== 'string' || returned !== saved[0].text) out.push('save() return');
          // Cancelling te:save stops the save: no te:saved, null returned.
          saved.length = 0;
          const cancel = (e) => e.preventDefault();
          document.addEventListener('te:save', cancel);
          if (ed.chrome.save() !== null) out.push('cancelled save returned text');
          if (saved.length !== 0) out.push('cancelled save dispatched te:saved');
          document.removeEventListener('te:save', cancel);
          document.removeEventListener('te:saved', onSaved);
          teTest.resetWriteOn();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_warning_is_styled_in_write_on_and_default_in_plain() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          teTest.resetWriteOn();
          const ed = window.__teEditor;
          const out = [];
          ed.openDocument(teFixtures.malformed.md, 'bad.md');
          const notice = document.querySelector('.te-warning');
          if (!teTest.visible(notice)) out.push('notice not visible in plain');
          const plainBg = getComputedStyle(notice).backgroundColor;
          if (plainBg !== 'rgba(0, 0, 0, 0)') out.push('plain background ' + plainBg);
          document.querySelector('.te-chrome-counter').click();
          const st = getComputedStyle(notice);
          if (st.backgroundColor === 'rgba(0, 0, 0, 0)') out.push('write-on panel background missing');
          if (st.display !== 'flex') out.push('write-on display ' + st.display);
          if (st.color === getComputedStyle(document.body).color) out.push('write-on text not dimmed');
          if (!teTest.visible(notice)) out.push('notice not visible in write-on');
          notice.querySelector('.te-warning-dismiss').click();
          if (teTest.visible(notice)) out.push('dismissed notice still visible in write-on');
          teTest.resetWriteOn();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
