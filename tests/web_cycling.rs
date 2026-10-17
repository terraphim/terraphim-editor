//! Mechanical partition of the original cycling tests; shared real fixtures unchanged.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
#[path = "support/cycling.rs"]
mod cycling;
use cycling::*;
wasm_bindgen_test_configure!(run_in_browser);

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
