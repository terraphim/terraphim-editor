//! Browser tests for the Lab popover and its marks (issue #14, spec R-8.1,
//! R-8.2). Run with `wasm-pack test --headless --chrome`. Real scripts, real
//! stylesheets, the real exported document and Lab API on
//! `window.wasmBindings` and the committed fixture
//! `tests/fixtures/lab/lab.md`; nothing is mocked. The expected marks are
//! computed in the same test by the `terraphim_lab` engine itself, so the
//! browser must show exactly what the engine returns. See `tests/web.rs` for
//! why the browser tests are split across binaries.

use super::*;

pub(super) const FIXTURE: &str = include_str!("../fixtures/lab/lab.md");

/// Fresh editor, fixture open, Write_On on, helpers on `window.teLab`.
pub(super) fn setup() {
    let _document = fresh_full_editor();
    let src = format!(
        r##"(() => {{
          window.teLab = {{
            fixture: {fixture},
            editor() {{ return window.__teEditor; }},
            lab() {{ return window.__teEditor.lab; }},
            popover() {{ return window.__teEditor.lab.root; }},
            pill() {{ return teTest.control('lab'); }},
            item(id) {{ return teLab.popover().querySelector('[data-action="' + id + '"]'); }},
            marks() {{ return teLab.editor().decorations.get('lab-marks'); }},
            triples() {{ return teLab.marks().map((m) => [m.start, m.end, m.data.kind]); }},
            markEl(id) {{
              return teLab.editor().surface.root.querySelector('[data-te-decoration~="lab-marks:' + id + '"]');
            }},
          }};
          const ed = window.__teEditor;
          ed.openDocument(teLab.fixture, 'lab-test-' + Math.random());
          ed.chrome.setMode('write-on');
          return ed.lab ? 'ok' : 'no lab';
        }})()"##,
        fixture = js_string_literal(FIXTURE),
    );
    assert_eq!(js_string(&src), "ok");
}
