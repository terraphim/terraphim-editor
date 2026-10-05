//! Browser tests for moving blocks in the Blocks view (issue #58): Move up /
//! Move down buttons and Alt+ArrowUp / Alt+ArrowDown on a focused card swap
//! a block with its neighbour through `MarkdownEditor.moveRange`, so the
//! spans and ghosts inside it travel with it, as one undo step. Run with
//! `wasm-pack test --headless --chrome`. Real scripts, the real exported
//! document API and fixtures built through the crate; nothing is mocked.
//! Every scenario runs as short steps with a yield between them
//! (`support::run_steps`).
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

/// Four blocks, the last without a trailing newline:
/// "# Title\n\nPass me a paperclip. Drop this.\n\nSecond block.\n\nLast block"
/// with span `s1` over "paperclip" (alternative "eraser") and ghost `g1`
/// over " Drop this.", both inside block 1.
fn annotated() -> String {
    let mut doc =
        Document::new("# Title\n\nPass me a paperclip. Drop this.\n\nSecond block.\n\nLast block");
    let id = doc.add_span(SpanKind::Word, 19, 28).unwrap();
    doc.add_alternative(&id, "eraser", Source::Human, None)
        .unwrap();
    doc.ghost(29, 40).unwrap();
    write(&doc)
}

/// "One.\n\nTwo.\n\nThree tail.\n\nFour head.\n" with ghost `g1` over
/// "tail.\n\nFour" (across the boundary between blocks 2 and 3).
fn straddling() -> String {
    let mut doc = Document::new("One.\n\nTwo.\n\nThree tail.\n\nFour head.\n");
    doc.ghost(18, 29).unwrap();
    write(&doc)
}

/// Helpers shared by the steps: `T.where()` reads the model directly
/// (`ed.annotations()` re-syncs by text first and would hide drift) and
/// `T.check(label, text, annotations)` compares text, model body, blocks and
/// annotations.
const HELPERS: &str = r##"
  const api = window.wasmBindings;
  T.where = () => {
    const a = api.document_annotations();
    const sp = a.spans.map((x) => x.id + '@' + x.anchor.start + ':' + x.alts.length);
    const g = a.ghosts.map((x) => x.id + '@' + x.anchor.start + ':' + x.anchor.text);
    return sp.concat(g).join(' ') + ' aside=' + (a.setAside.spans.length + a.setAside.ghosts.length);
  };
  T.check = (label, text, annotations) => {
    const o = [];
    if (s.getText() !== text) o.push(label + ': text ' + JSON.stringify(s.getText()));
    if (api.document_body() !== s.getText()) o.push(label + ': model body differs');
    if (annotations !== null && T.where() !== annotations) o.push(label + ': ' + T.where());
    if (BlocksView.serialise(bv.model) !== s.getText()) o.push(label + ': stale blocks');
    return o;
  };
  T.texts = () => BlocksView.parse(s.getText()).blocks.map((b) => b.text).join('|');
"##;

