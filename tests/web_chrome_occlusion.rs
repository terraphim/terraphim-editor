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

#[wasm_bindgen_test]
fn counter_remains_a_pointer_target_with_alternatives_and_overflow_open() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          const counter = ed.chrome.counter;
          const errors = [];
          const hit = element => {
            const rect = element.getBoundingClientRect();
            return document.elementFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2);
          };
          for (const panel of [ed.altPanel, ed.overflow]) {
            ed.chrome.setMode('write-on');
            panel.open();
            if (!panel.isOpen()) errors.push('panel did not open');
            const panelControls = panel === ed.altPanel
              ? Object.values(ed.altPanel.tabs) : [ed.overflow.closeButton];
            for (const control of panelControls) {
              const panelTarget = hit(control);
              if (!panelTarget || !control.contains(panelTarget)) errors.push('panel control covered');
            }
            const target = hit(counter);
            if (!target || !counter.contains(target)) {
              errors.push('counter covered by ' + target?.className);
              panel.close();
              continue;
            }
            // Dispatch to the actual browser hit target, not directly to a
            // covered button. This is a DOM pointer contract, not trusted UI.
            target.click();
            if (counter.getAttribute('aria-pressed') !== 'false') errors.push('counter click did not leave Write_On');
            if (panel.isOpen()) errors.push('counter mode change left panel open');
          }
          return errors.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
