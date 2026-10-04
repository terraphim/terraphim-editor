//! Ghost layer acceptance tests (R-5, decision 2026-10-04): ghosts are ranges
//! independent of alternative spans, elastic under edits and swaps, merged
//! when they meet, split on revive, re-anchored like spans, and dropped on
//! export.

use terraphim_alternatives::{
    Document, EditError, Ghost, Source, SpanKind, UnresolvedReason, parse, utf16_len, write,
};

/// UTF-16 offset of the first occurrence of `needle` in the body.
fn at(doc: &Document, needle: &str) -> usize {
    utf16_len(&doc.body[..doc.body.find(needle).expect(needle)])
}

/// UTF-16 range of the first occurrence of `needle` in the body.
fn range(doc: &Document, needle: &str) -> (usize, usize) {
    let start = at(doc, needle);
    (start, start + utf16_len(needle))
}

/// Adds a word span over `word` with the given alternatives.
fn span(doc: &mut Document, word: &str, alts: &[&str]) -> String {
    let (start, end) = range(doc, word);
    let id = doc.add_span(SpanKind::Word, start, end).unwrap();
    for alt in alts {
        doc.add_alternative(&id, *alt, Source::Human, None).unwrap();
    }
    id
}

/// Every ghost's anchor matches the body at its UTF-16 offsets, ghosts are
/// sorted and never overlap, and the document survives a save and reload.
fn assert_consistent(doc: &Document) {
    let units: Vec<u16> = doc.body.encode_utf16().collect();
    for ghost in &doc.annotations.ghosts {
        let text = String::from_utf16(&units[ghost.anchor.start..ghost.anchor.end]).unwrap();
        assert_eq!(text, ghost.anchor.text, "ghost {}", ghost.id);
        assert!(!ghost.anchor.text.is_empty(), "ghost {} is empty", ghost.id);
    }
    for pair in doc.annotations.ghosts.windows(2) {
        assert!(pair[0].anchor.end <= pair[1].anchor.start, "{pair:?}");
    }
    let saved = write(doc);
    assert_eq!(&parse(&saved).unwrap(), doc);
}

fn ghost_view(doc: &Document) -> Vec<(&str, usize, usize, &str)> {
    doc.annotations
        .ghosts
        .iter()
        .map(|g: &Ghost| {
            (
                g.id.as_str(),
                g.anchor.start,
                g.anchor.end,
                g.anchor.text.as_str(),
            )
        })
        .collect()
}

// ----- ghosting over spans ---------------------------------------------------

#[test]
fn sentence_containing_a_word_span_can_be_ghosted_and_swaps_resize_it() {
    let mut d = Document::new("Hello there. Pass me a paperclip now. Bye.");
    let word = span(&mut d, "paperclip", &["eraser", "thumbtack"]);
    let (start, end) = range(&d, "Pass me a paperclip now.");
    let g = d.ghost(start, end).unwrap();
    assert_eq!(g, "g1");
    assert_eq!(d.span(&word).unwrap().anchor.text, "paperclip", "span kept");

    // Swap to a shorter word: the article flips too; both edits lie inside.
    d.set_active(&word, 1).unwrap();
    assert_eq!(d.body, "Hello there. Pass me an eraser now. Bye.");
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 13, 35, "Pass me an eraser now.")]
    );
    assert_consistent(&d);
    assert_eq!(d.export(), "Hello there. Bye.");

    // Longer word, article back to "a".
    d.cycle_active(&word, 1).unwrap();
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 13, 37, "Pass me a thumbtack now.")]
    );
    d.set_active(&word, 0).unwrap();
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 13, 37, "Pass me a paperclip now.")]
    );
    assert_eq!(d.export(), "Hello there. Bye.");
    assert_consistent(&d);
}

#[test]
fn ghosting_exactly_one_span_follows_its_swaps() {
    let mut d = Document::new("Take a paperclip home.");
    let word = span(&mut d, "paperclip", &["eraser"]);
    assert_eq!(d.ghost_span(&word).unwrap(), "g1");
    d.set_active(&word, 1).unwrap();
    // The article lies before the ghost, so it shifts the ghost.
    assert_eq!(d.body, "Take an eraser home.");
    assert_eq!(ghost_view(&d), vec![("g1", 8, 14, "eraser")]);
    assert_eq!(d.export(), "Take an home.");
    assert_eq!(
        d.ghost_span("nope"),
        Err(EditError::UnknownSpan("nope".into()))
    );
    assert_consistent(&d);
}

