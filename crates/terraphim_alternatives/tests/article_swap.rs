//! a/an fix-up acceptance tests (R-2.6).

use terraphim_alternatives::{Document, Source, SpanKind, byte_to_utf16, utf16_len};

/// Builds a document with a span over the first occurrence of `word` and the
/// given alternatives (the original being `word`).
fn doc(body: &str, word: &str, alts: &[&str]) -> (Document, String) {
    let mut doc = Document::new(body);
    let start = byte_to_utf16(body, body.find(word).unwrap()).unwrap();
    let id = doc
        .add_span(SpanKind::Word, start, start + utf16_len(word))
        .unwrap();
    for alt in alts {
        doc.add_alternative(&id, *alt, Source::Human, None).unwrap();
    }
    (doc, id)
}

fn assert_anchor_matches(doc: &Document, id: &str) {
    let span = doc.span(id).unwrap();
    let units: Vec<u16> = doc.body.encode_utf16().collect();
    let text = String::from_utf16(&units[span.anchor.start..span.anchor.end]).unwrap();
    assert_eq!(text, span.anchor.text);
}

#[test]
fn paperclip_eraser_thumbtack_and_back() {
    let (mut d, id) = doc(
        "Hand me a paperclip, please.",
        "paperclip",
        &["eraser", "thumbtack"],
    );
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.body, "Hand me an eraser, please.");
    assert_anchor_matches(&d, &id);
    d.set_active(&id, 2).unwrap();
    assert_eq!(d.body, "Hand me a thumbtack, please.");
    assert_anchor_matches(&d, &id);
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.body, "Hand me an eraser, please.");
    d.set_active(&id, 0).unwrap();
    assert_eq!(d.body, "Hand me a paperclip, please.");
    assert_anchor_matches(&d, &id);
}

#[test]
fn cycling_wraps_in_both_directions() {
    let (mut d, id) = doc(
        "Hand me a paperclip.",
        "paperclip",
        &["eraser", "thumbtack"],
    );
    assert_eq!(d.cycle_active(&id, -1).unwrap(), 2);
    assert_eq!(d.body, "Hand me a thumbtack.");
    assert_eq!(d.cycle_active(&id, 1).unwrap(), 0);
    assert_eq!(d.cycle_active(&id, 1).unwrap(), 1);
    assert_eq!(d.body, "Hand me an eraser.");
}

#[test]
fn no_article_means_no_change_elsewhere() {
    let (mut d, id) = doc("The paperclip is here.", "paperclip", &["eraser"]);
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.body, "The eraser is here.");
}

#[test]
fn capitalised_article_keeps_its_case() {
    let (mut d, id) = doc("A paperclip. AN eraser?", "paperclip", &["eraser"]);
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.body, "An eraser. AN eraser?");
    d.set_active(&id, 0).unwrap();
    assert_eq!(d.body, "A paperclip. AN eraser?");
}

#[test]
fn article_not_immediately_preceding_is_untouched() {
    let (mut d, id) = doc("It is a big paperclip.", "paperclip", &["eraser"]);
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.body, "It is a big eraser.");
}

#[test]
fn only_the_preceding_article_changes() {
    let (mut d, id) = doc("A cat, a paperclip and a dog.", "paperclip", &["eraser"]);
    d.set_active(&id, 1).unwrap();
    assert_eq!(d.body, "A cat, an eraser and a dog.");
}

#[test]
fn later_spans_shift_when_the_article_grows() {
    let mut d = Document::new("A paperclip and a 🎯 tension.");
    let p = d.add_span(SpanKind::Word, 2, 11).unwrap();
    d.add_alternative(&p, "eraser", Source::Human, None)
        .unwrap();
    let t_start = utf16_len("A paperclip and a 🎯 ");
    let t = d.add_span(SpanKind::Word, t_start, t_start + 7).unwrap();
    d.add_alternative(&t, "struggle", Source::Human, None)
        .unwrap();

    d.set_active(&p, 1).unwrap();
    assert_eq!(d.body, "An eraser and a 🎯 tension.");
    assert_anchor_matches(&d, &p);
    assert_anchor_matches(&d, &t);
    d.set_active(&t, 1).unwrap();
    assert_eq!(d.body, "An eraser and a 🎯 struggle.");
    assert_anchor_matches(&d, &t);
}

#[test]
fn article_inside_another_span_is_left_alone() {
    let mut d = Document::new("Take a paperclip.");
    let article = d.add_span(SpanKind::Word, 5, 6).unwrap();
    d.add_alternative(&article, "one", Source::Human, None)
        .unwrap();
    let p = d.add_span(SpanKind::Word, 7, 16).unwrap();
    d.add_alternative(&p, "eraser", Source::Human, None)
        .unwrap();
    d.set_active(&p, 1).unwrap();
    assert_eq!(d.body, "Take a eraser.");
    assert_anchor_matches(&d, &article);
}

#[test]
fn stale_anchor_is_refused() {
    let (mut d, id) = doc("Hand me a paperclip.", "paperclip", &["eraser"]);
    d.body.insert_str(0, "Now ");
    let before = d.clone();
    assert!(d.set_active(&id, 1).is_err());
    assert_eq!(d, before, "document unchanged on error");
}
