//! Context re-anchoring acceptance tests (issue #36, decision 2026-10-05:
//! "require context"): anchors store the text around them, and re-anchoring
//! accepts a moved occurrence only when that context agrees, so an edit
//! inside a span is reported instead of attached to identical text elsewhere.

mod common;

use common::strip_context;
use terraphim_alternatives::{
    Anchor, CONTEXT_UNITS, Document, Ghost, Source, Span, SpanKind, UnresolvedReason, parse,
    utf16_len, write,
};

/// UTF-16 offset of the first occurrence of `needle` in the body.
fn at(doc: &Document, needle: &str) -> usize {
    utf16_len(&doc.body[..doc.body.find(needle).expect(needle)])
}

/// Adds a word span over the first `word` with one human alternative.
fn span(doc: &mut Document, word: &str) -> String {
    let start = at(doc, word);
    let id = doc
        .add_span(SpanKind::Word, start, start + utf16_len(word))
        .unwrap();
    doc.add_alternative(&id, "alternative", Source::Human, None)
        .unwrap();
    id
}

fn context(anchor: &Anchor) -> (&str, &str) {
    (
        anchor.before.as_deref().expect("before"),
        anchor.after.as_deref().expect("after"),
    )
}

/// The document's stored context is what a fresh capture would store, and
/// it survives a save and reload unchanged.
fn assert_context_current(doc: &Document) {
    let mut refreshed = doc.clone();
    refreshed.refresh_context();
    assert_eq!(&refreshed, doc, "context is current");
    assert_eq!(&parse(&write(doc)).unwrap(), doc, "round trip");
}

const STORY: &str = "We felt the tension in the room before anyone spoke. \
                     Much later, the word tension came up again.";

// ----- acceptance: false attachment ------------------------------------------

#[test]
fn edit_inside_a_span_is_reported_not_attached_to_identical_text() {
    let mut doc = Document::new(STORY);
    let id = span(&mut doc, "tension");
    // Another editor rewrites the span's own text. The old text now occurs
    // exactly once, in an unrelated sentence.
    doc.body = doc.body.replacen("the tension in", "the tenseness in", 1);

    let mut legacy = doc.clone();
    strip_context(&mut legacy);
    let report = legacy.reanchor();
    assert!(
        report.is_clean(),
        "without context the old rule attaches it"
    );
    assert_eq!(
        legacy.annotations.spans[0].anchor.start,
        at(&legacy, "tension")
    );

    let report = doc.reanchor();
    assert_eq!(report.unresolved.len(), 1);
    assert_eq!(report.unresolved[0].span.id, id);
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::Missing);
    assert!(doc.annotations.spans.is_empty(), "not attached");
}

#[test]
fn edit_inside_a_ghost_is_reported_not_attached_to_identical_text() {
    let mut doc = Document::new("Keep this. Drop me now. Keep that. Drop me now");
    let start = at(&doc, "Drop me now");
    doc.ghost(start, start + 11).unwrap();
    doc.body = doc.body.replacen("Drop me now.", "Drop me later.", 1);
    let report = doc.reanchor();
    assert_eq!(report.unresolved_ghosts.len(), 1);
    assert_eq!(
        report.unresolved_ghosts[0].reason,
        UnresolvedReason::Missing
    );
    assert!(doc.annotations.ghosts.is_empty());
}

// ----- acceptance: edits elsewhere --------------------------------------------

#[test]
fn spans_survive_edits_made_before_them_in_another_editor() {
    let mut doc = Document::new(STORY);
    let id = span(&mut doc, "tension");
    doc.body
        .insert_str(0, "# A heading 😀\n\nA new opening paragraph.\n\n");
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.moved, vec![id.clone()]);
    assert_eq!(doc.span(&id).unwrap().anchor.start, at(&doc, "tension"));
    assert_context_current(&doc);

    // A deletion before the span shifts it back.
    doc.body = doc.body.replacen("# A heading 😀\n\n", "", 1);
    assert!(doc.reanchor().is_clean());
    assert_eq!(doc.span(&id).unwrap().anchor.start, at(&doc, "tension"));
}

