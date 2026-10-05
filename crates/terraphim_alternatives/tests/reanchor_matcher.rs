//! Re-anchoring cases that depend on how occurrences are found (issue #30):
//! overlapping occurrences, an anchor inside another anchor's occurrence,
//! case sensitivity, single-character and blank anchors, astral and CJK
//! offsets, and many spans sharing one text. Each case edits the body directly,
//! as an outside editor would, then calls `reanchor`.

use terraphim_alternatives::{
    Document, Source, SpanKind, UnresolvedReason, utf16_len, utf16_to_byte,
};

/// Adds a span over `start..end` with one human alternative so it is not inert.
fn span(doc: &mut Document, start: usize, end: usize) -> String {
    let id = doc.add_span(SpanKind::Word, start, end).unwrap();
    doc.add_alternative(&id, "alt", Source::Human, None)
        .unwrap();
    id
}

/// The span's anchor range, and the body text that range covers now.
fn located(doc: &Document, id: &str) -> (usize, usize, String) {
    let a = &doc.span(id).unwrap().anchor;
    let (s, e) = (
        utf16_to_byte(&doc.body, a.start).unwrap(),
        utf16_to_byte(&doc.body, a.end).unwrap(),
    );
    (a.start, a.end, doc.body[s..e].to_string())
}

#[test]
fn overlapping_occurrences_of_one_anchor_are_all_candidates() {
    let mut doc = Document::new("aa");
    span(&mut doc, 0, 2);
    doc.body = "baaa".to_string();
    let report = doc.reanchor();
    assert_eq!(
        report.unresolved[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![1, 2]
        },
        "`aa` occurs twice, overlapping, in `aaa`"
    );
}

#[test]
fn overlapping_occurrence_at_the_hint_is_claimed() {
    let mut doc = Document::new("xaa");
    let id = span(&mut doc, 1, 3);
    doc.body = "xaaa".to_string();
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert!(report.moved.is_empty());
    assert_eq!(located(&doc, &id), (1, 3, "aa".to_string()));
}

#[test]
fn anchor_inside_another_anchors_occurrence_is_found_and_skipped_once_claimed() {
    let mut doc = Document::new("concatenate and cat");
    let long = span(&mut doc, 0, 11);
    let short = span(&mut doc, 16, 19);
    doc.body.insert_str(0, "X ");
    let report = doc.reanchor();
    // `cat` also occurs inside `concatenate` (offset 5), but that occurrence
    // overlaps the claim of the first span, so only offset 18 remains.
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(located(&doc, &long), (2, 13, "concatenate".to_string()));
    assert_eq!(located(&doc, &short), (18, 21, "cat".to_string()));
}

#[test]
fn occurrence_inside_a_longer_word_counts_as_a_candidate() {
    let mut doc = Document::new("cat");
    span(&mut doc, 0, 3);
    doc.body = "a concatenated cat".to_string();
    let report = doc.reanchor();
    assert_eq!(
        report.unresolved[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![5, 15]
        },
        "matching ignores word boundaries"
    );
}

#[test]
fn matching_is_case_sensitive() {
    let mut doc = Document::new("The cat. the dog. THE end.");
    let id = span(&mut doc, 0, 3);
    doc.body.insert_str(0, "Hi. ");
    let report = doc.reanchor();
    assert!(report.is_clean(), "`the` and `THE` are not `The`");
    assert_eq!(report.moved, vec![id.clone()]);
    assert_eq!(located(&doc, &id), (4, 7, "The".to_string()));
}

#[test]
fn case_variants_are_separate_texts_for_contention() {
    let mut doc = Document::new("The the");
    let upper = span(&mut doc, 0, 3);
    let lower = span(&mut doc, 4, 7);
    doc.body.insert_str(0, ">> ");
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(located(&doc, &upper).0, 3);
    assert_eq!(located(&doc, &lower).0, 7);
}

#[test]
fn single_character_anchors_reanchor() {
    let mut doc = Document::new("a b c");
    let id = span(&mut doc, 2, 3);
    doc.body.insert(0, 'x');
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(located(&doc, &id), (3, 4, "b".to_string()));

    let mut doc = Document::new("b");
    span(&mut doc, 0, 1);
    doc.body = "abcb".to_string();
    let report = doc.reanchor();
    assert_eq!(
        report.unresolved[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![1, 3]
        }
    );
}

