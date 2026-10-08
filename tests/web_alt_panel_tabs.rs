//! Browser tests for the alternatives side panel (issue #10), the chrome
//! control, dots and tabs: the `●●●` control with ARIA, provenance glyphs and
//! colours from a file's `source` fields, an AI addition undone, Ctrl+Z inside
//! the panel and a draft keeping its own undo, tabs by keyboard, the overlap
//! refusal, a pending sentence created and swapped, save and reopen, a dot
//! click opening the panel without taking focus, and the video layout
//! (one-line tabs, the quiet count, tab and hint placement; issue #107).
//! Split from `web_alt_panel.rs` to keep each binary inside its time budget.
//! Run with `wasm-pack test --headless --chrome --test web_alt_panel_tabs`.
//! Real scripts, the real exported document API and real documents; nothing
//! is mocked.
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

/// The tab row as the panel builds it, for the sized-frame checks.
const TABS_MARKUP: &str = r#"<aside class="te-alt-panel"><div class="te-alt-tabs" role="tablist"><button class="te-alt-tab">Word</button><button class="te-alt-tab">Sentence</button><button class="te-alt-tab">Paragraph</button></div></aside>"#;

/// R-4.2, R-4.1 (issue #107), as in the video at 1:15 to 1:35: the three
/// cursive tabs sit on one line at every panel width (the 12rem floor at a
/// 900px window, 205px at 1140px, the 20rem cap at 2000px); the routine
/// count is announced but not shown, while a message that asks for
/// something stays visible; the tabs sit just above the document's first
/// line and the key hint at the panel's foot, clear of the corner controls.
#[wasm_bindgen_test]
async fn test_tabs_one_line_quiet_count_and_video_layout() {
    for vw in [900.0, 1140.0, 2000.0] {
        let r = eval_in_viewport(
            vw,
            800.0,
            TABS_MARKUP,
            ".te-alt-tabs",
            r#"const t = Array.from(el.children).map((x) => x.getBoundingClientRect());
            const p = d.querySelector('.te-alt-panel'), cs = getComputedStyle(p);
            const content = p.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
            return JSON.stringify({ tops: Array.from(new Set(t.map((r) => Math.round(r.top)))), row: t[2].right - t[0].left, content });"#,
        )
        .await;
        let v: serde_json::Value = serde_json::from_str(&r).unwrap();
        assert_eq!(
            v["tops"].as_array().unwrap().len(),
            1,
            "tabs wrap at a {vw}px window: {r}"
        );
        assert!(
            v["row"].as_f64().unwrap() <= v["content"].as_f64().unwrap() + 0.5,
            "tab row wider than the panel at a {vw}px window: {r}"
        );
    }

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
          const tabs = Object.values(P.tabs).map((t) => t.getBoundingClientRect());
          if (new Set(tabs.map((r) => Math.round(r.top))).size !== 1) out.push('tabs wrap: ' + JSON.stringify(tabs.map((r) => r.top)));
          // The routine count: announced, not shown.
          const st = P.statusEl;
          if (!/^Word: 3 alternatives, 1 of 3 active\.$/.test(st.textContent)) out.push('status text ' + st.textContent);
          if (st.getAttribute('aria-live') !== 'polite' || st.getAttribute('role') !== 'status') out.push('status not live');
          const sr = st.getBoundingClientRect();
          if (sr.width > 1 || sr.height > 1) out.push('count is visible: ' + JSON.stringify([sr.width, sr.height]));
          // The tabs sit just above the first line of the document.
          const line = s.rangeForOffsets(0, 4).getClientRects()[0];
          const tb = P.tablist.getBoundingClientRect();
          if (!(tb.bottom <= line.top && line.top - tb.top < 64)) out.push('tabs not just above the first line: tabs ' + JSON.stringify([tb.top, tb.bottom]) + ' line ' + line.top);
          // The hint at the panel's foot, above the bottom-left controls.
          const hint = P.root.querySelector('.te-alt-hint').getBoundingClientRect();
          const pr = P.root.getBoundingClientRect(), pad = parseFloat(getComputedStyle(P.root).paddingBottom);
          if (Math.abs(hint.bottom - (pr.bottom - pad)) > 1) out.push('hint not at the foot: ' + hint.bottom + ' vs ' + (pr.bottom - pad));
          if (hint.bottom > teTest.control('save').getBoundingClientRect().top) out.push('hint under the save control');
          if (hint.top < P.list.getBoundingClientRect().bottom) out.push('hint overlaps the list');
        "##
    );
    let result = run_steps(&[
        first.as_str(),
        // A message that asks for something stays visible.
        r##"
          const P = teAlt.P();
          ed.altPanel.open({ start: 21, end: 31 });
          const st = P.statusEl, sr = st.getBoundingClientRect();
          if (!/type a first alternative/.test(st.textContent)) return 'no pending message: ' + st.textContent;
          if (sr.width <= 1 || sr.height <= 1) out.push('message hidden');
          P.close({ restoreFocus: false });
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