#[test]
fn article_fix_up_at_ghost_boundaries() {
    // Ghost starts at the article: the respelt article stays ghosted.
    let mut d = Document::new("Take a paperclip home.");
    let word = span(&mut d, "paperclip", &["eraser"]);
    let (start, end) = range(&d, "a paperclip");
    d.ghost(start, end).unwrap();
    d.set_active(&word, 1).unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 5, 14, "an eraser")]);
    assert_eq!(d.export(), "Take home.");
    assert_consistent(&d);

    // Ghost ends at the article: the swap is outside, the article inside.
    let mut d = Document::new("Take a paperclip home.");
    let word = span(&mut d, "paperclip", &["eraser"]);
    d.ghost(0, 6).unwrap();
    d.set_active(&word, 1).unwrap();
    assert_eq!(d.body, "Take an eraser home.");
    assert_eq!(ghost_view(&d), vec![("g1", 0, 7, "Take an")]);
    assert_eq!(d.export(), "eraser home.");
    assert_consistent(&d);
}

#[test]
fn partial_overlap_with_a_span_exports_only_the_ghosted_part() {
    let mut d = Document::new("One two three four.");
    let (start, end) = range(&d, "three four");
    let phrase = d.add_span(SpanKind::Sentence, start, end).unwrap();
    d.add_alternative(&phrase, "five", Source::Human, None)
        .unwrap();
    let (start, end) = range(&d, "two three");
    d.ghost(start, end).unwrap();
    assert_eq!(d.export(), "One four.");
    assert_consistent(&d);

    // Swapping the partly ghosted span trims the ghost to its surviving text;
    // the replacement is not ghosted.
    d.set_active(&phrase, 1).unwrap();
    assert_eq!(d.body, "One two five.");
    assert_eq!(ghost_view(&d), vec![("g1", 4, 8, "two ")]);
    assert_eq!(d.export(), "One five.");
    d.set_active(&phrase, 0).unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 4, 8, "two ")]);
    assert_consistent(&d);
}

#[test]
fn spans_inside_a_ghost_keep_their_alternatives_after_revive() {
    let mut d = Document::new("The tension rises.");
    let word = span(&mut d, "tension", &["pressure"]);
    d.ghost(0, 18).unwrap();
    assert!(d.revive(0, 18).unwrap());
    assert!(d.annotations.ghosts.is_empty());
    assert_eq!(d.span(&word).unwrap().alts.len(), 2);
    assert_eq!(
        write(&d),
        write(&{
            let mut fresh = Document::new("The tension rises.");
            span(&mut fresh, "tension", &["pressure"]);
            fresh
        })
    );
}

// ----- merge and revive ------------------------------------------------------

#[test]
fn overlapping_and_touching_ghosts_merge_keeping_the_first_id() {
    let mut d = Document::new("aaa bbb ccc ddd");
    assert_eq!(d.ghost(4, 7).unwrap(), "g1");
    assert_eq!(d.ghost(12, 15).unwrap(), "g2");
    assert_eq!(d.ghost(0, 3).unwrap(), "g3");
    assert_eq!(ghost_view(&d).len(), 3);
    // 3..4 touches g3 (ends at 3) and g1 (starts at 4); g3 starts first.
    assert_eq!(d.ghost(3, 4).unwrap(), "g3");
    assert_eq!(
        ghost_view(&d),
        vec![("g3", 0, 7, "aaa bbb"), ("g2", 12, 15, "ddd")]
    );
    // Overlapping both remaining ghosts.
    assert_eq!(d.ghost(6, 13).unwrap(), "g3");
    assert_eq!(ghost_view(&d), vec![("g3", 0, 15, "aaa bbb ccc ddd")]);
    // A range already inside a ghost changes nothing.
    assert_eq!(d.ghost(2, 5).unwrap(), "g3");
    assert_eq!(ghost_view(&d), vec![("g3", 0, 15, "aaa bbb ccc ddd")]);
    assert_eq!(d.export(), "");
    assert_consistent(&d);
}

#[test]
fn revive_splits_trims_and_removes() {
    let mut d = Document::new("one two three");
    d.ghost(0, 13).unwrap();
    assert_eq!(d.revive(4, 7), Ok(true));
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 0, 4, "one "), ("g2", 7, 13, " three")]
    );
    assert_eq!(d.export(), "two");
    assert_consistent(&d);

    // Trim the head of g1, remove g2 entirely.
    assert_eq!(d.revive(0, 2), Ok(true));
    assert_eq!(d.revive(5, 13), Ok(true));
    assert_eq!(ghost_view(&d), vec![("g1", 2, 4, "e ")]);
    // Nothing ghosted there.
    assert_eq!(d.revive(6, 9), Ok(false));
    // Touching is not overlapping.
    assert_eq!(d.revive(4, 6), Ok(false));
    assert_eq!(d.revive(2, 4), Ok(true));
    assert!(d.annotations.ghosts.is_empty());
    assert_eq!(write(&d), "one two three", "no block once empty");
}

