//! Browser tests for opening documents (issue #76): the file input and drag
//! and drop of files (the question before a dirty document is replaced is in
//! `web_files_replace.rs`). Run
//! with `wasm-pack test --headless --chrome`. Real `File`s through a real
//! `<input type=file>` and real `DragEvent`s with `DataTransfer`, the real
//! exported document API and the persistence fixtures; nothing is mocked
//! (see `tests/support/files.rs`). See `tests/web.rs` for why the browser
//! tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_open_through_the_file_input_restores_annotations_and_round_trips() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const canonical = teFiles.canonicalFull(ed);
        ed.persistence.load(teFixtures.plain.md, 'plain.md');
        if (ed.annotations().spans.length !== 0) return 'plain fixture has spans';
        const events = [];
        const rec = (e) => events.push(e.type + ':' + (e.detail.name || ''));
        document.addEventListener('te:opened', rec);
        try {
          teFiles.setInputFile(ed.persistence.fileInput, teFiles.file(canonical, 'notes.md'));
          if (!(await ed.persistence.lastOpen)) return 'open through the input failed';
          const a = ed.annotations();
          if (a.spans.length !== 3) out.push('spans ' + a.spans.length);
          if (a.ghosts.length !== 2) out.push('ghosts ' + a.ghosts.length);
          if (!a.overflow.startsWith('Stashed idea.')) out.push('overflow ' + JSON.stringify(a.overflow));
          const body = ed.surface.getText();
          if (body.includes('terraphim-alternatives')) out.push('block reached the surface');
          if (!body.startsWith('# Why isn')) out.push('body ' + JSON.stringify(body.slice(0, 20)));
          if (ed.documentKey !== 'notes.md') out.push('documentKey ' + ed.documentKey);
          if (ed.chrome.storageKey() !== window.WRITE_ON_STORAGE_PREFIX + 'notes.md') out.push('chrome key ' + ed.chrome.storageKey());
          if (ed.persistence.fileName !== 'notes.md' || ed.persistence.handle !== null) out.push('file state');
          if (ed.persistence.dirty || document.title.startsWith('•')) out.push('dirty after open ' + ed.persistence.dirty);
          if (!document.querySelector('.te-files-notice').hidden) out.push('notice shown after a clean open');
          if (events.join(',') !== 'te:opened:notes.md') out.push('events ' + events.join(','));
          // Round trip: saving the opened file gives back exactly its text.
          const cap = teFiles.captureDownloads();
          try {
            ed.persistence.save();
            await ed.persistence.lastWrite;
            if (cap.list.length !== 1) return out.concat('downloads ' + cap.list.length).join('; ');
            if (cap.list[0].name !== 'notes.md') out.push('download name ' + cap.list[0].name);
            if ((await teFiles.read(cap.list[0])) !== canonical) out.push('open -> save is not the file text');
          } finally {
            cap.stop();
          }
          // A .txt file opens too; a picture does not, and says so.
          teFiles.setInputFile(ed.persistence.fileInput, teFiles.file('Plain words.\n', 'list.txt'));
          if (!(await ed.persistence.lastOpen)) out.push('txt did not open');
          if (ed.surface.getText() !== 'Plain words.\n') out.push('txt body ' + JSON.stringify(ed.surface.getText()));
          const png = new File(['x'], 'photo.png', { type: 'image/png' });
          teFiles.setInputFile(ed.persistence.fileInput, png);
          if (await ed.persistence.lastOpen) out.push('png opened');
          const notice = document.querySelector('.te-files-notice');
          if (notice.hidden || !notice.textContent.includes('photo.png is not a Markdown or text file')) out.push('png notice ' + JSON.stringify(notice.textContent));
          if (ed.surface.getText() !== 'Plain words.\n') out.push('png replaced the text');
          if (ed.persistence.fileInput.value !== '') out.push('input value not reset');
        } finally {
          document.removeEventListener('te:opened', rec);
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_dropping_a_file_opens_it_and_text_drops_are_untouched() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const canonical = teFiles.canonicalFull(ed);
        ed.persistence.load(teFixtures.plain.md, 'plain.md');
        // A Markdown file dropped on the surface opens.
        let r = teFiles.dropFile(ed.surface.root, teFiles.file(canonical, 'dropped.md'));
        if (!r.over) out.push('file dragover not accepted');
        if (!r.drop) out.push('file drop not prevented');
        if (!(await ed.persistence.lastOpen)) return out.concat('drop did not open').join('; ');
        if (ed.annotations().spans.length !== 3) out.push('spans after drop ' + ed.annotations().spans.length);
        if (ed.documentKey !== 'dropped.md') out.push('key after drop ' + ed.documentKey);
        // On the bare page (the Write_On margins) and the Overflow panel too.
        document.querySelector('.te-chrome-counter').click();
        teFiles.dropFile(document.body, teFiles.file(teFixtures.plain.md, 'margin.md'));
        if (!(await ed.persistence.lastOpen) || ed.documentKey !== 'margin.md') out.push('body drop ' + ed.documentKey);
        teTest.control('overflow').click();
        const panel = document.querySelector('.te-overflow');
        teFiles.dropFile(panel, teFiles.file(canonical, 'panel.md'));
        if (!(await ed.persistence.lastOpen) || ed.documentKey !== 'panel.md') out.push('panel drop ' + ed.documentKey);
        teTest.control('overflow').click();
        document.querySelector('.te-chrome-counter').click();
        // Something that is not Markdown or text: a notice, nothing opened.
        r = teFiles.dropFile(ed.surface.root, new File(['x'], 'photo.png', { type: 'image/png' }));
        if (!r.drop) out.push('png drop not prevented (the browser would navigate to it)');
        const notice = document.querySelector('.te-files-notice');
        if (notice.hidden || notice.dataset.kind !== 'error') out.push('png drop notice');
        if (ed.documentKey !== 'panel.md') out.push('png drop changed the document');
        // A text drag is the surface's own drop: inserted, nothing opened.
        const before = ed.surface.getText();
        const dt = new DataTransfer();
        dt.setData('text/plain', 'DROPPED');
        teFiles.drag(ed.surface.root, dt);
        if (ed.surface.getText().length !== before.length + 7 || !ed.surface.getText().includes('DROPPED')) out.push('text drop not inserted');
        if (ed.documentKey !== 'panel.md') out.push('text drop opened a document');
        // A drop elsewhere on a host page is left to the host.
        const host = document.createElement('div');
        document.body.appendChild(host);
        r = teFiles.dropFile(host, teFiles.file('x', 'host.md'));
        host.remove();
        if (r.over || r.drop) out.push('took a file drop outside the editor');
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