#[test]
fn astral_and_cjk_offsets_are_utf16() {
    // "𝄞" is two UTF-16 units (four bytes); each CJK character is one unit
    // (three bytes).
    let mut doc = Document::new("𝄞 中文 cat");
    let cjk = span(&mut doc, 3, 5);
    let cat = span(&mut doc, 6, 9);
    let prefix = "é😀漢 ";
    doc.body.insert_str(0, prefix);
    let shift = utf16_len(prefix);
    assert_eq!(shift, 5);
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(
        located(&doc, &cjk),
        (3 + shift, 5 + shift, "中文".to_string())
    );
    assert_eq!(
        located(&doc, &cat),
        (6 + shift, 9 + shift, "cat".to_string())
    );
}

#[test]
fn astral_anchor_text_reanchors() {
    let mut doc = Document::new("note 😀😀 end");
    let id = span(&mut doc, 5, 9);
    doc.body = "😀 note 😀😀 end".to_string();
    let report = doc.reanchor();
    // `😀😀` occurs once; the lone `😀` before it is not a match.
    assert!(report.is_clean());
    assert_eq!(located(&doc, &id), (8, 12, "😀😀".to_string()));
}

#[test]
fn many_spans_sharing_text_are_never_guessed() {
    let sentence = "The tension holds. ";
    let units = utf16_len(sentence);
    let mut doc = Document::new(sentence.repeat(50));
    for i in 0..50 {
        span(&mut doc, i * units + 4, i * units + 11);
    }
    // Unchanged body: every hint is an occurrence.
    let report = doc.clone().reanchor();
    assert!(report.is_clean() && report.moved.is_empty());

    doc.body.insert_str(0, "Heading. ");
    let report = doc.reanchor();
    assert_eq!(report.unresolved.len(), 50);
    let expected: Vec<usize> = (0..50).map(|i| 9 + i * units + 4).collect();
    for unresolved in &report.unresolved {
        assert_eq!(
            unresolved.reason,
            UnresolvedReason::Ambiguous {
                candidates: expected.clone()
            }
        );
    }
    assert!(doc.annotations.spans.is_empty());
}

#[test]
fn blank_ghost_and_span_still_reanchor() {
    // The matcher refuses blank patterns; whitespace-only anchors must still
    // be found.
    let mut doc = Document::new("One.\n\nTwo.  Three.");
    doc.ghost(4, 6).unwrap();
    let wide = span(&mut doc, 10, 12);
    doc.body.insert_str(0, "Zero. ");
    let report = doc.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.moved, vec![wide.clone(), "g1".to_string()]);
    assert_eq!(located(&doc, &wide), (16, 18, "  ".to_string()));
    let ghost = &doc.annotations.ghosts[0].anchor;
    assert_eq!(
        (ghost.start, ghost.end, ghost.text.as_str()),
        (10, 12, "\n\n")
    );
}

#[test]
fn blank_anchor_with_several_occurrences_is_ambiguous() {
    let mut doc = Document::new("a\n\nb");
    doc.ghost(1, 3).unwrap();
    doc.body = "zz\n\nb\n\n\nc".to_string();
    let report = doc.reanchor();
    assert_eq!(
        report.unresolved_ghosts[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![2, 5, 6]
        }
    );
}

#[test]
fn spans_and_ghosts_sharing_text_place_independently() {
    let mut doc = Document::new("one cat here");
    let id = span(&mut doc, 4, 7);
    doc.ghost(4, 7).unwrap();
    doc.body.insert_str(0, ">> ");
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert_eq!(report.moved, vec![id.clone(), "g1".to_string()]);
    assert_eq!(located(&doc, &id).0, 7);
    assert_eq!(doc.annotations.ghosts[0].anchor.start, 7);
}

#[test]
fn document_without_spans_or_ghosts_is_clean() {
    let mut doc = Document::new("Nothing anchored here.");
    let report = doc.reanchor();
    assert!(report.is_clean());
    assert!(report.moved.is_empty());
}
