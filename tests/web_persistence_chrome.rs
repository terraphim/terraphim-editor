//! Browser tests for persistence (issue #6) across undo and reopen, and
//! together with the Write_On chrome (issue #7): set-aside annotations
//! reattaching, the open warning, chrome counts and save. Run with
//! `wasm-pack test --headless --chrome`. Real scripts, the real exported
//! document API and real fixtures; nothing is mocked. See `tests/web.rs` for
//! why the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_ctrl_z_after_typing_inside_a_span_reattaches_its_alternatives() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          ed.openDocument(teFixtures.full.md);
          const before = ed.annotations();
          const s2 = before.spans.find((x) => x.id === 's2');
          // Real typing inside "struggle" (five alternatives).
          s.focus();
          s.setSelectionOffsets(s2.anchor.start + 3);
          if (!document.execCommand('insertText', false, 'x')) out.push('execCommand failed');
          let a = ed.annotations();
          if (a.spans.some((x) => x.id === 's2')) out.push('span still live after edit');
          if (!a.setAside.spans.some((x) => x.id === 's2')) out.push('span not set aside');
          const notice = document.querySelector('.te-warning');
          if (!notice || notice.hidden) out.push('no detach notice');
          else {
            if (notice.dataset.kind !== 'detached') out.push('kind ' + notice.dataset.kind);
            if (!notice.textContent.includes('1 span with 5 alternatives')) out.push('text ' + notice.textContent);
            if (!notice.textContent.includes('preserved')) out.push('notice does not say preserved');
            if (notice.contains(document.activeElement)) out.push('notice took focus');
          }
          if (document.activeElement !== s.root) out.push('surface lost focus');
          // Saving now keeps the detached alternatives in the block.
          const saved = ed.saveDocument();
          if (!saved.includes('"friction"') || !saved.includes('"challenge"')) out.push('save dropped detached alternatives');
          // Ctrl+Z replays the inverse edit and the span comes back.
          if (!teTest.key(s.root, 'z', { ctrlKey: true })) out.push('ctrl+z not handled');
          a = ed.annotations();
          const back = a.spans.find((x) => x.id === 's2');
          if (!back) out.push('span not re-attached');
          else if (JSON.stringify(back) !== JSON.stringify(s2)) out.push('re-attached span differs ' + JSON.stringify(back));
          if (a.setAside.spans.length || a.setAside.ghosts.length) out.push('set aside after undo ' + JSON.stringify(a.setAside));
          if (!notice.hidden) out.push('detach notice not cleared after re-attach');
          // Redo detaches it again, with the notice.
          teTest.key(s.root, 'z', { ctrlKey: true, shiftKey: true });
          if (ed.annotations().spans.some((x) => x.id === 's2')) out.push('redo did not detach');
          if (notice.hidden) out.push('redo did not warn');
          teTest.key(s.root, 'z', { ctrlKey: true });
          if (!ed.annotations().spans.some((x) => x.id === 's2')) out.push('second undo did not re-attach');
          const ids = ed.annotations().spans.map((x) => x.id).sort().join();
          if (ids !== 's1,s2,s3') out.push('final spans ' + ids);
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_detached_alternatives_survive_save_and_reopen() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          ed.openDocument(teFixtures.full.md, 'full.md');
          if (ed.documentKey !== 'full.md') out.push('documentKey ' + ed.documentKey);
          const s3 = ed.annotations().spans.find((x) => x.id === 's3');
          s.focus();
          s.setSelectionOffsets(s3.anchor.start + 2);
          document.execCommand('insertText', false, 'Q');
          const saved = ed.saveDocument();
          // Reopen: the detached span is set aside again, preserved, and the
          // open notice says so.
          const opened = ed.openDocument(saved, { name: 'again.md' });
          if (ed.documentKey !== 'again.md') out.push('documentKey object form');
          if (opened.unresolved !== 1) out.push('unresolved ' + opened.unresolved);
          if (!opened.warning || !opened.warning.includes('preserved')) out.push('open warning ' + opened.warning);
          const a = ed.annotations();
          const kept = a.setAside.spans.find((x) => x.id === 's3');
          if (!kept || kept.alts.length !== 3) out.push('s3 not preserved ' + JSON.stringify(a.setAside));
          if (ed.saveDocument() !== saved) out.push('save/open cycle changed the file');
          // Deleting the typed letter re-attaches it in the reopened file.
          s.focus();
          s.setSelectionOffsets(s3.anchor.start + 2, s3.anchor.start + 3);
          document.execCommand('delete');
          if (!ed.annotations().spans.some((x) => x.id === 's3')) out.push('not re-attached after fix');
          // An open without a name leaves documentKey alone.
          ed.openDocument('plain');
          if (ed.documentKey !== 'again.md') out.push('documentKey changed by unnamed open');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

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

