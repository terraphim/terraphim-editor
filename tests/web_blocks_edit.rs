//! Browser tests for the Blocks view (issue #19): structural edits, Write_On
//! interplay, drafts kept across outside changes and Escape, and keyboard
//! access to every block action. Run with `wasm-pack test --headless --chrome`. Real scripts,
//! the real exported document API, real fixtures and real editing commands
//! (`document.execCommand`); nothing is mocked. Split from `web_blocks.rs` to
//! keep each binary well inside its 20 s budget; every scenario runs as short
//! steps with a yield between them (`support::run_steps`).
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_insert_delete_and_write_on_mode() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_block_fixtures();
    let result = run_steps(&[
        r##"
          s.setText('# A\n\npara one\n\npara two\n');
          bv.setView('blocks');
          const expect = (name, want) => {
            if (s.getText() !== want) out.push(name + ' ' + JSON.stringify(s.getText()));
            if (BlocksView.serialise(bv.model) !== s.getText()) out.push(name + ': stale blocks');
          };
          let ta = bv.insertBlockAfter(0);
          ta.value = 'new';
          bv.commitEdit();
          expect('insert', '# A\n\nnew\n\npara one\n\npara two\n');
          teTest.key(bv.cards()[3], 'Delete');
          expect('delete last', '# A\n\nnew\n\npara one\n');
          if (document.activeElement !== bv.cards()[2]) out.push('focus after delete');
          bv.deleteBlock(0);
          expect('delete first', 'new\n\npara one\n');
          ta = bv.insertBlockAfter(1);
          ta.value = 'end';
          bv.commitEdit();
          expect('append', 'new\n\npara one\n\nend\n');
          s.undo();
          expect('one undo step per operation', 'new\n\npara one\n');
          ta = bv.insertBlockAfter(0);
          ta.value = '   ';
          bv.commitEdit();
          expect('blank insert ignored', 'new\n\npara one\n');
          if (bv.list.querySelector('[data-action="up"], [data-action="down"]')) out.push('move controls present');
        "##,
        // A new block between annotated blocks shifts every later anchor and
        // sets nothing aside.
        r##"
          ed.openDocument(teBlockFixtures.full);
          T.before = ed.annotations();
        "##,
        r##"
          const ta = bv.insertBlockAfter(0);
          ta.value = 'Inserted.';
          bv.commitEdit();
        "##,
        r##"
          const before = T.before;
          const after = ed.annotations();
          const d = 'Inserted.\n\n'.length;
          // Anchor context (#36) legitimately changes next to the insertion;
          // positions, texts and everything else must match exactly.
          const plain = (a) => { const { before, after, ...anchor } = a.anchor; return { ...a, anchor }; };
          const moved = (a) => { const p = plain(a); return p.anchor.start >= 31 ? { ...p, anchor: { ...p.anchor, start: p.anchor.start + d, end: p.anchor.end + d } } : p; };
          if (JSON.stringify([after.spans.map(plain), after.ghosts.map(plain)]) !== JSON.stringify([before.spans.map(moved), before.ghosts.map(moved)])) out.push('anchors after insert ' + JSON.stringify(after.ghosts));
          if (ed.setAsideCount !== 0) out.push('set aside after insert');
        "##,
        // Write_On: Blocks is not shown; it resumes in plain mode.
        r##"
          const chrome = ed.chrome;
          chrome.toggle();
          if (document.body.dataset.mode !== 'write-on') out.push('not in write-on');
          if (bv.view !== 'text') out.push('blocks still active in write-on');
          if (!teTest.visible(s.root)) out.push('surface hidden in write-on');
          if (teTest.visible(bv.container) || teTest.visible(document.querySelector('.te-view-toggle'))) out.push('blocks UI visible in write-on');
          bv.setView('blocks');
          if (bv.view !== 'text') out.push('blocks allowed in write-on');
          chrome.toggle();
          if (bv.view !== 'blocks') out.push('blocks not resumed in plain mode');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_open_block_draft_survives_outside_changes() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_block_fixtures();
    let result = run_steps(&[
        // The user opens a block and selects part of the draft. The next step
        // runs in a later task, after the selectionchange is delivered, as it
        // would be for a real user.
        r##"
          s.setText('# A\n\npara one\n\npara two\n');
          bv.setView('blocks');
          const ta = bv.editBlock(2);
          ta.value = 'para two edited';
          ta.setSelectionRange(5, 8);
          if (ta !== document.activeElement) return 'editor not focused';
        "##,
        // 1. Edits outside the block keep the editor, its draft, focus and
        //    selection; undo from elsewhere maps too.
        r##"
          const notice = bv.draftsElement;
          const ta = bv.editing.textarea;
          s.replaceRange(0, 0, 'Intro.\n\n');
          if (!bv.editing || bv.editing.textarea !== ta || !ta.isConnected) return 'editor closed by an edit before the block';
          if (ta.value !== 'para two edited' || document.activeElement !== ta) out.push('draft or focus lost');
          if (ta.selectionStart !== 5 || ta.selectionEnd !== 8) out.push('textarea selection lost');
          if (bv.editing.start !== 23) out.push('start not shifted: ' + bv.editing.start);
          if (bv.cards().length !== 4 || bv.cards()[3] !== bv.editing.card) out.push('cards not rebuilt around the editor');
          s.undo();
          if (!bv.editing || bv.editing.start !== 15) out.push('undo elsewhere: ' + (bv.editing && bv.editing.start));
          const end = s.getText().length;
          s.replaceRange(end, end, '\nTail.\n');
          if (!bv.editing) out.push('editor closed by an edit after the block');
          if (!notice.hidden) out.push('notice shown without a conflict');
          bv.commitEdit();
          if (s.getText() !== '# A\n\npara one\n\npara two edited\n\nTail.\n') out.push('commit after outside edits ' + JSON.stringify(s.getText()));
        "##,
        // 2. An overlapping edit that rewrote the block keeps the draft in the
        //    notice; Apply inserts it as a new paragraph and leaves the
        //    rewritten block intact. Undo is one step per change.
        r##"
          const notice = bv.draftsElement;
          const ta = bv.editBlock(1);
          ta.value = 'para ONE draft';
          s.replaceRange(5, 9, 'PARA');
          if (bv.editing || bv.list.querySelector('textarea')) out.push('editor still open after overlap');
          const kept = bv.keptDrafts();
          if (kept.length !== 1 || kept[0].value !== 'para ONE draft' || kept[0].originalText !== 'para one' || kept[0].reason !== 'changed') out.push('draft not kept ' + JSON.stringify(kept));
          if (!teTest.visible(notice) || !notice.textContent.includes('para ONE draft')) out.push('notice not visible with the draft');
          if (s.getText() !== '# A\n\nPARA one\n\npara two edited\n\nTail.\n') out.push('outside edit not applied');
          const apply = notice.querySelector('[data-draft-action="apply"]');
          if (apply.textContent !== 'Insert as new paragraph' || apply.dataset.draftMode !== 'insert') out.push('label for a rewritten block: ' + apply.textContent);
          apply.click();
          if (s.getText() !== '# A\n\nPARA one\n\npara ONE draft\n\npara two edited\n\nTail.\n') out.push('apply insert ' + JSON.stringify(s.getText()));
          if (!notice.hidden || bv.keptDrafts().length) out.push('notice not cleared after apply');
          if (window.wasmBindings.document_body() !== s.getText()) out.push('model body differs after apply');
          s.undo();
          if (s.getText() !== '# A\n\nPARA one\n\npara two edited\n\nTail.\n') out.push('undo apply ' + JSON.stringify(s.getText()));
          s.undo();
          if (s.getText() !== '# A\n\npara one\n\npara two edited\n\nTail.\n') out.push('undo outside edit ' + JSON.stringify(s.getText()));
          if (bv.cards().length !== 4) out.push('cards after undo');
        "##,
        // 3. When undo restores the block's original text the label turns to
        //    "Replace block" and Apply replaces it.
        r##"
          const notice = bv.draftsElement;
          const ta = bv.editBlock(1);
          ta.value = 'para ONE draft';
          s.replaceRange(5, 9, 'PARA');
          const apply = () => notice.querySelector('[data-draft-action="apply"]');
          if (apply().textContent !== 'Insert as new paragraph') out.push('label before undo: ' + apply().textContent);
          s.undo();
          if (s.getText() !== '# A\n\npara one\n\npara two edited\n\nTail.\n') out.push('undo ' + JSON.stringify(s.getText()));
          if (apply().textContent !== 'Replace block' || apply().dataset.draftMode !== 'replace') out.push('label after undo: ' + apply().textContent);
          if (bv.applyDraft(bv.keptDrafts()[0].id) !== 'replace') out.push('apply did not replace');
          if (s.getText() !== '# A\n\npara ONE draft\n\npara two edited\n\nTail.\n') out.push('apply replace ' + JSON.stringify(s.getText()));
          s.undo();
          if (s.getText() !== '# A\n\npara one\n\npara two edited\n\nTail.\n') out.push('undo replace ' + JSON.stringify(s.getText()));
        "##,
        // 4. openDocument with different content during an edit keeps the
        //    draft; Apply inserts it at the block index without touching the
        //    opened text or its annotations; Discard drops another draft.
        r##"
          const notice = bv.draftsElement;
          let ta = bv.editBlock(0);
          ta.value = '# Draft heading';
          ed.openDocument(teBlockFixtures.full);
          T.body = s.getText();
        "##,
        r##"
          const notice = bv.draftsElement;
          const body = T.body;
          const k2 = bv.keptDrafts();
          if (k2.length !== 1 || k2[0].reason !== 'replaced' || k2[0].value !== '# Draft heading' || k2[0].anchor !== null) out.push('open: draft ' + JSON.stringify(k2));
          if (!notice.textContent.includes('replaced')) out.push('open: notice text');
          const apply = notice.querySelector('[data-draft-action="apply"]');
          if (apply.textContent !== 'Insert as new paragraph') out.push('open: label ' + apply.textContent);
          apply.click();
          if (s.getText() !== '# Draft heading\n\n' + body) out.push('open: apply ' + JSON.stringify(s.getText().slice(0, 60)));
        "##,
        r##"
          const ann = ed.annotations();
          if (ann.spans.length !== 3 || ann.ghosts.length !== 2 || ed.setAsideCount !== 0) out.push('open: annotations after apply');
        "##,
        r##"
          const ta = bv.editBlock(1);
          ta.value = 'Throwaway draft';
          ed.openDocument(teBlockFixtures.full);
        "##,
        r##"
          T.saved = ed.saveDocument();
        "##,
        r##"
          const notice = bv.draftsElement;
          notice.querySelector('[data-draft-action="discard"]').click();
          if (bv.keptDrafts().length || !notice.hidden) out.push('discard');
          if (ed.saveDocument() !== T.saved) out.push('discard changed the document');
        "##,
        // 5. A kept draft follows typing (execCommand) in the text view and
        //    applies there (after the rewritten block).
        r##"
          const notice = bv.draftsElement;
          s.setText('# A\n\npara one\n');
          const ta = bv.editBlock(1);
          ta.value = 'para 1';
          s.replaceRange(5, 6, 'P');
          bv.setView('text');
          if (!teTest.visible(notice)) out.push('notice hidden in the text view');
          s.focus();
          s.setSelectionOffsets(0);
          if (!document.execCommand('insertText', false, 'Top ')) out.push('execCommand failed');
          if (bv.keptDrafts()[0].anchor !== 9) out.push('anchor not mapped: ' + bv.keptDrafts()[0].anchor);
          notice.querySelector('[data-draft-action="apply"]').click();
          if (s.getText() !== 'Top # A\n\nPara one\n\npara 1\n') out.push('apply in text view ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('surface not canonical');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_every_block_action_is_keyboard_reachable() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let result = run_steps(&[
        // Focus order and roving action buttons; Escape returns to the block.
        r##"
          s.setText('# A\n\npara one\n');
          bv.setView('blocks');
          bv.setCurrent(0);
          const tabbable = () => Array.from(bv.container.querySelectorAll('*')).filter((el) => el.tabIndex >= 0 && (el.matches('button, textarea') || el.classList.contains('te-block')));
          const names = (els) => els.map((el) => el.dataset.action || (el.classList.contains('te-block') ? 'card' + el.dataset.index : el.className)).join(',');
          if (names(tabbable()) !== 'card0,edit,insert,delete,te-blocks-add') out.push('tab order ' + names(tabbable()));
          for (const b of bv.cards()[0].querySelectorAll('.te-block-action')) {
            if (!b.getAttribute('aria-keyshortcuts')) out.push('no aria-keyshortcuts on ' + b.dataset.action);
          }
          if (!/Shift\+Enter/.test(bv.cards()[0].getAttribute('aria-keyshortcuts'))) out.push('card keyshortcuts');
          teTest.key(document.activeElement, 'ArrowDown');
          if (names(tabbable()) !== 'card1,edit,insert,delete,te-blocks-add') out.push('roving actions ' + names(tabbable()));
          const edit = bv.cards()[1].querySelector('[data-action="edit"]');
          edit.focus();
          if (document.activeElement !== edit) out.push('edit button not focusable');
          if (!edit.parentNode.matches('.te-block:focus-within .te-block-actions')) out.push('actions not revealed while focused');
          teTest.key(edit, 'Escape');
          if (document.activeElement !== bv.cards()[1]) out.push('Escape did not return to the block');
        "##,
        // Edit: button activation and Enter.
        r##"
          const edit = bv.cards()[1].querySelector('[data-action="edit"]');
          edit.focus();
          edit.click();
          let ta = bv.list.querySelector('textarea');
          if (!ta || ta.value !== 'para one') out.push('edit button');
          teTest.key(ta, 'Escape');
          teTest.key(document.activeElement, 'Enter');
          ta = bv.list.querySelector('textarea');
          if (!ta || ta.value !== 'para one') out.push('Enter edit');
          teTest.key(ta, 'Escape');
        "##,
        // Insert below: button activation and Shift+Enter.
        r##"
          const insert = bv.cards()[1].querySelector('[data-action="insert"]');
          insert.focus();
          insert.click();
          let ta = bv.list.querySelector('textarea');
          if (!ta || ta.value !== '') return 'insert button';
          ta.value = 'x';
          teTest.key(ta, 'Enter', { ctrlKey: true });
          if (s.getText() !== '# A\n\npara one\n\nx\n') out.push('insert button result ' + JSON.stringify(s.getText()));
          bv.setCurrent(0);
          teTest.key(document.activeElement, 'Enter', { shiftKey: true });
          ta = bv.list.querySelector('textarea');
          if (!ta) return out.concat('Shift+Enter did not insert').join('; ');
          ta.value = 'y';
          teTest.key(ta, 'Enter', { ctrlKey: true });
          if (s.getText() !== '# A\n\ny\n\npara one\n\nx\n') out.push('Shift+Enter result ' + JSON.stringify(s.getText()));
        "##,
        // Delete: button activation and the Delete key.
        r##"
          bv.setCurrent(3);
          const del = bv.cards()[3].querySelector('[data-action="delete"]');
          del.focus();
          del.click();
          if (s.getText() !== '# A\n\ny\n\npara one\n') out.push('delete button ' + JSON.stringify(s.getText()));
          bv.setCurrent(1);
          teTest.key(document.activeElement, 'Delete');
          if (s.getText() !== '# A\n\npara one\n') out.push('Delete key ' + JSON.stringify(s.getText()));
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_escape_keeps_typed_drafts() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let result = run_steps(&[
        // Escape after typing: the editor closes, the draft goes to the
        // notice (aria-live) and pendingDrafts(), focus returns to the card,
        // and the body is unchanged.
        r##"
          T.doc = '# A\n\npara one\n\npara two\n';
          s.setText(T.doc);
          bv.setView('blocks');
          const ta = bv.editBlock(1);
          ta.value = 'para ONE';
          teTest.key(ta, 'Escape');
          const notice = bv.draftsElement;
          if (bv.editing || bv.list.querySelector('textarea')) out.push('editor still open');
          if (s.getText() !== T.doc) out.push('Escape changed the body');
          if (document.activeElement !== bv.cards()[1]) out.push('focus not on the card');
          const kept = bv.keptDrafts();
          if (kept.length !== 1 || kept[0].reason !== 'escaped' || kept[0].value !== 'para ONE' || kept[0].originalText !== 'para one') out.push('kept ' + JSON.stringify(kept));
          if (JSON.stringify(bv.pendingDrafts()) !== JSON.stringify([{ value: 'para ONE', originalText: 'para one', anchor: 5, index: 1, isNew: false }])) out.push('pending ' + JSON.stringify(bv.pendingDrafts()));
          if (!teTest.visible(notice) || notice.getAttribute('aria-live') !== 'polite' || !notice.textContent.includes('para ONE')) out.push('notice not announced');
          const apply = notice.querySelector('[data-draft-action="apply"]');
          if (apply.textContent !== 'Replace block') out.push('label ' + apply.textContent);
        "##,
        // Apply replaces the block; one undo step reverts it.
        r##"
          bv.draftsElement.querySelector('[data-draft-action="apply"]').click();
          if (s.getText() !== '# A\n\npara ONE\n\npara two\n') out.push('apply ' + JSON.stringify(s.getText()));
          if (bv.keptDrafts().length || !bv.draftsElement.hidden) out.push('notice not cleared');
          s.undo();
          if (s.getText() !== T.doc) out.push('undo ' + JSON.stringify(s.getText()));
        "##,
        // Discard removes an escaped draft; Escape with nothing typed (or a
        // blank new paragraph) leaves no notice.
        r##"
          let ta = bv.editBlock(2);
          ta.value = 'gone';
          teTest.key(ta, 'Escape');
          if (bv.keptDrafts().length !== 1) out.push('second draft not kept');
          bv.draftsElement.querySelector('[data-draft-action="discard"]').click();
          if (bv.keptDrafts().length || !bv.draftsElement.hidden || s.getText() !== T.doc) out.push('discard');
          ta = bv.editBlock(1);
          teTest.key(ta, 'Escape');
          if (bv.keptDrafts().length || !bv.draftsElement.hidden) out.push('notice for an unchanged block');
          ta = bv.insertBlockAfter(0);
          ta.value = '   ';
          teTest.key(ta, 'Escape');
          if (bv.keptDrafts().length || !bv.draftsElement.hidden) out.push('notice for a blank new paragraph');
          if (bv.list.querySelector('.te-block-new')) out.push('blank new card left behind');
        "##,
        // An escaped new paragraph is reported by destroy().
        r##"
          const ta = bv.insertBlockAfter(0);
          ta.value = 'new para';
          teTest.key(ta, 'Escape');
          const apply = bv.draftsElement.querySelector('[data-draft-action="apply"]');
          if (!apply || apply.textContent !== 'Insert as new paragraph') out.push('new paragraph label');
          const events = [];
          const on = (e) => events.push(e.detail.drafts);
          document.addEventListener('te:blocks-drafts', on);
          const ret = ed.destroy();
          document.removeEventListener('te:blocks-drafts', on);
          const want = JSON.stringify([{ value: 'new para', originalText: '', anchor: 5, index: 1, isNew: true }]);
          if (JSON.stringify(ret) !== want) out.push('destroy returned ' + JSON.stringify(ret));
          if (events.length !== 1 || JSON.stringify(events[0]) !== want) out.push('event ' + JSON.stringify(events));
          if (s.getText() !== T.doc) out.push('teardown changed the document');
          const fresh = new MarkdownEditor(window.EditorConfig);
          fresh.initialize();
          window.__teEditor = fresh;
          teTest.resetBlocks();
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
