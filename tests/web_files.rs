//! Browser tests for saving (issue #76): the download fallback, the file
//! name logic, Ctrl+S and cancelling te:save. Run with `wasm-pack test
//! --headless --chrome`. Real scripts, the real exported document API, real
//! `File`s; nothing is mocked (see `tests/support/files.rs` for how
//! downloads are observed). Elsewhere: writing back to a kept file handle,
//! cancelling te:open / te:markdown and teardown in `web_files_handles.rs`;
//! opening in `web_files_open.rs` and `web_files_replace.rs`; dirty state
//! in `web_files_drafts.rs`; drafts in `web_files_restore.rs` and
//! `web_files_identity.rs` (with the shortcut scope); overlapping saves and
//! opens in `web_files_race.rs`; the Markdown export dialog in
//! `web_files_markdown.rs`. See `tests/web.rs` for why the browser tests
//! are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_file_name_logic() {
    let _document = fresh_files_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const eq = (got, want, what) => { if (got !== want) out.push(what + ': ' + JSON.stringify(got)); };
          const n = window.teSuggestedFileName;
          eq(n('notes.md', 'x'), 'notes.md', 'name kept');
          eq(n('Draft.MARKDOWN', ''), 'Draft.MARKDOWN', 'markdown extension kept');
          eq(n('list.txt', ''), 'list.txt', 'txt kept');
          eq(n('chapter one', ''), 'chapter one.md', '.md added');
          eq(n('a/b\\c:d.md', ''), 'c-d.md', 'path stripped, colon replaced');
          eq(n('h:0badf00d', '# Why isn\'t it *obvious*?\n\ntext'), 'why-isn-t-it-obvious.md', 'hash key uses the heading');
          eq(n(undefined, 'intro\n\n## Café Society ##\n'), 'café-society.md', 'unicode heading, closing hashes');
          eq(n(null, 'no heading here'), 'untitled.md', 'untitled');
          eq(n('', '#not a heading'), 'untitled.md', 'hash without space is not a heading');
          eq(window.teExportFileName('notes.md'), 'notes-export.md', 'export name');
          eq(window.teExportFileName('list.txt'), 'list-export.md', 'export name from txt');
          const ok = window.teIsOpenableFile;
          if (!ok({ name: 'a.md' }) || !ok({ name: 'b.markdown' }) || !ok({ name: 'c.TXT' })) out.push('extensions');
          if (!ok({ name: 'noext', type: 'text/markdown' })) out.push('markdown mime');
          if (ok({ name: 'photo.png', type: 'image/png' })) out.push('png accepted');
          // The welcome document has no key: its heading names the file.
          const ed = teFiles.reinit();
          eq(ed.persistence.suggestedName(), 'welcome-to-markdown-editor.md', 'welcome name');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_save_downloads_the_serialised_document() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        let ed = teFiles.reinit();
        const canonical = teFiles.canonicalFull(ed);
        ed.persistence.load(canonical, 'notes.md');
        const cap = teFiles.captureDownloads();
        const events = [];
        const types = ['te:save', 'te:saved', 'te:written'];
        const rec = (e) => events.push(e.type + (e.detail.method ? ':' + e.detail.method : ''));
        types.forEach((t) => document.addEventListener(t, rec));
        try {
          // Write_On floppy.
          document.querySelector('.te-chrome-counter').click();
          teTest.control('save').click();
          await ed.persistence.lastWrite;
          if (cap.list.length !== 1) return 'downloads after floppy: ' + cap.list.length;
          const d = cap.list[0];
          if (d.name !== 'notes.md') out.push('name ' + d.name);
          if (!d.connected) out.push('anchor was not in the document');
          if (!d.href.startsWith('blob:')) out.push('href ' + d.href);
          if (events.join(',') !== 'te:save,te:saved,te:written:download') out.push('events ' + events.join(','));
          const text = await teFiles.read(d);
          if (text !== canonical) out.push('downloaded text differs from the saved document');
          if (text !== ed.saveDocument()) out.push('downloaded text differs from saveDocument()');
          if (!text.includes('```terraphim-alternatives')) out.push('annotation block missing');
          // Ctrl+S anywhere prevents the browser's Save page and saves.
          events.length = 0;
          const prevented = teTest.key(ed.surface.root, 's', { ctrlKey: true });
          if (!prevented) out.push('Ctrl+S not prevented');
          // Cmd+S too.
          if (!teTest.key(document.body, 's', { metaKey: true })) out.push('Cmd+S not prevented');
          await ed.persistence.lastWrite;
          if (cap.list.length !== 3) out.push('downloads after Ctrl+S, Cmd+S: ' + cap.list.length);
          // Ctrl+Shift+S and Ctrl+Alt+S are left alone.
          if (teTest.key(document.body, 's', { ctrlKey: true, shiftKey: true })) out.push('Ctrl+Shift+S taken');
          if (teTest.key(document.body, 's', { ctrlKey: true, altKey: true })) out.push('Ctrl+Alt+S taken');
          // The plain toolbar's Save button.
          document.querySelector('.te-chrome-counter').click();
          const tb = document.querySelector('.toolbar [data-te-file="save"]');
          if (!tb) return out.concat('no toolbar save button').join('; ');
          if (tb.textContent.trim() !== 'Save document') out.push('toolbar label ' + JSON.stringify(tb.textContent.trim()));
          if (!tb.querySelector('i.fa-floppy-disk')) out.push('toolbar icon');
          tb.click();
          await ed.persistence.lastWrite;
          if (cap.list.length !== 4) out.push('downloads after toolbar: ' + cap.list.length);
          // A host that stores documents itself cancels te:save: no
          // serialisation, no download.
          events.length = 0;
          const cancel = (e) => e.preventDefault();
          document.addEventListener('te:save', cancel);
          const r = ed.persistence.save();
          teTest.key(document.body, 's', { ctrlKey: true });
          document.removeEventListener('te:save', cancel);
          await ed.persistence.lastWrite;
          if (r !== null) out.push('cancelled save returned text');
          if (cap.list.length !== 4) out.push('cancelled save downloaded');
          if (events.join(',') !== 'te:save,te:save') out.push('cancelled events ' + events.join(','));
          // chrome.save() goes through the same flow and returns the text.
          document.querySelector('.te-chrome-counter').click();
          const t2 = ed.chrome.save();
          await ed.persistence.lastWrite;
          if (t2 !== canonical) out.push('chrome.save() text');
          if (cap.list.length !== 5) out.push('chrome.save() did not download');
          document.querySelector('.te-chrome-counter').click();
        } finally {
          types.forEach((t) => document.removeEventListener(t, rec));
          cap.stop();
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
