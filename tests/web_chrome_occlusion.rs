//! Real browser hit testing for corner controls while side panels are open.
#![cfg(target_arch = "wasm32")]

mod support;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn save_and_open_remain_pointer_targets_above_the_alternatives_panel() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          ed.chrome.setMode('write-on');
          ed.altPanel.open();
          if (!ed.altPanel.isOpen()) return 'panel did not open';
          const errors = [];
          for (const name of ['save', 'open']) {
            const button = teTest.control(name);
            const rect = button.getBoundingClientRect();
            const hit = document.elementFromPoint(rect.x + rect.width/2, rect.y + rect.height/2);
            if (!hit || !button.contains(hit)) errors.push(name + ' covered by ' + hit?.className);
          }
          return errors.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