#[test]
fn ghost_at_finds_the_covering_ghost() {
    let mut d = Document::new("keep cut keep");
    d.ghost(5, 8).unwrap();
    assert!(d.ghost_at(4).is_none());
    assert_eq!(d.ghost_at(5).unwrap().anchor.text, "cut");
    assert_eq!(d.ghost_at(7).unwrap().id, "g1");
    assert!(d.ghost_at(8).is_none(), "end is exclusive");
}

#[test]
fn ghost_ids_are_unique_across_spans_and_ghosts() {
    let mut d = Document::new("alpha beta");
    let id = d.add_span(SpanKind::Word, 0, 5).unwrap();
    d.add_alternative(&id, "x", Source::Human, None).unwrap();
    d.annotations.spans[0].id = "g1".into();
    assert_eq!(d.ghost(6, 10).unwrap(), "g2");
    assert_eq!(d.add_span(SpanKind::Word, 6, 10).unwrap(), "s1");
}

#[test]
fn invalid_ranges_are_refused() {
    let mut d = Document::new("a𝄞b");
    for (start, end) in [(1, 1), (2, 3), (1, 2), (3, 1), (0, 9)] {
        assert_eq!(
            d.ghost(start, end),
            Err(EditError::InvalidRange { start, end })
        );
        assert_eq!(
            d.revive(start, end),
            Err(EditError::InvalidRange { start, end })
        );
    }
    assert!(d.annotations.ghosts.is_empty());
}

// ----- elastic under live edits ---------------------------------------------

#[test]
fn insertions_at_ghost_boundaries_are_outside() {
    let mut d = Document::new("ab GHOST cd");
    d.ghost(3, 8).unwrap();
    assert!(d.apply_edit(3, 3, "X").unwrap().is_empty());
    assert_eq!(ghost_view(&d), vec![("g1", 4, 9, "GHOST")]);
    d.apply_edit(9, 9, "Y").unwrap();
    assert_eq!(d.body, "ab XGHOSTY cd");
    assert_eq!(ghost_view(&d), vec![("g1", 4, 9, "GHOST")]);
    // Strictly inside grows the ghost.
    d.apply_edit(6, 6, "-").unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 4, 10, "GH-OST")]);
    assert_eq!(d.export(), "ab XY cd");
    assert_consistent(&d);
}

#[test]
fn insertion_between_touching_ghosts_is_not_ghosted() {
    let mut d = Document::new("abcdef");
    d.ghost(0, 3).unwrap();
    d.revive(1, 2).unwrap();
    d.ghost(3, 6).unwrap(); // touches g2 ("c" at 2..3) so merges into it
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 0, 1, "a"), ("g2", 2, 6, "cdef")]
    );
    d.revive(3, 4).unwrap();
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 0, 1, "a"), ("g2", 2, 3, "c"), ("g3", 4, 6, "ef")]
    );
    // Delete "d": g2 and g3 now touch; edits never merge them.
    d.apply_edit(3, 4, "").unwrap();
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 0, 1, "a"), ("g2", 2, 3, "c"), ("g3", 3, 5, "ef")]
    );
    // Typing between them lands outside both.
    d.apply_edit(3, 3, "X").unwrap();
    assert_eq!(d.body, "abcXef");
    assert_eq!(d.export(), "bX");
    assert_consistent(&d);
}

#[test]
fn edits_inside_resize_and_edits_before_shift() {
    let mut d = Document::new("Lead. Ghost text here. Tail.");
    let (start, end) = range(&d, "Ghost text here.");
    d.ghost(start, end).unwrap();
    // Replace "text" with "words" (typing over a selection inside the ghost).
    let (s, e) = range(&d, "text");
    d.apply_edit(s, e, "words").unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 6, 23, "Ghost words here.")]);
    // Replacing the whole ghost keeps it ghosted.
    d.apply_edit(6, 23, "New.").unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 6, 10, "New.")]);
    // Edit before: shift only.
    d.apply_edit(0, 4, "Intro").unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 7, 11, "New.")]);
    // Edit after: untouched.
    let len = utf16_len(&d.body);
    d.apply_edit(len - 5, len, "End.").unwrap();
    assert_eq!(d.body, "Intro. New. End.");
    assert_eq!(ghost_view(&d), vec![("g1", 7, 11, "New.")]);
    assert_eq!(d.export(), "Intro. End.");
    assert_consistent(&d);
}

