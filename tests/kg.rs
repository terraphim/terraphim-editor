//! Native tests for the knowledge-graph alternatives bridge (issue #13):
//! loading a thesaurus, the derived KG spans of an open document, swaps with
//! the core's capitalisation and a/an fix-up, the annotation block excluded,
//! block spans winning over KG terms, refused swaps, and "AI alternatives for
//! selection" (R-8.6). The real fixture thesaurus and the real
//! `terraphim_lsp_core`; nothing is mocked.
#![cfg(not(target_arch = "wasm32"))]

use terraphim_alternatives::{utf16_to_byte, write, Document, Source, SpanKind};
use terraphim_editor::{
    clear_thesaurus, kg_append, kg_lookup, kg_spans, kg_spans_json, kg_swap, load_thesaurus,
    thesaurus_name, DocumentSession, KgError, SwapEdit, KG_MODEL,
};

const THESAURUS: &str = include_str!("fixtures/kg/thesaurus.json");
const DOC: &str = include_str!("fixtures/kg/doc.md");

/// A session with `source` open and the fixture thesaurus loaded.
fn session(source: &str) -> DocumentSession {
    load_thesaurus(THESAURUS).expect("fixture thesaurus loads");
    let mut session = DocumentSession::new();
    session.open(source);
    session
}

/// `id: alt|alt|...@active` for every KG span.
fn summary(session: &DocumentSession) -> Vec<String> {
    kg_spans(session)
        .iter()
        .map(|s| format!("{}: {}@{}", s.id, s.alts.join("|"), s.active))
        .collect()
}

/// UTF-16 offset of the first `needle` in `text` (after `from` bytes).
fn at(text: &str, needle: &str, from: usize) -> usize {
    let byte = from + text[from..].find(needle).expect("needle present");
    text[..byte].encode_utf16().count()
}

/// Applies a swap edit to `text`, checking the deleted text.
fn apply(text: &str, edit: &SwapEdit) -> String {
    let start = utf16_to_byte(text, edit.start).unwrap();
    let end = start + edit.deleted_text.len();
    assert_eq!(&text[start..end], edit.deleted_text);
    format!("{}{}{}", &text[..start], edit.inserted_text, &text[end..])
}

#[test]
fn loading_reports_the_thesaurus_and_a_bad_one_keeps_the_previous() {
    let summary = load_thesaurus(THESAURUS).unwrap();
    assert_eq!(summary["name"], "writing-fixture");
    assert_eq!(summary["concepts"], 7);
    assert_eq!(thesaurus_name().as_deref(), Some("writing-fixture"));

    let error = load_thesaurus("{ not json").unwrap_err();
    assert!(matches!(error, KgError::Thesaurus(_)), "{error:?}");
    assert_eq!(thesaurus_name().as_deref(), Some("writing-fixture"));
    let mut s = DocumentSession::new();
    s.open(DOC);
    assert!(!kg_spans(&s).is_empty(), "the previous thesaurus stays");

    clear_thesaurus();
    assert!(kg_spans(&s).is_empty());
    assert_eq!(thesaurus_name(), None);
}

#[test]
fn kg_spans_list_every_synonym_in_concept_order_with_the_text_active() {
    let s = session(DOC);
    assert_eq!(
        summary(&s),
        [
            "kg-8-0: Use|Employ|Utilise@0",
            "kg-2-0: eraser|rubber@0",
            "kg-8-1: use|employ|utilise@0",
            // paperclip is a single-term concept: no alternatives, no span,
            // but it still takes its ordinal (kg-7-0).
            "kg-8-2: USE|EMPLOY|UTILISE@0",
            "kg-1-0: decision|choice|judgment|option@1",
        ]
    );
    let body = s.body();
    let eraser = &kg_spans(&s)[1];
    assert_eq!(
        (eraser.start, eraser.end),
        (at(body, "eraser", 0), at(body, "eraser", 0) + 6)
    );
    let json = kg_spans_json(&s);
    assert_eq!(json[1]["source"], "kg");
    assert_eq!(json[1]["kind"], "word");
    assert_eq!(json[1]["anchor"]["text"], "eraser");
    assert_eq!(json[1]["alts"][1]["text"], "rubber");
    assert_eq!(json[1]["alts"][1]["source"], "kg");
    assert_eq!(json[1]["conceptId"], 2);
    assert_eq!(json[4]["nterm"], "decision");
}