#[test]
fn a_change_right_next_to_the_anchor_on_one_side_still_matches() {
    // Before side rewritten (and the span moved), after side intact.
    let mut doc = Document::new(STORY);
    let id = span(&mut doc, "tension");
    doc.body = doc
        .body
        .replacen("We felt the tension", "Everyone sensed a tension", 1);
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(doc.span(&id).unwrap().anchor.start, at(&doc, "tension"));
    assert_eq!(
        context(&doc.span(&id).unwrap().anchor).0,
        "Everyone sensed a ",
        "context refreshed after re-anchoring"
    );

    // After side rewritten, before side intact; a prefix moves the span.
    let mut doc = Document::new(STORY);
    let id = span(&mut doc, "tension");
    doc.body = doc
        .body
        .replacen("tension in the room before", "tension, unmistakably,", 1);
    doc.body.insert_str(0, "Prologue. ");
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(doc.span(&id).unwrap().anchor.start, at(&doc, "tension"));
}

#[test]
fn a_change_on_both_sides_of_a_moved_anchor_is_reported() {
    let mut doc = Document::new(STORY);
    span(&mut doc, "tension");
    doc.body = doc.body.replacen(
        "We felt the tension in the room before",
        "Then came a sudden tension, and nobody",
        1,
    );
    let report = doc.reanchor();
    assert_eq!(report.unresolved[0].reason, UnresolvedReason::Missing);
}

#[test]
fn the_hint_wins_even_when_context_disagrees() {
    let mut doc = Document::new(STORY);
    let id = span(&mut doc, "tension");
    // Rewrite both sides without moving the span.
    doc.body = doc.body.replacen("We felt the", "Xx yyyy zzz", 1);
    doc.body = doc.body.replacen(" in the room", " on the moon", 1);
    let report = doc.reanchor();
    assert!(report.is_clean() && report.moved.is_empty());
    assert_eq!(context(&doc.span(&id).unwrap().anchor).0, "Xx yyyy zzz ");
}

#[test]
fn equally_good_candidates_are_ambiguous() {
    // Two sentences identical for well over CONTEXT_UNITS on each side of
    // "cat"; the span's own sentence is moved off its hint.
    let sentence = "There was a very calm old cat sleeping on a warm windowsill all day. ";
    let mut doc = Document::new(format!("Intro. {sentence}"));
    let id = span(&mut doc, "cat");
    doc.body = format!("A new intro. {sentence}{sentence}");
    let report = doc.reanchor();
    let first = at(&doc, "cat");
    let second = first + utf16_len(sentence);
    assert_eq!(report.unresolved[0].span.id, id);
    // Both have the same agreeing text before ("...calm old ") and after.
    assert_eq!(
        report.unresolved[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![first, second]
        }
    );
}

#[test]
fn two_items_wanting_the_same_occurrence_are_both_ambiguous() {
    // Hand-built (as from an edited file): two spans with the same text and
    // context, neither at its hint.
    let mut doc = Document::new("Intro: the big cat sat down.");
    let make = |id: &str, start: usize| Span {
        id: id.into(),
        kind: SpanKind::Word,
        anchor: Anchor {
            before: Some("the big ".into()),
            after: Some(" sat down.".into()),
            ..Anchor::new(start, start + 3, "cat")
        },
        active: 0,
        alts: vec![
            terraphim_alternatives::Alternative::new("cat", Source::Original),
            terraphim_alternatives::Alternative::new("dog", Source::Human),
        ],
    };
    doc.annotations.spans = vec![make("s1", 40), make("s2", 80)];
    let report = doc.reanchor();
    assert_eq!(report.unresolved.len(), 2);
    for unresolved in &report.unresolved {
        assert_eq!(
            unresolved.reason,
            UnresolvedReason::Ambiguous {
                candidates: vec![15]
            }
        );
    }
}

