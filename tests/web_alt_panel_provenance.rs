//! Actual browser rendering and save/reopen of KG versus AI provenance.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn kg_and_ai_labels_remain_distinct_after_save_reopen() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let mut doc = Document::new("Decision: launch the pilot.");
    let id = doc.add_span(SpanKind::Word, 0, 8).unwrap();
    doc.add_alternative(&id, "Judgment", Source::Ai, Some("kg".into()))
        .unwrap();
    doc.add_alternative(&id, "Resolution", Source::Ai, Some("provider".into()))
        .unwrap();
    doc.add_alternative(&id, "Choice", Source::Human, None)
        .unwrap();
    let fixture = js_string_literal(&write(&doc));
    let setup = format!(
        r##"
        ed.openDocument({fixture});
        ed.chrome.setMode('write-on');
        s.focus(); s.setSelectionOffsets(0, 8);
        teTest.control('alternatives').click();
        "##
    );
    let check = r##"
        const labels = teAlt.inputs().slice(0, 4).map(i => i.getAttribute('aria-label'));
        if (!labels[0].includes('original, read-only')) out.push('original label');
        if (!labels[1].includes('knowledge graph synonym') || labels[1].includes('AI')) out.push('KG label');
        if (!labels[2].includes('suggested by AI')) out.push('AI label');
        if (!labels[3].includes('written by you')) out.push('human label');
        if (teAlt.P().lines()[1].model !== 'kg') out.push('lost KG provenance');
        if (s.getText() !== 'Decision: launch the pilot.') out.push('body changed');
    "##;
    let result = run_steps(&[
        &setup,
        check,
        r##"
        const saved = ed.saveDocument();
        ed.openDocument(saved);
        s.focus(); s.setSelectionOffsets(0, 8);
        ed.altPanel.open({ spanId: teAlt.spans()[0].id });
        "##,
        check,
    ])
    .await;
    assert_eq!(result, "");
}
