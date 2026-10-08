//! Helpers for the Overflow panel browser tests (issue #12), shared by
//! `web_overflow.rs` and `web_overflow_layout.rs`.

use super::{js_string, js_string_literal};

/// Installs `window.__teSrc` and the small helpers the steps share.
pub fn install_overflow_helpers(src: &str) {
    let script = format!(
        r##"window.__teSrc = {src};
        window.teOv = {{
          api() {{ return window.wasmBindings; }},
          // Live and set-aside annotation counts, read from the model.
          where() {{
            const a = window.wasmBindings.document_annotations();
            return 'spans=' + a.spans.length + ' ghosts=' + a.ghosts.length +
              ' aside=' + (a.setAside.spans.length + a.setAside.ghosts.length);
          }},
          type(area, value) {{
            area.value = value;
            area.dispatchEvent(new InputEvent('input', {{ bubbles: true }}));
          }},
        }};
        'ok'"##,
        src = js_string_literal(src)
    );
    assert_eq!(js_string(&script), "ok");
}
