//! Browser tests for in-place cycling of alternatives (issue #9, spec R-2.4,
//! R-2.6, R-3.3, R-3.6): hovering a span and pressing ArrowUp/ArrowDown,
//! Alt+ArrowUp/Alt+ArrowDown with the caret in a span, and clicking a dot
//! swap the active alternative through the real exported
//! `set_active_alternative`, fixing a preceding "a"/"an", as one undo step
//! whose undo and redo replay the swap in the model. Run with `wasm-pack test
//! --headless --chrome`. Real scripts, real stylesheets and the real
//! document API; nothing is mocked. See `tests/web.rs` for why the browser
//! tests are split across binaries.

use super::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

/// "Pass me a paperclip. Drop this.\n\nSecond line.\n" with span `s1` over
/// "paperclip" (alternatives "eraser" and "thumbtack") and ghost `g1` over
/// " Drop this.", built through the crate so the fixture is always valid.
fn fixture() -> String {
    let mut doc = Document::new("Pass me a paperclip. Drop this.\n\nSecond line.\n");
    let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
    doc.add_alternative(&id, "eraser", Source::Human, None)
        .unwrap();
    doc.add_alternative(&id, "thumbtack", Source::Human, None)
        .unwrap();
    doc.ghost(20, 31).unwrap();
    write(&doc)
}

/// Fresh editor in Write_On mode with the fixture open, plus helpers on
/// `window.teCyc`.
fn setup() {
    let _document = fresh_full_editor();
    let script = format!(
        r##"(() => {{
          const ed = window.__teEditor;
          const api = window.wasmBindings;
          ed.openDocument({src}, 'cycling-test-' + Math.random());
          ed.chrome.setMode('write-on');
          ed.indicators.flush();
          window.teCyc = {{
            ed, api,
            // The model read directly: ed.annotations() re-syncs by text
            // first, which would hide a model that drifted from the surface.
            model() {{
              const a = api.document_annotations();
              const s = a.spans.map((x) => x.id + '@' + x.anchor.start + '#' + x.active);
              const g = a.ghosts.map((x) => x.id + '@' + x.anchor.start);
              return s.concat(g).join(' ') + ' aside=' + (a.setAside.spans.length + a.setAside.ghosts.length);
            }},
            // Text, model and lit dot after the indicator refresh.
            state() {{
              ed.indicators.flush();
              const dots = Array.from(document.querySelectorAll('.te-indicators [data-span-id="s1"] .te-ind-dot'));
              const lit = dots.findIndex((d) => d.classList.contains('te-ind-dot--active'));
              const sync = api.document_body() === ed.surface.getText() ? '' : ' DRIFT';
              return ed.surface.getText().split('.')[0] + ' | ' + teCyc.model() + ' | lit=' + lit + sync;
            }},
            // The pointer is placed over the centre of the span's first
            // rendered box, with real client coordinates: a plain arrow
            // re-checks them against the live layout before swapping.
            hover(id) {{
              const el = ed.surface.root.querySelector('[data-te-decoration~="indicators:' + id + '"]');
              const r = el.getClientRects()[0];
              teCyc.pointAt(el, r.left + r.width / 2, r.top + r.height / 2);
            }},
            // Move the pointer to (x, y) within the surface, over `el`.
            pointAt(el, x, y) {{
              const init = {{ bubbles: true, clientX: x, clientY: y }};
              el.dispatchEvent(new MouseEvent('mouseover', init));
              el.dispatchEvent(new MouseEvent('mousemove', init));
            }},
            unhover() {{
              ed.surface.root.dispatchEvent(new MouseEvent('mouseleave'));
            }},
            key(key, opts) {{ return teTest.key(ed.surface.root, key, opts); }},
            dot(i) {{
              const dot = document.querySelectorAll('.te-indicators [data-span-id="s1"] .te-ind-dot')[i];
              dot.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true }}));
            }},
          }};
          return 'ok';
        }})()"##,
        src = js_string_literal(&fixture()),
    );
    assert_eq!(js_string(&script), "ok");
}

pub(super) const PAPERCLIP: &str = "Pass me a paperclip | s1@10#0 g1@20 aside=0 | lit=0";
pub(super) const ERASER: &str = "Pass me an eraser | s1@11#1 g1@18 aside=0 | lit=1";
pub(super) const THUMBTACK: &str = "Pass me a thumbtack | s1@10#2 g1@20 aside=0 | lit=2";

/// Run `body` (JavaScript statements using `out`, `teCyc` and `ed`) after
/// setup; it returns the problems found, joined.
pub(super) fn run(body: &str) -> String {
    setup();
    js_string(&format!(
        r##"(() => {{
          const out = [];
          const ed = teCyc.ed;
          const expect = (label, want) => {{
            const got = teCyc.state();
            if (got !== want) out.push(label + ': ' + got);
          }};
          {body}
          return out.join('; ');
        }})()"##
    ))
}
