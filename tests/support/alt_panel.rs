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
  // Type `text` into `input` key by key, as the browser does: keydown, then
  // (unless cancelled) the character inserted at the caret and an `input`
  // event (insertText), then keyup.
  typeKeys(input, text) {
    input.focus();
    for (const ch of text) {
      const target = document.activeElement;
      const down = new KeyboardEvent('keydown', { key: ch, bubbles: true, cancelable: true });
      target.dispatchEvent(down);
      if (!down.defaultPrevented) {
        const a = target.selectionStart;
        const b = target.selectionEnd;
        target.value = target.value.slice(0, a) + ch + target.value.slice(b);
        target.setSelectionRange(a + ch.length, a + ch.length);
        target.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: ch }));
      }
      target.dispatchEvent(new KeyboardEvent('keyup', { key: ch, bubbles: true }));
    }
  },
  // Backspace `n` times in the focused input.
  backspace(n = 1) {
    for (let i = 0; i < n; i += 1) {
      const target = document.activeElement;
      const down = new KeyboardEvent('keydown', { key: 'Backspace', bubbles: true, cancelable: true });
      target.dispatchEvent(down);
      if (!down.defaultPrevented) {
        const a = target.selectionStart;
        const b = target.selectionEnd;
        const from = a === b ? Math.max(0, a - 1) : a;
        target.value = target.value.slice(0, from) + target.value.slice(b);
        target.setSelectionRange(from, from);
        target.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'deleteContentBackward' }));
      }
      target.dispatchEvent(new KeyboardEvent('keyup', { key: 'Backspace', bubbles: true }));
    }
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
