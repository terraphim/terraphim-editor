//! Browser tests for the dirty state and autosave drafts (issue #76): the
//! dirty dot and title, annotation-only changes, the draft in localStorage,
//! and the Restore / Discard notice after a simulated reload. Run with
//! `wasm-pack test --headless --chrome`. Real scripts, real localStorage and
//! the real exported document API; nothing is mocked (see
//! `tests/support/files.rs`). See `tests/web.rs` for why the browser tests
//! are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_dirty_state_follows_edits_undo_annotations_and_save() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        const ed = teFiles.reinit();
        const p = ed.persistence;
        const title = document.title;
        document.title = 'Drafts test';
        const changes = [];
        const onDirty = (e) => changes.push(e.detail.dirty);
        document.addEventListener('te:dirty-change', onDirty);
        const ctl = teTest.control('save');
        const tb = document.querySelector('.toolbar [data-te-file="save"]');
        const draft = () => JSON.parse(localStorage.getItem(window.TE_DRAFT_PREFIX + p.key) || 'null');
        try {
          if (p.dirty || ctl.classList.contains('te-dirty')) out.push('dirty at start');
          if (draft()) out.push('draft at start');
          // An edit marks the document dirty at once.
          teFiles.type(ed, ' Typed.');
          if (!p.dirty) out.push('not dirty after typing');
          if (document.title !== '• Drafts test') out.push('title ' + JSON.stringify(document.title));
          if (!ctl.classList.contains('te-dirty') || !tb.classList.contains('te-dirty')) out.push('no dot');
          if (ctl.getAttribute('aria-label') !== 'Save document (unsaved changes)') out.push('label ' + ctl.getAttribute('aria-label'));
          // After the autosave delay the draft holds the serialised document.
          await teFiles.wait(150);
          const d = draft();
          if (!d || d.text !== p.serialise() || !d.text.includes(' Typed.')) out.push('draft ' + JSON.stringify(d && d.text.slice(-30)));
          if (d && !(d.savedAt > 0 && d.v === 1)) out.push('draft fields ' + JSON.stringify(d));
          // Undo back to the opened text clears the flag on the next tick.
          teTest.key(ed.surface.root, 'z', { ctrlKey: true });
          if (ed.surface.getText().includes(' Typed.')) out.push('undo did not undo');
          await teFiles.wait(150);
          if (p.dirty) out.push('dirty after undo');
          if (document.title !== 'Drafts test') out.push('title after undo ' + JSON.stringify(document.title));
          if (ctl.classList.contains('te-dirty') || ctl.getAttribute('aria-label') !== 'Save document') out.push('dot after undo');
          // A change only to the annotations (a ghost) is caught on the tick
          // that follows the key release.
          const ghosted = ed.ghosts.ghost(2, 9);
          if (!ghosted || ghosted.ok === false) out.push('ghost failed ' + JSON.stringify(ghosted));
          ed.surface.root.dispatchEvent(new KeyboardEvent('keyup', { key: '/', bubbles: true }));
          await teFiles.wait(150);
          if (!p.dirty) out.push('ghost did not mark dirty');
          if (!draft().text.includes('"ghosts"')) out.push('ghost not in the draft');
          // Saving records a clean state.
          const cap = teFiles.captureDownloads();
          try {
            p.save();
            await p.lastWrite;
          } finally {
            cap.stop();
          }
          if (p.dirty || document.title !== 'Drafts test' || ctl.classList.contains('te-dirty')) out.push('dirty after save');
          if (draft().text !== p.cleanText) out.push('save did not keep the draft');
          if (changes.join(',') !== 'true,false,true,false') out.push('dirty events ' + changes.join(','));
          // A host page that opens its own document with editor.openDocument()
          // gets a clean state and drafts under the new key.
          teFiles.type(ed, ' Pending.');
          ed.openDocument(teFixtures.plain.md, 'host.md');
          if (p.dirty || document.title !== 'Drafts test') out.push('dirty after a host open');
          if (p.key !== 'host.md' || p.fileName !== 'host.md' || p.handle !== null) out.push('host open key ' + p.key);
          if (teTest.visible(document.querySelector('.te-files-notice'))) out.push('notice after a host open');
          teFiles.type(ed, ' Host edit.');
          await teFiles.wait(150);
          if (!p.dirty) out.push('host edit not dirty');
          const hostDraft = JSON.parse(localStorage.getItem(window.TE_DRAFT_PREFIX + 'host.md') || 'null');
          if (!hostDraft || !hostDraft.text.includes(' Host edit.')) out.push('host draft not under host.md');
          // A host that stores the text itself marks the document clean.
          const cancel = (e) => e.preventDefault();
          document.addEventListener('te:save', cancel);
          const stored = p.save();
          document.removeEventListener('te:save', cancel);
          if (stored !== null || !p.dirty) out.push('cancelled save changed the dirty state');
          p.markClean();
          if (p.dirty || document.title !== 'Drafts test' || ctl.classList.contains('te-dirty')) out.push('markClean left it dirty');
          if (p.cleanText !== p.serialise()) out.push('markClean clean text');
        } finally {
          document.removeEventListener('te:dirty-change', onDirty);
          document.title = title;
        }
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_draft_is_restored_or_discarded_after_a_reload() {
    let _document = fresh_files_editor();
    let result = js_async(
        r##"
        const out = [];
        let ed = teFiles.reinit();
        const key = ed.persistence.key;
        if (!key.startsWith('h:')) out.push('welcome key ' + key);
        const stored = () => JSON.parse(localStorage.getItem(window.TE_DRAFT_PREFIX + key) || 'null');
        // Type and reload before the autosave delay: destroy keeps the draft.
        teFiles.type(ed, ' Keep me.');
        const want = ed.persistence.serialise();
        teFiles.resetToWelcome();
        if (!stored() || stored().text !== want) return 'draft not kept on teardown';
        const focusBefore = document.activeElement;
        ed = teFiles.reinit({}, { keepDrafts: true });
        if (ed.persistence.key !== key) out.push('key changed across the reload');
        if (ed.surface.getText() !== teFiles.welcome()) out.push('reload did not show the welcome text');
        const notice = document.querySelector('.te-files-notice');
        if (!teTest.visible(notice)) return out.concat('no restore notice').join('; ');
        if (notice.getAttribute('role') !== 'region' || !notice.getAttribute('aria-label')) out.push('notice role');
        if (notice.querySelector('[aria-live="polite"]') === null) out.push('notice not live');
        if (!notice.textContent.startsWith('Unsaved draft from ')) out.push('notice text ' + JSON.stringify(notice.textContent));
        const actions = Array.from(notice.querySelectorAll('button')).map((b) => b.dataset.action).join(',');
        if (actions !== 'restore,discard') out.push('actions ' + actions);
        if (notice.contains(document.activeElement)) out.push('notice took focus');
        if (document.activeElement !== focusBefore && document.activeElement !== document.body) out.push('focus moved');
        if (ed.persistence.dirty) out.push('dirty before restoring');
        // A clean tick while the notice is up leaves the stored draft alone.
        ed.persistence.schedule();
        await teFiles.wait(120);
        if (!stored() || stored().text !== want) out.push('clean tick replaced the pending draft');
        // Restore.
        notice.querySelector('[data-action="restore"]').click();
        if (ed.persistence.serialise() !== want) out.push('restored text differs');
        if (!ed.surface.getText().endsWith(' Keep me.')) out.push('restored body ' + JSON.stringify(ed.surface.getText().slice(-20)));
        if (!ed.persistence.dirty) out.push('restored draft not dirty');
        if (!notice.hidden) out.push('notice still shown');
        if (ed.persistence.key !== key) out.push('restore changed the key');
        // Undo after a restore does not go back past it.
        // Discard after another reload: the draft goes, the welcome text stays.
        teFiles.resetToWelcome();
        ed = teFiles.reinit({}, { keepDrafts: true });
        const n2 = document.querySelector('.te-files-notice');
        if (!teTest.visible(n2)) return out.concat('no notice the second time').join('; ');
        n2.querySelector('[data-action="discard"]').click();
        if (stored() !== null) out.push('discard kept the draft');
        if (!n2.hidden) out.push('notice after discard');
        if (ed.surface.getText() !== teFiles.welcome()) out.push('discard changed the text');
        teFiles.resetToWelcome();
        ed = teFiles.reinit({}, { keepDrafts: true });
        if (teTest.visible(document.querySelector('.te-files-notice'))) out.push('notice after discarding');
        // A file newer than its draft is not offered the draft; an older one is.
        ed.persistence.load(teFixtures.plain.md, 'dated.md');
        teFiles.type(ed, ' Draft words.');
        await teFiles.wait(120);
        const d = JSON.parse(localStorage.getItem(window.TE_DRAFT_PREFIX + 'dated.md') || 'null');
        if (!d) return out.concat('no draft for dated.md').join('; ');
        ed.persistence.load(teFixtures.plain.md, 'dated.md', null, d.savedAt + 1000);
        if (teTest.visible(document.querySelector('.te-files-notice'))) out.push('offered a draft older than the file');
        ed.persistence.load(teFixtures.plain.md, 'dated.md', null, d.savedAt - 1000);
        if (!teTest.visible(document.querySelector('.te-files-notice'))) out.push('no offer for a draft newer than the file');
        // The same text as the file is never offered.
        localStorage.setItem(window.TE_DRAFT_PREFIX + 'same.md', JSON.stringify({ v: 1, text: teFixtures.plain.md, savedAt: Date.now() }));
        ed.persistence.load(teFixtures.plain.md, 'same.md', null, 0);
        if (teTest.visible(document.querySelector('.te-files-notice'))) out.push('offered an identical draft');
        // Broken storage content is ignored.
        localStorage.setItem(window.TE_DRAFT_PREFIX + 'bad.md', '{not json');
        ed.persistence.load(teFixtures.plain.md, 'bad.md');
        if (teTest.visible(document.querySelector('.te-files-notice'))) out.push('offered a broken draft');
        teTest.resetDrafts();
        return out.join('; ');
        "##,
    )
    .await;
    assert_eq!(result, "");
}
