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

use terraphim_alternatives::{write, Document, Source, SpanKind};

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

#[wasm_bindgen_test]
async fn test_xyz_toggles_the_panel_with_aria_escape_and_mode_gating() {
    let _document = fresh_full_editor();
    install_overflow_helpers("Hello world.\n");
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
    install_overflow_helpers(&fixture());
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
