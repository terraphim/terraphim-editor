//! Browser tests for the alternatives side panel (issue #10), the chrome
//! control, dots and tabs: the `●●●` control with ARIA, provenance glyphs and
//! colours from a file's `source` fields, an AI addition undone, Ctrl+Z inside
//! the panel and a draft keeping its own undo, tabs by keyboard, the overlap
//! refusal, a pending sentence created and swapped, save and reopen, and a
//! dot click opening the panel without taking focus. Split from
//! `web_alt_panel.rs` to keep each binary inside its time budget. Run with
//! `wasm-pack test --headless --chrome --test web_alt_panel_tabs`. Real
//! scripts, the real exported document API and real documents; nothing is
//! mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

/// "Pass me a paperclip. Drop this." with word span s1 over "paperclip"
/// whose alternatives are "eraser" (AI) and "thumbtack" (the author's).
fn provenance_fixture() -> String {
    let mut doc = Document::new("Pass me a paperclip. Drop this.\n");
    let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
    doc.add_alternative(&id, "eraser", Source::Ai, Some("fixture".into()))
        .unwrap();
    doc.add_alternative(&id, "thumbtack", Source::Human, None)
        .unwrap();
    write(&doc)
}

#[wasm_bindgen_test]
async fn test_chrome_dot_provenance_tabs_and_save() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let fixture = js_string_literal(&provenance_fixture());
    let first = format!(
        r##"
          ed.openDocument({fixture});
          ed.chrome.setMode('write-on');
          s.focus();
          s.setSelectionOffsets(12, 12);
          teTest.control('alternatives').click();
          const P = teAlt.P();
          if (!P.isOpen()) return 'chrome control did not open the panel';
          const control = teTest.control('alternatives');
          if (control.getAttribute('aria-expanded') !== 'true' || control.getAttribute('aria-controls') !== P.root.id) out.push('control ARIA');
          if (P.target.spanId !== 's1') return 'target ' + JSON.stringify(P.target);
          if (document.activeElement !== teAlt.inputs()[0]) out.push('focus not on the active line');
        "##
    );
    let result = run_steps(&[
        first.as_str(),
        // Provenance glyphs come from the `source` field (R-4.4).
        r##"
          const lines = Array.from(teAlt.P().list.querySelectorAll('.te-alt-line:not(.te-alt-line--new)'));
          const glyphs = lines.map((li) => li.querySelector('.te-alt-glyph').classList.contains('fa-robot') ? 'robot' : 'dot');
          if (JSON.stringify(glyphs) !== '["dot","robot","dot"]') out.push('glyphs ' + glyphs);
          if (JSON.stringify(lines.map((li) => li.dataset.source)) !== '["original","ai","human"]') out.push('sources');
          if (!teAlt.inputs()[0].readOnly || teAlt.inputs()[1].readOnly) out.push('read-only original');
          const activeGlyph = getComputedStyle(lines[0].querySelector('.te-alt-glyph')).color;
          const otherGlyph = getComputedStyle(lines[1].querySelector('.te-alt-glyph')).color;
          if (activeGlyph !== 'rgb(140, 134, 230)' || activeGlyph === otherGlyph) out.push('active glyph ' + activeGlyph + ' / ' + otherGlyph);
          if (!/AI/.test(teAlt.inputs()[1].getAttribute('aria-label'))) out.push('AI label');
          const tabs = teAlt.P().root.querySelector('[role="tablist"]');
          if (!tabs || tabs.querySelectorAll('[role="tab"]').length !== 3) out.push('tablist');
          if (getComputedStyle(teAlt.P().tabs.word).color !== 'rgb(140, 134, 230)') out.push('active tab not lavender');
        "##,
        // An AI addition is undone like any other (R-4.6).
        r##"
          teAlt.P().addAlternative('quill', { source: 'ai' });
          const last = teAlt.P().list.querySelectorAll('.te-alt-line:not(.te-alt-line--new)')[3];
          if (!last || !last.querySelector('.fa-robot')) return 'AI line glyph';
          s.undo();
          teAlt.P().flush();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","eraser","thumbtack"]') out.push('undo AI add ' + teAlt.texts());
          s.redo();
          teAlt.P().flush();
          if (teAlt.texts()[3] !== 'quill' || !teAlt.inSync()) out.push('redo AI add');
        "##,
        // Ctrl+Z inside the panel undoes the document (no uncommitted typing).
        r##"
          teAlt.newInput().focus();
          const prevented = teTest.key(teAlt.newInput(), 'z', { ctrlKey: true });
          teAlt.P().flush();
          if (!prevented || teAlt.texts().length !== 3) out.push('Ctrl+Z in the panel ' + teAlt.texts());
          if (document.activeElement !== teAlt.newInput()) out.push('focus after Ctrl+Z');
          teAlt.newInput().value = 'dra';
          if (teTest.key(teAlt.newInput(), 'z', { ctrlKey: true })) out.push('took Ctrl+Z from a draft');
          teAlt.newInput().value = '';
        "##,
        // Tabs: a sentence around a word span cannot be a span; keyboard moves.
        r##"
          const P = teAlt.P();
          P.tabs.word.focus();
          teTest.key(P.tabs.word, 'ArrowRight');
          if (P.tab !== 'sentence' || document.activeElement !== P.tabs.sentence) return 'ArrowRight on tabs';
          if (P.tabs.sentence.getAttribute('aria-selected') !== 'true') out.push('aria-selected');
          if (!P.target.message || !P.list.hidden) out.push('overlap not refused: ' + JSON.stringify(P.target));
          teTest.key(P.tabs.sentence, 'ArrowRight');
          if (P.tab !== 'paragraph') out.push('paragraph tab');
          teTest.key(P.tabs.paragraph, 'Home');
          if (P.tab !== 'word' || P.target.spanId !== 's1') out.push('back to the word span');
          // "Drop this." has no span: the sentence tab offers it as pending.
          ed.altPanel.open({ start: 21, end: 31 });
          if (!P.target.pending || P.target.pending.kind !== 'sentence' || P.target.pending.text !== 'Drop this.') out.push('pending sentence ' + JSON.stringify(P.target));
        "##,
        r##"
          teAlt.type(teAlt.newInput(), 'Leave it.');
          const spans = teAlt.spans();
          if (spans.length !== 2 || spans[1].kind !== 'sentence') return 'sentence span ' + JSON.stringify(spans.map((x) => x.kind));
          teAlt.P().setActiveIndex(1);
          if (s.getText() !== 'Pass me a paperclip. Leave it.\n') out.push('sentence swap ' + JSON.stringify(s.getText()));
          T.saved = ed.saveDocument();
        "##,
        // Save and reopen keep every line, its provenance and the active one.
        r##"
          ed.openDocument('plain\n');
          ed.openDocument(T.saved);
          const spans = teAlt.spans();
          const s1 = spans.find((x) => x.id === 's1');
          if (!s1 || JSON.stringify(s1.alts.map((a) => a.source)) !== '["original","ai","human"]') return 'reopened ' + JSON.stringify(spans);
          if (s.getText() !== 'Pass me a paperclip. Leave it.\n') out.push('reopened text');
          teAlt.P().open({ spanId: spans[1].id });
          if (teAlt.active() !== 1 || teAlt.P().tab !== 'sentence') out.push('reopened active ' + teAlt.active());
          teAlt.P().close({ restoreFocus: false });
        "##,
        // A dot click shows that span without taking focus or the event.
        r##"
          ed.indicators.flush();
          s.focus();
          const dot = document.querySelector('.te-indicators [data-span-id="s1"] .te-ind-dot');
          if (!dot) return 'no dot';
          let seen = null;
          const spy = (e) => { seen = e.defaultPrevented; };
          document.addEventListener('te:dot', spy);
          dot.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          document.removeEventListener('te:dot', spy);
          const P = teAlt.P();
          if (!P.isOpen() || P.target.spanId !== 's1') out.push('dot did not open the span');
          if (seen === true) out.push('te:dot cancelled');
          if (P.root.contains(document.activeElement)) out.push('dot moved focus into the panel');
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
