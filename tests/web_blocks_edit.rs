//! Browser tests for the Blocks view (issue #19): structural edits, Write_On
//! interplay, drafts kept across outside changes and keyboard access to every
//! block action. Run with `wasm-pack test --headless --chrome`. Real scripts,
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
          const before = ed.annotations();
          const ta = bv.insertBlockAfter(0);
          ta.value = 'Inserted.';
          bv.commitEdit();
          const after = ed.annotations();
          const d = 'Inserted.\n\n'.length;
          const moved = (a) => a.anchor.start >= 31 ? { ...a, anchor: { ...a.anchor, start: a.anchor.start + d, end: a.anchor.end + d } } : a;
          if (JSON.stringify([after.spans, after.ghosts]) !== JSON.stringify([before.spans.map(moved), before.ghosts.map(moved)])) out.push('anchors after insert ' + JSON.stringify(after.ghosts));
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
        // 2. An overlapping edit keeps the draft in the notice; Apply works
        //    and undo afterwards is one step per change.
        r##"
          const notice = bv.draftsElement;
          const ta = bv.editBlock(1);
          ta.value = 'para ONE draft';
          s.replaceRange(5, 9, 'PARA');
          if (bv.editing || bv.list.querySelector('textarea')) out.push('editor still open after overlap');
          const kept = bv.keptDrafts();
          if (kept.length !== 1 || kept[0].value !== 'para ONE draft' || kept[0].reason !== 'changed') out.push('draft not kept ' + JSON.stringify(kept));
          if (!teTest.visible(notice) || !notice.textContent.includes('para ONE draft')) out.push('notice not visible with the draft');
          if (s.getText() !== '# A\n\nPARA one\n\npara two edited\n\nTail.\n') out.push('outside edit not applied');
          notice.querySelector('[data-draft-action="apply"]').click();
          if (s.getText() !== '# A\n\npara ONE draft\n\npara two edited\n\nTail.\n') out.push('apply ' + JSON.stringify(s.getText()));
          if (!notice.hidden || bv.keptDrafts().length) out.push('notice not cleared after apply');
          if (window.wasmBindings.document_body() !== s.getText()) out.push('model body differs after apply');
          s.undo();
          if (s.getText() !== '# A\n\nPARA one\n\npara two edited\n\nTail.\n') out.push('undo apply ' + JSON.stringify(s.getText()));
          s.undo();
          if (s.getText() !== '# A\n\npara one\n\npara two edited\n\nTail.\n') out.push('undo outside edit ' + JSON.stringify(s.getText()));
          if (bv.cards().length !== 4) out.push('cards after undo');
        "##,
        // 3. openDocument during an edit keeps the draft; Discard works.
        r##"
          const notice = bv.draftsElement;
          const ta = bv.editBlock(0);
          ta.value = '# Draft heading';
          ed.openDocument(teBlockFixtures.full);
          const k2 = bv.keptDrafts();
          if (k2.length !== 1 || k2[0].reason !== 'replaced' || k2[0].value !== '# Draft heading') out.push('open: draft ' + JSON.stringify(k2));
          if (!notice.textContent.includes('replaced')) out.push('open: notice text');
          const saved = ed.saveDocument();
          if (ed.annotations().spans.length !== 3) out.push('open: spans');
          notice.querySelector('[data-draft-action="discard"]').click();
          if (bv.keptDrafts().length || !notice.hidden) out.push('discard');
          if (ed.saveDocument() !== saved) out.push('discard changed the document');
        "##,
        // 4. A kept draft follows typing (execCommand) in the text view and
        //    applies there.
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
          if (s.getText() !== 'Top # A\n\npara 1\n') out.push('apply in text view ' + JSON.stringify(s.getText()));
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
