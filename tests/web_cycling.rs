//! Browser tests for in-place cycling of alternatives (issue #9, spec R-2.4,
//! R-2.6, R-3.3, R-3.6): hovering a span and pressing ArrowUp/ArrowDown,
//! Alt+ArrowUp/Alt+ArrowDown with the caret in a span, and clicking a dot
//! swap the active alternative through the real exported
//! `set_active_alternative`, fixing a preceding "a"/"an", as one undo step
//! whose undo and redo replay the swap in the model. Run with `wasm-pack test
//! --headless --chrome`. Real scripts, real stylesheets and the real
//! document API; nothing is mocked. See `tests/web.rs` for why the browser
//! tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

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

const PAPERCLIP: &str = "Pass me a paperclip | s1@10#0 g1@20 aside=0 | lit=0";
const ERASER: &str = "Pass me an eraser | s1@11#1 g1@18 aside=0 | lit=1";
const THUMBTACK: &str = "Pass me a thumbtack | s1@10#2 g1@20 aside=0 | lit=2";

/// Run `body` (JavaScript statements using `out`, `teCyc` and `ed`) after
/// setup; it returns the problems found, joined.
fn run(body: &str) -> String {
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

#[wasm_bindgen_test]
fn test_hover_and_arrows_cycle_with_wrap_and_one_undo_step_each() {
    let body = format!(
        r##"
          expect('opened', {PAPERCLIP:?});
          const depth = ed.surface.historyIndex;
          // Without hover, plain arrows are left to the browser.
          ed.surface.focus();
          ed.surface.setSelectionOffsets(3);
          if (teCyc.key('ArrowDown') || teCyc.key('ArrowUp')) out.push('unhovered arrow prevented');
          expect('unhovered', {PAPERCLIP:?});

          teCyc.hover('s1');
          if (!teCyc.key('ArrowDown')) out.push('hovered ArrowDown not prevented');
          expect('down 1', {ERASER:?});
          // The caret before the span stays where it was.
          if (ed.surface.getSelectionOffsets().start !== 3) out.push('caret moved');
          // The pointer has not moved and the span was re-rendered: the
          // hover is kept by span id.
          teCyc.key('ArrowDown');
          expect('down 2', {THUMBTACK:?});
          teCyc.key('ArrowDown');
          expect('down wraps', {PAPERCLIP:?});
          teCyc.key('ArrowUp');
          expect('up wraps', {THUMBTACK:?});
          if (ed.surface.historyIndex !== depth + 4) out.push('history ' + (ed.surface.historyIndex - depth));
          // Modified arrows and other keys are not taken while hovering.
          if (teCyc.key('ArrowUp', {{ shiftKey: true }}) || teCyc.key('ArrowLeft')) out.push('other key prevented');

          // Leaving the span ends the hover.
          teCyc.unhover();
          if (teCyc.key('ArrowDown')) out.push('arrow after leaving prevented');
          expect('after leave', {THUMBTACK:?});

          // Each swap is one undo step; undo walks back through them.
          ed.surface.undo();
          expect('undo 1', {PAPERCLIP:?});
          ed.surface.undo();
          expect('undo 2', {THUMBTACK:?});
          ed.surface.undo();
          expect('undo 3', {ERASER:?});
          ed.surface.undo();
          expect('undo 4', {PAPERCLIP:?});
          if (ed.surface.historyIndex !== depth) out.push('undo depth');
        "##
    );
    assert_eq!(run(&body), "");
}

#[wasm_bindgen_test]
fn test_article_fix_up_sequence_with_undo_redo_and_typing() {
    let body = format!(
        r##"
          // The a/an demo sequence (R-2.6): a paperclip, an eraser, a thumbtack.
          teCyc.hover('s1');
          teCyc.key('ArrowDown');
          expect('an eraser', {ERASER:?});
          teCyc.key('ArrowDown');
          expect('a thumbtack', {THUMBTACK:?});
          // Undo restores the previous alternative and its article.
          ed.surface.undo();
          expect('undo', {ERASER:?});
          ed.surface.undo();
          expect('undo again', {PAPERCLIP:?});
          ed.surface.redo();
          expect('redo', {ERASER:?});
          ed.surface.redo();
          expect('redo again', {THUMBTACK:?});
          ed.surface.undo();
          expect('undo after redo', {ERASER:?});

          // Typing after a swap, then undoing both, unwinds in order.
          ed.surface.replaceRange(0, 0, 'Now ', {{ source: 'api' }});
          if (teCyc.model() !== 's1@15#1 g1@22 aside=0') out.push('typed ' + teCyc.model());
          ed.surface.undo();
          ed.surface.undo();
          expect('undo typing and swap', {PAPERCLIP:?});
          if (teCyc.api.save_document() !== {src}) out.push('saved file differs from the original');
        "##,
        src = js_string_literal(&fixture()),
    );
    assert_eq!(run(&body), "");
}

#[wasm_bindgen_test]
fn test_dot_click_jumps_and_alt_arrows_cycle_from_the_caret() {
    let body = format!(
        r##"
          // Clicking dot i jumps straight to alternative i (R-3.6, inferred).
          const depth = ed.surface.historyIndex;
          teCyc.dot(2);
          expect('dot 2', {THUMBTACK:?});
          teCyc.dot(2);
          if (ed.surface.historyIndex !== depth + 1) out.push('active dot recorded a step');
          teCyc.dot(1);
          expect('dot 1', {ERASER:?});
          // A cancelled te:dot does nothing.
          const cancel = (e) => e.preventDefault();
          document.addEventListener('te:dot', cancel);
          teCyc.dot(0);
          document.removeEventListener('te:dot', cancel);
          expect('cancelled', {ERASER:?});

          // Keyboard users: Alt+arrows with the caret inside the span, or at
          // its end, cycle without any hover.
          teCyc.unhover();
          ed.surface.focus();
          ed.surface.setSelectionOffsets(13);
          if (!teCyc.key('ArrowDown', {{ altKey: true }})) out.push('Alt+ArrowDown not prevented');
          expect('alt down', {THUMBTACK:?});
          const sel = ed.surface.getSelectionOffsets();
          if (sel.start !== 13 || sel.end !== 13) out.push('caret ' + JSON.stringify(sel));
          ed.surface.setSelectionOffsets(19);
          teCyc.key('ArrowDown', {{ altKey: true }});
          expect('alt down at the end', {PAPERCLIP:?});
          teCyc.key('ArrowUp', {{ altKey: true }});
          expect('alt up wraps', {THUMBTACK:?});
          // Outside any span Alt+arrows are left to the browser.
          ed.surface.setSelectionOffsets(2);
          if (teCyc.key('ArrowDown', {{ altKey: true }})) out.push('Alt+ArrowDown outside prevented');
          // A selection reaching past the span is not "in" it.
          ed.surface.setSelectionOffsets(5, 15);
          if (teCyc.key('ArrowDown', {{ altKey: true }})) out.push('Alt+ArrowDown over a wider selection prevented');
          expect('outside', {THUMBTACK:?});

          // In plain mode (no indicators) nothing is taken.
          ed.chrome.setMode('plain');
          ed.surface.setSelectionOffsets(13);
          if (teCyc.key('ArrowDown', {{ altKey: true }})) out.push('plain mode Alt+ArrowDown prevented');
          ed.chrome.setMode('write-on');
        "##
    );
    assert_eq!(run(&body), "");
}

#[wasm_bindgen_test]
fn test_save_reopen_keeps_the_active_alternative_and_refused_swaps_change_nothing() {
    let body = format!(
        r##"
          teCyc.dot(1);
          const saved = ed.saveDocument();
          const opened = ed.openDocument(saved, 'cycling-reopen-' + Math.random());
          if (opened.unresolved !== 0 || opened.warning) out.push('reopen ' + JSON.stringify(opened));
          ed.chrome.setMode('write-on');
          expect('reopened', {ERASER:?});
          if (ed.exportDocument() !== 'Pass me an eraser.\n\nSecond line.\n') out.push('export ' + JSON.stringify(ed.exportDocument()));
          // Cycling continues from the saved active index.
          teCyc.hover('s1');
          teCyc.key('ArrowDown');
          expect('cycled after reopen', {THUMBTACK:?});

          // A swap the model refuses throws and changes nothing.
          const depth = ed.surface.historyIndex;
          for (const [id, i] of [['s1', 9], ['s9', 1]]) {{
            let threw = false;
            try {{ ed.swapAlternative(id, i); }} catch (err) {{ threw = true; }}
            if (!threw) out.push('swap ' + id + '/' + i + ' did not throw');
          }}
          // Making the active alternative active again records nothing.
          const same = ed.swapAlternative('s1', 2);
          if (same.edit !== null) out.push('same index edit ' + JSON.stringify(same.edit));
          if (ed.surface.historyIndex !== depth) out.push('refused swaps recorded');
          expect('refused', {THUMBTACK:?});
        "##
    );
    assert_eq!(run(&body), "");
}

/// "Give me a eraser." (an article typed against the a/an rule) with span
/// `s1` over "eraser" and alternative "thumbtack".
fn ungrammatical_fixture() -> String {
    let mut doc = Document::new("Give me a eraser.\n");
    let id = doc.add_span(SpanKind::Word, 10, 16).unwrap();
    doc.add_alternative(&id, "thumbtack", Source::Human, None)
        .unwrap();
    write(&doc)
}

#[wasm_bindgen_test]
fn test_undo_to_an_ungrammatical_article_keeps_model_and_surface_equal() {
    let body = format!(
        r##"
          const api = teCyc.api;
          ed.openDocument({src}, 'cycling-article-' + Math.random());
          ed.chrome.setMode('write-on');
          ed.indicators.flush();
          ed.swapAlternative('s1', 1);
          if (ed.surface.getText() !== 'Give me a thumbtack.\n') out.push('swap ' + JSON.stringify(ed.surface.getText()));
          // Undo restores the author's text exactly; the model, which would
          // derive "an eraser", is re-synced from the surface.
          ed.surface.undo();
          if (ed.surface.getText() !== 'Give me a eraser.\n') out.push('undo ' + JSON.stringify(ed.surface.getText()));
          if (api.document_body() !== ed.surface.getText()) out.push('model drifted: ' + JSON.stringify(api.document_body()));
          const a = api.document_annotations();
          const s = a.spans.map((x) => x.id + '@' + x.anchor.start + '#' + x.active).join(' ');
          if (s !== 's1@10#0') out.push('spans ' + s + ' aside=' + a.setAside.spans.length);
          ed.surface.redo();
          if (ed.surface.getText() !== 'Give me a thumbtack.\n' || api.document_body() !== ed.surface.getText()) out.push('redo');
        "##,
        src = js_string_literal(&ungrammatical_fixture()),
    );
    assert_eq!(run(&body), "");
}

#[wasm_bindgen_test]
fn test_stale_hover_never_swallows_plain_arrows() {
    let body = format!(
        r##"
          const root = ed.surface.root;
          ed.surface.focus();
          ed.surface.setSelectionOffsets(3);
          const depth = ed.surface.historyIndex;
          // The point over the start of "Second line." (plain text, no span).
          const second = (() => {{
            const r = document.createRange();
            const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
            let n;
            while ((n = walker.nextNode())) {{
              const i = n.data.indexOf('Second');
              if (i >= 0) {{ r.setStart(n, i); r.setEnd(n, i + 1); break; }}
            }}
            const b = r.getClientRects()[0];
            return {{ el: n.parentElement, x: b.left + b.width / 2, y: b.top + b.height / 2 }};
          }})();

          // 1. Hover, then move the pointer onto plain text: no swap.
          teCyc.hover('s1');
          teCyc.pointAt(second.el, second.x, second.y);
          if (teCyc.key('ArrowDown')) out.push('moved away: arrow prevented');
          expect('moved away', {PAPERCLIP:?});

          // 2. Hover, then leave the surface: no swap.
          teCyc.hover('s1');
          teCyc.unhover();
          if (teCyc.key('ArrowDown')) out.push('left: arrow prevented');
          expect('left', {PAPERCLIP:?});

          // 3. Hover, then window blur: no swap.
          teCyc.hover('s1');
          window.dispatchEvent(new Event('blur'));
          if (teCyc.key('ArrowDown')) out.push('blur: arrow prevented');
          expect('blur', {PAPERCLIP:?});

          // 4. Hover, then type before the span so it shifts out from under
          // the still pointer: the stale hover does not take the arrow.
          teCyc.hover('s1');
          ed.surface.replaceRange(0, 0, 'Please, if you would kindly, ');
          ed.indicators.flush();
          const caret = ed.surface.getSelectionOffsets().start;
          if (teCyc.key('ArrowDown')) out.push('shifted: arrow prevented');
          if (ed.surface.getSelectionOffsets().start !== caret) out.push('shifted: caret changed');
          const shifted = teCyc.state();
          if (!shifted.startsWith('Please, if you would kindly, Pass me a paperclip') || !shifted.includes('#0'))
            out.push('shifted: ' + shifted);
          ed.surface.undo();
          expect('undo typing', {PAPERCLIP:?});
          if (ed.surface.historyIndex !== depth) out.push('history ' + (ed.surface.historyIndex - depth));

          // 5. Hovering again, repeated swaps still work, including when the
          // new alternative's text no longer reaches the pointer ("an
          // eraser" is shorter than "a paperclip"): the hover is held across
          // a swap of the same span.
          const el = root.querySelector('[data-te-decoration~="indicators:s1"]');
          const r = el.getClientRects()[0];
          teCyc.pointAt(el, r.right - 2, r.top + r.height / 2);
          if (!teCyc.key('ArrowDown')) out.push('hover again: not prevented');
          expect('down 1', {ERASER:?});
          if (!teCyc.key('ArrowDown')) out.push('held: not prevented');
          expect('down 2', {THUMBTACK:?});
          teCyc.key('ArrowDown');
          expect('down 3', {PAPERCLIP:?});
        "##
    );
    assert_eq!(run(&body), "");
}
