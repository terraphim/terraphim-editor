//! Browser tests for persistence (issue #6) under edits: the malformed-block
//! warning, counts that include ghosted text, typing inside a ghost with
//! undo, and CRLF files. Run with `wasm-pack test --headless --chrome`. Real
//! scripts, the real exported document API and real fixtures
//! (`tests/support/fixtures.rs`); nothing is mocked. Open, save, reopen and
//! the golden files are in `web_persistence.rs`. See `tests/web.rs` for why
//! the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_malformed_block_shows_one_non_blocking_warning_and_is_kept() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          const warnings = [];
          ed.onWarning((w) => warnings.push(w));
          const source = teFixtures.malformed.md;
          s.focus();
          const opened = ed.openDocument(source);
          if (opened.body !== 'Body survives bad JSON.') out.push('body ' + JSON.stringify(opened.body));
          if (s.getText() !== opened.body) out.push('surface ' + JSON.stringify(s.getText()));
          if (!opened.warning) out.push('no warning returned');
          if (warnings.length !== 1 || warnings[0] !== opened.warning) out.push('listener calls ' + JSON.stringify(warnings));
          const notices = document.querySelectorAll('.te-warning');
          if (notices.length !== 1) out.push(notices.length + ' notices');
          const notice = notices[0];
          if (!notice || notice.hidden) out.push('notice hidden');
          else {
            if (notice.getAttribute('role') !== 'status') out.push('role ' + notice.getAttribute('role'));
            if (!notice.textContent.includes('could not be read')) out.push('text ' + notice.textContent);
            if (notice.contains(document.activeElement)) out.push('notice took focus');
          }
          // Non-blocking: editing continues and the block survives the save.
          if (document.activeElement !== s.root) out.push('surface lost focus');
          if (ed.saveDocument() !== source) out.push('save lost the raw block');
          s.setSelectionOffsets(0, 4);
          document.execCommand('insertText', false, 'Text');
          if (ed.saveDocument() !== source.replace('Body', 'Text')) out.push('edited save ' + JSON.stringify(ed.saveDocument()));
          if (!ed.annotations().preservedBlock) out.push('preservedBlock flag');
          // Dismissing hides it; opening a clean file keeps it hidden.
          notice.querySelector('.te-warning-dismiss').click();
          if (!notice.hidden) out.push('dismiss did not hide');
          ed.openDocument(teFixtures.plain.md);
          if (!notice.hidden) out.push('warning survives a clean open');
          if (document.querySelectorAll('.te-warning').length !== 1) out.push('notice duplicated');
          ed.openDocument(source);
          if (notice.hidden) out.push('reopening malformed did not warn');
          if (document.querySelectorAll('.te-warning').length !== 1) out.push('notice duplicated on reopen');
          ed.destroy();
          if (document.querySelector('.te-warning')) out.push('destroy left the notice');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_counts_include_ghosted_text_and_follow_edits() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          ed.openDocument(teFixtures.full.md);
          const text = s.getText();
          // The editor's word definition (issue #59), as chrome.js mirrors it.
          const wordRe = /[\p{Alphabetic}\p{N}_]+(?:['’\-./][\p{Alphabetic}\p{N}_]+)*/gu;
          const words = (text.match(wordRe) || []).length;
          const chars = Array.from(text).length;
          const c = ed.counts();
          if (c.words !== words || c.chars !== chars) out.push('counts ' + JSON.stringify(c) + ' want ' + words + '/' + chars);
          const exportWords = (ed.exportDocument().match(wordRe) || []).length;
          if (!(exportWords < c.words)) out.push('ghosted words not counted');
          s.focus();
          s.setSelectionOffsets(s.getText().length);
          document.execCommand('insertText', false, ' extra');
          if (ed.counts().words !== words + 1) out.push('count after typing ' + ed.counts().words);
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_typing_inside_ghost_and_undo_keep_model_in_step() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          ed.openDocument(teFixtures.full.md);
          const g2 = ed.annotations().ghosts[1];
          // Type inside the second ghost: it grows and the text stays ghosted.
          s.focus();
          s.setSelectionOffsets(g2.anchor.start + 4);
          document.execCommand('insertText', false, 'XY');
          const grown = ed.annotations().ghosts[1];
          if (grown.anchor.end !== g2.anchor.end + 2) out.push('ghost did not grow ' + JSON.stringify(grown.anchor));
          if (ed.exportDocument().includes('XY')) out.push('ghosted typing exported');
          // Undo replays the inverse edit into the model.
          s.undo();
          if (window.wasmBindings.document_body() !== s.getText()) out.push('model drifted after undo');
          const back = ed.annotations().ghosts[1];
          if (JSON.stringify(back.anchor) !== JSON.stringify(g2.anchor)) out.push('ghost after undo ' + JSON.stringify(back.anchor));
          // Typing inside a word with alternatives detaches that span.
          const s3 = ed.annotations().spans.find((x) => x.id === 's3');
          s.setSelectionOffsets(s3.anchor.start + 1);
          document.execCommand('insertText', false, 'z');
          const a = ed.annotations();
          if (a.spans.some((x) => x.id === 's3')) out.push('edited span still attached');
          const kept = a.setAside.spans.find((x) => x.id === 's3');
          if (!kept || kept.alts.length !== 3) out.push('detached span not kept ' + JSON.stringify(a.setAside));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_crlf_file_reanchors_after_surface_normalises_line_endings() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          // Body with CRLF line endings; the block itself stays as written.
          const md = teFixtures.full.md;
          const cut = md.indexOf('```terraphim-alternatives');
          const crlf = md.slice(0, cut).replace(/\n/g, '\r\n') + md.slice(cut);
          const opened = ed.openDocument(crlf);
          if (s.getText().includes('\r')) out.push('surface kept CR');
          if (window.wasmBindings.document_body() !== s.getText()) out.push('model body not aligned');
          const a = ed.annotations();
          if (a.spans.length !== 3 || a.ghosts.length !== 2) out.push('annotations lost ' + JSON.stringify(a.setAside));
          if (ed.exportDocument() !== teFixtures.full.export) out.push('export ' + JSON.stringify(ed.exportDocument()));
          const saved = ed.saveDocument();
          ed.openDocument(saved);
          const b = ed.annotations();
          if (JSON.stringify(b.spans) !== JSON.stringify(a.spans)) out.push('spans after reopen');
          if (JSON.stringify(b.ghosts) !== JSON.stringify(a.ghosts)) out.push('ghosts after reopen');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