#[test]
fn swaps_apply_the_cores_capitalisation_and_article_as_one_edit() {
    let mut s = session(DOC);
    let mut text = s.body().to_string();

    // an eraser -> a rubber: one edit over the article and the term.
    let swap = kg_swap(&mut s, "kg-2-0", 1).unwrap();
    assert_eq!((swap.from, swap.to), (0, 1));
    let edit = swap.edit.clone().unwrap();
    assert_eq!(edit.start, at(&text, "an eraser", 0));
    assert_eq!(edit.deleted_text, "an eraser");
    assert_eq!(edit.inserted_text, "a rubber");
    text = apply(&text, &edit);
    assert_eq!(s.body(), text);
    assert!(text.contains("Use a rubber, then"));
    assert_eq!(
        swap.range,
        (at(&text, "rubber", 0), at(&text, "rubber", 0) + 6)
    );
    assert!(swap.outcome.detached.is_empty());
    // The id survives the swap; the lit dot moved.
    assert!(summary(&s).contains(&"kg-2-0: eraser|rubber@1".to_string()));

    // Capitalisation follows the text: Use / use / USE.
    for (id, index, want) in [
        ("kg-8-0", 2, "Utilise a rubber"),
        ("kg-8-1", 1, "then employ a paperclip"),
        ("kg-8-2", 1, "EMPLOY IT."),
    ] {
        let edit = kg_swap(&mut s, id, index).unwrap().edit.unwrap();
        text = apply(&text, &edit);
        assert_eq!(s.body(), text);
        assert!(text.contains(want), "{id}: {text}");
    }

    // a rubber -> an eraser, and wrap the first word back to "Use".
    text = apply(&text, &kg_swap(&mut s, "kg-2-0", 0).unwrap().edit.unwrap());
    text = apply(&text, &kg_swap(&mut s, "kg-8-0", 0).unwrap().edit.unwrap());
    assert!(text.starts_with("# Writing sample\n\nUse an eraser, then employ"));
    assert_eq!(s.body(), text);

    // Derived spans are never saved: no annotation block appears.
    assert_eq!(s.save(), text);
}

#[test]
fn choosing_the_active_form_or_a_bad_target_changes_nothing() {
    let mut s = session(DOC);
    let before = s.clone();
    let swap = kg_swap(&mut s, "kg-1-0", 1).unwrap();
    assert_eq!(swap.edit, None);
    assert_eq!(s, before);
    assert_eq!(
        kg_swap(&mut s, "kg-1-0", 4).unwrap_err(),
        KgError::InvalidIndex {
            id: "kg-1-0".into(),
            index: 4
        }
    );
    assert_eq!(
        kg_swap(&mut s, "kg-7-0", 0).unwrap_err(),
        KgError::UnknownSpan("kg-7-0".into())
    );
    assert_eq!(s, before);
}

/// The fixture body with block span `s1` over "eraser" (human alternative
/// "pencil") and `s2` over the article "an", plus an alternative in the block
/// that mentions KG terms.
fn annotated() -> String {
    let body = DOC;
    let mut doc = Document::new(body);
    let eraser = at(body, "eraser", 0);
    let s1 = doc.add_span(SpanKind::Word, eraser, eraser + 6).unwrap();
    doc.add_alternative(&s1, "pencil", Source::Human, None)
        .unwrap();
    let an = at(body, "an eraser", 0);
    let s2 = doc.add_span(SpanKind::Word, an, an + 2).unwrap();
    doc.add_alternative(&s2, "one choice of", Source::Human, None)
        .unwrap();
    write(&doc)
}

