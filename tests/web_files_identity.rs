//! Browser tests for draft identity and shortcut scope (issue #76, PR #78
//! review): two files with the same name keep separate drafts, and in an
//! embedding page Ctrl/Cmd+S and Ctrl/Cmd+O act only with focus in the
//! editor. Run with `wasm-pack test --headless --chrome`. Real
//! origin-private file handles, real IndexedDB and localStorage, real key
//! events; nothing is mocked (see `tests/support/files.rs`). See
//! `tests/web.rs` for why the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_two_files_named_alike_keep_separate_drafts() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit({ fileSystemAccess: undefined });
        const p = ed.persistence;
        const root = await navigator.storage.getDirectory();
        const one = await root.getDirectoryHandle('one76', { create: true });
        const two = await root.getDirectoryHandle('two76', { create: true });
        const shown = () => teTest.visible(document.querySelector('.te-files-notice'));
        try {
          const h1 = await teFiles.opfsFile(one, 'notes.md', 'First notes.\n');
          const h2 = await teFiles.opfsFile(two, 'notes.md', 'Second notes.\n');
          if (!(await p.openFile(await h1.getFile(), h1))) return 'open one failed';
          await p.identityReady;
          const id1 = p.draftId;
          if (!id1 || !id1.startsWith('fs:')) return 'draft id ' + id1;
          teFiles.type(ed, ' Unsaved one.');
          await teFiles.wait(120);
          const d1 = JSON.parse(localStorage.getItem(window.TE_DRAFT_PREFIX + id1) || 'null');
          if (!d1 || !d1.text.includes('Unsaved one.')) out.push('no draft for the first notes.md');
          if (localStorage.getItem(window.TE_DRAFT_PREFIX + 'notes.md') !== null) out.push('draft keyed by the bare name');
          // The other notes.md: its own id, no offer of the first one's draft.
          const open2 = p.openFile(await h2.getFile(), h2);
          let d = null;
          for (let i = 0; i < 50 && !(d = document.querySelector('.te-files-replace')); i += 1) await teFiles.wait(10);
          if (!d) return out.concat('no replace question').join('; ');
          d.querySelector('[data-action="discard"]').click();
          if (!(await open2)) return out.concat('open two failed').join('; ');
          await p.identityReady;
          if (!p.draftId || p.draftId === id1) out.push('same draft id for two files: ' + p.draftId);
          if (shown()) out.push('offered the first file\'s draft for the second');
          // The first file again, through a new handle to the same entry:
          // the same id, and its draft is offered, naming the file.
          const again = await one.getFileHandle('notes.md');
          if (!(await p.openFile(await again.getFile(), again))) return out.concat('reopen failed').join('; ');
          await p.identityReady;
          if (p.draftId !== id1) out.push('reopened id ' + p.draftId);
          if (!shown()) out.push('no offer on reopening the first file');
          else {
            const notice = document.querySelector('.te-files-notice');
            if (!notice.textContent.startsWith('Unsaved draft of notes.md from ')) out.push('notice ' + notice.textContent);
            notice.querySelector('[data-action="restore"]').click();
            if (!ed.surface.getText().includes('Unsaved one.')) out.push('restore lost the text');
          }
        } finally {
          await root.removeEntry('one76', { recursive: true }).catch(() => {});
          await root.removeEntry('two76', { recursive: true }).catch(() => {});
          teTest.resetDrafts();
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_embedded_shortcuts_act_only_with_focus_in_the_editor() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        let ed = teFiles.reinit({ standalone: false });
        let events = 0;
        const count = () => { events += 1; };
        document.addEventListener('te:save', count);
        document.addEventListener('te:open', count);
        const host = document.createElement('input');
        document.body.appendChild(host);
        const cap = teFiles.captureDownloads();
        try {
          // Focus in a host page input: the host's shortcuts are left alone.
          host.focus();
          if (teTest.key(host, 's', { ctrlKey: true })) out.push('took Ctrl+S from a host input');
          if (teTest.key(host, 'o', { metaKey: true })) out.push('took Cmd+O from a host input');
          if (teTest.key(document.body, 's', { ctrlKey: true })) out.push('took Ctrl+S on the bare host page');
          if (events !== 0) out.push('events from host focus ' + events);
          // Focus in the surface: handled.
          ed.surface.focus();
          if (!teTest.key(ed.surface.root, 's', { ctrlKey: true })) out.push('Ctrl+S in the surface not handled');
          await ed.persistence.lastWrite;
          if (events !== 1 || cap.list.length !== 1) out.push('surface save ' + events + ' ' + cap.list.length);
          // In the Write_On chrome and the export dialog: handled.
          document.querySelector('.te-chrome-counter').click();
          const ctl = teTest.control('save');
          if (!teTest.key(ctl, 's', { metaKey: true })) out.push('Cmd+S on the chrome not handled');
          teTest.control('markdown').click();
          const area = document.querySelector('.te-files-markdown textarea');
          if (!teTest.key(area, 's', { ctrlKey: true })) out.push('Ctrl+S in the export dialog not handled');
          document.querySelector('.te-files-markdown').close();
          await teFiles.wait(30);
          document.querySelector('.te-chrome-counter').click();
          // The standalone page (index.html) takes them anywhere.
          ed = teFiles.reinit({ standalone: true });
          host.focus();
          if (!teTest.key(host, 's', { ctrlKey: true })) out.push('standalone page did not take Ctrl+S');
        } finally {
          document.removeEventListener('te:save', count);
          document.removeEventListener('te:open', count);
          host.remove();
          cap.stop();
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