#[test]
fn deleting_all_of_a_ghosts_text_removes_it() {
    let mut d = Document::new("keep drop keep drop2 keep");
    d.ghost(5, 9).unwrap();
    d.ghost(15, 20).unwrap();
    // Exactly the ghost's text.
    d.apply_edit(5, 9, "").unwrap();
    assert_eq!(ghost_view(&d), vec![("g2", 11, 16, "drop2")]);
    // A deletion covering the ghost and more.
    d.apply_edit(10, 17, "").unwrap();
    assert_eq!(d.body, "keep  keepkeep");
    assert!(d.annotations.ghosts.is_empty());
    assert_eq!(write(&d), d.body);
}

#[test]
fn partial_edits_trim_the_ghost() {
    let mut d = Document::new("Alpha beta gamma delta");
    d.ghost(6, 16).unwrap();
    // Head cut: the replacement "X" is not ghosted.
    d.apply_edit(0, 8, "X").unwrap();
    assert_eq!(d.body, "Xta gamma delta");
    assert_eq!(ghost_view(&d), vec![("g1", 1, 9, "ta gamma")]);
    // Tail cut: "gamma de" replaced by "!"; only "ta " survives.
    d.apply_edit(4, 12, "!").unwrap();
    assert_eq!(d.body, "Xta !lta");
    assert_eq!(ghost_view(&d), vec![("g1", 1, 4, "ta ")]);
    // A replacement ending exactly at the ghost's end lies inside it.
    d.apply_edit(2, 4, "!!").unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 1, 4, "t!!")]);
    assert_consistent(&d);
}

#[test]
fn stale_ghosts_block_edits_that_would_resize_them() {
    let mut d = Document::new("The big tension.");
    let word = span(&mut d, "tension", &["pressure"]);
    d.ghost(0, 16).unwrap();
    // An outside editor changes ghosted text without going through apply_edit.
    d.body = d.body.replace("big", "BIG");
    let before = d.clone();
    assert_eq!(
        d.set_active(&word, 1),
        Err(EditError::StaleAnchor("g1".into()))
    );
    assert_eq!(
        d.apply_edit(5, 5, "x"),
        Err(EditError::StaleAnchor("g1".into()))
    );
    assert_eq!(d.ghost(0, 3), Err(EditError::StaleAnchor("g1".into())));
    assert_eq!(d.revive(0, 3), Err(EditError::StaleAnchor("g1".into())));
    assert_eq!(d, before, "unchanged on error");
    // Edits that leave the ghost alone still work.
    d.apply_edit(16, 16, " More.").unwrap();
    assert_eq!(d.export(), "The BIG tension. More.", "stale ghost not cut");
    // Re-anchoring reports it rather than guessing.
    let report = d.reanchor();
    assert_eq!(report.unresolved_ghosts.len(), 1);
    assert_eq!(
        report.unresolved_ghosts[0].reason,
        UnresolvedReason::Missing
    );
    assert!(!report.is_clean());
}

#[test]
fn stale_ghost_on_the_article_skips_the_fix_up() {
    let mut d = Document::new("Take a paperclip.");
    let word = span(&mut d, "paperclip", &["eraser"]);
    d.ghost(0, 6).unwrap();
    d.body = d.body.replace("Take", "Make");
    d.set_active(&word, 1).unwrap();
    assert_eq!(d.body, "Make a eraser.", "article left alone");
    assert_eq!(ghost_view(&d), vec![("g1", 0, 6, "Take a")]);
}

// ----- multi-byte text -------------------------------------------------------

#[test]
fn multi_byte_and_astral_text_in_ghosts() {
    let mut d = Document::new("前言 𝄞 a paperclip 中文 end.");
    let word = span(&mut d, "paperclip", &["eraser"]);
    let (start, end) = range(&d, "𝄞 a paperclip 中文");
    assert_eq!((start, end), (3, 20), "clef is two units, CJK one each");
    d.ghost(start, end).unwrap();
    d.set_active(&word, 1).unwrap();
    assert_eq!(d.body, "前言 𝄞 an eraser 中文 end.");
    assert_eq!(ghost_view(&d), vec![("g1", 3, 18, "𝄞 an eraser 中文")]);
    assert_eq!(d.export(), "前言 end.");
    // Split inside the CJK and around the clef.
    assert_eq!(d.revive(5, 6), Ok(true), "revive the space after the clef");
    assert_eq!(
        ghost_view(&d),
        vec![("g1", 3, 5, "𝄞"), ("g2", 6, 18, "an eraser 中文")]
    );
    assert_eq!(
        d.revive(4, 5),
        Err(EditError::InvalidRange { start: 4, end: 5 })
    );
    assert_consistent(&d);
    let saved = write(&d);
    assert_eq!(write(&parse(&saved).unwrap()), saved);
}

