//! Browser tests for the alternatives side panel (issue #10): opening it from
//! the chrome, from "Alternatives for selection" (Ctrl+Shift+A) and from a
//! dot; adding, editing, deleting and reordering alternatives; Up/Down making
//! a line active with a live document update; provenance glyphs from the
//! `source` field; undo and redo of every operation with the model and the
//! surface in step; keyboard navigation and Escape (keyboard toggling, the
//! menu and teardown are in `web_alt_panel_keys.rs`; the chrome control,
//! provenance, tabs, save and reopen and the dot click in
//! `web_alt_panel_tabs.rs`; the width in `web_alt_panel_width.rs`; all split
//! to keep each binary inside its time budget).
//! Run with `wasm-pack test --headless --chrome`. Real scripts,
//! the real exported document API and real documents; nothing is mocked.
//! Every scenario runs as short steps with a yield between them
//! (`support::run_steps`).
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_selection_add_up_down_edit_and_undo() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        // Ctrl+Shift+A on a selection in plain mode: Write_On turns on, the
        // panel opens on a pending word span and focus goes to the new line.
        r##"
          s.setText('Pass me a paperclip. Drop this.');
          s.focus();
          s.setSelectionOffsets(10, 19);
          const prevented = teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          const P = teAlt.P();
          if (!prevented) out.push('Ctrl+Shift+A not cancelled');
          if (!P.isOpen()) return 'panel not open';
          if (document.body.dataset.mode !== 'write-on') out.push('Write_On not turned on');
          if (P.tab !== 'word') out.push('tab ' + P.tab);
          if (!P.target.pending) out.push('not pending: ' + JSON.stringify(P.target));
          if (JSON.stringify(teAlt.texts()) !== '["paperclip"]') out.push('lines ' + teAlt.texts());
          if (document.activeElement !== teAlt.newInput()) out.push('focus not on the new line');
          if (teAlt.spans().length !== 0) out.push('a span was created before any alternative');
          if (!P.root.closest('#app') || P.root.getAttribute('aria-label') !== 'Alternatives') out.push('panel root');
          if (!document.querySelector('#app').classList.contains('te-alt-open')) out.push('text not dimmed');
        "##,
        // Enter on the new line creates the span with its first alternative
        // and makes it active, live, with the a/an fix-up (issue #106).
        r##"
          teAlt.type(teAlt.newInput(), 'eraser');
          const spans = teAlt.spans();
          if (spans.length !== 1 || JSON.stringify(spans[0].alts.map((a) => [a.text, a.source])) !== '[["paperclip","original"],["eraser","human"]]') return 'model ' + JSON.stringify(spans);
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.active() !== 1) out.push('not active: ' + s.getText());
          if (teAlt.P().target.spanId !== spans[0].id) out.push('panel not on the span');
          if (document.activeElement !== teAlt.newInput()) out.push('focus left the new line');
          if (teAlt.newInput().value !== '') out.push('new line not cleared');
          ed.indicators.flush();
          if (teAlt.dotsOf(spans[0].id) !== 2) out.push('dots ' + teAlt.dotsOf(spans[0].id));
          T.id = spans[0].id;
        "##,
        r##"
          teAlt.type(teAlt.newInput(), 'thumbtack');
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","eraser","thumbtack"]') return 'lines ' + teAlt.texts();
          if (s.getText() !== 'Pass me a thumbtack. Drop this.' || teAlt.active() !== 2) out.push('added not active: ' + s.getText());
          // Up from the new line: focus goes to the last (active) alternative.
          teTest.key(teAlt.newInput(), 'ArrowUp');
          if (s.getText() !== 'Pass me a thumbtack. Drop this.') out.push('text ' + s.getText());
          if (teAlt.active() !== 2) out.push('active ' + teAlt.active());
          if (document.activeElement !== teAlt.inputs()[2]) out.push('focus not on line 3');
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        r##"
          teTest.key(teAlt.inputs()[2], 'ArrowUp');
          if (s.getText() !== 'Pass me an eraser. Drop this.') out.push('a/an: ' + s.getText());
          if (teAlt.active() !== 1) out.push('active ' + teAlt.active());
          if (!teAlt.inputs()[1].closest('li').classList.contains('te-alt-line--active')) out.push('active line not marked');
          // Down twice: thumbtack, then the new line (active unchanged).
          teTest.key(teAlt.inputs()[1], 'ArrowDown');
          teTest.key(teAlt.inputs()[2], 'ArrowDown');
          if (teAlt.active() !== 2 || document.activeElement !== teAlt.newInput()) out.push('down: ' + teAlt.active());
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        // Each change is one undo step (an addition with its activation
        // included); undo and redo keep model and text in step.
        r##"
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.active() !== 1) out.push('undo down: ' + s.getText());
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me a thumbtack. Drop this.' || teAlt.active() !== 2) out.push('undo up: ' + s.getText());
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || JSON.stringify(teAlt.texts()) !== '["paperclip","eraser"]' || teAlt.active() !== 1) out.push('undo addition: ' + s.getText() + ' ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model out of step after undo');
          s.redo();
          s.redo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.active() !== 1 || teAlt.texts().length !== 3) out.push('redo: ' + s.getText() + ' ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model out of step after redo');
        "##,
        // Editing the active line rewrites the text (with the a/an fix-up).
        r##"
          teAlt.type(teAlt.inputs()[1], 'pencil');
          if (s.getText() !== 'Pass me a pencil. Drop this.') out.push('edit active: ' + s.getText());
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil","thumbtack"]') out.push('lines ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model out of step');
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.texts()[1] !== 'eraser') out.push('undo edit: ' + teAlt.texts());
          s.redo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me a pencil. Drop this.' || !teAlt.inSync()) out.push('redo edit');
          // Editing an inactive line changes annotations only, still one step.
          teAlt.type(teAlt.inputs()[2], 'pin');
          if (teAlt.texts()[2] !== 'pin' || s.getText() !== 'Pass me a pencil. Drop this.') out.push('edit inactive');
          s.undo();
          teAlt.P().flush();
          if (teAlt.texts()[2] !== 'thumbtack') out.push('undo inactive edit: ' + teAlt.texts());
        "##,
        // Reorder with Alt+Up: the active alternative stays active.
        r##"
          teAlt.inputs()[2].focus();
          teTest.key(teAlt.inputs()[2], 'ArrowUp', { altKey: true });
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","thumbtack","pencil"]') return 'reorder ' + teAlt.texts();
          if (teAlt.active() !== 2 || s.getText() !== 'Pass me a pencil. Drop this.') out.push('active after reorder');
          if (document.activeElement !== teAlt.inputs()[1]) out.push('focus did not follow the line');
          teTest.key(teAlt.inputs()[1], 'ArrowUp', { altKey: true });
          if (teAlt.texts()[0] !== 'paperclip') out.push('moved above the original');
          s.undo();
          teAlt.P().flush();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil","thumbtack"]') out.push('undo reorder ' + teAlt.texts());
        "##,
        // Delete: the button, then an emptied line. Deleting the last one
        // removes the span and its indicator (R-2.7); undo brings both back.
        r##"
          teAlt.P().list.querySelectorAll('.te-alt-delete')[1].click();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil"]') return 'delete button ' + teAlt.texts();
          teAlt.type(teAlt.inputs()[1], '');
          if (teAlt.spans().length !== 0) return 'span kept ' + JSON.stringify(teAlt.spans());
          if (s.getText() !== 'Pass me a paperclip. Drop this.') out.push('original not shown: ' + s.getText());
          if (!teAlt.P().target.pending) out.push('panel did not fall back to pending');
          ed.indicators.flush();
          if (teAlt.dotsOf(T.id) !== 0) out.push('indicator kept');
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        r##"
          s.undo();
          s.undo();
          teAlt.P().flush();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil","thumbtack"]') out.push('undo deletes ' + teAlt.texts());
          if (s.getText() !== 'Pass me a pencil. Drop this.' || !teAlt.inSync()) out.push('undo delete text ' + s.getText());
          ed.indicators.flush();
          if (teAlt.dotsOf(T.id) !== 3) out.push('indicator not back');
        "##,
        // Escape closes and returns focus to the surface.
        r##"
          teTest.key(teAlt.inputs()[1], 'Escape');
          if (teAlt.P().isOpen()) out.push('still open');
          if (teTest.visible(teAlt.P().root)) out.push('still visible');
          if (document.activeElement !== s.root) out.push('focus not returned');
          if (document.querySelector('#app').classList.contains('te-alt-open')) out.push('still dimmed');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