#[test]
fn context_and_legacy_items_share_one_pass() {
    // A context span is placed first; a legacy span with the same text then
    // finds that occurrence claimed and the other one free.
    let mut doc = Document::new("one cat here and one cat there");
    let with_context = span(&mut doc, "cat");
    let second = at(&doc, "cat there");
    let legacy = doc.add_span(SpanKind::Word, second, second + 3).unwrap();
    doc.add_alternative(&legacy, "dog", Source::Human, None)
        .unwrap();
    doc.annotations.spans[1].anchor.before = None;
    doc.annotations.spans[1].anchor.after = None;
    doc.body.insert_str(0, ">> ");
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(doc.span(&with_context).unwrap().anchor.start, 7);
    assert_eq!(doc.span(&legacy).unwrap().anchor.start, 24);
    // Re-anchoring stored context on the placed legacy span.
    assert!(doc.span(&legacy).unwrap().anchor.has_context());
}

// ----- capture ------------------------------------------------------------------

#[test]
fn context_is_captured_at_document_start_and_end() {
    let mut doc = Document::new("First word and last");
    let first = span(&mut doc, "First");
    let last = span(&mut doc, "last");
    assert_eq!(
        context(&doc.span(&first).unwrap().anchor),
        ("", " word and last")
    );
    assert_eq!(
        context(&doc.span(&last).unwrap().anchor),
        ("First word and ", "")
    );
    let whole = doc.ghost(0, utf16_len("First word and last")).unwrap();
    let ghost = &doc.ghost_at(0).unwrap().anchor;
    assert_eq!(doc.ghost_at(0).unwrap().id, whole);
    assert_eq!(context(ghost), ("", ""));
    assert_context_current(&doc);
}

#[test]
fn context_is_captured_in_utf16_units_for_multi_byte_and_astral_text() {
    // Each 😀 is two UTF-16 units; "é" and CJK are one.
    let body = format!("{} café 中文 target {}", "😀".repeat(20), "𝄞".repeat(20));
    let mut doc = Document::new(body);
    let id = span(&mut doc, "target");
    let anchor = &doc.span(&id).unwrap().anchor;
    let (before, after) = context(anchor);
    assert!(utf16_len(before) <= CONTEXT_UNITS && utf16_len(after) <= CONTEXT_UNITS);
    assert!(before.ends_with(" café 中文 "), "{before:?}");
    assert!(
        before.starts_with('😀'),
        "whole characters only: {before:?}"
    );
    assert_eq!(
        after,
        format!(" {}", "𝄞".repeat(15)),
        "31 units: no half pair"
    );
    // A prefix of astral text moves the span; context still agrees.
    doc.body.insert_str(0, "😀😀 ");
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(doc.span(&id).unwrap().anchor.start, at(&doc, "target"));
}

#[test]
fn ghost_merges_and_splits_capture_current_context() {
    let mut doc = Document::new("Alpha beta gamma delta epsilon zeta.");
    let left = doc.ghost(at(&doc, "beta"), at(&doc, "beta") + 4).unwrap();
    doc.ghost(at(&doc, "delta"), at(&doc, "delta") + 5).unwrap();
    // Ghosting " gamma " touches both: one ghost "beta gamma delta" keeping
    // the left id.
    let merged = doc
        .ghost(at(&doc, " gamma "), at(&doc, " gamma ") + 7)
        .unwrap();
    assert_eq!(merged, left);
    let ghost = &doc.annotations.ghosts[0].anchor;
    assert_eq!(ghost.text, "beta gamma delta");
    assert_eq!(context(ghost), ("Alpha ", " epsilon zeta."));
    assert_context_current(&doc);

    // Split by reviving "gamma": both parts store their own context.
    doc.revive(at(&doc, "gamma"), at(&doc, "gamma") + 5)
        .unwrap();
    let parts: Vec<(&str, (&str, &str))> = doc
        .annotations
        .ghosts
        .iter()
        .map(|g| (g.anchor.text.as_str(), context(&g.anchor)))
        .collect();
    assert_eq!(
        parts,
        vec![
            ("beta ", ("Alpha ", "gamma delta epsilon zeta.")),
            (" delta", ("Alpha beta gamma", " epsilon zeta.")),
        ]
    );
    assert_context_current(&doc);
}

