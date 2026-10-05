//! Browser tests for knowledge-graph alternatives (issue #13): with the
//! committed fixture thesaurus loaded, a KG word shows one dot per synonym
//! of its concept with its own form lit; hovering and ArrowUp/ArrowDown
//! walk the synonyms with wrap, keeping the text's capitalisation; each swap
//! is one undo step and undo and redo restore the text and the dots. Dot
//! clicks, Alt+arrows, case and a/an are in `tests/web_kg_case.rs`.
//! Real scripts, the real exported document API over the real
//! terraphim_lsp_core; nothing is mocked. See `tests/web.rs` for why the
//! browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::kg::run;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

const LINE: &str = "Use an eraser, then use a paperclip. USE IT. Every choice counts.";

#[wasm_bindgen_test]
fn test_kg_words_show_one_dot_per_synonym_with_their_form_lit() {
    let body = format!(
        r##"
          if (teKg.line() !== {LINE:?}) out.push('line ' + teKg.line());
          // Concept order: concept name first, then the other synonyms.
          const want = {{ 'kg-8-0': '3/0', 'kg-2-0': '2/0', 'kg-8-1': '3/0', 'kg-8-2': '3/0', 'kg-1-0': '4/1' }};
          for (const id of Object.keys(want)) {{
            if (teKg.dots(id) !== want[id]) out.push(id + ' dots ' + teKg.dots(id));
          }}
          // A single-term concept (paperclip) gets nothing.
          if (document.querySelector('.te-indicators [data-span-id="kg-7-0"]')) out.push('paperclip has dots');
          const holder = document.querySelector('.te-indicators [data-span-id="kg-1-0"]');
          if (!holder || holder.dataset.source !== 'kg') out.push('no kg source marker');
          // The text decoration is under "choice" and described.
          const el = teKg.span('kg-1-0');
          if (!el || el.textContent !== 'choice') out.push('decoration ' + (el && el.textContent));
          const desc = el && document.getElementById(el.getAttribute('aria-describedby'));
          if (!desc || desc.textContent !== 'Word: 4 alternatives, 2 of 4 active (knowledge graph synonyms)') {{
            out.push('description ' + (desc && desc.textContent));
          }}
          // The spans come from the model: annotations().kg.
          const kg = ed.annotations().kg.map((s) => s.id + ':' + s.anchor.text).join(' ');
          if (kg !== 'kg-8-0:Use kg-2-0:eraser kg-8-1:use kg-8-2:USE kg-1-0:choice') out.push('kg ' + kg);
          // Without a thesaurus there are no KG dots; loading brings them back.
          ed.clearThesaurus();
          if (document.querySelector('.te-indicators [data-span-id^="kg-"]')) out.push('dots after clear');
          ed.loadThesaurus({thesaurus});
          if (teKg.dots('kg-1-0') !== '4/1') out.push('reload ' + teKg.dots('kg-1-0'));
          // A bad thesaurus throws and keeps the loaded one.
          let threw = false;
          try {{ ed.loadThesaurus('{{ not json'); }} catch (e) {{ threw = true; }}
          if (!threw) out.push('bad thesaurus accepted');
          if (teKg.dots('kg-1-0') !== '4/1') out.push('after bad ' + teKg.dots('kg-1-0'));
        "##,
        thesaurus = js_string_literal(KG_THESAURUS),
    );
    assert_eq!(run(KG_DOC, &body), "");
}

#[wasm_bindgen_test]
fn test_hover_arrows_walk_synonyms_with_wrap_as_one_undo_step_each() {
    let body = r##"
          const depth = ed.surface.historyIndex;
          teKg.hover('kg-8-0');
          if (!teKg.key('ArrowDown')) out.push('ArrowDown not taken');
          expect('down 1', 'kg-8-0', 'Employ an eraser, then use a paperclip. USE IT. Every choice counts. | 3/1');
          // The hover is held across the swap (same id).
          teKg.key('ArrowDown');
          expect('down 2', 'kg-8-0', 'Utilise an eraser, then use a paperclip. USE IT. Every choice counts. | 3/2');
          teKg.key('ArrowDown');
          expect('wraps', 'kg-8-0', 'Use an eraser, then use a paperclip. USE IT. Every choice counts. | 3/0');
          teKg.key('ArrowUp');
          expect('up wraps', 'kg-8-0', 'Utilise an eraser, then use a paperclip. USE IT. Every choice counts. | 3/2');
          if (ed.surface.historyIndex !== depth + 4) out.push('history ' + (ed.surface.historyIndex - depth));
          ed.surface.undo();
          expect('undo 1', 'kg-8-0', 'Use an eraser, then use a paperclip. USE IT. Every choice counts. | 3/0');
          ed.surface.undo();
          expect('undo 2', 'kg-8-0', 'Utilise an eraser, then use a paperclip. USE IT. Every choice counts. | 3/2');
          ed.surface.undo();
          ed.surface.undo();
          expect('undo all', 'kg-8-0', 'Use an eraser, then use a paperclip. USE IT. Every choice counts. | 3/0');
          if (ed.surface.historyIndex !== depth) out.push('undo depth');
          ed.surface.redo();
          expect('redo', 'kg-8-0', 'Employ an eraser, then use a paperclip. USE IT. Every choice counts. | 3/1');
        "##;
    assert_eq!(run(KG_DOC, body), "");
}
