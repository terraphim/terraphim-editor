//! Browser test for the draft id of a first save-as when IndexedDB is
//! unavailable (issue #76, PR #78 review round 2): two save-as targets named
//! `notes.md` still get distinct `file:<name>|<size>|<lastModified>` draft
//! ids, kept for the session. Run with `wasm-pack test --headless --chrome`.
//! Real origin-private file handles and localStorage; the only shims are on
//! browser objects in the test page (IndexedDB hidden, the save picker
//! returning the chosen handle). See `tests/support/files.rs` and
//! `tests/web.rs`.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_save_as_without_indexeddb_keeps_same_name_drafts_apart() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const root = await navigator.storage.getDirectory();
        const one = await root.getDirectoryHandle('save-one76', { create: true });
        const two = await root.getDirectoryHandle('save-two76', { create: true });
        const h1 = await one.getFileHandle('notes.md', { create: true });
        const h2 = await two.getFileHandle('notes.md', { create: true });
        // Browser-object shims on the test page only: IndexedDB is hidden
        // (as when it is blocked or disabled), and the save picker returns
        // the origin-private handle the test chose (headless Chrome refuses
        // the real picker without a gesture). The editor's code is untouched.
        const picker = window.showSaveFilePicker;
        const idbOwn = Object.getOwnPropertyDescriptor(window, 'indexedDB');
        Object.defineProperty(window, 'indexedDB', { value: undefined, configurable: true, writable: true });
        let target = h1;
        window.showSaveFilePicker = async () => target;
        try {
          let ed = teFiles.reinit({ fileSystemAccess: undefined });
          teFiles.type(ed, ' The first notes.');
          ed.persistence.save();
          if (!(await ed.persistence.lastWrite)) return 'first save-as failed';
          await ed.persistence.identityReady;
          const id1 = ed.persistence.draftId;
          if (ed.persistence.handle !== h1) out.push('first handle not kept');
          if (!id1 || !id1.startsWith('file:notes.md|')) out.push('first id ' + id1);
          if (!localStorage.getItem(window.TE_DRAFT_PREFIX + id1)) out.push('no draft under the first id');
          // A later save keeps the id chosen at the first save-as.
          teFiles.type(ed, ' More.');
          ed.persistence.save();
          await ed.persistence.lastWrite;
          if (ed.persistence.draftId !== id1) out.push('id changed on a later save: ' + ed.persistence.draftId);
          // Another editor saves as the other notes.md.
          target = h2;
          ed = teFiles.reinit({ fileSystemAccess: undefined }, { keepDrafts: true });
          teFiles.type(ed, ' A second, longer set of notes.');
          ed.persistence.save();
          if (!(await ed.persistence.lastWrite)) return out.concat('second save-as failed').join('; ');
          await ed.persistence.identityReady;
          const id2 = ed.persistence.draftId;
          if (!id2 || !id2.startsWith('file:notes.md|')) out.push('second id ' + id2);
          if (id2 === id1) out.push('both notes.md share draft id ' + id1);
          if (!localStorage.getItem(window.TE_DRAFT_PREFIX + id1) || !localStorage.getItem(window.TE_DRAFT_PREFIX + id2)) out.push('a draft was lost');
          if (localStorage.getItem(window.TE_DRAFT_PREFIX + 'notes.md') !== null) out.push('draft keyed by the bare name');
        } finally {
          if (idbOwn) Object.defineProperty(window, 'indexedDB', idbOwn);
          else delete window.indexedDB;
          window.showSaveFilePicker = picker;
          await root.removeEntry('save-one76', { recursive: true }).catch(() => {});
          await root.removeEntry('save-two76', { recursive: true }).catch(() => {});
          teTest.resetDrafts();
        }
        if (typeof window.indexedDB !== 'object') out.push('IndexedDB not restored');
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