#[wasm_bindgen_test]
async fn test_move_carries_span_and_ghost_with_one_undo_step() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let setup = format!(
        "{HELPERS}
          T.src = {src};
          ed.openDocument(T.src);
          bv.setView('blocks');
          T.original = '# Title\\n\\nPass me a paperclip. Drop this.\\n\\nSecond block.\\n\\nLast block';
          T.down = '# Title\\n\\nSecond block.\\n\\nPass me a paperclip. Drop this.\\n\\nLast block';
          T.last = '# Title\\n\\nSecond block.\\n\\nLast block\\n\\nPass me a paperclip. Drop this.';
          T.at = {{ original: 's1@19:2 g1@29: Drop this. aside=0', down: 's1@34:2 g1@44: Drop this. aside=0', last: 's1@46:2 g1@56: Drop this. aside=0' }};
          out.push(...T.check('opened', T.original, T.at.original));
          T.depth = s.historyIndex;",
        src = js_string_literal(&annotated()),
    );
    let result = run_steps(&[
        &setup,
        // Alt+ArrowDown on a focused card: the block, its span and its ghost
        // move down as one undo step; focus follows the card.
        r##"
          // The text surface's Alt+Arrow cycling (#9) listens on its root;
          // the cards are outside it, so the keys never reach it.
          if (s.root.contains(bv.list) || bv.list.contains(s.root)) out.push('cards inside the surface root');
          let reached = 0;
          const spy = () => { reached += 1; };
          s.root.addEventListener('keydown', spy);
          bv.setCurrent(1);
          if (!teTest.key(bv.cards()[1], 'ArrowDown', { altKey: true })) out.push('Alt+ArrowDown not handled');
          s.root.removeEventListener('keydown', spy);
          if (reached) out.push('Alt+ArrowDown on a card reached the surface root');
          out.push(...T.check('down', T.down, T.at.down));
          if (s.historyIndex !== T.depth + 1) out.push('not one undo step: ' + (s.historyIndex - T.depth));
          if (document.activeElement !== bv.cards()[2]) out.push('focus did not follow the card');
          if (!bv.noticeElement.hidden) out.push('notice shown after a move');
          T.saved = ed.saveDocument();
        "##,
        // One undo restores order and annotations; redo moves them again.
        r##"
          bv.undo();
          out.push(...T.check('undo', T.original, T.at.original));
          if (wasmBindings.save_document() !== T.src) out.push('undo: saved file differs from the original');
          bv.redo();
          out.push(...T.check('redo', T.down, T.at.down));
          if (wasmBindings.save_document() !== T.saved) out.push('redo: saved file differs');
        "##,
        // The saved file reopens with both annotations in the new place.
        r##"
          const opened = ed.openDocument(T.saved);
          if (opened.unresolved !== 0 || opened.warning) out.push('reopen ' + JSON.stringify(opened));
          out.push(...T.check('reopened', T.down, T.at.down));
          T.depth = s.historyIndex;
        "##,
        // The Move up button moves it back and keeps focus on the moved
        // card's Move up button, so it can be pressed again.
        r##"
          bv.setCurrent(2);
          const up = bv.cards()[2].querySelector('[data-action="up"]');
          if (up.hidden || up.tabIndex !== 0) out.push('Move up not offered on the focused card');
          up.focus();
          up.click();
          out.push(...T.check('button up', T.original, T.at.original));
          const again = bv.cards()[1].querySelector('[data-action="up"]');
          if (document.activeElement !== again) out.push('focus not on the moved card button');
          if (s.historyIndex !== T.depth + 1) out.push('button: not one undo step');
        "##,
        // Moving past the last block, which has no trailing newline: the
        // separators are put back so the blocks stay intact, still one step.
        r##"
          bv.setCurrent(1);
          teTest.key(bv.cards()[1], 'ArrowDown', { altKey: true });
          const mid = s.historyIndex;
          bv.cards()[2].querySelector('[data-action="down"]').click();
          out.push(...T.check('past last', T.last, T.at.last));
          if (T.texts() !== '# Title|Second block.|Last block|Pass me a paperclip. Drop this.') out.push('blocks ' + T.texts());
          if (s.historyIndex !== mid + 1) out.push('fix-up not folded into one step: ' + (s.historyIndex - mid));
          if (document.activeElement !== bv.cards()[3]) out.push('focus after moving to the end');
          T.lastSaved = ed.saveDocument();
        "##,
        // First and last boundaries: no button, the keys are no-ops.
        r##"
          const before = s.historyIndex;
          const cards = bv.cards();
          if (!cards[0].querySelector('[data-action="up"]').hidden) out.push('Move up on the first block');
          if (!cards[3].querySelector('[data-action="down"]').hidden) out.push('Move down on the last block');
          if (cards[1].querySelector('[data-action="up"]').hidden || cards[1].querySelector('[data-action="down"]').hidden) out.push('middle block missing a move button');
          if (bv.moveBlock(0, 'up') || bv.moveBlock(3, 'down')) out.push('boundary move reported as moved');
          bv.setCurrent(3);
          if (!teTest.key(bv.cards()[3], 'ArrowDown', { altKey: true })) out.push('Alt+ArrowDown on the last block not consumed');
          bv.setCurrent(0);
          teTest.key(bv.cards()[0], 'ArrowUp', { altKey: true });
          out.push(...T.check('boundaries', T.last, T.at.last));
          if (s.historyIndex !== before) out.push('boundary move recorded');
          if (document.activeElement !== bv.cards()[0]) out.push('focus lost at a boundary');
        "##,
        // Undo of the move past the last block restores text and anchors,
        // and the saved file reopens with the blocks intact.
        r##"
          bv.undo();
          out.push(...T.check('undo past last', T.down, T.at.down));
          const opened = ed.openDocument(T.lastSaved);
          if (opened.unresolved !== 0) out.push('reopen last ' + JSON.stringify(opened));
          out.push(...T.check('reopened last', T.last, T.at.last));
        "##,
        // Moving the last block (no trailing newline) up, and a heading
        // followed by a single newline: each block stays a block.
        r##"
          s.setText('A\n\nB\n\nC');
          bv.moveBlock(2, 'up');
          out.push(...T.check('last up', 'A\n\nC\n\nB', null));
          s.setText('# H\npara\n\nnext\n');
          bv.moveBlock(0, 'down');
          out.push(...T.check('heading down', 'para\n\n# H\n\nnext\n', null));
          const depth = s.historyIndex;
          bv.undo();
          out.push(...T.check('heading undo', '# H\npara\n\nnext\n', null));
          if (s.historyIndex !== depth - 1) out.push('heading move not one step');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_refused_move_and_moving_while_editing() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let setup = format!(
        "{HELPERS}
          T.src = {src};
          ed.openDocument(T.src);
          bv.setView('blocks');
          T.text = 'One.\\n\\nTwo.\\n\\nThree tail.\\n\\nFour head.\\n';
          T.ghost = T.where();
          out.push(...T.check('opened', T.text, 'g1@18:tail.\\n\\nFour aside=0'));
          T.depth = s.historyIndex;",
        src = js_string_literal(&straddling()),
    );
    let result = run_steps(&[
        &setup,
        // A ghost across a block boundary: the move is refused, its message
        // names the ghost in the notice region and nothing changes.
        r##"
          bv.setCurrent(2);
          teTest.key(bv.cards()[2], 'ArrowDown', { altKey: true });
          out.push(...T.check('refused', T.text, T.ghost));
          if (s.historyIndex !== T.depth) out.push('refused move recorded');
          const n = bv.noticeElement;
          if (n.hidden || !teTest.visible(n)) out.push('notice not shown');
          if (!n.closest('[role="status"][aria-live="polite"]')) out.push('notice not a live region');
          if (!/g1/.test(n.textContent)) out.push('message does not name the ghost: ' + n.textContent);
          if (bv.noticeDismiss.tabIndex !== 0) out.push('dismiss not reachable by Tab');
          if (document.activeElement !== bv.cards()[2]) out.push('focus moved on refusal');
          if (bv.moveBlock(3, 'up')) out.push('up into the ghost not refused');
          out.push(...T.check('refused up', T.text, T.ghost));
        "##,
        // A move clear of the ghost works and clears the notice.
        r##"
          bv.moveBlock(0, 'down');
          out.push(...T.check('clear move', 'Two.\n\nOne.\n\nThree tail.\n\nFour head.\n', 'g1@18:tail.\n\nFour aside=0'));
          if (!bv.noticeElement.hidden) out.push('notice not cleared');
          if (bv.noticeDismiss.tabIndex !== -1) out.push('hidden dismiss still in the tab order');
        "##,
        // Moving the block being edited commits the draft, then moves it.
        r##"
          s.setText('A\n\nB\n\nC\n');
          const ta = bv.editBlock(0);
          ta.value = 'A edited';
          bv.cards()[0].querySelector('[data-action="down"]').click();
          out.push(...T.check('edit then move', 'B\n\nA edited\n\nC\n', null));
          if (bv.editing) out.push('editor still open');
          if (bv.keptDrafts().length) out.push('draft kept instead of applied');
        "##,
        // Editing an earlier block that gains a block: the intended block
        // still moves (its index is mapped through the commit).
        r##"
          s.setText('A\n\nB\n\nC\n');
          const ta = bv.editBlock(0);
          ta.value = 'A\n\nA2';
          if (!bv.moveBlock(2, 'up')) out.push('move after a splitting commit failed');
          out.push(...T.check('split then move', 'A\n\nA2\n\nC\n\nB\n', null));
        "##,
        // A draft kept after an outside change overlapped its block stays
        // kept (and follows the text) when blocks move; it is never lost.
        r##"
          s.setText('A\n\nB\n\nC\n');
          const ta = bv.editBlock(1);
          ta.value = 'B draft';
          s.replaceRange(3, 4, 'X');
          if (bv.editing || bv.keptDrafts().length !== 1) return 'draft not kept by the overlapping edit';
          if (!bv.moveBlock(0, 'down')) out.push('move with a kept draft failed');
          out.push(...T.check('kept draft', 'X\n\nA\n\nC\n', null));
          const kept = bv.keptDrafts();
          if (kept.length !== 1 || kept[0].value !== 'B draft') out.push('draft lost ' + JSON.stringify(kept));
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

/// A body with one annotation over `range`: a ghost, or a span with
/// alternative "2." when `span` is set.
fn separator_case(body: &str, range: (usize, usize), span: bool) -> String {
    let mut doc = Document::new(body);
    if span {
        let id = doc.add_span(SpanKind::Word, range.0, range.1).unwrap();
        doc.add_alternative(&id, "2.", Source::Human, None).unwrap();
    } else {
        doc.ghost(range.0, range.1).unwrap();
    }
    write(&doc)
}

#[wasm_bindgen_test]
async fn test_fix_ups_respect_annotations_and_kept_drafts_follow_moves() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let setup = format!(
        "{HELPERS}
          T.ghostSep = {ghost_sep};
          T.spanSep = {span_sep};
          T.ghostText = {ghost_text};
          T.ghostClean = {ghost_clean};
          bv.setView('blocks');",
        ghost_sep = js_string_literal(&separator_case("One.\n\nTwo.\n\nLast", (6, 12), false)),
        span_sep = js_string_literal(&separator_case("One.\n\nTwo.\n\nLast", (6, 12), true)),
        ghost_text = js_string_literal(&separator_case("One.\n\nTwo.\n\nLast", (6, 10), false)),
        ghost_clean = js_string_literal(&separator_case(
            "One.\n\nTwo.\n\nThree.\n\nLast",
            (6, 12),
            false
        )),
    );
    let result = run_steps(&[
        &setup,
        // A ghost or a span holding the block's trailing separator, where the
        // move past the last block needs a separator fix-up: refused by
        // name, nothing changed.
        r##"
          for (const [label, src, id, at] of [['ghost', T.ghostSep, 'g1', 'g1@6:Two.\n\n aside=0'], ['span', T.spanSep, 's1', 's1@6:2 aside=0']]) {
            ed.openDocument(src);
            const depth = s.historyIndex;
            if (bv.moveBlock(1, 'down')) out.push(label + ': move not refused');
            out.push(...T.check(label + ' refused', 'One.\n\nTwo.\n\nLast', at));
            if (s.historyIndex !== depth) out.push(label + ': refused move recorded');
            const n = bv.noticeElement;
            if (n.hidden || !n.textContent.includes('"' + id + '"')) out.push(label + ': notice ' + n.textContent);
          }
        "##,
        // A ghost on the block text only: the fix-ups sit at its edges, so
        // the move goes through with the ghost intact.
        r##"
          ed.openDocument(T.ghostText);
          if (!bv.moveBlock(1, 'down')) out.push('text-only ghost move refused: ' + bv.noticeElement.textContent);
          out.push(...T.check('text-only ghost', 'One.\n\nLast\n\nTwo.', 'g1@12:Two. aside=0'));
        "##,
        // A ghost holding its separator where no fix-up is needed: a clean
        // move, the ghost carried unchanged.
        r##"
          ed.openDocument(T.ghostClean);
          if (!bv.moveBlock(1, 'down')) out.push('clean move refused: ' + bv.noticeElement.textContent);
          out.push(...T.check('clean', 'One.\n\nThree.\n\nTwo.\n\nLast', 'g1@14:Two.\n\n aside=0'));
          bv.undo();
          out.push(...T.check('clean undo', 'One.\n\nTwo.\n\nThree.\n\nLast', 'g1@6:Two.\n\n aside=0'));
        "##,
        // A kept draft in the moved block follows it down, up and through
        // undo; Apply replaces the right block.
        r##"
          s.setText('A\n\nX\n\nC\n');
          const ta = bv.editBlock(1);
          ta.value = 'X draft';
          bv.cancelEdit();
          const d = () => bv.keptDrafts()[0];
          if (!d() || d().anchor !== 3) return 'draft not kept at the block: ' + JSON.stringify(bv.keptDrafts());
          bv.moveBlock(1, 'down');
          if (s.getText() !== 'A\n\nC\n\nX\n' || d().anchor !== 6) out.push('down: anchor ' + d().anchor + ' ' + JSON.stringify(s.getText()));
          bv.moveBlock(2, 'up');
          if (d().anchor !== 3) out.push('up: anchor ' + d().anchor);
          bv.undo();
          if (s.getText() !== 'A\n\nC\n\nX\n' || d().anchor !== 6) out.push('undo: anchor ' + d().anchor);
          const t = bv.draftTarget(d());
          if (t.mode !== 'replace' || t.block.text !== 'X') out.push('target ' + JSON.stringify(t));
          if (bv.applyDraft(d().id) !== 'replace') out.push('apply mode');
          if (s.getText() !== 'A\n\nC\n\nX draft\n') out.push('applied ' + JSON.stringify(s.getText()));
        "##,
        // A kept draft in a block the move passes over follows that block.
        r##"
          s.setText('A\n\nX\n\nC\n');
          const ta = bv.editBlock(2);
          ta.value = 'C draft';
          bv.cancelEdit();
          bv.moveBlock(1, 'down');
          const d = bv.keptDrafts()[0];
          const t = bv.draftTarget(d);
          if (d.anchor !== 3 || t.mode !== 'replace' || t.block.text !== 'C') out.push('passed over ' + JSON.stringify([d.anchor, t]));
          bv.discardDraft(d.id);
        "##,
        // A draft whose block changed is inserted after the right block once
        // that block has moved.
        r##"
          s.setText('A\n\nX\n\nC\n');
          const ta = bv.editBlock(1);
          ta.value = 'X draft';
          s.replaceRange(3, 4, 'Y');
          bv.moveBlock(1, 'down');
          const d = bv.keptDrafts()[0];
          if (!d) return 'draft lost';
          const t = bv.draftTarget(d);
          if (t.mode !== 'insert' || t.at !== 7) out.push('changed target ' + JSON.stringify(t));
          if (bv.applyDraft(d.id) !== 'insert') out.push('changed apply mode');
          if (s.getText() !== 'A\n\nC\n\nY\n\nX draft\n') out.push('inserted ' + JSON.stringify(s.getText()));
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
