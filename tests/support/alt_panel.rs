//! Helpers for the alternatives panel browser tests (issue #10), shared by
//! `web_alt_panel.rs` and `web_alt_panel_keys.rs`.

use super::js_string;

/// Shared helpers for the steps, as `window.teAlt`.
pub const ALT_PANEL_HELPERS_JS: &str = r##"
window.teAlt = {
  P() { return window.__teEditor.altPanel; },
  texts() { return teAlt.P().lines().map((l) => l.text); },
  active() { const l = teAlt.P().lines(); return l.findIndex((x) => x.active); },
  inputs() { return Array.from(teAlt.P().list.querySelectorAll('.te-alt-input')); },
  newInput() { const i = teAlt.inputs(); return i[i.length - 1]; },
  spans() { return window.__teEditor.annotations().spans; },
  // Model and surface agree (the panel's one hard rule).
  inSync() {
    const ed = window.__teEditor;
    return ed.documentApi().document_body() === ed.surface.getText();
  },
  type(input, value) {
    input.focus();
    input.value = value;
    return teTest.key(input, 'Enter');
  },
  dotsOf(id) {
    return document.querySelectorAll('.te-indicators [data-span-id="' + id + '"] .te-ind-dot').length;
  },
};
'ok'
"##;

/// Install `window.teAlt` (the alternatives panel helpers, issue #10).
pub fn install_alt_panel_helpers() {
    assert_eq!(js_string(ALT_PANEL_HELPERS_JS), "ok");
}
