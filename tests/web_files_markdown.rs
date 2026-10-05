//! Browser tests for the Markdown export view behind `M↓` (issue #73): the
//! dialog shows exactly `export_document()`, Copy and Download .md work,
//! Escape closes it and focus returns, in Write_On and plain mode. Run with
//! `wasm-pack test --headless --chrome`. Real scripts, the real exported
//! document API and the persistence fixtures; nothing is mocked (see
//! `tests/support/files.rs`). See `tests/web.rs` for why the browser tests
//! are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_markdown_dialog_shows_the_export_with_copy_and_download() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const canonical = teFiles.canonicalFull(ed);
        ed.persistence.load(canonical, 'notes.md');
        const events = [];
        const rec = (e) => events.push(e.type + (e.detail.editor === ed ? '' : '!editor'));
        document.addEventListener('te:markdown', rec);
        const cap = teFiles.captureDownloads();
        try {
          document.querySelector('.te-chrome-counter').click();
          const ctl = teTest.control('markdown');
          ctl.focus();
          ctl.click();
          const d = document.querySelector('.te-files-markdown');
          if (!d) return 'no dialog';
          if (!d.open || !d.matches(':modal')) out.push('not a modal dialog');
          if (!teTest.visible(d)) out.push('dialog not visible');
          const label = document.getElementById(d.getAttribute('aria-labelledby'));
          if (!label || label.textContent !== 'Markdown') out.push('label');
          const area = d.querySelector('textarea');
          if (!area.readOnly) out.push('text area not read-only');
          if (area.getAttribute('aria-label') !== 'Exported Markdown') out.push('area label');
          const exported = ed.exportDocument();
          if (area.value !== exported) out.push('dialog text differs from exportDocument()');
          if (exported !== teFixtures.full.export) out.push('export differs from the golden export');
          if (area.value.includes('terraphim-alternatives')) out.push('annotation block in the export');
          if (area.value.includes('This whole paragraph might go')) out.push('ghosted text in the export');
          if (area.value.includes('Stashed idea')) out.push('overflow in the export');
          if (document.activeElement !== area) out.push('focus not in the text');
          if (events.join(',') !== 'te:markdown') out.push('events ' + events.join(','));
          // Copy: the Clipboard API, or (as in headless Chrome, where it is
          // refused) the selected text and a message.
          d.querySelector('[data-action="copy"]').click();
          const how = await ed.persistence.lastCopy;
          const status = d.querySelector('[role="status"]').textContent;
          if (how === 'clipboard' || how === 'execCommand') {
            if (status !== 'Copied to the clipboard.') out.push('copy status ' + status);
          } else if (how === 'selected') {
            if (!status.startsWith('Copying is blocked here')) out.push('fallback status ' + status);
            if (area.selectionStart !== 0 || area.selectionEnd !== area.value.length) out.push('text not selected');
          } else {
            out.push('copy result ' + how);
          }
          // Download .md: the export, named after the document.
          d.querySelector('[data-action="download"]').click();
          await ed.persistence.lastWrite;
          if (cap.list.length !== 1) out.push('downloads ' + cap.list.length);
          else {
            if (cap.list[0].name !== 'notes-export.md') out.push('name ' + cap.list[0].name);
            if ((await teFiles.read(cap.list[0])) !== exported) out.push('downloaded text differs from the export');
          }
          if (!d.querySelector('[role="status"]').textContent.includes('notes-export.md')) out.push('download status');
          // Escape (requestClose) closes it and focus returns to M-down.
          if (typeof d.requestClose === 'function') d.requestClose(); else d.close();
          await teFiles.wait(30);
          if (document.querySelector('.te-files-markdown')) out.push('dialog left after Escape');
          if (document.activeElement !== ctl) out.push('focus did not return: ' + (document.activeElement && document.activeElement.className));
          // M-down twice gives one dialog. (The close event is queued, so
          // the steps above and below wait a little after closing.)
          ctl.click();
          ctl.click();
          await teFiles.wait(30);
          if (document.querySelectorAll('.te-files-markdown').length !== 1) out.push('dialogs ' + document.querySelectorAll('.te-files-markdown').length);
          document.querySelector('.te-files-markdown .te-files-close').click();
          await teFiles.wait(30);
          if (document.querySelector('.te-files-markdown')) out.push('close button left the dialog');
          // Plain mode: the toolbar's Markdown button opens the same view.
          document.querySelector('.te-chrome-counter').click();
          const tb = document.querySelector('.toolbar [data-te-file="markdown"]');
          if (!tb || !tb.querySelector('i.fa-markdown')) return out.concat('no toolbar Markdown button').join('; ');
          tb.click();
          const d2 = document.querySelector('.te-files-markdown');
          if (!d2 || !d2.open || d2.querySelector('textarea').value !== exported) out.push('plain-mode dialog');
          // Edits show up the next time it opens.
          d2.close();
          await teFiles.wait(30);
          teFiles.type(ed, ' Fresh words.');
          tb.click();
          if (!document.querySelector('.te-files-markdown textarea').value.includes('Fresh words.')) out.push('stale export');
          document.querySelector('.te-files-markdown').close();
          await teFiles.wait(30);
        } finally {
          document.removeEventListener('te:markdown', rec);
          cap.stop();
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
