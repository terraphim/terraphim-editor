//! Browser tests for saves and opens that overlap (issue #76, PR #78
//! review): a save to file A still writing when file B is opened must not
//! hand A's handle, key or clean state back to the editor, and a slower
//! earlier open must not replace a later one. Run with `wasm-pack test
//! --headless --chrome`. Real origin-private file handles and real `File`s;
//! the only shims are browser-API ones that let the test choose when a
//! write or a read completes (`teFiles.slowHandle`, a `File` subclass whose
//! `text()` waits for a gate). The editor's code is not replaced. See
//! `tests/support/files.rs` and `tests/web.rs`.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_a_late_save_to_a_never_takes_over_b() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit({ fileSystemAccess: undefined });
        const p = ed.persistence;
        const root = await navigator.storage.getDirectory();
        const dir = await root.getDirectoryHandle('race76', { create: true });
        const written = [];
        const onWritten = (e) => written.push(e.detail.name + (e.detail.stale ? ':stale' : ''));
        document.addEventListener('te:written', onWritten);
        try {
          const realA = await teFiles.opfsFile(dir, 'a.md', 'File A.\n');
          const realB = await teFiles.opfsFile(dir, 'b.md', 'File B.\n');
          const gate = teFiles.gate();
          const slowA = teFiles.slowHandle(realA, gate);
          if (!(await p.openFile(await realA.getFile(), slowA))) return 'open A failed';
          teFiles.type(ed, ' Edited A.');
          // Save A: the write waits on the gate.
          const savedA = p.save();
          const writeA = p.lastWrite;
          // Open B meanwhile (A is still dirty, so discard).
          const openB = p.openFile(await realB.getFile(), realB);
          let d = null;
          for (let i = 0; i < 50 && !(d = document.querySelector('.te-files-replace')); i += 1) await teFiles.wait(10);
          if (!d) return 'no replace question';
          d.querySelector('[data-action="discard"]').click();
          if (!(await openB)) return 'open B failed';
          // Now A's write completes.
          gate.open();
          if (!(await writeA)) out.push('write to A reported failure');
          if ((await (await realA.getFile()).text()) !== savedA) out.push('A does not hold its own save');
          if (p.handle !== realB) out.push('handle went back to A');
          if (p.fileName !== 'b.md' || ed.documentKey !== 'b.md' || p.key !== 'b.md') out.push('key ' + p.fileName + ' ' + ed.documentKey + ' ' + p.key);
          if (p.dirty) out.push('B dirty after A completed');
          if (!ed.surface.getText().startsWith('File B.')) out.push('surface ' + JSON.stringify(ed.surface.getText()));
          if (written.join(',') !== 'a.md:stale') out.push('written ' + written.join(','));
          // Saving B writes B's file and leaves A alone.
          teFiles.type(ed, ' Edited B.');
          const savedB = p.save();
          if (!(await p.lastWrite)) out.push('save B failed');
          if ((await (await realB.getFile()).text()) !== savedB) out.push('B file does not hold B');
          if ((await (await realA.getFile()).text()) !== savedA) out.push('saving B touched A');
        } finally {
          document.removeEventListener('te:written', onWritten);
          await root.removeEntry('race76', { recursive: true }).catch(() => {});
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_a_slow_earlier_open_never_replaces_a_later_one() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const p = ed.persistence;
        const slowFile = (text, name, gate) => {
          class SlowFile extends File {
            text() { return gate.promise.then(() => super.text()); }
          }
          return new SlowFile([text], name, { type: 'text/markdown' });
        };
        // A slow read, then a fast one: the fast one wins.
        let gate = teFiles.gate();
        const slow = p.openFile(slowFile('Slow A.\n', 'slow.md', gate));
        if (!(await p.openFile(teFiles.file('Fast B.\n', 'fast.md')))) return 'fast open failed';
        gate.open();
        if (await slow) out.push('the slow open loaded');
        if (ed.surface.getText() !== 'Fast B.\n' || ed.documentKey !== 'fast.md') out.push('after slow open ' + ed.documentKey);
        // A host's openDocument during a slow open wins too.
        gate = teFiles.gate();
        const slow2 = p.openFile(slowFile('Slow C.\n', 'slow2.md', gate));
        ed.openDocument('Host text.\n', 'host.md');
        gate.open();
        if (await slow2) out.push('slow open replaced the host document');
        if (ed.surface.getText() !== 'Host text.\n' || p.key !== 'host.md') out.push('after host open ' + p.key);
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
