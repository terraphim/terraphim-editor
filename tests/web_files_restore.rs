//! Browser tests for restoring autosave drafts (issue #76): the Restore /
//! Discard notice after a simulated reload, drafts of files opened without
//! a handle (keyed by name, size and modification time), and identical or
//! broken drafts. Run with `wasm-pack test --headless --chrome`. Real
//! localStorage and the real exported document API; nothing is mocked (see
//! `tests/support/files.rs`). See `tests/web.rs` for why the browser tests
//! are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

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
        if (!notice.textContent.startsWith('Unsaved draft of this document from ')) out.push('notice text ' + JSON.stringify(notice.textContent));
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
        // A file opened without a handle keys its draft by name, size and
        // modification time: reopened unchanged, its newer draft is offered,
        // naming the file.
        const shown = () => teTest.visible(document.querySelector('.te-files-notice'));
        const size = new Blob([teFixtures.plain.md]).size;
        const past = Date.now() - 60000;
        ed.persistence.load(teFixtures.plain.md, 'dated.md', null, past, size);
        teFiles.type(ed, ' Draft words.');
        await teFiles.wait(120);
        const d = JSON.parse(localStorage.getItem(window.TE_DRAFT_PREFIX + 'file:dated.md|' + size + '|' + past) || 'null');
        if (!d) return out.concat('no draft for dated.md').join('; ');
        if (d.name !== 'dated.md') out.push('draft name ' + d.name);
        if (localStorage.getItem(window.TE_DRAFT_PREFIX + 'dated.md') !== null) out.push('draft keyed by the bare name');
        ed.persistence.load(teFixtures.plain.md, 'dated.md', null, past, size);
        if (!shown()) out.push('no offer for a draft newer than the file');
        else if (!document.querySelector('.te-files-notice').textContent.startsWith('Unsaved draft of dated.md from ')) out.push('notice does not name the file');
        // Same name, other size or time: another file, nothing offered.
        ed.persistence.load(teFixtures.plain.md, 'dated.md', null, past + 1, size);
        if (shown()) out.push('offered the draft of another dated.md (time)');
        ed.persistence.load(teFixtures.plain.md, 'dated.md', null, past, size + 1);
        if (shown()) out.push('offered the draft of another dated.md (size)');
        // A file modified after its draft was taken is not offered it.
        const future = Date.now() + 60000;
        ed.persistence.load(teFixtures.plain.md, 'later.md', null, future, size);
        teFiles.type(ed, ' Older draft.');
        await teFiles.wait(120);
        ed.persistence.load(teFixtures.plain.md, 'later.md', null, future, size);
        if (shown()) out.push('offered a draft older than the file');
        // The same text as the file is never offered.
        localStorage.setItem(window.TE_DRAFT_PREFIX + 'same.md', JSON.stringify({ v: 1, text: teFixtures.plain.md, savedAt: Date.now() }));
        ed.persistence.load(teFixtures.plain.md, 'same.md');
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
