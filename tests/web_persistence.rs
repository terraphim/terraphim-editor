//! Browser tests for persistence (issue #6): the document API, open, edit,
//! save, reopen, golden preview and export, and plain Markdown. The
//! malformed-block warning, counts, ghost typing and line endings are in
//! `web_persistence_edits.rs`. Run with `wasm-pack test --headless --chrome`. Real scripts, the
//! real exported document API and real fixtures (`tests/support/fixtures.rs`);
//! nothing is mocked. Reattachment across undo and reopen is in
//! `web_persistence_reattach.rs`; persistence with the Write_On chrome (issue
//! #7) is in `web_persistence_chrome.rs`.
//! See `tests/web.rs` for why the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_document_api_is_exposed_through_wasm_bindings() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const out = [];
          if (!ed.documentApi()) out.push('no document api');
          // The welcome text is the open document and the model tracks it.
          if (window.wasmBindings.document_body() !== ed.surface.getText()) out.push('model body differs on load');
          ed.surface.setText('one two');
          if (window.wasmBindings.document_body() !== 'one two') out.push('setText not mirrored');
          const c = ed.counts();
          if (c.words !== 2 || c.chars !== 7) out.push('counts ' + JSON.stringify(c));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_open_edit_save_reopen_restores_alternatives_ghosts_and_overflow() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const s = ed.surface;
          const out = [];
          const opened = ed.openDocument(teFixtures.full.md);
          if (opened.warning !== null) out.push('unexpected warning ' + opened.warning);
          const text = s.getText();
          if (text.includes('terraphim-alternatives') || text.includes('"spans"')) out.push('block on the surface');
          if (s.root.textContent !== text) out.push('surface DOM differs from text');
          if (teTest.preview().includes('terraphim-alternatives')) out.push('block in the preview');
          if (s.canUndo()) out.push('undo reaches the previous document');
          const before = ed.annotations();

          // Native typing before every annotation, then a deletion after them.
          s.focus();
          s.setSelectionOffsets(0);
          if (!document.execCommand('insertText', false, 'Draft: ')) out.push('execCommand failed');
          const end = s.getText().length;
          s.setSelectionOffsets(end - 1, end);
          document.execCommand('delete');
          if (window.wasmBindings.document_body() !== s.getText()) out.push('model drifted from surface');

          const saved = ed.saveDocument();
          if (!saved.startsWith(s.getText())) out.push('save does not start with the body');
          if (!saved.includes('```terraphim-alternatives')) out.push('save lost the block');

          // Reopen in a fresh model: everything is back, shifted by 7.
          ed.openDocument('something else');
          const reopened = ed.openDocument(saved);
          if (reopened.body !== s.getText()) out.push('reopened body');
          const after = ed.annotations();
          // Anchors carry before/after context (#36), which legitimately
          // changes next to the two edits; compare everything else exactly.
          const plain = (a) => { const { before, after, ...anchor } = a.anchor; return { ...a, anchor }; };
          const shift = (a) => { const p = plain(a); return { ...p, anchor: { ...p.anchor, start: p.anchor.start + 7, end: p.anchor.end + 7 } }; };
          const want = JSON.stringify({ spans: before.spans.map(shift), ghosts: before.ghosts.map(shift), overflow: before.overflow });
          const got = JSON.stringify({ spans: after.spans.map(plain), ghosts: after.ghosts.map(plain), overflow: after.overflow });
          if (got !== want) out.push('annotations differ: ' + got);
          // The context next to the inserted prefix now includes it.
          if (after.spans[0].anchor.before !== 'Draft: # ') out.push('s1 context before ' + JSON.stringify(after.spans[0].anchor.before));
          if (after.spans.map((x) => x.active).join() !== '1,3,1') out.push('active indices ' + after.spans.map((x) => x.active));
          if (after.ghosts.length !== 2) out.push('ghosts ' + after.ghosts.length);
          if (!after.overflow.includes('Stashed idea.')) out.push('overflow lost');
          if (ed.saveDocument() !== saved) out.push('second save differs');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_preview_and_export_match_golden_files() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const out = [];
          for (const [name, f] of Object.entries(teFixtures)) {
            ed.openDocument(f.md);
            const preview = teTest.preview();
            if (preview !== f.preview) out.push(name + ' preview ' + JSON.stringify(preview));
            const exported = ed.exportDocument();
            if (exported !== f.export) out.push(name + ' export ' + JSON.stringify(exported));
          }
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_plain_markdown_opens_without_block_or_warning() {
    let _document = fresh_full_editor();
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const out = [];
          const warnings = [];
          ed.onWarning((w) => warnings.push(w));
          const opened = ed.openDocument(teFixtures.plain.md);
          if (opened.body !== teFixtures.plain.md) out.push('body ' + JSON.stringify(opened.body));
          if (ed.surface.getText() !== teFixtures.plain.md) out.push('surface text');
          if (ed.saveDocument() !== teFixtures.plain.md) out.push('save changed a plain file');
          if (ed.exportDocument() !== teFixtures.plain.md) out.push('export changed a plain file');
          const notice = document.querySelector('.te-warning');
          if (notice && !notice.hidden) out.push('warning shown');
          if (warnings.some((w) => w !== null)) out.push('warning emitted ' + JSON.stringify(warnings));
          const a = ed.annotations();
          if (a.spans.length || a.ghosts.length || a.overflow) out.push('annotations ' + JSON.stringify(a));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
