//! Browser tests for the question before a dirty document is replaced
//! (issue #76): Save first / Discard changes / Cancel / Escape. Run with
//! `wasm-pack test --headless --chrome`. Real `File`s, the real exported
//! document API and the persistence fixtures; nothing is mocked (see
//! `tests/support/files.rs`). See `tests/web.rs` for why the browser tests
//! are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_replacing_a_dirty_document_asks_first() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const canonical = teFiles.canonicalFull(ed);
        ed.persistence.load(teFixtures.plain.md, 'plain.md');
        teFiles.type(ed, ' Unsaved words.');
        if (!ed.persistence.dirty) return 'typing did not mark the document dirty';
        const typed = ed.surface.getText();
        const dialog = () => document.querySelector('.te-files-replace');
        const answer = async (action) => {
          const p = ed.persistence.openFile(teFiles.file(canonical, 'next.md'));
          // Reading the file is asynchronous; wait for the question.
          for (let i = 0; i < 50 && !dialog(); i += 1) await teFiles.wait(10);
          const d = dialog();
          if (!d) return { problem: 'no dialog' };
          const info = {
            open: d.open,
            modal: d.matches(':modal'),
            text: d.textContent,
            buttons: Array.from(d.querySelectorAll('button')).map((b) => b.dataset.action).join(','),
            focus: document.activeElement && document.activeElement.dataset.action,
            label: d.getAttribute('aria-labelledby') && document.getElementById(d.getAttribute('aria-labelledby')).textContent,
          };
          if (action === 'escape') {
            // requestClose() is what Escape does: cancel, then close.
            if (typeof d.requestClose === 'function') d.requestClose(); else d.close();
          } else {
            d.querySelector('[data-action="' + action + '"]').click();
          }
          info.result = await p;
          info.left = !!dialog();
          return info;
        };
        // Cancel: nothing changes.
        let i = await answer('cancel');
        if (i.problem) return i.problem;
        if (!i.open || !i.modal) out.push('dialog not modal');
        if (i.buttons !== 'save,discard,cancel') out.push('buttons ' + i.buttons);
        if (i.focus !== 'save') out.push('focus ' + i.focus);
        if (i.label !== 'Unsaved changes') out.push('label ' + i.label);
        if (!i.text.includes('Opening next.md replaces the current document')) out.push('text ' + i.text);
        if (i.result !== false || i.left) out.push('cancel result ' + i.result + ' left ' + i.left);
        if (ed.surface.getText() !== typed || !ed.persistence.dirty) out.push('cancel changed the document');
        // Escape: the same as Cancel.
        i = await answer('escape');
        if (i.result !== false || ed.surface.getText() !== typed) out.push('escape opened the file');
        // Save first, but the host cancels te:save: the open stops too.
        const cancel = (e) => e.preventDefault();
        document.addEventListener('te:save', cancel);
        i = await answer('save');
        document.removeEventListener('te:save', cancel);
        if (i.result !== false || ed.surface.getText() !== typed) out.push('cancelled save still opened');
        // Save first: the current document is saved (downloaded), then opened.
        const cap = teFiles.captureDownloads();
        try {
          i = await answer('save');
          if (i.result !== true) out.push('save first did not open');
          if (cap.list.length !== 1) out.push('save first downloads ' + cap.list.length);
          else {
            if (cap.list[0].name !== 'plain.md') out.push('saved as ' + cap.list[0].name);
            if (!(await teFiles.read(cap.list[0])).includes('Unsaved words.')) out.push('saved text lacks the edit');
          }
        } finally {
          cap.stop();
        }
        if (ed.documentKey !== 'next.md' || ed.annotations().spans.length !== 3) out.push('not opened after save');
        if (ed.persistence.dirty) out.push('dirty after open');
        // Discard: opened without saving.
        teFiles.type(ed, ' More.');
        i = await answer('discard');
        if (i.result !== true || ed.surface.getText().includes('More.')) out.push('discard did not replace');
        // A clean document is replaced without asking.
        const p = ed.persistence.openFile(teFiles.file(teFixtures.plain.md, 'quiet.md'));
        if (!(await p) || dialog()) out.push('asked about a clean document');
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
