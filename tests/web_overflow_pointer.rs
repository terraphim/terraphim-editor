//! Cross-panel pointer fallback against the real DOM and WASM document.
//! Synthetic gestures prove the contract, not trusted Safari acceptance.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

// Pointer gestures need a cross-panel fallback when the browser changes the
// selection without beginning HTML drag-and-drop. Synthetic events exercise
// the contract only; trusted Safari drag still requires native acceptance.
#[wasm_bindgen_test]
async fn test_selected_page_pointer_drag_stashes_once_and_refuses_stale_gestures() {
    let _document = fresh_full_editor();
    install_overflow_helpers("Alpha beta gamma.\n");
    let r = run_steps(&[
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        const ov = ed.overflow;
        ov.open();
        s.focus(); s.setSelectionOffsets(6, 10);
        T.point = (target, type, x, y, buttons = 1) => target.dispatchEvent(new PointerEvent(type, {
          bubbles: true, cancelable: true, pointerId: 7, pointerType: 'mouse',
          isPrimary: true, button: 0, buttons, clientX: x, clientY: y,
        }));
        T.begin = () => {
          s.focus(); s.setSelectionOffsets(6, 10);
          const rect = window.getSelection().getRangeAt(0).getBoundingClientRect();
          T.x = rect.left + rect.width / 2; T.y = rect.top + rect.height / 2;
          T.point(s.root, 'pointerdown', T.x, T.y);
        };
        T.finish = () => {
          const rect = ov.area.getBoundingClientRect();
          T.point(ov.area, 'pointermove', rect.left + 5, rect.top + 5);
          T.point(ov.area, 'pointerup', rect.left + 5, rect.top + 5, 0);
        };
        T.depth = s.historyIndex;
        T.begin();
        // A browser may collapse the live selection after pointerdown.
        s.setSelectionOffsets(0);
        T.finish();
        if (s.getText() !== 'Alpha  gamma.\n') out.push('pointer stash body ' + JSON.stringify(s.getText()));
        if (teOv.api().document_overflow() !== 'beta') out.push('pointer stash overflow');
        if (s.historyIndex !== T.depth + 1) out.push('pointer stash not one step');
        T.finish();
        if (teOv.api().document_overflow() !== 'beta') out.push('replayed pointer stash');
        s.undo();
        if (s.getText() !== window.__teSrc || teOv.api().document_overflow() !== '') out.push('pointer undo');
        s.redo();
        if (s.getText() !== 'Alpha  gamma.\n' || teOv.api().document_overflow() !== 'beta') out.push('pointer redo');
        T.saved = ed.saveDocument(); ed.openDocument(T.saved);
        if (teOv.api().document_overflow() !== 'beta') out.push('pointer save/reopen');"#,
        r#"ed.openDocument(window.__teSrc); ed.overflow.open();
        T.begin();
        s.replaceRange(0, 0, 'Changed ', { source: 'test' });
        const changed = s.getText(); T.finish();
        if (s.getText() !== changed || teOv.api().document_overflow() !== '') out.push('stale gesture mutated');
        ed.openDocument(window.__teSrc); ed.overflow.open();
        T.begin(); T.point(s.root, 'pointercancel', T.x, T.y, 0); T.finish();
        if (s.getText() !== window.__teSrc) out.push('cancelled gesture mutated');
        T.begin(); teTest.key(s.root, 'Escape'); T.finish();
        if (s.getText() !== window.__teSrc) out.push('escape gesture mutated');
        T.begin();
        // Native HTML drag supersedes the pointer fallback, even if no drop follows.
        s.root.dispatchEvent(new DragEvent('dragstart', { bubbles: true, dataTransfer: new DataTransfer() }));
        T.finish();
        if (s.getText() !== window.__teSrc) out.push('native drag double handling');"#,
        r#"ed.openDocument(window.__teSrc); ed.overflow.open();
        T.begin(); ed.openDocument(window.__teSrc); T.finish();
        if (s.getText() !== window.__teSrc) out.push('same-body replacement mutated');
        T.begin(); T.point(ed.overflow.area, 'pointerup', T.x, T.y, 0);
        if (s.getText() !== window.__teSrc) out.push('click without movement mutated');
        T.begin(); T.point(s.root, 'pointermove', T.x + 10, T.y);
        T.point(s.root, 'pointerup', T.x + 10, T.y, 0);
        if (s.getText() !== window.__teSrc) out.push('same-page release mutated');
        T.begin();
        ed.overflow.area.dispatchEvent(new PointerEvent('pointerup', {
          bubbles: true, pointerId: 8, button: 0,
        }));
        if (s.getText() !== window.__teSrc) out.push('wrong pointer mutated');
        T.finish();
        if (s.getText() !== 'Alpha  gamma.\n') out.push('wrong pointer consumed matching gesture');
        ed.openDocument(window.__teSrc); ed.overflow.open();
        T.begin(); ed.overflow.close(); ed.overflow.open(); T.finish();
        if (s.getText() !== window.__teSrc) out.push('closed gesture mutated');
        T.begin(); ed.chrome.setMode('plain'); ed.chrome.setMode('write-on'); ed.overflow.open(); T.finish();
        if (s.getText() !== window.__teSrc) out.push('mode exit gesture mutated');
        T.begin(); T.point(ed.overflow.area, 'pointermove', T.x + 10, T.y, 0); T.finish();
        if (s.getText() !== window.__teSrc) out.push('unheld pointer mutated');
        T.begin(); window.dispatchEvent(new Event('blur')); T.finish();
        if (s.getText() !== window.__teSrc) out.push('blur gesture mutated');
        ed.openDocument(window.__teSrc + '\n```terraphim-alternatives\n{ \"version\": 1, \"spans\": [,] }\n```\n');
        ed.overflow.open();
        const preserved = ed.saveDocument();
        if (!ed.overflow.area.readOnly) out.push('malformed fixture not read-only');
        T.begin(); T.finish();
        if (ed.saveDocument() !== preserved) out.push('read-only gesture mutated');"#,
    ]).await;
    assert!(r.is_empty(), "{r}");
}
