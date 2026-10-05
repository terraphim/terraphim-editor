//! Browser tests for knowledge-graph alternatives (issue #13), part two:
//! derived KG spans are never saved and come back on reopen; a span from the
//! annotation block wins over a KG term; "AI alternatives for selection"
//! (Ctrl+Shift+G and the R-7.3 menu item) appends the KG synonyms to the
//! span, bot-marked and after the writer's own, as one undo step. Real
//! scripts, the real exported document API over the real
//! terraphim_lsp_core; nothing is mocked. See `tests/web.rs` for why the
//! browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::kg::run;
use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

/// The fixture document with block span `s1` over "eraser" (human
/// alternative "pencil").
fn annotated() -> String {
    let body = KG_DOC;
    let start = body.find("eraser").unwrap();
    let mut doc = Document::new(body);
    let id = doc
        .add_span(SpanKind::Word, start, start + "eraser".len())
        .unwrap();
    doc.add_alternative(&id, "pencil", Source::Human, None)
        .unwrap();
    write(&doc)
}

#[wasm_bindgen_test]
fn test_kg_spans_are_never_saved_and_block_spans_win() {
    let body = r##"
          // A swapped KG word saves as plain text: no annotation block.
          teKg.dot('kg-1-0', 2);
          const saved = ed.saveDocument();
          if (saved !== ed.surface.getText()) out.push('saved has more than the body');
          if (saved.includes('terraphim-alternatives')) out.push('a block was written');
          if (!saved.includes('Every judgment counts.')) out.push('saved ' + JSON.stringify(saved));
          // Reopened, the KG spans are derived again from the text.
          ed.openDocument(saved, 'kg-reopen');
          ed.chrome.setMode('write-on');
          if (teKg.dots('kg-1-0') !== '4/2') out.push('reopened ' + teKg.dots('kg-1-0'));
          if (ed.annotations().spans.length !== 0) out.push('block spans after reopen');
        "##;
    assert_eq!(run(KG_DOC, body), "");

    let body = r##"
          // "eraser" is the writer's span: two dots (original, pencil) and no
          // KG span over it; the other KG words are still offered.
          if (teKg.dots('s1') !== '2/0') out.push('s1 ' + teKg.dots('s1'));
          if (document.querySelector('.te-indicators [data-span-id="kg-2-0"]')) out.push('kg over s1');
          if (teKg.dots('kg-8-0') !== '3/0') out.push('kg-8-0 ' + teKg.dots('kg-8-0'));
        "##;
    assert_eq!(run(&annotated(), body), "");
}

#[wasm_bindgen_test]
fn test_ai_alternatives_for_selection_append_bot_marked_lines_as_one_step() {
    let body = r##"
          const s = ed.surface;
          const menu = ed.selectionMenu;
          // The menu offers the item on a KG word, and not elsewhere.
          const avail = (start, end) => menu.availableItems(menu.contextFor({ start, end })).map((i) => i.id);
          const choice = teKg.at('choice');
          if (!avail(choice, choice + 6).includes('ai-alternatives')) out.push('not offered on choice');
          if (avail(teKg.at('Every'), teKg.at('Every') + 5).includes('ai-alternatives')) out.push('offered on Every');

          // Ctrl+Shift+G on "choice" (no span yet): a word span with the
          // other synonyms as AI lines, the KG span giving way to it.
          const depth = s.historyIndex;
          s.focus();
          s.setSelectionOffsets(choice, choice + 6);
          if (!teKg.key('G', { ctrlKey: true, shiftKey: true })) out.push('Ctrl+Shift+G not taken');
          const span = ed.annotations().spans.find((x) => x.anchor.text === 'choice');
          if (!span) {
            out.push('no span created');
          } else {
            const lines = span.alts.map((a) => a.text + ':' + a.source + (a.model ? '/' + a.model : '')).join(' ');
            if (lines !== 'choice:original decision:ai/kg judgment:ai/kg option:ai/kg') out.push('lines ' + lines);
            if (teKg.dots(span.id) !== '4/0') out.push('span dots ' + teKg.dots(span.id));
          }
          if (ed.annotations().kg.some((x) => x.id === 'kg-1-0')) out.push('kg span still derived');
          if (s.historyIndex !== depth + 1) out.push('history ' + (s.historyIndex - depth));
          if (s.getText().indexOf('choice') !== choice) out.push('text changed');
          // Undo removes the span; the KG span is derived again. Redo is back.
          s.undo();
          if (ed.annotations().spans.length !== 0) out.push('span after undo');
          if (teKg.dots('kg-1-0') !== '4/1') out.push('kg after undo ' + teKg.dots('kg-1-0'));
          s.redo();
          if (ed.annotations().spans.length !== 1) out.push('span after redo');
        "##;
    assert_eq!(run(KG_DOC, body), "");

    let body = r##"
          // On the writer's span the KG line goes after their own.
          const at = teKg.at('eraser');
          ed.surface.setSelectionOffsets(at, at + 6);
          const change = ed.kgAlternativesForSelection();
          if (!change || change.span !== 's1') out.push('change ' + JSON.stringify(change));
          const lines = ed.annotations().spans[0].alts.map((a) => a.text + ':' + a.source).join(' ');
          if (lines !== 'eraser:original pencil:human rubber:ai') out.push('lines ' + lines);
          // Again: nothing new, nothing recorded.
          const depth = ed.surface.historyIndex;
          if (ed.kgAlternativesForSelection() !== null) out.push('second append changed something');
          if (ed.surface.historyIndex !== depth) out.push('second append recorded');
          // The panel lists them with the robot glyph.
          if (ed.altPanel && ed.altPanel.isOpen()) {
            const robots = document.querySelectorAll('.te-alt-glyph--ai.fa-robot').length;
            if (robots !== 1) out.push('robot glyphs ' + robots);
          } else {
            out.push('panel not open');
          }
        "##;
    assert_eq!(run(&annotated(), body), "");
}
