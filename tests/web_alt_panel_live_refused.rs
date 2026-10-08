//! Browser tests for the alternatives panel, part six (issue #106): a new
//! alternative typed live that duplicates an existing one. The model refuses
//! it and keeps the last text it accepted; the line keeps what was typed;
//! Escape cancels; Enter does not keep refused text; leaving the line keeps
//! the accepted text, puts the line back to it and says so. Also the a/an
//! fix-up while typing, and IME composition waiting for its end. Split from
//! `web_alt_panel_live.rs` to keep each binary inside its time budget. Run
//! with `wasm-pack test --headless --chrome --test
//! web_alt_panel_live_refused`. Real scripts and the real document API;
//! nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_live_alternative_duplicate_is_refused() {
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
        // A duplicate is refused while typing: the model keeps the last text
        // it accepted, the line keeps what was typed, and Escape cancels.
        r##"
          const P = teAlt.P();
          teAlt.typeKeys(teAlt.newInput(), 'challenge');
          const line = document.activeElement;
          if (line.value !== 'challenge') out.push('line ' + JSON.stringify(line.value));
          if (s.getText() !== 'Much of the challeng in design.') out.push('model text ' + JSON.stringify(s.getText()));
          if (!/already|Not changed/.test(P.statusEl.textContent)) out.push('status ' + JSON.stringify(P.statusEl.textContent));
          teTest.key(line, 'Escape');
          if (s.getText() !== 'Much of the challenge in design.') out.push('after Escape ' + JSON.stringify(s.getText()));
          if (!teAlt.inSync()) out.push('model and surface differ');
        "##,
        // Enter does not keep refused text: the line stays live. Leaving it
        // then keeps what the model accepted, puts the line back to that and
        // says so; one undo takes it all back.
        r##"
          const P = teAlt.P();
          P.open({ spanId: teAlt.spans()[0].id });
          const before = s.historyIndex;
          teAlt.typeKeys(teAlt.newInput(), 'challenge');
          const line = document.activeElement;
          teTest.key(line, 'Enter');
          if (P.live === null) out.push('Enter kept refused text');
          if (document.activeElement !== line || line.value !== 'challenge') out.push('line after Enter ' + JSON.stringify(line.value));
          s.focus();
          if (P.live !== null) out.push('still live after leaving');
          if (s.getText() !== 'Much of the challeng in design.') out.push('kept ' + JSON.stringify(s.getText()));
          if (teAlt.inputs()[2].value !== 'challeng') out.push('line not put back: ' + JSON.stringify(teAlt.inputs()[2].value));
          if (!/kept “challeng”/.test(P.statusEl.textContent)) out.push('status ' + JSON.stringify(P.statusEl.textContent));
          if (s.historyIndex !== before + 1) out.push('steps ' + (s.historyIndex - before));
          s.undo();
          P.flush();
          if (s.getText() !== 'Much of the challenge in design.' || JSON.stringify(teAlt.texts()) !== '["tension","challenge"]') out.push('undo ' + JSON.stringify(s.getText()) + ' ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model and surface differ');
        "##,
        // The a/an fix-up applies live, and composition (IME) waits for its end.
        r##"
          s.setText('Pass me a paperclip.');
          s.focus();
          s.setSelectionOffsets(10, 19);
          teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          const input = teAlt.newInput();
          input.focus();
          input.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
          input.value = 'e';
          input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertCompositionText', data: 'e', isComposing: true }));
          if (s.getText() !== 'Pass me a paperclip.') out.push('changed while composing ' + JSON.stringify(s.getText()));
          input.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: 'e' }));
          if (s.getText() !== 'Pass me an e.') out.push('after composition ' + JSON.stringify(s.getText()));
          teAlt.typeKeys(document.activeElement, 'raser');
          if (s.getText() !== 'Pass me an eraser.') out.push('a/an ' + JSON.stringify(s.getText()));
          teTest.key(document.activeElement, 'Escape');
          if (s.getText() !== 'Pass me a paperclip.') out.push('Escape ' + JSON.stringify(s.getText()));
          if (!teAlt.inSync()) out.push('model and surface differ');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
