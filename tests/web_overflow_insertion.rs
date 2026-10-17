//! Browser tests for the Overflow panel (issue #12, spec R-6.1 to R-6.5):
//! the XYZ toggle, ARIA and Escape, stashing as a move and one undo step
//! (menu and Ctrl+Shift+X), pulling back with Ctrl+Enter and by drag and
//! drop, persistence through save and open, export, the preserved-block
//! refusal and destroy(); Ctrl+Shift+D and the full-height placement are in
//! `web_overflow_layout.rs`. Run with `wasm-pack test --headless --chrome`.
//! Real scripts and the real document API; nothing is mocked. Each test is a
//! short list of `run_steps` steps with one or two model calls each.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_ctrl_enter_and_drop_insert_at_the_caret() {
    let _document = fresh_full_editor();
    install_overflow_helpers("Alpha beta gamma.\n");
    let r = run_steps(&[
        // The caret is placed as a click would (a DOM selection in the
        // surface), then focus moves to the panel.
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        s.setSelectionOffsets(0);
        s.focus();
        const point = s.offsetToPoint(6);
        getSelection().collapse(point.node, point.offset);
        document.dispatchEvent(new Event('selectionchange'));
        const ov = ed.overflow;
        ov.open();
        teOv.type(ov.area, 'One\nTwo words');
        ov.flush();
        ov.area.setSelectionRange(4, 13);
        T.depth = s.historyIndex;
        if (!teTest.key(ov.area, 'Enter', { ctrlKey: true })) out.push('Ctrl+Enter not handled');
        if (s.getText() !== 'Alpha Two wordsbeta gamma.\n') out.push('text ' + JSON.stringify(s.getText()));
        if (teOv.api().document_body() !== s.getText()) out.push('model body differs');
        if (s.historyIndex !== T.depth + 1) out.push('not one undo step');
        if (teOv.api().document_overflow() !== 'One\nTwo words') out.push('overflow changed: pull back is a copy');
        if (document.activeElement !== ov.area) out.push('focus left the panel');
        if (ov.area.selectionStart !== 4 || ov.area.selectionEnd !== 13) out.push('panel selection lost');
        s.undo();
        if (s.getText() !== 'Alpha beta gamma.\n') out.push('undo ' + JSON.stringify(s.getText()));"#,
        // Nothing selected: the current line. At the start of a line it is a
        // paragraph of its own, separated as terraphim-lsp separates it.
        r#"const ov = ed.overflow;
        s.setSelectionOffsets(0);
        ov.area.focus();
        ov.area.setSelectionRange(1, 1);
        ov.pullBack();
        if (s.getText() !== 'One\n\nAlpha beta gamma.\n') out.push('line ' + JSON.stringify(s.getText()));
        if (teOv.api().document_overflow() !== 'One\nTwo words') out.push('overflow changed');
        s.undo();
        if (s.getText() !== 'Alpha beta gamma.\n') out.push('undo line ' + JSON.stringify(s.getText()));"#,
        // At the end of a line the text no longer glues to the next line
        // ("stuff.Pass me"): one blank line on each side, as in terraphim-lsp.
        r#"const ov = ed.overflow;
        ed.openDocument('Some stuff.\nPass me a paperclip.\n');
        ed.chrome.setMode('write-on');
        ov.open();
        teOv.type(ov.area, 'Para.\n\nOther.');
        ov.flush();
        s.setSelectionOffsets(11);
        ov.area.focus();
        ov.area.setSelectionRange(0, 5);
        T.depth = s.historyIndex;
        ov.pullBack();
        if (s.getText() !== 'Some stuff.\n\nPara.\n\nPass me a paperclip.\n') out.push('line end ' + JSON.stringify(s.getText()));
        if (teOv.api().document_body() !== s.getText()) out.push('model body differs');
        if (s.historyIndex !== T.depth + 1) out.push('not one undo step');
        if (teOv.api().document_overflow() !== 'Para.\n\nOther.') out.push('overflow changed');
        s.undo();
        if (s.getText() !== 'Some stuff.\nPass me a paperclip.\n') out.push('undo ' + JSON.stringify(s.getText()));"#,
        // The surface keeps "\n" line ends (it normalises a CRLF file on
        // open), so the separators it gets are "\n" too.
        r#"const ov = ed.overflow;
        ed.openDocument('A.\r\nB.\r\n');
        ed.chrome.setMode('write-on');
        ov.open();
        teOv.type(ov.area, 'Para.\n\nOther.');
        ov.flush();
        s.setSelectionOffsets(2);
        ov.area.focus();
        ov.area.setSelectionRange(0, 5);
        ov.pullBack();
        if (s.getText() !== 'A.\n\nPara.\n\nB.\n') out.push('crlf file ' + JSON.stringify(s.getText()));
        if (teOv.api().document_body() !== s.getText()) out.push('model body differs');
        ed.openDocument('Some stuff.\nPass me a paperclip.\n');
        ed.chrome.setMode('write-on');
        ov.open();
        teOv.type(ov.area, 'Para.\n\nOther.');
        ov.flush();"#,
        // Selected panel text dragged into the page is pulled back the same
        // way at the drop point, a copy. The drop lands on the surface root,
        // where EditorSurface's own drop listener sits: the text goes in once.
        r#"const ov = ed.overflow;
        ov.area.setSelectionRange(7, 13);
        const dt = new DataTransfer();
        dt.setData('text/plain', 'Other.');
        const start = new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: dt });
        ov.area.dispatchEvent(start);
        if (start.defaultPrevented) out.push('panel drag cancelled');
        const rect = s.getCaretRect(17);
        T.depth = s.historyIndex;
        s.root.dispatchEvent(new DragEvent('drop', {
          bubbles: true, cancelable: true, dataTransfer: dt,
          clientX: rect.left + 0.5, clientY: rect.top + rect.height / 2,
        }));
        if (s.getText() !== 'Some stuff.\nPass Other.me a paperclip.\n') out.push('panel drop ' + JSON.stringify(s.getText()));
        if (s.historyIndex !== T.depth + 1) out.push('panel drop not one undo step');
        if (teOv.api().document_overflow() !== 'Para.\n\nOther.') out.push('panel drop changed the overflow');
        s.undo();
        ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        ov.open();
        teOv.type(ov.area, 'One\nTwo words');
        ov.flush();"#,
        // Text dropped from elsewhere is inserted as it is at the drop point.
        // (A script-made DataTransfer is read-only for effectAllowed outside
        // a trusted drag, so the copy-only effect cannot be asserted here;
        // the dragstart must at least not be cancelled.)
        r#"const start = new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: new DataTransfer() });
        ed.overflow.area.dispatchEvent(start);
        if (start.defaultPrevented) out.push('panel drag cancelled');
        const drop = new DataTransfer();
        drop.setData('text/plain', 'DROP ');
        const rect = s.getCaretRect(11);
        T.depth = s.historyIndex;
        s.root.dispatchEvent(new DragEvent('drop', {
          bubbles: true, cancelable: true, dataTransfer: drop,
          clientX: rect.left + 0.5, clientY: rect.top + rect.height / 2,
        }));
        if (s.getText() !== 'Alpha beta DROP gamma.\n') out.push('drop ' + JSON.stringify(s.getText()));
        if (s.historyIndex !== T.depth + 1) out.push('drop not one undo step');
        if (teOv.api().document_body() !== s.getText()) out.push('model body differs');"#,
        // A page selection dragged onto the panel is stashed.
        r#"s.focus();
        s.setSelectionOffsets(11, 16);
        s.root.dispatchEvent(new DragEvent('dragstart', { bubbles: true, cancelable: true, dataTransfer: new DataTransfer() }));
        const dt = new DataTransfer();
        dt.setData('text/plain', 'DROP ');
        const over = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt });
        ed.overflow.area.dispatchEvent(over);
        if (!over.defaultPrevented) out.push('dragover not accepted');
        ed.overflow.area.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt }));
        if (s.getText() !== 'Alpha beta gamma.\n') out.push('page drag ' + JSON.stringify(s.getText()));
        if (teOv.api().document_overflow() !== 'One\nTwo words\n\nDROP ') out.push('overflow ' + JSON.stringify(teOv.api().document_overflow()));"#,
    ])
    .await;
    assert!(r.is_empty(), "{r}");
}

