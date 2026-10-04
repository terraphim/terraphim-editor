//! Re-anchoring acceptance tests (R-9.2): edits before, inside and after a
//! span, duplicate anchor text, and a deleted anchor. Each case edits the
//! body directly, as an outside editor would, then calls `reanchor`.

use terraphim_alternatives::{Document, Source, SpanKind, UnresolvedReason, utf16_len};

/// "The tension rises. Then 𝄞 the end." with a span on "tension".
fn doc_with_span() -> Document {
    let mut doc = Document::new("The tension rises. Then 𝄞 the end.");
    let id = doc.add_span(SpanKind::Word, 4, 11).unwrap();
    doc.add_alternative(&id, "pressure", Source::Human, None)
        .unwrap();
    doc
}

fn anchor(doc: &Document) -> (usize, usize) {
    let a = &doc.annotations.spans[0].anchor;
    (a.start, a.end)
}

#[test]
fn unchanged_body_is_clean_and_nothing_moves() {
    let mut doc = doc_with_span();
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert!(report.moved.is_empty());
    assert_eq!(anchor(&doc), (4, 11));
}

#[test]
fn insert_before_shifts_by_utf16_length() {
    let mut doc = doc_with_span();
    doc.body.insert_str(0, "Ünïcødé 𝄞 ");
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(report.moved, vec!["s1".to_string()]);
    let shift = utf16_len("Ünïcødé 𝄞 ");
    assert_eq!(shift, 11, "clef counts as two units");
    assert_eq!(anchor(&doc), (4 + shift, 11 + shift));
}

#[test]
fn delete_before_shifts_back() {
    let mut doc = doc_with_span();
    doc.body.replace_range(0..4, "");
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(anchor(&doc), (0, 7));
}

#[test]
fn insert_and_delete_after_leave_the_span_alone() {
    let mut doc = doc_with_span();
    doc.body.push_str(" Appended 𝄞.");
    doc.body = doc.body.replacen("rises", "", 1);
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert!(report.moved.is_empty());
    assert_eq!(anchor(&doc), (4, 11));
}

#[test]
fn insert_inside_reports_missing() {
    let mut doc = doc_with_span();
    doc.body = doc.body.replacen("tension", "tensions", 1);
    // "tensions" still contains "tension" at the hint, so this one survives.
    assert!(doc.reanchor().is_clean());

    let mut doc = doc_with_span();
    doc.body = doc.body.replacen("tension", "tenXsion", 1);
    let report = doc.reanchor();
    assert_eq!(report.unresolved.len(), 1);
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::Missing);
    assert_eq!(
        report.unresolved[0].span.id, "s1",
        "span returned, not dropped"
    );
    assert!(doc.annotations.spans.is_empty());
}

#[test]
fn delete_inside_reports_missing() {
    let mut doc = doc_with_span();
    doc.body = doc.body.replacen("tension", "tion", 1);
    let report = doc.reanchor();
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::Missing);
}

#[test]
fn deleted_anchor_reports_missing() {
    let mut doc = doc_with_span();
    doc.body = doc.body.replacen("tension ", "", 1);
    let report = doc.reanchor();
    assert_eq!(report.unresolved.len(), 1);
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::Missing);
}

#[test]
fn duplicate_text_at_the_hint_is_resolved_by_the_hint() {
    let mut doc = Document::new("the cat and the cat");
    let id = doc.add_span(SpanKind::Word, 16, 19).unwrap();
    doc.add_alternative(&id, "dog", Source::Human, None)
        .unwrap();
    doc.body.push_str(" sat");
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(anchor(&doc), (16, 19));
}

#[test]
fn duplicate_text_away_from_the_hint_is_ambiguous_not_guessed() {
    let mut doc = Document::new("the cat and the cat");
    let id = doc.add_span(SpanKind::Word, 16, 19).unwrap();
    doc.add_alternative(&id, "dog", Source::Human, None)
        .unwrap();
    doc.body.insert_str(0, "Oh, ");
    let report = doc.reanchor();
    assert_eq!(
        report.unresolved[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![8, 20]
        }
    );
    assert!(doc.annotations.spans.is_empty());
}

#[test]
fn two_spans_sharing_text_do_not_collapse_onto_one_survivor() {
    let mut doc = Document::new("the cat and the cat");
    for start in [4, 16] {
        let id = doc.add_span(SpanKind::Word, start, start + 3).unwrap();
        doc.add_alternative(&id, "dog", Source::Human, None)
            .unwrap();
    }
    // Delete the first "cat" and shift the survivor away from both hints.
    doc.body = "Oh no, the and the cat".to_string();
    let report = doc.reanchor();
    assert_eq!(report.unresolved.len(), 2);
    for unresolved in &report.unresolved {
        assert_eq!(
            unresolved.reason,
            UnresolvedReason::Ambiguous {
                candidates: vec![19]
            }
        );
    }
}

#[test]
fn hint_claim_leaves_the_other_span_missing() {
    let mut doc = Document::new("the cat and the cat");
    for start in [4, 16] {
        let id = doc.add_span(SpanKind::Word, start, start + 3).unwrap();
        doc.add_alternative(&id, "dog", Source::Human, None)
            .unwrap();
    }
    doc.body = "the cat and the".to_string();
    let report = doc.reanchor();
    assert_eq!(doc.annotations.spans.len(), 1);
    assert_eq!(doc.annotations.spans[0].id, "s1");
    assert_eq!(report.unresolved[0].span.id, "s2");
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::Missing);
}

#[test]
fn spans_with_different_text_reanchor_independently() {
    let mut doc = Document::new("Alpha beta gamma.");
    for (start, end) in [(0, 5), (11, 16)] {
        let id = doc.add_span(SpanKind::Word, start, end).unwrap();
        doc.add_alternative(&id, "x", Source::Human, None).unwrap();
    }
    doc.body = "Intro. Alpha and beta and gamma.".to_string();
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(doc.annotations.spans[0].anchor.start, 7);
    assert_eq!(doc.annotations.spans[1].anchor.start, 26);
}

#[test]
fn reanchored_document_supports_further_edits() {
    let mut doc = doc_with_span();
    doc.body.insert_str(0, "Prologue. ");
    assert!(doc.reanchor().is_clean());
    doc.set_active("s1", 1).unwrap();
    assert!(doc.body.starts_with("Prologue. The pressure rises."));
}
