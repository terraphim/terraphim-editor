//! Span lifecycle (R-2.7, R-5), live edits, export (R-9.3) and counts
//! (decision 3).

use terraphim_alternatives::{
    Counts, Document, EditError, Source, SpanFate, SpanKind, parse, write,
};

fn word_doc() -> (Document, String) {
    let mut d = Document::new("The tension rises.");
    let id = d.add_span(SpanKind::Word, 4, 11).unwrap();
    d.add_alternative(&id, "pressure", Source::Human, None)
        .unwrap();
    d.add_alternative(&id, "struggle", Source::Ai, Some("m".into()))
        .unwrap();
    (d, id)
}

#[test]
fn ids_are_sequential_and_skip_used_numbers() {
    let mut d = Document::new("one two three");
    assert_eq!(d.add_span(SpanKind::Word, 0, 3).unwrap(), "s1");
    assert_eq!(d.add_span(SpanKind::Word, 4, 7).unwrap(), "s2");
    d.set_ghost("s1", false).unwrap(); // inert -> removed
    assert_eq!(d.add_span(SpanKind::Word, 8, 13).unwrap(), "s3");
}

#[test]
fn add_span_rejects_bad_ranges_and_overlaps() {
    let mut d = Document::new("Café 𝄞 tension");
    assert_eq!(
        d.add_span(SpanKind::Word, 3, 3),
        Err(EditError::InvalidRange { start: 3, end: 3 })
    );
    assert!(matches!(
        d.add_span(SpanKind::Word, 6, 6),
        Err(EditError::InvalidRange { .. })
    ));
    assert!(
        matches!(
            d.add_span(SpanKind::Word, 5, 6),
            Err(EditError::InvalidRange { .. })
        ),
        "splits the clef"
    );
    assert!(matches!(
        d.add_span(SpanKind::Word, 9, 3),
        Err(EditError::InvalidRange { .. })
    ));
    assert!(matches!(
        d.add_span(SpanKind::Word, 0, 99),
        Err(EditError::InvalidRange { .. })
    ));
    let id = d.add_span(SpanKind::Word, 8, 15).unwrap();
    assert_eq!(d.span(&id).unwrap().anchor.text, "tension");
    assert_eq!(
        d.add_span(SpanKind::Sentence, 0, 15),
        Err(EditError::Overlap(id))
    );
}

#[test]
fn add_alternative_validates_and_deduplicates() {
    let (mut d, id) = word_doc();
    assert_eq!(
        d.add_alternative(&id, "", Source::Human, None),
        Err(EditError::EmptyText)
    );
    assert_eq!(
        d.add_alternative(&id, "x", Source::Original, None),
        Err(EditError::OriginalImmutable)
    );
    assert_eq!(d.add_alternative(&id, "pressure", Source::Ai, None), Ok(1));
    assert_eq!(
        d.add_alternative("nope", "x", Source::Human, None),
        Err(EditError::UnknownSpan("nope".into()))
    );
    assert_eq!(
        d.set_active(&id, 7),
        Err(EditError::InvalidIndex {
            id: id.clone(),
            index: 7
        })
    );
}

#[test]
fn removing_the_active_alternative_reverts_to_the_original() {
    let (mut d, id) = word_doc();
    d.set_active(&id, 2).unwrap();
    assert_eq!(d.remove_alternative(&id, 2), Ok(SpanFate::Kept));
    assert_eq!(d.body, "The tension rises.");
    assert_eq!(d.span(&id).unwrap().active, 0);
}

#[test]
fn removing_an_earlier_alternative_keeps_the_active_one() {
    let (mut d, id) = word_doc();
    d.set_active(&id, 2).unwrap();
    d.remove_alternative(&id, 1).unwrap();
    let span = d.span(&id).unwrap();
    assert_eq!(span.active, 1);
    assert_eq!(span.active_alternative().text, "struggle");
    assert_eq!(d.body, "The struggle rises.");
}

#[test]
fn removing_all_non_original_alternatives_removes_the_span() {
    let (mut d, id) = word_doc();
    assert_eq!(
        d.remove_alternative(&id, 0),
        Err(EditError::OriginalImmutable)
    );
    assert_eq!(d.remove_alternative(&id, 2), Ok(SpanFate::Kept));
    assert_eq!(d.remove_alternative(&id, 1), Ok(SpanFate::Removed));
    assert!(d.annotations.spans.is_empty());
    assert_eq!(d.body, "The tension rises.");
    assert_eq!(write(&d), "The tension rises.", "no block once empty");
}

#[test]
fn clear_keeps_the_visible_text_as_plain_text() {
    let (mut d, id) = word_doc();
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.clear_alternatives(&id), Ok(SpanFate::Removed));
    assert_eq!(d.body, "The pressure rises.");
    assert!(d.annotations.spans.is_empty());
}

#[test]
fn ghosted_span_survives_losing_its_alternatives() {
    let (mut d, id) = word_doc();
    d.set_active(&id, 1).unwrap();
    d.set_ghost(&id, true).unwrap();
    assert_eq!(d.clear_alternatives(&id), Ok(SpanFate::Kept));
    let span = d.span(&id).unwrap();
    assert_eq!(span.alts.len(), 1);
    assert_eq!(span.alts[0].source, Source::Original);
    assert_eq!(span.alts[0].text, "pressure");
    // Reviving a span with nothing else to keep removes it.
    assert_eq!(d.set_ghost(&id, false), Ok(SpanFate::Removed));
    assert_eq!(d.body, "The pressure rises.");
}

