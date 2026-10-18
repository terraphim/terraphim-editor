//! Retained real-file writes must not depend on global picker availability.
//! Uses browser-owned OPFS files; no filesystem, permission, or picker shims.
//! WIP: Chrome155 still hits the original6s bound in auto-write and exclusive-
//! lock admission. Preserve these failures; this file is not a qualification claim.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn retained_writable_file_is_updated_with_automatic_capability_detection() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const ed = teFiles.reinit({ fileSystemAccess: undefined });
        const root = await navigator.storage.getDirectory();
        const dir = await root.getDirectoryHandle('retained-capability-regression', { create: true });
        const handle = await dir.getFileHandle('actual-retained.md', { create: true });
        const initial = teFiles.canonicalFull(ed);
        const writer = await handle.createWritable();
        await writer.write(initial);
        await writer.close();
        // Observe/cancel unintended downloads to keep the test's effects inside
        // its real OPFS directory. This does not replace any file operation.
        const downloads = teFiles.captureDownloads();
        try {
          if (!(await ed.persistence.openFile(await handle.getFile(), handle))) return 'real file did not open';
          teFiles.type(ed, ' Actual retained-file edit.');
          const expected = ed.persistence.save();
          if (!(await ed.persistence.lastWrite)) return 'save failed';
          const actual = await (await handle.getFile()).text();
          console.log('TERRAPHIM_RETAINED_FILE_RESULT ' + JSON.stringify({policy:'automatic',expected,actual,downloads:downloads.list.length,fsAccess:ed.persistence.fsAccess,openPicker:typeof window.showOpenFilePicker,savePicker:typeof window.showSaveFilePicker}));
          if (actual !== expected) return 'real retained file was not updated';
          if (!actual.includes('Actual retained-file edit.')) return 'edit absent from real file';
          if (downloads.list.length !== 0) return 'download substituted for a retained-file write';
          if (ed.persistence.dirty) return 'successful real write remained dirty';
          return '';
        } finally {
          downloads.stop();
          await root.removeEntry('retained-capability-regression', { recursive: true });
        }
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn explicit_file_access_disable_preserves_the_retained_file() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const ed = teFiles.reinit({ fileSystemAccess: false });
        const root = await navigator.storage.getDirectory();
        const dir = await root.getDirectoryHandle('retained-policy-regression', { create: true });
        const handle = await dir.getFileHandle('policy-owned.md', { create: true });
        const initial = teFiles.canonicalFull(ed);
        const writer = await handle.createWritable();
        await writer.write(initial);
        await writer.close();
        const downloads = teFiles.captureDownloads();
        try {
          if (!(await ed.persistence.openFile(await handle.getFile(), handle))) return 'real file did not open';
          teFiles.type(ed, ' Explicit download-only edit.');
          const expected = ed.persistence.save();
          if (!(await ed.persistence.lastWrite)) return 'download-only save failed';
          const actual = await (await handle.getFile()).text();
          console.log('TERRAPHIM_RETAINED_FILE_RESULT ' + JSON.stringify({policy:'disabled',initial,expected,actual,downloads:downloads.list.length}));
          if (actual !== initial) return 'explicitly disabled file access wrote the retained file';
          if (downloads.list.length !== 1) return 'download-only policy did not request one copy';
          if (await teFiles.read(downloads.list[0]) !== expected) return 'download copy differs from serialized document';
          return '';
        } finally {
          downloads.stop();
          await root.removeEntry('retained-policy-regression', { recursive: true });
        }
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn actual_retained_file_write_failure_preserves_dirty_document() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const ed = teFiles.reinit({ fileSystemAccess: undefined });
        const root = await navigator.storage.getDirectory();
        const dir = await root.getDirectoryHandle('retained-locked-regression', { create: true });
        const handle = await dir.getFileHandle('actually-locked.md', { create: true });
        const initial = teFiles.canonicalFull(ed);
        const writer = await handle.createWritable();
        await writer.write(initial);
        await writer.close();
        const workerSource = `
          let lock = null;
          self.onmessage = async ({data}) => {
            try {
              if (data.kind === 'lock') {
                if (typeof data.handle.createSyncAccessHandle !== 'function') throw new Error('Real OPFS sync access handles unavailable');
                lock = await data.handle.createSyncAccessHandle();
                self.postMessage({kind:'locked',size:lock.getSize()});
              } else if (data.kind === 'release') {
                if (lock) { lock.close(); lock = null; }
                self.postMessage({kind:'released'});
              }
            } catch (error) { self.postMessage({kind:'error',name:error.name,message:error.message}); }
          };
        `;
        const workerURL = URL.createObjectURL(new Blob([workerSource], {type:'text/javascript'}));
        const worker = new Worker(workerURL);
        const downloads = teFiles.captureDownloads();
        const waiters = new Map();
        const request = (kind, payload = {}) => new Promise((resolve, reject) => {
          waiters.set(kind === 'lock' ? 'locked' : 'released', {resolve,reject});
          worker.postMessage({kind,...payload});
        });
        worker.onmessage = ({data}) => {
          if (data.kind === 'error') {
            for (const waiter of waiters.values()) waiter.reject(new Error(data.name+': '+data.message));
            waiters.clear();
          } else {
            const waiter=waiters.get(data.kind);
            if (waiter) { waiters.delete(data.kind);waiter.resolve(data); }
          }
        };
        worker.onerror = (error) => { for (const waiter of waiters.values()) waiter.reject(new Error(error.message));waiters.clear(); };
        try {
          if (!(await ed.persistence.openFile(await handle.getFile(), handle))) return 'real file did not open';
          teFiles.type(ed, ' Keep this unsaved edit.');
          const expectedBody = ed.surface.getText();
          const before = await (await handle.getFile()).text();
          if (before !== initial) return 'actual file changed before lock';
          const ready = await request('lock', {handle});
          console.log('TERRAPHIM_REAL_OPFS_LOCK '+JSON.stringify(ready));
          ed.persistence.save();
          const written = await ed.persistence.lastWrite;
          const dirtyAfterFailure = ed.persistence.dirty;
          const actualBody = ed.surface.getText();
          await request('release');
          const actual = await (await handle.getFile()).text();
          console.log('TERRAPHIM_RETAINED_FILE_RESULT '+JSON.stringify({policy:'real-exclusive-opfs-lock',written,dirty:dirtyAfterFailure,initial,actual,expectedBody,actualBody,downloads:downloads.list.length}));
          if (written) return 'exclusively locked real file reported a successful write';
          if (!dirtyAfterFailure) return 'failed real write cleared dirty state';
          if (actualBody !== expectedBody) return 'failed real write changed the document';
          if (actual !== initial) return 'failed real write changed the actual file';
          if (downloads.list.length !== 0) return 'write failure silently substituted a download';
          return '';
        } finally {
          // Shutdown only this fixture-owned worker. Termination releases any
          // lock even if the test was cancelled while awaiting an I/O result.
          worker.terminate();
          URL.revokeObjectURL(workerURL);
          downloads.stop();
          await root.removeEntry('retained-locked-regression', { recursive: true });
        }
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn automatic_save_without_a_handle_retains_real_download_payload() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const ed = teFiles.reinit({ fileSystemAccess: undefined });
        const downloads = teFiles.captureDownloads();
        try {
          teFiles.type(ed, ' Preserve the download fallback.');
          const expected = ed.persistence.save();
          if (!(await ed.persistence.lastWrite)) return 'download fallback failed';
          if (downloads.list.length !== 1) return 'expected one actual download request';
          const actual = await teFiles.read(downloads.list[0]);
          console.log('TERRAPHIM_RETAINED_FILE_RESULT ' + JSON.stringify({policy:'no-retained-handle',expected,actual,downloads:downloads.list.length}));
          if (actual !== expected) return 'download payload differs from serialized document';
          if (ed.persistence.handle) return 'download invented a retained file handle';
          return '';
        } finally { downloads.stop(); }
        "##,
    )
    .await;
    assert_eq!(result, "");
}