#[test]
fn block_spans_win_and_the_block_is_never_analysed() {
    let source = annotated();
    assert!(source.contains("terraphim-alternatives"));
    let mut s = session(&source);
    assert_eq!(s.body(), DOC);
    let spans = summary(&s);
    // "eraser" is the writer's span; the block's "one choice of" is not text.
    assert!(!spans.iter().any(|x| x.starts_with("kg-2-0")), "{spans:?}");
    assert_eq!(spans.len(), 4, "{spans:?}");
    let body_len = s.body().encode_utf16().count();
    assert!(kg_spans(&s).iter().all(|k| k.end <= body_len));

    // Swapping "Use" leaves the writer's spans alone; save keeps only them.
    kg_swap(&mut s, "kg-8-0", 1).unwrap();
    let saved = s.save();
    assert!(saved.starts_with("# Writing sample\n\nEmploy an eraser"));
    let mut reopened = DocumentSession::new();
    reopened.open(&saved);
    let ids: Vec<&str> = reopened
        .document()
        .annotations
        .spans
        .iter()
        .map(|sp| sp.id.as_str())
        .collect();
    assert_eq!(ids, ["s1", "s2"]);
}

#[test]
fn a_swap_that_would_change_a_block_span_is_refused() {
    // Only "an" is the writer's span: eraser is a KG span, but swapping it to
    // "rubber" would rewrite the article inside span s1.
    let body = DOC;
    let mut doc = Document::new(body);
    let an = at(body, "an eraser", 0);
    let s2 = doc.add_span(SpanKind::Word, an, an + 2).unwrap();
    doc.add_alternative(&s2, "the", Source::Human, None)
        .unwrap();
    let mut s = session(&write(&doc));
    let before = s.clone();
    assert_eq!(
        kg_swap(&mut s, "kg-2-0", 1).unwrap_err(),
        KgError::Overlaps("s1".into())
    );
    assert_eq!(s, before);
}

#[test]
fn ai_alternatives_for_selection_append_after_the_writers_own() {
    let mut s = session(&annotated());
    let body = s.body().to_string();

    // Selection on "eraser" (span s1, human "pencil"): KG lines follow it.
    let eraser = at(&body, "eraser", 0);
    let lookup = kg_lookup(&s, eraser, eraser + 6);
    assert_eq!(lookup["span"], "s1");
    assert_eq!(lookup["alternatives"][0]["text"], "rubber");
    assert_eq!(
        kg_append(&mut s, eraser, eraser + 6).unwrap(),
        ("s1".into(), 1)
    );
    let span = &s.document().annotations.spans[0];
    let alts: Vec<(&str, Source, Option<&str>)> = span
        .alts
        .iter()
        .map(|a| (a.text.as_str(), a.source, a.model.as_deref()))
        .collect();
    assert_eq!(
        alts,
        [
            ("eraser", Source::Original, None),
            ("pencil", Source::Human, None),
            ("rubber", Source::Ai, Some(KG_MODEL)),
        ]
    );
    // Asking again adds nothing.
    assert_eq!(
        kg_append(&mut s, eraser, eraser + 6).unwrap(),
        ("s1".into(), 0)
    );

    // A caret in "choice" (no span yet): a word span is created and the KG
    // span gives way to it.
    let choice = at(&body, "choice", 0) + 2;
    let (id, added) = kg_append(&mut s, choice, choice).unwrap();
    assert_eq!(added, 3);
    let span = s
        .document()
        .annotations
        .spans
        .iter()
        .find(|sp| sp.id == id)
        .unwrap();
    assert_eq!(span.anchor.text, "choice");
    let texts: Vec<&str> = span.alts.iter().map(|a| a.text.as_str()).collect();
    assert_eq!(texts, ["choice", "decision", "judgment", "option"]);
    assert!(!summary(&s).iter().any(|x| x.starts_with("kg-1-0")));
    // They are persisted, as AI lines from the KG.
    assert!(s.save().contains("\"model\": \"kg\"") || s.save().contains("\"model\":\"kg\""));

    // No KG term: refused, nothing changes.
    let before = s.clone();
    let sample = at(&body, "sample", 0);
    assert_eq!(
        kg_append(&mut s, sample, sample + 6).unwrap_err(),
        KgError::NoTerm
    );
    assert!(kg_lookup(&s, sample, sample + 6).is_null());
    assert_eq!(s, before);
}
