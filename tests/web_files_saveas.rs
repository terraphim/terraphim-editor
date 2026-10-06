//! Browser test for the draft id of a first save-as when IndexedDB is
//! unavailable (issue #76, PR #78 review round 2): two save-as targets named
//! `notes.md` still get distinct `file:<name>|<size>|<lastModified>` draft
//! ids, kept for the session. Run with `wasm-pack test --headless --chrome`.
//! Real origin-private file handles and localStorage; the only shims are on
//! browser objects in the test page (IndexedDB hidden, the save picker
//! returning the chosen handle). Also the write-permission decisions for a
//! kept handle (a test-page handle shim answers the permission questions).
//! See `tests/support/files.rs` and
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

#[wasm_bindgen_test]
async fn test_a_kept_handle_without_write_permission_never_loses_the_text() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit({ fileSystemAccess: undefined });
        const p = ed.persistence;
        const root = await navigator.storage.getDirectory();
        const dir = await root.getDirectoryHandle('perm76', { create: true });
        // Browser-object shims on the test page only: a handle that answers
        // the permission questions as the test says (a real origin-private
        // handle is always granted) and delegates everything else to the
        // real handle; and a save picker whose behaviour the test sets.
        const answers = { query: 'prompt', request: 'granted' };
        const calls = [];
        const real = await teFiles.opfsFile(dir, 'kept.md', 'Original.\n');
        const kept = {
          kind: 'file', name: 'kept.md', real,
          getFile: () => real.getFile(),
          createWritable: () => real.createWritable(),
          isSameEntry: (o) => real.isSameEntry(o && o.real ? o.real : o),
          queryPermission: async (o) => { calls.push('query:' + o.mode); return answers.query; },
          requestPermission: async (o) => { calls.push('request:' + o.mode); return answers.request; },
        };
        const picker = window.showSaveFilePicker;
        let pickerDoes = async () => { throw new DOMException('not expected', 'SecurityError'); };
        window.showSaveFilePicker = () => pickerDoes();
        const cap = teFiles.captureDownloads();
        const written = [];
        const onWritten = (e) => written.push(e.detail.method + (e.detail.copy ? ':copy' : ''));
        document.addEventListener('te:written', onWritten);
        const fileText = async (h) => (await h.getFile()).text();
        const notice = () => document.querySelector('.te-files-notice').textContent;
        try {
          if (!(await p.openFile(await real.getFile(), kept))) return 'open failed';
          // Prompt, then granted: written to the file, clean.
          teFiles.type(ed, ' Granted.');
          const t1 = p.save();
          if (!(await p.lastWrite)) out.push('granted save failed');
          if ((await fileText(real)) !== t1) out.push('granted save not in the file');
          if (calls.join(',') !== 'query:readwrite,request:readwrite') out.push('calls ' + calls.join(','));
          if (p.dirty) out.push('dirty after a granted save');
          // Denied, and the save-as picker is cancelled: nothing written.
          answers.query = 'denied';
          answers.request = 'denied';
          pickerDoes = async () => { throw new DOMException('cancelled', 'AbortError'); };
          teFiles.type(ed, ' Denied.');
          p.save();
          if (await p.lastWrite) out.push('cancelled fallback reported a write');
          if (!p.dirty) out.push('marked clean though nothing was written');
          if ((await fileText(real)) !== t1) out.push('denied file changed');
          if (!notice().includes('No permission to write kept.md: nothing was saved')) out.push('notice ' + notice());
          if (cap.list.length !== 0) out.push('downloaded after a cancelled picker');
          // Denied, and the picker is refused too: a copy is downloaded,
          // the document stays dirty and keeps its handle.
          pickerDoes = async () => { throw new DOMException('no gesture', 'SecurityError'); };
          const t3 = p.save();
          if (!(await p.lastWrite)) out.push('download fallback failed');
          if (cap.list.length !== 1) out.push('downloads ' + cap.list.length);
          else if ((await teFiles.read(cap.list[0])) !== t3) out.push('downloaded copy differs');
          if (!p.dirty) out.push('marked clean after only a copy was downloaded');
          if (p.handle !== kept) out.push('handle dropped after the copy');
          if ((await fileText(real)) !== t1) out.push('file changed by the copy');
          if (!notice().includes('downloaded a copy')) out.push('copy notice ' + notice());
          if (written[written.length - 1] !== 'download:copy') out.push('written ' + written.join(','));
          // Denied, and the user picks another file: saved there, which
          // becomes the document and is clean.
          const other = await dir.getFileHandle('other.md', { create: true });
          pickerDoes = async () => other;
          const t4 = p.save();
          if (!(await p.lastWrite)) out.push('save-as fallback failed');
          if ((await fileText(other)) !== t4) out.push('save-as file differs');
          if (p.handle !== other || p.fileName !== 'other.md') out.push('document not moved to other.md');
          if (p.dirty) out.push('dirty after saving as another file');
          if (!notice().includes('saved as other.md instead')) out.push('save-as notice ' + notice());
          if ((await fileText(real)) !== t1) out.push('kept.md changed');
        } finally {
          window.showSaveFilePicker = picker;
          document.removeEventListener('te:written', onWritten);
          cap.stop();
          await root.removeEntry('perm76', { recursive: true }).catch(() => {});
          teTest.resetDrafts();
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
