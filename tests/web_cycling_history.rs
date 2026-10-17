//! Mechanical partition of the original cycling tests; shared real fixtures unchanged.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};
#[path = "support/cycling.rs"]
mod cycling;
use cycling::{run, ERASER, PAPERCLIP, THUMBTACK};
wasm_bindgen_test_configure!(run_in_browser);

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

          // 6. A re-layout ends the post-swap hold: swap to the shorter
          // "an eraser" under a still pointer past its end, resize, and the
          // next plain arrow is the browser's again.
          const el2 = root.querySelector('[data-te-decoration~="indicators:s1"]');
          const r2 = el2.getClientRects()[0];
          teCyc.pointAt(el2, r2.right - 2, r2.top + r2.height / 2);
          if (!teCyc.key('ArrowDown')) out.push('relayout: first swap not prevented');
          expect('relayout swap', {ERASER:?});
          window.dispatchEvent(new Event('resize'));
          if (teCyc.key('ArrowDown')) out.push('relayout: held hover took the arrow');
          expect('after relayout', {ERASER:?});

          // 7. Another decoration layer re-rendering also ends the hold
          // (e.g. Lab marks), while this layer's own refresh does not.
          ed.swapAlternative('s1', 0);
          ed.indicators.flush();
          expect('back to paperclip', {PAPERCLIP:?});
          const el3 = root.querySelector('[data-te-decoration~="indicators:s1"]');
          const r3 = el3.getClientRects()[0];
          teCyc.pointAt(el3, r3.right - 2, r3.top + r3.height / 2);
          if (!teCyc.key('ArrowDown')) out.push('layer: first swap not prevented');
          expect('layer swap', {ERASER:?});
          ed.decorations.set('te-test', [{{ id: 'probe', start: 0, end: 4, className: 'te-test-probe' }}]);
          if (teCyc.key('ArrowDown')) out.push('layer: held hover took the arrow');
          expect('after other layer', {ERASER:?});
          ed.decorations.clear('te-test');
        "##
    );
    assert_eq!(run(&body), "");
}
