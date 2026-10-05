//! Browser tests for the Overflow panel (issue #12, spec R-6.1 to R-6.5):
//! the XYZ toggle, ARIA and Escape, stashing as a move and one undo step
//! (menu and Ctrl+Shift+X), pulling back with Ctrl+Enter and by drag and
//! drop, persistence through save and open, export, the preserved-block
//! refusal and destroy(). Run with `wasm-pack test --headless --chrome`.
//! Real scripts and the real document API; nothing is mocked. Each test is a
//! short list of `run_steps` steps with one or two model calls each.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

/// "Pass me a paperclip. Drop this.\n\nSecond block.\n" with span `s1` over
/// "paperclip" (alternative "eraser") and a ghost over " Drop this.".
fn fixture() -> String {
    let mut doc = Document::new("Pass me a paperclip. Drop this.\n\nSecond block.\n");
    let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
    doc.add_alternative(&id, "eraser", Source::Human, None)
        .unwrap();
    doc.ghost(20, 31).unwrap();
    write(&doc)
}

/// Installs `window.__teSrc` and the small helpers the steps share.
fn install(src: &str) {
    let script = format!(
        r##"window.__teSrc = {src};
        window.teOv = {{
          api() {{ return window.wasmBindings; }},
          // Live and set-aside annotation counts, read from the model.
          where() {{
            const a = window.wasmBindings.document_annotations();
            return 'spans=' + a.spans.length + ' ghosts=' + a.ghosts.length +
              ' aside=' + (a.setAside.spans.length + a.setAside.ghosts.length);
          }},
          type(area, value) {{
            area.value = value;
            area.dispatchEvent(new InputEvent('input', {{ bubbles: true }}));
          }},
        }};
        'ok'"##,
        src = js_string_literal(src)
    );
    assert_eq!(js_string(&script), "ok");
}

#[wasm_bindgen_test]
async fn test_xyz_toggles_the_panel_with_aria_escape_and_mode_gating() {
    let _document = fresh_full_editor();
    install("Hello world.\n");
    let r = run_steps(&[
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        const ov = ed.overflow;
        if (!ov) return 'no editor.overflow';
        if (ov.isOpen()) out.push('open at start');
        const xyz = teTest.control('overflow');
        xyz.focus();
        xyz.click();
        if (!ov.isOpen() || !teTest.visible(ov.root)) out.push('XYZ did not open the panel');
        if (ov.root.getAttribute('role') !== 'complementary') out.push('role');
        const title = document.getElementById(ov.root.getAttribute('aria-labelledby'));
        if (!title || title.textContent !== 'Overflow') out.push('label');
        if (getComputedStyle(title).color !== 'rgb(140, 134, 230)') out.push('title colour ' + getComputedStyle(title).color);
        if (!/Caveat|cursive/.test(getComputedStyle(title).fontFamily)) out.push('title face');
        if (!/mono/i.test(getComputedStyle(ov.area).fontFamily)) out.push('text area face');
        if (document.activeElement !== ov.area) out.push('focus not in the text area');
        if (xyz.getAttribute('aria-expanded') !== 'true') out.push('aria-expanded');
        if (xyz.getAttribute('aria-controls') !== ov.root.id) out.push('aria-controls');
        const hint = ov.root.querySelector('.te-overflow-hint').textContent;
        if (hint !== 'ctrl+⏎ or drag into the page to use') out.push('hint ' + hint);
        if (document.body.dataset.teOverflow !== 'open') out.push('body attribute');"#,
        r#"const ov = ed.overflow;
        const xyz = teTest.control('overflow');
        teTest.key(ov.area, 'Escape');
        if (ov.isOpen()) out.push('Escape did not close');
        if (document.activeElement !== xyz) out.push('focus did not return to XYZ');
        if (xyz.getAttribute('aria-expanded') !== 'false') out.push('aria-expanded after close');
        if (document.body.dataset.teOverflow) out.push('body attribute kept');
        xyz.click();
        xyz.click();
        if (ov.isOpen()) out.push('second click did not close');"#,
        r#"const ov = ed.overflow;
        ov.open();
        ed.chrome.setMode('plain');
        if (ov.isOpen()) out.push('plain mode kept the panel open');
        if (ov.open()) out.push('opened in plain mode');
        if (teTest.visible(ov.root)) out.push('visible in plain mode');"#,
    ])
    .await;
    assert!(r.is_empty(), "{r}");
}