#[test]
fn ghost_and_revive_keep_alternatives() {
    let (mut d, id) = word_doc();
    assert_eq!(d.set_ghost(&id, true), Ok(SpanFate::Kept));
    assert_eq!(d.set_ghost(&id, false), Ok(SpanFate::Kept));
    assert_eq!(d.span(&id).unwrap().alts.len(), 3);
}

#[test]
fn apply_edit_shifts_before_keeps_after_and_detaches_inside() {
    let mut d = Document::new("Alpha beta gamma delta");
    for (s, e) in [(6, 10), (11, 16)] {
        let id = d.add_span(SpanKind::Word, s, e).unwrap();
        d.add_alternative(&id, "x", Source::Human, None).unwrap();
    }
    // Insert before both (astral char: two units).
    assert!(d.apply_edit(0, 0, "𝄞 ").unwrap().is_empty());
    assert_eq!(d.annotations.spans[0].anchor.start, 9);
    // Insertion exactly at a span's start counts as before; at its end, after.
    assert!(d.apply_edit(9, 9, "[").unwrap().is_empty());
    assert_eq!(d.annotations.spans[0].anchor.start, 10);
    assert!(d.apply_edit(14, 14, "]").unwrap().is_empty());
    assert_eq!(d.body, "𝄞 Alpha [beta] gamma delta");
    assert_eq!(d.annotations.spans[1].anchor.start, 16);
    // Delete after: nothing moves.
    d.apply_edit(21, 27, "").unwrap();
    assert_eq!(d.body, "𝄞 Alpha [beta] gamma");
    // Edit inside "gamma": detached and returned, not dropped.
    let detached = d.apply_edit(17, 18, "A").unwrap();
    assert_eq!(detached.len(), 1);
    assert_eq!(detached[0].anchor.text, "gamma");
    assert_eq!(d.annotations.spans.len(), 1);
    assert_eq!(d.body, "𝄞 Alpha [beta] gAmma");
    // The remaining span is still usable.
    d.set_active("s1", 1).unwrap();
    assert_eq!(d.body, "𝄞 Alpha [x] gAmma");
    assert!(matches!(
        d.apply_edit(1, 1, "x"),
        Err(EditError::InvalidRange { .. })
    ));
}

#[test]
fn export_drops_ghosted_spans_and_omits_overflow_and_block() {
    let mut d = Document::new(
        "The big tension rises. It hedges. Done.\n\nKeep me.\n\nCut paragraph.\n\nEnd.\n",
    );
    let big = d.add_span(SpanKind::Word, 4, 7).unwrap();
    d.set_ghost(&big, true).unwrap();
    let hedge = d.add_span(SpanKind::Sentence, 23, 33).unwrap();
    d.set_ghost(&hedge, true).unwrap();
    let para = d.add_span(SpanKind::Paragraph, 51, 65).unwrap();
    d.set_ghost(&para, true).unwrap();
    let t = d.add_span(SpanKind::Word, 8, 15).unwrap();
    d.add_alternative(&t, "pressure", Source::Human, None)
        .unwrap();
    d.set_active(&t, 1).unwrap();
    d.annotations.overflow = "secret stash".into();

    let exported = d.export();
    assert_eq!(exported, "The pressure rises. Done.\n\nKeep me.\n\nEnd.\n");
    assert!(!exported.contains("terraphim-alternatives"));
    assert!(!exported.contains("secret"));

    // Export of a parsed file equals export of the in-memory document.
    assert_eq!(parse(&write(&d)).unwrap().export(), exported);
}

#[test]
fn export_tidies_spaces_at_line_starts_and_before_punctuation() {
    let mut d = Document::new("Big start here, and an ending word.");
    let a = d.add_span(SpanKind::Word, 0, 3).unwrap();
    d.set_ghost(&a, true).unwrap();
    let b = d.add_span(SpanKind::Word, 30, 34).unwrap();
    d.set_ghost(&b, true).unwrap();
    assert_eq!(d.export(), "start here, and an ending.");
}

#[test]
fn export_skips_stale_spans_rather_than_cutting_the_wrong_text() {
    let mut d = Document::new("Keep this, drop that.");
    let id = d.add_span(SpanKind::Word, 11, 15).unwrap();
    d.set_ghost(&id, true).unwrap();
    d.body.insert_str(0, ">> ");
    assert_eq!(d.export(), ">> Keep this, drop that.");
}

#[test]
fn counts_include_ghosted_text_and_exclude_the_block() {
    let mut d = Document::new("Café 🎯 has five words.");
    let id = d.add_span(SpanKind::Word, 0, 4).unwrap();
    d.set_ghost(&id, true).unwrap();
    d.annotations.overflow = "many many more words in the stash".into();
    let expected = Counts {
        words: 5,
        chars: 22,
    };
    assert_eq!(d.counts(), expected);
    assert_eq!(parse(&write(&d)).unwrap().counts(), expected);
}