#[wasm_bindgen_test]
fn test_open_warning_clears_once_set_aside_annotations_reattach() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          // Same-length damage to two spans' text, so their stored offsets
          // still point where the fixed text will be.
          const md = teFixtures.full.md;
          const cut = md.indexOf('```terraphim-alternatives');
          const body = md.slice(0, cut).replace('eraser holding', 'erasXr holding').replace('struggle', 'strugglX');
          const opened = ed.openDocument(body + md.slice(cut));
          // s2, s3 and the ghost g1 (which contains "eraser") are set aside.
          if (opened.unresolved !== 3) out.push('unresolved ' + opened.unresolved);
          const notice = document.querySelector('.te-warning');
          if (!notice || notice.hidden) return 'no open notice';
          if (notice.dataset.kind !== 'set-aside') out.push('kind ' + notice.dataset.kind);
          // Fix "struggle": the notice stays and its count follows.
          const text = s.getText();
          let at = text.indexOf('strugglX') + 7;
          s.focus();
          s.setSelectionOffsets(at, at + 1);
          document.execCommand('insertText', false, 'e');
          if (!ed.annotations().spans.some((x) => x.id === 's2')) out.push('s2 not re-attached');
          if (notice.hidden) out.push('notice cleared with one still set aside');
          else if (!notice.textContent.startsWith('2 annotations')) out.push('count text ' + notice.textContent);
          // Fix "eraser": s3 and g1 re-attach, nothing is set aside, the notice clears.
          at = s.getText().indexOf('erasXr') + 4;
          s.setSelectionOffsets(at, at + 1);
          document.execCommand('insertText', false, 'e');
          if (!ed.annotations().spans.some((x) => x.id === 's3')) out.push('s3 not re-attached');
          if (!ed.annotations().ghosts.some((x) => x.id === 'g1')) out.push('g1 not re-attached');
          if (!notice.hidden) out.push('notice not cleared: ' + notice.textContent);
          // The malformed warning, by contrast, survives edits.
          ed.openDocument(teFixtures.malformed.md);
          s.focus();
          s.setSelectionOffsets(0);
          document.execCommand('insertText', false, 'x');
          if (notice.hidden || notice.dataset.kind !== 'malformed') out.push('malformed warning cleared by an edit');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_full_resync_that_sets_annotations_aside_shows_and_clears_the_notice() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          ed.openDocument(teFixtures.full.md);
          const notice = () => document.querySelector('.te-warning');
          if (notice() && !notice().hidden) out.push('notice after a clean open');
          // Drift: change the surface without mirroring it into the model, as
          // an edit the model could not follow would. The next read re-syncs
          // the whole body; the chrome counter's counts() call does it at once.
          const text = s.getText();
          const at = text.indexOf('struggle');
          ed.suppressModelSync = true;
          s.replaceRange(at, at + 8, 'tension!');
          ed.suppressModelSync = false;
          ed.counts();
          if (window.wasmBindings.document_body() !== s.getText()) out.push('model not re-synced');
          const a = ed.annotations();
          if (!a.setAside.spans.some((x) => x.id === 's2')) out.push('s2 not set aside by the re-sync');
          if (!notice() || notice().hidden) out.push('re-sync showed no notice');
          else if (notice().dataset.kind !== 's' + 'et-aside' || !notice().textContent.includes('preserved')) out.push('notice ' + notice().dataset.kind + ' ' + notice().textContent);
          // Drift back: the re-sync re-attaches s2 and clears the notice.
          ed.suppressModelSync = true;
          s.replaceRange(at, at + 8, 'struggle');
          ed.suppressModelSync = false;
          ed.saveDocument();
          if (!ed.annotations().spans.some((x) => x.id === 's2')) out.push('s2 not re-attached by the re-sync');
          if (!notice().hidden) out.push('notice not cleared after re-attach');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
