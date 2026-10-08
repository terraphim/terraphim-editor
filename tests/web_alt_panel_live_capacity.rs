//! Browser tests for the alternatives panel, part seven (issue #106): a live
//! addition started with the history at capacity. After resetHistory() with
//! a limit of 2, the state before the addition is the oldest entry; trimming
//! waits from before the first step, so Escape still restores and one Ctrl+Z
//! still undoes, for typed text and for text set without typing. Split from
//! `web_alt_panel_live_limit.rs` to keep each binary inside its time budget.
//! Run with `wasm-pack test --headless --chrome --test
//! web_alt_panel_live_capacity`. Real scripts and the real document API;
//! nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_live_alternative_with_the_history_at_capacity() {
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
        // At capacity: after resetHistory() the state before the addition is
        // the oldest entry, and a limit of 2 would shift it out at the second
        // step unless trimming waits from the first. Escape still restores,
        // Enter is still one step that one Ctrl+Z takes back; the same for
        // text set without typing.
        r##"
          const P = teAlt.P();
          P.open({ spanId: teAlt.spans()[0].id });
          const was = s.getText();
          const limit = s.historyLimit;
          s.resetHistory();
          s.historyLimit = 2;
          teAlt.typeKeys(teAlt.newInput(), 'abc');
          teTest.key(document.activeElement, 'Escape');
          if (s.getText() !== was) out.push('Escape at capacity ' + JSON.stringify(s.getText()));
          P.open({ spanId: teAlt.spans()[0].id });
          teAlt.typeKeys(teAlt.newInput(), 'def');
          teTest.key(document.activeElement, 'Enter');
          if (!s.getText().includes(' def ')) out.push('Enter ' + JSON.stringify(s.getText()));
          if (s.history.length > 2) out.push('not trimmed ' + s.history.length);
          teTest.key(document.activeElement, 'z', { ctrlKey: true });
          if (s.getText() !== was) out.push('one undo at capacity ' + JSON.stringify(s.getText()));
          s.resetHistory();
          teAlt.newInput().focus();
          teAlt.newInput().value = 'ghi';
          teTest.key(teAlt.newInput(), 'Enter');
          if (!s.getText().includes(' ghi ')) out.push('set value ' + JSON.stringify(s.getText()));
          teTest.key(document.activeElement, 'z', { ctrlKey: true });
          if (s.getText() !== was) out.push('set value undo ' + JSON.stringify(s.getText()));
          if (s.historyLimit !== 2) out.push('limit ' + s.historyLimit);
          if (!teAlt.inSync()) out.push('model and surface differ');
          s.historyLimit = limit;
          P.close({ restoreFocus: false });
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
