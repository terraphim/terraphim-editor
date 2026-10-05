//! Browser tests for moving text with its annotations (issue #44):
//! `MarkdownEditor.moveRange` moves a paragraph holding a span and a ghost
//! through the real exported `move_document_range`, as one undo step whose
//! undo and redo replay the move in the model. Run with `wasm-pack test
//! --headless --chrome`. Real scripts and the real document API; nothing is
//! mocked. See `tests/web.rs` for why the browser tests are split across
//! binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

/// "Pass me a paperclip. Drop this.\n\nSecond block.\n\n" with span `s1`
/// over "paperclip" (alternative "eraser") and ghost `g1` over " Drop this.",
/// built through the crate so the fixture is always valid.
fn fixture() -> String {
    let mut doc = Document::new("Pass me a paperclip. Drop this.\n\nSecond block.\n\n");
    let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
    doc.add_alternative(&id, "eraser", Source::Human, None)
        .unwrap();
    doc.ghost(20, 31).unwrap();
    write(&doc)
}

#[wasm_bindgen_test]
fn test_move_range_keeps_span_and_ghost_through_save_undo_and_redo() {
    let _document = fresh_full_editor();
    let script = format!(
        r##"(() => {{
          const src = {src};
          const ed = window.__teEditor;
          const api = window.wasmBindings;
          const out = [];
          const moved = 'Second block.\n\nPass me a paperclip. Drop this.\n\n';
          const original = 'Pass me a paperclip. Drop this.\n\nSecond block.\n\n';
          // Read the model directly: ed.annotations() re-syncs by text first,
          // which would hide a model that drifted from the surface.
          const where = () => {{
            const a = api.document_annotations();
            const s = a.spans.map((x) => x.id + '@' + x.anchor.start + ':' + x.alts.length);
            const g = a.ghosts.map((x) => x.id + '@' + x.anchor.start + ':' + x.anchor.text);
            return s.concat(g).join(' ') + ' aside=' + (a.setAside.spans.length + a.setAside.ghosts.length);
          }};
          const check = (label, text, annotations) => {{
            if (ed.surface.getText() !== text) out.push(label + ': text ' + JSON.stringify(ed.surface.getText()));
            if (api.document_body() !== ed.surface.getText()) out.push(label + ': model body differs');
            if (where() !== annotations) out.push(label + ': ' + where());
          }};

          ed.openDocument(src);
          check('opened', original, 's1@10:2 g1@20: Drop this. aside=0');
          const depth = ed.surface.historyIndex;

          const r = ed.moveRange(0, 33, 48);
          if (!r.moved || r.start !== 15 || r.end !== 48 || r.detached.length !== 0) {{
            out.push('outcome ' + JSON.stringify(r));
          }}
          check('moved', moved, 's1@25:2 g1@35: Drop this. aside=0');
          if (ed.surface.historyIndex !== depth + 1) out.push('not one undo step');
          const sel = ed.surface.getSelectionOffsets();
          if (sel.start !== 15 || sel.end !== 48) out.push('selection ' + JSON.stringify(sel));
          const saved = ed.saveDocument();

          // A move that would split the span is refused and changes nothing.
          let refused = false;
          try {{ ed.moveRange(12, 33, 48); }} catch (err) {{ refused = /s1/.test(String(err)); }}
          if (!refused) out.push('a straddling move was not refused by name');
          check('refused', moved, 's1@25:2 g1@35: Drop this. aside=0');
          if (ed.surface.historyIndex !== depth + 1) out.push('refused move recorded');

          // Undo restores the order and the annotations exactly.
          ed.surface.undo();
          check('undo', original, 's1@10:2 g1@20: Drop this. aside=0');
          if (api.save_document() !== src) out.push('undo: saved file differs from the original');
          ed.surface.redo();
          check('redo', moved, 's1@25:2 g1@35: Drop this. aside=0');
          if (api.save_document() !== saved) out.push('redo: saved file differs');

          // Typing after the move, then undoing both, unwinds in order.
          ed.surface.replaceRange(0, 0, 'New. ', {{ source: 'api' }});
          check('typed', 'New. ' + moved, 's1@30:2 g1@40: Drop this. aside=0');
          ed.surface.undo();
          ed.surface.undo();
          check('undo twice', original, 's1@10:2 g1@20: Drop this. aside=0');

          // The file saved after the move reopens with both annotations.
          const opened = ed.openDocument(saved);
          if (opened.unresolved !== 0 || opened.warning) out.push('reopen ' + JSON.stringify(opened));
          check('reopened', moved, 's1@25:2 g1@35: Drop this. aside=0');
          if (ed.exportDocument() !== 'Second block.\n\nPass me a paperclip.\n\n') {{
            out.push('export ' + JSON.stringify(ed.exportDocument()));
          }}

          // Moving text past an identical copy is a no-op.
          ed.surface.setText('abab');
          const before = ed.surface.historyIndex;
          const noop = ed.moveRange(0, 2, 4);
          if (noop.moved || ed.surface.historyIndex !== before) out.push('no-op ' + JSON.stringify(noop));
          return out.join('; ');
        }})()"##,
        src = js_string_literal(&fixture()),
    );
    assert_eq!(js_string(&script), "");
}
