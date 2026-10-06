//! Browser tests for the save and open flow (issue #76): writing back to a
//! kept file handle (real origin-private file handles), cancelling te:open
//! and te:markdown, and teardown. Run with `wasm-pack test --headless
//! --chrome`. Nothing is mocked; see `tests/support/files.rs` and
//! `tests/web_files.rs`.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_save_writes_back_to_a_kept_file_handle() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        // The File System Access path: the default option, which headless
        // Chrome supports for origin-private file handles.
        let ed = teFiles.reinit({ fileSystemAccess: undefined });
        if (!ed.persistence.fsAccess) return 'File System Access not detected';
        const dir = await navigator.storage.getDirectory();
        const handle = await dir.getFileHandle('kept.md', { create: true });
        const canonical = teFiles.canonicalFull(ed);
        {
          const w = await handle.createWritable();
          await w.write(canonical);
          await w.close();
        }
        // Open from the handle: the handle is kept and the name is the key.
        const opened = await ed.persistence.openFile(await handle.getFile(), handle);
        if (!opened) return 'openFile returned false';
        if (ed.persistence.handle !== handle) out.push('handle not kept');
        if (ed.documentKey !== 'kept.md') out.push('documentKey ' + ed.documentKey);
        if (ed.annotations().spans.length !== 3) out.push('spans ' + ed.annotations().spans.length);
        // Edit and save: written to the same file, no picker, no download.
        const cap = teFiles.captureDownloads();
        const written = [];
        const onWritten = (e) => written.push(e.detail.method + ':' + e.detail.name);
        document.addEventListener('te:written', onWritten);
        try {
          teFiles.type(ed, ' Appended.');
          const text = ed.persistence.save();
          if (!(await ed.persistence.lastWrite)) return out.concat('write failed').join('; ');
          const onDisk = await (await handle.getFile()).text();
          if (onDisk !== text) out.push('file on disk differs from the saved text');
          if (!onDisk.includes('Appended.')) out.push('edit not written');
          if (cap.list.length !== 0) out.push('downloaded instead of writing');
          if (written.join(',') !== 'file:kept.md') out.push('written ' + written.join(','));
          if (ed.persistence.dirty) out.push('dirty after writing');
          // A first save with no handle asks for a file; without a user
          // gesture the picker refuses, and the save falls back to a
          // download named from the first heading (the new editor has no key).
          ed = teFiles.reinit({ fileSystemAccess: undefined });
          if (ed.persistence.handle) out.push('handle survived re-creating the editor');
          ed.persistence.save();
          await ed.persistence.lastWrite;
          if (cap.list.length !== 1) out.push('no download fallback: ' + cap.list.length);
          else if (cap.list[0].name !== 'why-isn-t-everything-obvious.md') out.push('fallback name ' + cap.list[0].name);
        } finally {
          document.removeEventListener('te:written', onWritten);
          cap.stop();
          await dir.removeEntry('kept.md').catch(() => {});
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_default_actions_are_cancelable_and_destroy_cleans_up() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const p = ed.persistence;
        document.querySelector('.te-chrome-counter').click();
        // te:open cancelled: the file input is never clicked.
        let inputClicks = 0;
        const onInputClick = (e) => { inputClicks += 1; e.preventDefault(); };
        p.fileInput.addEventListener('click', onInputClick);
        const cancel = (e) => e.preventDefault();
        document.addEventListener('te:open', cancel);
        teTest.control('open').click();
        if (!teTest.key(document.body, 'o', { ctrlKey: true })) out.push('Ctrl+O not prevented');
        document.removeEventListener('te:open', cancel);
        if (inputClicks !== 0) out.push('cancelled open clicked the input ' + inputClicks);
        // Not cancelled: the hidden input is clicked (the browser shows its
        // file chooser for a real click).
        teTest.control('open').click();
        teTest.key(document.body, 'o', { metaKey: true });
        if (inputClicks !== 2) out.push('open clicks ' + inputClicks);
        p.fileInput.removeEventListener('click', onInputClick);
        // te:markdown cancelled: no dialog.
        document.addEventListener('te:markdown', cancel);
        teTest.control('markdown').click();
        document.removeEventListener('te:markdown', cancel);
        if (document.querySelector('.te-files-markdown')) out.push('cancelled M-down opened the dialog');
        // The reference lists Ctrl+S and Ctrl+O.
        const keys = Array.from(document.querySelectorAll('.te-chrome-shortcut')).map((r) => r.dataset.key);
        if (!keys.includes('ctrl+s') || !keys.includes('ctrl+o')) out.push('reference keys ' + keys.join(','));
        // Destroy: notice, input, dialogs and toolbar buttons go, the title
        // loses its dirty prefix and the keys no longer act.
        const title = document.title;
        document.title = 'Files test';
        teFiles.type(ed, ' dirty');
        if (document.title !== '\u2022 Files test') out.push('title not marked dirty: ' + document.title);
        teTest.control('markdown').click();
        if (!document.querySelector('.te-files-markdown')?.open) out.push('dialog not open before destroy');
        p.showNotice('A notice');
        ed.destroy();
        for (const sel of ['.te-files-notice', '.te-files-input', '.te-files-dialog', '.te-files-toolbar', '.te-files-toolbar-divider']) {
          if (document.querySelector(sel)) out.push('left behind: ' + sel);
        }
        if (document.title !== 'Files test') out.push('title after destroy ' + JSON.stringify(document.title));
        document.title = title;
        let saves = 0;
        const onSave = () => { saves += 1; };
        document.addEventListener('te:save', onSave);
        document.addEventListener('te:open', onSave);
        const sPrevented = teTest.key(document.body, 's', { ctrlKey: true });
        teTest.key(document.body, 'o', { ctrlKey: true });
        document.removeEventListener('te:save', onSave);
        document.removeEventListener('te:open', onSave);
        if (saves !== 0) out.push('destroyed editor still handles keys');
        if (sPrevented) out.push('destroyed editor still prevents Ctrl+S');
        if (p.save() !== null) out.push('save after destroy');
        // A file drop after destroy is not taken.
        const dt = new DataTransfer();
        dt.items.add(teFiles.file('x', 'x.md'));
        const r = teFiles.drag(document.body, dt);
        if (r.over || r.drop) out.push('destroyed editor still takes file drops');
        // Leave a normal editor for later tests.
        teFiles.resetToWelcome();
        teFiles.reinit();
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
