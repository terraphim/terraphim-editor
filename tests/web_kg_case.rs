//! Browser tests for knowledge-graph alternatives (issue #13), the case and
//! article binary: a dot click turns "an eraser" into "a rubber" in one undo
//! step; Alt+arrows from the caret keep the text's case (use / USE) and walk
//! a row whose concept name may come first. Split from `tests/web_kg.rs` to
//! keep each binary inside the runner budget (see `tests/web.rs`). Real
//! scripts, the real exported document API over the real
//! terraphim_lsp_core; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::kg::run;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_article_and_case_follow_the_text_from_dots_and_alt_arrows() {
    let body = r##"
          // an eraser -> a rubber by a dot click, as one step.
          const depth = ed.surface.historyIndex;
          teKg.dot('kg-2-0', 1);
          expect('a rubber', 'kg-2-0', 'Use a rubber, then use a paperclip. USE IT. Every choice counts. | 2/1');
          ed.surface.undo();
          expect('undo restores an', 'kg-2-0', 'Use an eraser, then use a paperclip. USE IT. Every choice counts. | 2/0');
          ed.surface.redo();
          expect('redo', 'kg-2-0', 'Use a rubber, then use a paperclip. USE IT. Every choice counts. | 2/1');
          if (ed.surface.historyIndex !== depth + 1) out.push('history ' + (ed.surface.historyIndex - depth));

          // Alt+ArrowDown with the caret in the lower-case and upper-case uses.
          ed.surface.focus();
          ed.surface.setSelectionOffsets(teKg.at('use a paperclip') + 1);
          if (!teKg.key('ArrowDown', { altKey: true })) out.push('Alt+ArrowDown not taken');
          expect('lower', 'kg-8-1', 'Use a rubber, then employ a paperclip. USE IT. Every choice counts. | 3/1');
          ed.surface.setSelectionOffsets(teKg.at('USE IT') + 1);
          teKg.key('ArrowUp', { altKey: true });
          expect('upper wraps back', 'kg-8-2', 'Use a rubber, then employ a paperclip. UTILISE IT. Every choice counts. | 3/2');
          // The concept name may come first: decision < choice in the row.
          ed.surface.setSelectionOffsets(teKg.at('choice') + 2);
          teKg.key('ArrowUp', { altKey: true });
          expect('choice up', 'kg-1-0', 'Use a rubber, then employ a paperclip. UTILISE IT. Every decision counts. | 4/0');
          // The model never drifted: the body saves as the surface text.
          if (teKg.api.save_document() !== ed.surface.getText()) out.push('saved text differs');
        "##;
    assert_eq!(run(KG_DOC, body), "");
}