#[wasm_bindgen_test]
async fn test_stash_is_a_move_and_one_undo_step() {
    let _document = fresh_full_editor();
    install(&fixture());
    let r = run_steps(&[
        // Stash through the selection menu.
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        s.focus();
        s.setSelectionOffsets(8, 31);
        T.depth = s.historyIndex;
        if (!ed.selectionMenu.open(s.getSelectionOffsets())) return 'menu did not open';
        const row = document.querySelector('.te-selection-menu [data-item="stash"]');
        if (!row) return 'no stash item';
        if (row.querySelector('.te-selection-menu-label').textContent !== 'Stash this in Overflow') out.push('label');
        if (row.getAttribute('aria-keyshortcuts') !== 'Control+Shift+X') out.push('keys ' + row.getAttribute('aria-keyshortcuts'));
        row.click();
        if (s.getText() !== 'Pass me \n\nSecond block.\n') out.push('text ' + JSON.stringify(s.getText()));
        if (teOv.api().document_overflow() !== 'a paperclip. Drop this.') out.push('overflow ' + JSON.stringify(teOv.api().document_overflow()));
        if (teOv.api().document_body() !== s.getText()) out.push('model body differs');
        if (s.historyIndex !== T.depth + 1) out.push('not one undo step');
        if (!ed.overflow.isOpen()) out.push('panel not opened');
        if (ed.overflow.area.value !== 'a paperclip. Drop this.') out.push('panel ' + ed.overflow.area.value);
        if (ed.overflow.root.contains(document.activeElement)) out.push('stash took focus');
        if (teOv.where() !== 'spans=0 ghosts=0 aside=2') out.push(teOv.where());"#,
        // Undo restores both the text and the overflow; redo does both again.
        r#"s.undo();
        if (s.getText() !== 'Pass me a paperclip. Drop this.\n\nSecond block.\n') out.push('undo text');
        if (teOv.api().document_overflow() !== '') out.push('undo overflow ' + JSON.stringify(teOv.api().document_overflow()));
        if (ed.overflow.area.value !== '') out.push('undo panel');
        if (teOv.where() !== 'spans=1 ghosts=1 aside=0') out.push('undo ' + teOv.where());
        if (teOv.api().save_document() !== window.__teSrc) out.push('undo: saved file differs');
        if (s.historyIndex !== T.depth) out.push('undo depth');
        s.redo();
        if (s.getText() !== 'Pass me \n\nSecond block.\n') out.push('redo text');
        if (teOv.api().document_overflow() !== 'a paperclip. Drop this.') out.push('redo overflow');
        if (ed.overflow.area.value !== 'a paperclip. Drop this.') out.push('redo panel');"#,
        // Ctrl+Shift+X appends after a blank line, as one more step.
        r#"s.focus();
        s.setSelectionOffsets(10, 23);
        if (!teTest.key(s.root, 'X', { ctrlKey: true, shiftKey: true, code: 'KeyX' })) out.push('shortcut not handled');
        if (s.getText() !== 'Pass me \n\n\n') out.push('text ' + JSON.stringify(s.getText()));
        if (teOv.api().document_overflow() !== 'a paperclip. Drop this.\n\nSecond block.') out.push('overflow ' + JSON.stringify(teOv.api().document_overflow()));
        if (s.historyIndex !== T.depth + 2) out.push('depth');"#,
        // Panel typing after a stash survives undoing it, and an identical
        // copy typed by the author is not taken for the stashed text.
        r#"const chunk = '\n\nSecond block.';
        teOv.type(ed.overflow.area, 'a paperclip. Drop this.' + chunk + chunk + ' mine');
        s.undo();
        if (s.getText() !== 'Pass me \n\nSecond block.\n') out.push('undo text');
        if (teOv.api().document_overflow() !== 'a paperclip. Drop this.' + chunk + ' mine') out.push('rebased ' + JSON.stringify(teOv.api().document_overflow()));
        if (ed.overflow.area.value !== teOv.api().document_overflow()) out.push('panel ' + ed.overflow.area.value);
        if (ed.warningKind === 'overflow') out.push('notice shown for an exact undo');
        s.redo();
        if (teOv.api().document_overflow() !== 'a paperclip. Drop this.' + chunk + chunk + ' mine') out.push('redo ' + JSON.stringify(teOv.api().document_overflow()));"#,
        // Text typed before the stashed text: undo restores the page, leaves
        // the overflow alone and says so.
        r#"const edited = 'Note: a paperclip. Drop this.\n\nSecond block.\n\nSecond block. mine';
        teOv.type(ed.overflow.area, edited);
        s.undo();
        if (s.getText() !== 'Pass me \n\nSecond block.\n') out.push('undo text');
        if (teOv.api().document_overflow() !== edited) out.push('overflow changed ' + JSON.stringify(teOv.api().document_overflow()));
        if (ed.warningKind !== 'overflow' || !/left Overflow unchanged/.test(ed.warning)) out.push('notice ' + ed.warningKind);
        ed.showWarning(null);"#,
        // Plain mode: no stash item, the shortcut is left alone.
        r#"ed.chrome.setMode('plain');
        s.focus();
        s.setSelectionOffsets(0, 4);
        const before = s.getText();
        if (teTest.key(s.root, 'X', { ctrlKey: true, shiftKey: true, code: 'KeyX' })) out.push('shortcut handled in plain mode');
        if (s.getText() !== before) out.push('plain mode stashed');
        if (ed.selectionMenu.availableItems(ed.selectionMenu.contextFor({ start: 0, end: 4 })).some((i) => i.id === 'stash')) out.push('item offered in plain mode');"#,
    ])
    .await;
    assert!(r.is_empty(), "{r}");
}

#[wasm_bindgen_test]
async fn test_ctrl_enter_and_drop_insert_at_the_caret() {
    let _document = fresh_full_editor();
    install("Alpha beta gamma.\n");
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
        // Nothing selected: the current line. The caret is where undo left it.
        r#"const ov = ed.overflow;
        s.setSelectionOffsets(0);
        ov.area.focus();
        ov.area.setSelectionRange(1, 1);
        ov.pullBack();
        if (s.getText() !== 'OneAlpha beta gamma.\n') out.push('line ' + JSON.stringify(s.getText()));
        s.undo();"#,
        // Panel text dragged out is a copy; dropping inserts at the drop point.
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
    install("Body text.\n");
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
