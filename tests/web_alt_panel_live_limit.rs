//! Browser tests for the alternatives panel, part five (issue #106): a live
//! addition and the surface's history limit. History trimming waits from
//! before the first step until the addition is folded, so a long addition
//! is still one undo step and Escape still cancels it (the history at
//! capacity is in `web_alt_panel_live_capacity.rs`). Split from
//! `web_alt_panel_live_undo.rs` to keep
//! each binary inside its time budget. Run with `wasm-pack test --headless
//! --chrome --test web_alt_panel_live_limit`. Real scripts and the real
//! document API; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_live_alternative_and_the_history_limit() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        // "challenge" added to a pending span and made active.
        r##"
          s.setText('Much of the tension in design.');
          s.focus();
          s.setSelectionOffsets(12, 19);
          teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          // Set without typing (one model step): this binary is about the
          // history limit, not typing.
          teAlt.newInput().focus();
          teAlt.newInput().value = 'challenge';
          teTest.key(teAlt.newInput(), 'Enter');
          if (s.getText() !== 'Much of the challenge in design.') return 'setup ' + JSON.stringify(s.getText());
        "##,
        // A live addition longer than the history limit (five keys, limit 3)
        // still folds into one
        // step, and Escape still cancels it; the limit applies again after.
        r##"
          const P = teAlt.P();
          P.open({ spanId: teAlt.spans()[0].id });
          const was = s.getText();
          const limit = s.historyLimit;
          s.historyLimit = 3;
          teAlt.typeKeys(teAlt.newInput(), 'abcde');
          if (!s.getText().includes(' abcde ')) out.push('live ' + JSON.stringify(s.getText()));
          teTest.key(document.activeElement, 'Escape');
          if (s.getText() !== was) out.push('long Escape ' + JSON.stringify(s.getText()));
          if (s.historyLimit !== 3) out.push('limit ' + s.historyLimit);
          P.open({ spanId: teAlt.spans()[0].id });
          teAlt.typeKeys(teAlt.newInput(), 'klmno');
          teTest.key(document.activeElement, 'Enter');
          if (s.history.length > 3) out.push('history not trimmed: ' + s.history.length);
          s.undo();
          P.flush();
          if (s.getText() !== was) out.push('long undo ' + JSON.stringify(s.getText()));
          if (!teAlt.inSync()) out.push('model and surface differ');
          s.historyLimit = limit;
          P.close({ restoreFocus: false });
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
