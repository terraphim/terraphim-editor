//! Browser tests for the alternatives side panel, part four (issue #106,
//! R-4.3): a new alternative typed live is one undo step, from the panel
//! and from the document; leaving the line or changing the tab keeps it
//! (the a/an fix-up and IME composition are in `web_alt_panel_live_refused.rs`,
//! the history limit in `web_alt_panel_live_limit.rs`).
//! Typing goes through real keydown, input and keyup events
//! (`teAlt.typeKeys`). Split from `web_alt_panel_live.rs` to keep each
//! binary inside its time budget. Run with `wasm-pack test --headless
//! --chrome --test web_alt_panel_live_undo`. Real scripts and the real
//! document API; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_live_alternative_undo_leave_and_composition() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        // "challenge" typed live on a pending span and kept with Enter.
        r##"
          s.setText('Much of the tension in design.');
          s.focus();
          s.setSelectionOffsets(12, 19);
          teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          teAlt.typeKeys(teAlt.newInput(), 'challenge');
          teTest.key(document.activeElement, 'Enter');
          if (s.getText() !== 'Much of the challenge in design.') return 'setup ' + JSON.stringify(s.getText());
        "##,
        // ArrowUp from the live line keeps it and makes the line above active;
        // undo then takes back the swap, and one more undo the whole addition.
        r##"
          const P = teAlt.P();
          P.open({ spanId: teAlt.spans()[0].id });
          teAlt.typeKeys(teAlt.newInput(), 'friction');
          teTest.key(document.activeElement, 'ArrowUp');
          if (s.getText() !== 'Much of the challenge in design.') out.push('ArrowUp ' + JSON.stringify(s.getText()));
          if (JSON.stringify(teAlt.texts()) !== '["tension","challenge","friction"]' || teAlt.active() !== 1) out.push('lines ' + teAlt.texts() + ' ' + teAlt.active());
          teTest.key(document.activeElement, 'z', { ctrlKey: true });
          if (s.getText() !== 'Much of the friction in design.') out.push('undo swap ' + JSON.stringify(s.getText()));
          teTest.key(document.activeElement, 'z', { ctrlKey: true });
          if (s.getText() !== 'Much of the challenge in design.') out.push('undo addition ' + JSON.stringify(s.getText()));
          if (JSON.stringify(teAlt.texts()) !== '["tension","challenge"]') out.push('after undo ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model and surface differ');
          P.close({ restoreFocus: true });
        "##,
        // From the document: one Ctrl+Z removes the first addition with all
        // its keystrokes (no span is left), and redo brings it back active.
        r##"
          s.focus();
          teTest.key(s.root, 'z', { ctrlKey: true });
          if (s.getText() !== 'Much of the tension in design.') out.push('undo ' + JSON.stringify(s.getText()));
          if (teAlt.spans().length !== 0) out.push('spans left ' + teAlt.spans().length);
          teTest.key(s.root, 'z', { ctrlKey: true, shiftKey: true });
          if (s.getText() !== 'Much of the challenge in design.') out.push('redo ' + JSON.stringify(s.getText()));
          const span = teAlt.spans()[0];
          if (!span || span.active !== 1 || span.alts[1].text !== 'challenge') out.push('redo span ' + JSON.stringify(span));
          if (!teAlt.inSync()) out.push('model and surface differ');
        "##,
        // Leaving the line (focus to the text) or changing the tab keeps the
        // addition, each as one undo step.
        r##"
          const P = teAlt.P();
          P.open({ spanId: teAlt.spans()[0].id });
          const before = s.historyIndex;
          teAlt.typeKeys(teAlt.newInput(), 'grit');
          s.focus();
          if (P.live !== null) out.push('still live after leaving');
          if (s.getText() !== 'Much of the grit in design.' || s.historyIndex !== before + 1) out.push('blur ' + JSON.stringify(s.getText()) + ' ' + (s.historyIndex - before));
          P.open({ spanId: teAlt.spans()[0].id });
          teAlt.typeKeys(teAlt.newInput(), 'tug');
          P.setTab('sentence');
          if (P.live !== null) out.push('still live after the tab change');
          if (s.getText() !== 'Much of the tug in design.' || s.historyIndex !== before + 2) out.push('tab ' + JSON.stringify(s.getText()) + ' ' + (s.historyIndex - before));
          if (!teAlt.inSync()) out.push('model and surface differ');
          P.close({ restoreFocus: false });
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
