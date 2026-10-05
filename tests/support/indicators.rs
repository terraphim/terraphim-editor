//! Fixture and geometry helpers shared by the inline-indicator browser
//! binaries (`tests/web_indicators.rs` and
//! `tests/web_indicators_registry.rs`, issue #8).

use super::*;

pub const FIXTURE: &str = include_str!("../fixtures/indicators/indicators.md");

/// Geometry tolerance in CSS pixels.
pub const TOLERANCE: &str = "1.5";

/// Install the fixture and small geometry helpers as `window.teInd`.
pub fn install_helpers() {
    let src = format!(
        r##"(() => {{
          window.teInd = {{
            fixture: {fixture},
            tol: {TOLERANCE},
            editor() {{ return window.__teEditor; }},
            layer() {{ return window.__teEditor.indicators; }},
            // Open `md` (the fixture by default) under a fresh document key
            // and switch to Write_On mode.
            open(md, writeOn) {{
              const ed = window.__teEditor;
              ed.openDocument(md || teInd.fixture, 'indicators-test-' + Math.random());
              if (writeOn !== false) ed.chrome.setMode('write-on');
              ed.indicators.flush();
            }},
            span(id) {{ return teInd.editor().annotations().spans.find((s) => s.id === id); }},
            // Client rects of a span's text, from the surface itself.
            rects(id) {{
              const s = teInd.span(id);
              const r = teInd.editor().surface.rangeForOffsets(s.anchor.start, s.anchor.end);
              return Array.from(r.getClientRects()).filter((x) => x.width > 0 && x.height > 0);
            }},
            holder(id) {{ return document.querySelector('.te-indicators [data-span-id="' + id + '"]'); }},
            dots(id) {{ return Array.from(teInd.holder(id).querySelectorAll('.te-ind-dot')); }},
            lit(id) {{ return teInd.dots(id).findIndex((d) => d.classList.contains('te-ind-dot--active')); }},
            // Bounding box of the dots of a span.
            dotBox(id) {{
              const rs = teInd.dots(id).map((d) => d.getBoundingClientRect());
              return {{
                left: Math.min(...rs.map((r) => r.left)), right: Math.max(...rs.map((r) => r.right)),
                top: Math.min(...rs.map((r) => r.top)), bottom: Math.max(...rs.map((r) => r.bottom)),
              }};
            }},
            decorationEl(id) {{
              return teInd.editor().surface.root.querySelector('[data-te-decoration~="indicators:' + id + '"]');
            }},
            near(a, b) {{ return Math.abs(a - b) <= teInd.tol; }},
          }};
          return 'ok';
        }})()"##,
        fixture = js_string_literal(FIXTURE),
    );
    assert_eq!(js_string(&src), "ok");
}

pub fn setup() {
    let _document = fresh_full_editor();
    install_helpers();
}