// ----- re-anchoring ----------------------------------------------------------

#[test]
fn reanchor_moves_ghosts_and_overlapping_spans_independently() {
    let mut d = Document::new("Intro. The tension rises. Keep.");
    let word = span(&mut d, "tension", &["pressure"]);
    let (start, end) = range(&d, "The tension rises.");
    d.ghost(start, end).unwrap();
    d.body.insert_str(0, "𝄞 New. ");
    let report = d.reanchor();
    assert!(report.is_clean());
    assert_eq!(report.moved, vec![word.clone(), "g1".to_string()]);
    assert_eq!(ghost_view(&d), vec![("g1", 15, 33, "The tension rises.")]);
    assert_eq!(d.span(&word).unwrap().anchor.start, 19);
    // Further edits work on the re-anchored document.
    d.set_active(&word, 1).unwrap();
    assert_eq!(ghost_view(&d), vec![("g1", 15, 34, "The pressure rises.")]);
    assert_consistent(&d);
}

#[test]
fn reanchor_reports_missing_and_ambiguous_ghosts() {
    let mut d = Document::new("Keep. Drop me. Keep.");
    d.ghost(6, 14).unwrap();
    d.body = d.body.replace("Drop me. ", "");
    let report = d.reanchor();
    assert_eq!(report.unresolved_ghosts.len(), 1);
    assert_eq!(
        report.unresolved_ghosts[0].ghost.id, "g1",
        "returned, not lost"
    );
    assert_eq!(
        report.unresolved_ghosts[0].reason,
        UnresolvedReason::Missing
    );
    assert!(d.annotations.ghosts.is_empty());

    let mut d = Document::new("the cat and the cat");
    d.ghost(16, 19).unwrap();
    d.body.insert_str(0, "Oh, ");
    let report = d.reanchor();
    assert_eq!(
        report.unresolved_ghosts[0].reason,
        UnresolvedReason::Ambiguous {
            candidates: vec![8, 20]
        }
    );
    assert!(report.unresolved.is_empty());

    // Unchanged body: hint wins, nothing moves.
    let mut d = Document::new("the cat and the cat");
    d.ghost(16, 19).unwrap();
    d.body.push_str(" sat");
    let report = d.reanchor();
    assert!(report.is_clean() && report.moved.is_empty());
}

#[test]
fn ghosts_sharing_text_do_not_claim_the_same_occurrence() {
    let mut d = Document::new("x cat y cat z");
    d.ghost(2, 5).unwrap();
    d.ghost(8, 11).unwrap();
    d.body = "x y cat z".into();
    let report = d.reanchor();
    assert_eq!(report.unresolved_ghosts.len(), 2);
    assert!(d.annotations.ghosts.is_empty());
}

// ----- persistence and counts -----------------------------------------------

#[test]
fn programmatic_document_with_ghosts_is_byte_stable() {
    let mut d = Document::new("A 𝄞 clef, a café and a paperclip.\r\nSecond `line` here.\n");
    let word = span(&mut d, "paperclip", &["eraser", "`tick`"]);
    let (start, end) = range(&d, "a café and a paperclip");
    d.ghost(start, end).unwrap();
    let (start, end) = range(&d, "`line`");
    d.ghost(start, end).unwrap();
    d.set_active(&word, 1).unwrap();
    d.annotations.overflow = "stash ``` here".into();
    let written = write(&d);
    assert!(written.contains("\"ghosts\": ["));
    assert_eq!(parse(&written).unwrap(), d);
    assert_eq!(write(&parse(&written).unwrap()), written);
    assert_eq!(d.export(), "A 𝄞 clef,.\r\nSecond here.\n");
}

#[test]
fn counts_include_ghosted_text() {
    let mut d = Document::new("One two three.");
    let before = d.counts();
    d.ghost(0, 14).unwrap();
    assert_eq!(d.counts(), before);
    assert_eq!(d.export(), "");
}
