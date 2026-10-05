//! Browser tests for the alternatives side panel (issue #10): opening it from
//! the chrome, from "Alternatives for selection" (Ctrl+Shift+A) and from a
//! dot; adding, editing, deleting and reordering alternatives; Up/Down making
//! a line active with a live document update; provenance glyphs from the
//! `source` field; undo and redo of every operation with the model and the
//! surface in step; save and reopen; keyboard navigation, Escape and
//! `destroy()` (keyboard toggling, the menu and teardown are in
//! `web_alt_panel_keys.rs`, split to keep each binary inside its time budget).
//! Run with `wasm-pack test --headless --chrome`. Real scripts,
//! the real exported document API and real documents; nothing is mocked.
//! Every scenario runs as short steps with a yield between them
//! (`support::run_steps`).
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
async fn test_selection_add_up_down_edit_and_undo() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        // Ctrl+Shift+A on a selection in plain mode: Write_On turns on, the
        // panel opens on a pending word span and focus goes to the new line.
        r##"
          s.setText('Pass me a paperclip. Drop this.');
          s.focus();
          s.setSelectionOffsets(10, 19);
          const prevented = teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          const P = teAlt.P();
          if (!prevented) out.push('Ctrl+Shift+A not cancelled');
          if (!P.isOpen()) return 'panel not open';
          if (document.body.dataset.mode !== 'write-on') out.push('Write_On not turned on');
          if (P.tab !== 'word') out.push('tab ' + P.tab);
          if (!P.target.pending) out.push('not pending: ' + JSON.stringify(P.target));
          if (JSON.stringify(teAlt.texts()) !== '["paperclip"]') out.push('lines ' + teAlt.texts());
          if (document.activeElement !== teAlt.newInput()) out.push('focus not on the new line');
          if (teAlt.spans().length !== 0) out.push('a span was created before any alternative');
          if (!P.root.closest('#app') || P.root.getAttribute('aria-label') !== 'Alternatives') out.push('panel root');
          if (!document.querySelector('#app').classList.contains('te-alt-open')) out.push('text not dimmed');
        "##,
        // Enter on the new line creates the span with its first alternative,
        // live, without changing the text.
        r##"
          teAlt.type(teAlt.newInput(), 'eraser');
          const spans = teAlt.spans();
          if (spans.length !== 1 || JSON.stringify(spans[0].alts.map((a) => [a.text, a.source])) !== '[["paperclip","original"],["eraser","human"]]') return 'model ' + JSON.stringify(spans);
          if (s.getText() !== 'Pass me a paperclip. Drop this.') out.push('text changed');
          if (teAlt.P().target.spanId !== spans[0].id) out.push('panel not on the span');
          if (document.activeElement !== teAlt.newInput()) out.push('focus left the new line');
          if (teAlt.newInput().value !== '') out.push('new line not cleared');
          ed.indicators.flush();
          if (teAlt.dotsOf(spans[0].id) !== 2) out.push('dots ' + teAlt.dotsOf(spans[0].id));
          T.id = spans[0].id;
        "##,
        r##"
          teAlt.type(teAlt.newInput(), 'thumbtack');
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","eraser","thumbtack"]') return 'lines ' + teAlt.texts();
          // Up from the new line: the last alternative becomes active, live.
          teTest.key(teAlt.newInput(), 'ArrowUp');
          if (s.getText() !== 'Pass me a thumbtack. Drop this.') out.push('text ' + s.getText());
          if (teAlt.active() !== 2) out.push('active ' + teAlt.active());
          if (document.activeElement !== teAlt.inputs()[2]) out.push('focus not on line 3');
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        r##"
          teTest.key(teAlt.inputs()[2], 'ArrowUp');
          if (s.getText() !== 'Pass me an eraser. Drop this.') out.push('a/an: ' + s.getText());
          if (teAlt.active() !== 1) out.push('active ' + teAlt.active());
          if (!teAlt.inputs()[1].closest('li').classList.contains('te-alt-line--active')) out.push('active line not marked');
          // Down twice: thumbtack, then the new line (active unchanged).
          teTest.key(teAlt.inputs()[1], 'ArrowDown');
          teTest.key(teAlt.inputs()[2], 'ArrowDown');
          if (teAlt.active() !== 2 || document.activeElement !== teAlt.newInput()) out.push('down: ' + teAlt.active());
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        // Each change is one undo step; undo and redo keep model and text in step.
        r##"
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.active() !== 1) out.push('undo down: ' + s.getText());
          s.undo();
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me a paperclip. Drop this.' || teAlt.active() !== 0) out.push('undo up: ' + s.getText());
          if (!teAlt.inSync()) out.push('model out of step after undo');
          s.redo();
          s.redo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.active() !== 1) out.push('redo: ' + s.getText());
          if (!teAlt.inSync()) out.push('model out of step after redo');
        "##,
        // Editing the active line rewrites the text (with the a/an fix-up).
        r##"
          teAlt.type(teAlt.inputs()[1], 'pencil');
          if (s.getText() !== 'Pass me a pencil. Drop this.') out.push('edit active: ' + s.getText());
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil","thumbtack"]') out.push('lines ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model out of step');
          s.undo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me an eraser. Drop this.' || teAlt.texts()[1] !== 'eraser') out.push('undo edit: ' + teAlt.texts());
          s.redo();
          teAlt.P().flush();
          if (s.getText() !== 'Pass me a pencil. Drop this.' || !teAlt.inSync()) out.push('redo edit');
          // Editing an inactive line changes annotations only, still one step.
          teAlt.type(teAlt.inputs()[2], 'pin');
          if (teAlt.texts()[2] !== 'pin' || s.getText() !== 'Pass me a pencil. Drop this.') out.push('edit inactive');
          s.undo();
          teAlt.P().flush();
          if (teAlt.texts()[2] !== 'thumbtack') out.push('undo inactive edit: ' + teAlt.texts());
        "##,
        // Reorder with Alt+Up: the active alternative stays active.
        r##"
          teAlt.inputs()[2].focus();
          teTest.key(teAlt.inputs()[2], 'ArrowUp', { altKey: true });
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","thumbtack","pencil"]') return 'reorder ' + teAlt.texts();
          if (teAlt.active() !== 2 || s.getText() !== 'Pass me a pencil. Drop this.') out.push('active after reorder');
          if (document.activeElement !== teAlt.inputs()[1]) out.push('focus did not follow the line');
          teTest.key(teAlt.inputs()[1], 'ArrowUp', { altKey: true });
          if (teAlt.texts()[0] !== 'paperclip') out.push('moved above the original');
          s.undo();
          teAlt.P().flush();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil","thumbtack"]') out.push('undo reorder ' + teAlt.texts());
        "##,
        // Delete: the button, then an emptied line. Deleting the last one
        // removes the span and its indicator (R-2.7); undo brings both back.
        r##"
          teAlt.P().list.querySelectorAll('.te-alt-delete')[1].click();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil"]') return 'delete button ' + teAlt.texts();
          teAlt.type(teAlt.inputs()[1], '');
          if (teAlt.spans().length !== 0) return 'span kept ' + JSON.stringify(teAlt.spans());
          if (s.getText() !== 'Pass me a paperclip. Drop this.') out.push('original not shown: ' + s.getText());
          if (!teAlt.P().target.pending) out.push('panel did not fall back to pending');
          ed.indicators.flush();
          if (teAlt.dotsOf(T.id) !== 0) out.push('indicator kept');
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        r##"
          s.undo();
          s.undo();
          teAlt.P().flush();
          if (JSON.stringify(teAlt.texts()) !== '["paperclip","pencil","thumbtack"]') out.push('undo deletes ' + teAlt.texts());
          if (s.getText() !== 'Pass me a pencil. Drop this.' || !teAlt.inSync()) out.push('undo delete text ' + s.getText());
          ed.indicators.flush();
          if (teAlt.dotsOf(T.id) !== 3) out.push('indicator not back');
        "##,
        // Escape closes and returns focus to the surface.
        r##"
          teTest.key(teAlt.inputs()[1], 'Escape');
          if (teAlt.P().isOpen()) out.push('still open');
          if (teTest.visible(teAlt.P().root)) out.push('still visible');
          if (document.activeElement !== s.root) out.push('focus not returned');
          if (document.querySelector('#app').classList.contains('te-alt-open')) out.push('still dimmed');
        "##,
    ])
    .await;
    assert_eq!(result, "");
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
