//! Knowledge-graph alternatives fixture and helpers (issue #13), shared by
//! `tests/web_kg.rs` and `tests/web_kg_menu.rs`. The committed fixture
//! thesaurus is loaded through the real `MarkdownEditor.loadThesaurus`.

use super::*;

/// Fresh editor in Write_On mode with `source` open, the fixture thesaurus
/// loaded and helpers on `window.teKg`. Returns "ok".
pub fn setup(source: &str) {
    let _document = fresh_full_editor();
    let script = format!(
        r##"(() => {{
          const ed = window.__teEditor;
          const api = window.wasmBindings;
          ed.clearThesaurus();
          ed.openDocument({src}, 'kg-test-' + Math.random());
          ed.chrome.setMode('write-on');
          const loaded = ed.loadThesaurus({thesaurus});
          if (loaded.name !== 'writing-fixture') return 'thesaurus ' + JSON.stringify(loaded);
          window.teKg = {{
            ed, api,
            // The sentence line of the body.
            line() {{ return ed.surface.getText().split('\n')[2]; }},
            // Dots of span `id` after the indicator refresh: count, lit index.
            dots(id) {{
              ed.indicators.flush();
              const dots = Array.from(document.querySelectorAll('.te-indicators [data-span-id="' + id + '"] .te-ind-dot'));
              return dots.length + '/' + dots.findIndex((d) => d.classList.contains('te-ind-dot--active'));
            }},
            // Line, dots of `id`, and whether model and surface agree.
            state(id) {{
              const sync = api.document_body() === ed.surface.getText() ? '' : ' DRIFT';
              return teKg.line() + ' | ' + teKg.dots(id) + sync;
            }},
            span(id) {{ return ed.surface.root.querySelector('[data-te-decoration~="indicators:' + id + '"]'); }},
            hover(id) {{
              const el = teKg.span(id);
              const r = el.getClientRects()[0];
              const init = {{ bubbles: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 }};
              el.dispatchEvent(new MouseEvent('mouseover', init));
              el.dispatchEvent(new MouseEvent('mousemove', init));
            }},
            key(key, opts) {{ return teTest.key(ed.surface.root, key, opts); }},
            dot(id, i) {{
              ed.indicators.flush();
              const dot = document.querySelectorAll('.te-indicators [data-span-id="' + id + '"] .te-ind-dot')[i];
              dot.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true }}));
            }},
            // UTF-16 offset of `needle` in the surface text.
            at(needle) {{ return ed.surface.getText().indexOf(needle); }},
          }};
          return 'ok';
        }})()"##,
        src = js_string_literal(source),
        thesaurus = js_string_literal(KG_THESAURUS),
    );
    assert_eq!(js_string(&script), "ok");
}

/// Run `body` (statements using `out`, `teKg`, `ed` and `expect(label, id,
/// want)`) after [`setup`]; returns the problems found, joined.
pub fn run(source: &str, body: &str) -> String {
    setup(source);
    js_string(&format!(
        r##"(() => {{
          const out = [];
          const ed = teKg.ed;
          const expect = (label, id, want) => {{
            const got = teKg.state(id);
            if (got !== want) out.push(label + ': ' + got);
          }};
          {body}
          return out.join('; ');
        }})()"##
    ))
}
