//! Browser tests for the alternatives side panel, part three (issue #106,
//! R-4.3): a new alternative typed in the add line is live and active while
//! it is typed (video 1:24-1:30), kept as one undo step on Enter, cancelled
//! by Escape or by emptying the line (duplicates are in
//! `web_alt_panel_live_refused.rs`; undo, leaving the line, the a/an fix-up
//! and IME composition in `web_alt_panel_live_undo.rs`). Typing goes through real
//! keydown, input and keyup events (`teAlt.typeKeys`). Its own binary, to
//! keep each inside its time budget. Run with `wasm-pack test --headless
//! --chrome --test web_alt_panel_live`. Real scripts and the real document
//! API; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_new_alternative_is_live_while_typed() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        // Ctrl+Shift+A on "tension": a pending word span, focus on the add
        // line. As in the video (1:24-1:30), each keystroke shows the new
        // alternative in the document, active, with its dot.
        r##"
          s.setText('Much of the tension in design.');
          s.focus();
          s.setSelectionOffsets(12, 19);
          teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          const P = teAlt.P();
          if (!P.isOpen() || !P.target.pending) return 'not pending: ' + JSON.stringify(P.target);
          const base = s.history.length;
          teAlt.typeKeys(teAlt.newInput(), 'c');
          if (s.getText() !== 'Much of the c in design.') return 'first key ' + JSON.stringify(s.getText());
          if (JSON.stringify(teAlt.texts()) !== '["tension","c"]' || teAlt.active() !== 1) out.push('lines ' + teAlt.texts() + ' active ' + teAlt.active());
          const span = teAlt.spans()[0];
          if (!span || teAlt.dotsOf(span.id) !== 2) out.push('dots ' + (span && teAlt.dotsOf(span.id)));
          const line = teAlt.inputs()[1];
          if (document.activeElement !== line) out.push('focus not on the live line');
          if (line.selectionStart !== 1) out.push('caret ' + line.selectionStart);
          teAlt.typeKeys(document.activeElement, 'ha;le');
          if (s.getText() !== 'Much of the cha;le in design.') out.push('typo ' + JSON.stringify(s.getText()));
          teAlt.backspace(3);
          teAlt.typeKeys(document.activeElement, 'llenge');
          if (s.getText() !== 'Much of the challenge in design.') out.push('typed ' + JSON.stringify(s.getText()));
          if (JSON.stringify(teAlt.texts()) !== '["tension","challenge"]' || teAlt.active() !== 1) out.push('after typing ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model and surface differ');
          if (P.live === null) out.push('not live while typing');
          T.base = base;
        "##,
        // Enter keeps it, as ONE undo step, and offers a fresh empty line.
        r##"
          const P = teAlt.P();
          if (!teTest.key(document.activeElement, 'Enter')) out.push('Enter not handled');
          if (P.live !== null) out.push('still live after Enter');
          if (document.activeElement !== teAlt.newInput() || teAlt.newInput().value !== '') out.push('focus after Enter');
          if (s.history.length !== T.base + 1) out.push('history grew by ' + (s.history.length - T.base));
          if (s.getText() !== 'Much of the challenge in design.') out.push('text ' + JSON.stringify(s.getText()));
        "##,
        // Escape cancels a second one: the previous active alternative and its
        // text come back, the list is unchanged and the panel closes.
        r##"
          const P = teAlt.P();
          const before = s.history.length;
          teAlt.typeKeys(teAlt.newInput(), 'struggle');
          if (s.getText() !== 'Much of the struggle in design.') out.push('live ' + JSON.stringify(s.getText()));
          teTest.key(document.activeElement, 'Escape');
          if (s.getText() !== 'Much of the challenge in design.') out.push('Escape left ' + JSON.stringify(s.getText()));
          if (P.isOpen()) out.push('panel still open');
          if (document.activeElement !== s.root) out.push('focus not back on the text');
          const span = teAlt.spans()[0];
          if (JSON.stringify(span.alts.map((a) => a.text)) !== '["tension","challenge"]' || span.active !== 1) out.push('span ' + JSON.stringify(span));
          if (s.history.length !== before) out.push('Escape left history ' + (s.history.length - before));
          if (!teAlt.inSync()) out.push('model and surface differ');
        "##,
        // Emptying the line cancels it too and keeps what is left there; a
        // space alone starts nothing.
        r##"
          const P = teAlt.P();
          P.open({ spanId: teAlt.spans()[0].id });
          teAlt.typeKeys(teAlt.newInput(), 'xy');
          if (s.getText() !== 'Much of the xy in design.') out.push('live ' + JSON.stringify(s.getText()));
          teAlt.backspace(2);
          if (s.getText() !== 'Much of the challenge in design.') out.push('emptied ' + JSON.stringify(s.getText()));
          if (document.activeElement !== teAlt.newInput()) out.push('focus not on the add line');
          if (JSON.stringify(teAlt.texts()) !== '["tension","challenge"]') out.push('lines ' + teAlt.texts());
          teAlt.typeKeys(teAlt.newInput(), ' ');
          if (P.live !== null || JSON.stringify(teAlt.texts()) !== '["tension","challenge"]') out.push('a space started an addition');
          teAlt.backspace(1);
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