#[wasm_bindgen_test]
async fn test_panel_text_persists_is_not_exported_and_destroy_cleans_up() {
    let _document = fresh_full_editor();
    install_overflow_helpers("Body text.\n");
    let r = run_steps(&[
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        ed.overflow.open();
        teOv.type(ed.overflow.area, 'Stashed idea.');
        if (!ed.overflow.pending()) out.push('input not debounced');
        T.saved = ed.saveDocument();
        if (ed.overflow.pending()) out.push('save did not flush');
        if (!T.saved.includes('"overflow": "Stashed idea."')) out.push('saved ' + JSON.stringify(T.saved));
        const exported = ed.exportDocument();
        if (exported.includes('Stashed idea.') || exported !== 'Body text.\n') out.push('export ' + JSON.stringify(exported));"#,
        r#"ed.openDocument('Another file.\n');
        if (ed.overflow.area.value !== '') out.push('stale panel after open');
        ed.chrome.setMode('plain');
        ed.openDocument(T.saved);
        if (teOv.api().document_overflow() !== 'Stashed idea.') out.push('reopened ' + teOv.api().document_overflow());
        if (ed.saveDocument() !== T.saved) out.push('plain mode save dropped the overflow');
        ed.chrome.setMode('write-on');
        ed.overflow.open();
        if (ed.overflow.area.value !== 'Stashed idea.') out.push('panel ' + ed.overflow.area.value);"#,
        // A preserved malformed block: read-only panel, no stash.
        r#"ed.openDocument('Body.\n\n```terraphim-alternatives\n{ "version": 1, "spans": [,] }\n```\n');
        const ov = ed.overflow;
        if (!ov.area.readOnly || ov.note.hidden) out.push('not read-only');
        s.focus();
        s.setSelectionOffsets(0, 4);
        if (teTest.key(s.root, 'X', { ctrlKey: true, shiftKey: true, code: 'KeyX' })) out.push('stashed into an unsavable overflow');
        if (s.getText() !== 'Body.') out.push('text ' + JSON.stringify(s.getText()));"#,
        r#"ed.openDocument('Fine.\n');
        if (ed.overflow.area.readOnly) out.push('still read-only');
        ed.overflow.open();
        ed.destroy();
        if (document.querySelector('.te-overflow')) out.push('panel left in the DOM');
        if (document.body.dataset.teOverflow) out.push('body attribute left');
        document.dispatchEvent(new CustomEvent('te:overflow', { detail: {} }));
        if (!ed.overflow.destroyed) out.push('not destroyed');"#,
    ])
    .await;
    assert!(r.is_empty(), "{r}");
}