// ----- refresh ------------------------------------------------------------------

#[test]
fn context_follows_live_edits_swaps_and_the_article_fix_up() {
    let mut doc = Document::new("Pass me a paperclip and the stapler.");
    let clip = span(&mut doc, "paperclip");
    doc.add_alternative(&clip, "eraser", Source::Human, None)
        .unwrap();
    let stapler = span(&mut doc, "stapler");
    let ghost = doc.ghost(0, 4).unwrap();
    assert_context_current(&doc);

    // An edit between the spans changes both spans' context.
    let and = at(&doc, " and ");
    doc.apply_edit(and, and + 5, " together with ").unwrap();
    // 38 units precede "stapler": the window starts inside "me", which is
    // dropped.
    assert_eq!(
        context(&doc.span(&stapler).unwrap().anchor).0,
        " a paperclip together with the "
    );
    assert_eq!(
        context(&doc.span(&clip).unwrap().anchor).1,
        " together with the stapler."
    );
    assert_context_current(&doc);

    // A swap with the a/an fix-up changes the swapped span's `before`, the
    // neighbour's `before` and the ghost's `after`.
    doc.set_active(&clip, 2).unwrap();
    assert_eq!(doc.body, "Pass me an eraser together with the stapler.");
    assert_eq!(context(&doc.span(&clip).unwrap().anchor).0, "Pass me an ");
    // Exactly 32 units, starting on a word boundary.
    assert_eq!(
        context(&doc.span(&stapler).unwrap().anchor).0,
        " me an eraser together with the "
    );
    let g = doc
        .annotations
        .ghosts
        .iter()
        .find(|g| g.id == ghost)
        .unwrap();
    assert_eq!(context(&g.anchor).1, " me an eraser together with the ");
    assert_context_current(&doc);

    // Removing the alternative swaps back.
    doc.remove_alternative(&clip, 2).unwrap();
    assert_eq!(doc.body, "Pass me a paperclip together with the stapler.");
    assert_eq!(context(&doc.span(&clip).unwrap().anchor).0, "Pass me a ");
    assert_context_current(&doc);
}

#[test]
fn detached_and_stale_anchors_keep_their_context() {
    let mut doc = Document::new("one two three four");
    let id = span(&mut doc, "two");
    let original = doc.span(&id).unwrap().anchor.clone();
    let detached = doc
        .apply_edit(at(&doc, "two"), at(&doc, "two") + 3, "2")
        .unwrap();
    assert_eq!(detached[0].anchor, original, "returned as it was");

    // A stale anchor (body changed directly) is written with its old context.
    let mut doc = Document::new("one two three four");
    let id = span(&mut doc, "three");
    let stored = doc.span(&id).unwrap().anchor.clone();
    doc.body = "completely different".into();
    let saved = write(&doc);
    let reread = parse(&saved).unwrap();
    assert_eq!(reread.span(&id).unwrap().anchor, stored);
}

#[test]
fn hand_built_ghost_without_context_gains_it_on_write() {
    let mut doc = Document::new("abc def");
    doc.annotations.ghosts.push(Ghost {
        id: "g1".into(),
        anchor: Anchor::new(4, 7, "def"),
    });
    let reread = parse(&write(&doc)).unwrap();
    assert_eq!(context(&reread.annotations.ghosts[0].anchor), ("abc ", ""));
    assert!(
        !doc.annotations.ghosts[0].anchor.has_context(),
        "input untouched"
    );
}
