//! `Document::move_range` (issue #44): moving text carries the spans and
//! ghosts inside it, shifts the ones it passes, refuses to split any item,
//! refreshes anchor context and survives a save and reload.

use terraphim_alternatives::{
    Document, EditError, MoveOutcome, Source, SpanKind, parse, utf16_len, write,
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

/// Adds a word span over the first `word` with the given alternatives.
fn span(doc: &mut Document, word: &str, alts: &[&str]) -> String {
    let (start, end) = range(doc, word);
    let id = doc.add_span(SpanKind::Word, start, end).unwrap();
    for alt in alts {
        doc.add_alternative(&id, *alt, Source::Human, None).unwrap();
    }
    id
}

/// Ghosts the first occurrence of `text`.
fn ghost(doc: &mut Document, text: &str) -> String {
    let (start, end) = range(doc, text);
    doc.ghost(start, end).unwrap()
}

/// The text at an anchor's UTF-16 offsets.
fn text_at(doc: &Document, start: usize, end: usize) -> String {
    let units: Vec<u16> = doc.body.encode_utf16().collect();
    String::from_utf16(&units[start..end]).unwrap()
}

/// Every anchor matches the body, spans and ghosts never overlap within
/// their own list, ghosts are sorted, context is current, and the document
/// survives a save and reload.
fn assert_consistent(doc: &Document) {
    for s in &doc.annotations.spans {
        assert_eq!(
            text_at(doc, s.anchor.start, s.anchor.end),
            s.anchor.text,
            "span {}",
            s.id
        );
        assert_eq!(s.anchor.text, s.active_alternative().text, "span {}", s.id);
    }
    for g in &doc.annotations.ghosts {
        assert_eq!(
            text_at(doc, g.anchor.start, g.anchor.end),
            g.anchor.text,
            "ghost {}",
            g.id
        );
    }
    for pair in doc.annotations.ghosts.windows(2) {
        assert!(pair[0].anchor.end <= pair[1].anchor.start, "{pair:?}");
    }
    let mut spans: Vec<_> = doc
        .annotations
        .spans
        .iter()
        .map(|s| (s.anchor.start, s.anchor.end))
        .collect();
    spans.sort_unstable();
    for pair in spans.windows(2) {
        assert!(pair[0].1 <= pair[1].0, "{pair:?}");
    }
    let mut refreshed = doc.clone();
    refreshed.refresh_context();
    assert_eq!(&refreshed, doc, "context is current after the move");
    assert_eq!(&parse(&write(doc)).unwrap(), doc, "round trip");
}

/// Three paragraphs: a span and a ghost in the first, a span in the second,
/// a ghost over the whole third.
fn three_blocks() -> (Document, [String; 4]) {
    let mut doc = Document::new(
        "Pass me a paperclip. Drop this.\n\nThe tension is high.\n\nA closing line.\n",
    );
    let s1 = span(&mut doc, "paperclip", &["eraser"]);
    doc.set_active(&s1, 1).unwrap();
    let g1 = ghost(&mut doc, " Drop this.");
    let s2 = span(&mut doc, "tension", &["pressure"]);
    let g2 = ghost(&mut doc, "A closing line.");
    assert_consistent(&doc);
    (doc, [s1, g1, s2, g2])
}

#[test]
fn moving_a_block_forward_carries_its_span_and_ghost() {
    let (mut doc, [s1, g1, s2, g2]) = three_blocks();
    let first = "Pass me an eraser. Drop this.\n\n";
    let (start, end) = range(&doc, first);
    let to = at(&doc, "A closing");
    let outcome = doc.move_range(start, end, to).unwrap();

    assert_eq!(
        doc.body,
        "The tension is high.\n\nPass me an eraser. Drop this.\n\nA closing line.\n"
    );
    let landed = at(&doc, "Pass me");
    assert_eq!(
        outcome,
        MoveOutcome {
            start: landed,
            end: landed + utf16_len(first),
            carried: vec![s1.clone(), g1.clone()],
            moved: true,
        }
    );
    let span = doc.span(&s1).unwrap();
    assert_eq!((span.active, span.alts.len()), (1, 2), "alternatives kept");
    assert_eq!(span.anchor.start, at(&doc, "eraser"));
    assert_eq!(doc.ghost_at(at(&doc, " Drop")).unwrap().id, g1);
    assert_eq!(doc.span(&s2).unwrap().anchor.start, at(&doc, "tension"));
    assert_eq!(doc.ghost_at(at(&doc, "A closing")).unwrap().id, g2);
    assert_eq!(
        doc.annotations
            .ghosts
            .iter()
            .map(|g| g.id.as_str())
            .collect::<Vec<_>>(),
        vec![g1.as_str(), g2.as_str()],
        "ghosts stay sorted"
    );
    assert_consistent(&doc);

    // The carried span still works: swapping it edits the moved text.
    doc.set_active(&s1, 0).unwrap();
    assert!(doc.body.contains("Pass me a paperclip. Drop this."));
    assert_consistent(&doc);
}

#[test]
fn moving_a_block_backward_carries_its_span_and_ghost() {
    let (mut doc, [s1, g1, s2, g2]) = three_blocks();
    let (start, end) = range(&doc, "A closing line.\n");
    let to = at(&doc, "The tension");
    let outcome = doc.move_range(start, end, to).unwrap();

    assert_eq!(
        doc.body,
        "Pass me an eraser. Drop this.\n\nA closing line.\nThe tension is high.\n\n"
    );
    assert_eq!(outcome.carried, vec![g2.clone()]);
    assert_eq!((outcome.start, outcome.end), (to, to + 16));
    assert_eq!(doc.ghost_at(to).unwrap().id, g2);
    assert_eq!(doc.span(&s2).unwrap().anchor.start, at(&doc, "tension"));
    assert_eq!(doc.span(&s1).unwrap().anchor.start, at(&doc, "eraser"));
    assert_eq!(doc.ghost_at(at(&doc, " Drop")).unwrap().id, g1);
    assert_consistent(&doc);
}

#[test]
fn export_after_a_move_drops_ghosts_at_their_new_place() {
    let (mut doc, _) = three_blocks();
    let (start, end) = range(&doc, "Pass me an eraser. Drop this.\n\n");
    doc.move_range(start, end, utf16_len(&doc.body)).unwrap();
    assert_eq!(
        doc.export(),
        "The tension is high.\n\nPass me an eraser.\n\n"
    );
    let reloaded = parse(&write(&doc)).unwrap();
    assert_eq!(reloaded.export(), doc.export());
}

#[test]
fn moving_back_restores_the_document_exactly() {
    let (original, _) = three_blocks();
    let mut doc = original.clone();
    let (start, end) = range(&doc, "The tension is high.\n\n");
    let outcome = doc.move_range(start, end, 0).unwrap();
    assert_ne!(doc, original);
    doc.move_range(outcome.start, outcome.end, end).unwrap();
    assert_eq!(doc, original, "body, anchors and context all restored");
}

#[test]
fn every_move_on_a_small_document_is_refused_cleanly_or_reversible() {
    let (original, _) = three_blocks();
    let len = utf16_len(&original.body);
    let (mut moved, mut refused) = (0, 0);
    for start in 0..len {
        for end in start + 1..=len {
            for to in (0..=len).filter(|&t| t <= start || t >= end) {
                let mut doc = original.clone();
                match doc.move_range(start, end, to) {
                    Err(error) => {
                        assert!(matches!(error, EditError::Straddles(_)), "{error}");
                        assert_eq!(doc, original, "a refused move changes nothing");
                        refused += 1;
                    }
                    Ok(outcome) if !outcome.moved => assert_eq!(doc, original),
                    Ok(outcome) => {
                        moved += 1;
                        assert_eq!(utf16_len(&doc.body), len);
                        for s in &doc.annotations.spans {
                            assert_eq!(text_at(&doc, s.anchor.start, s.anchor.end), s.anchor.text);
                        }
                        for g in &doc.annotations.ghosts {
                            assert_eq!(text_at(&doc, g.anchor.start, g.anchor.end), g.anchor.text);
                        }
                        let back = if to > end { start } else { end };
                        doc.move_range(outcome.start, outcome.end, back).unwrap();
                        assert_eq!(doc, original, "move {start}..{end} to {to} and back");
                    }
                }
            }
        }
    }
    assert!(
        moved > 1000 && refused > 1000,
        "{moved} moved, {refused} refused"
    );
}

#[test]
fn a_ghost_holding_both_the_range_and_the_destination_keeps_its_extent() {
    let mut doc = Document::new("Keep. One two three. End.");
    let outer = ghost(&mut doc, "One two three.");
    let word = span(&mut doc, "two", &["2"]);
    let (start, end) = range(&doc, "two ");
    let to = at(&doc, "One");
    let outcome = doc.move_range(start, end, to).unwrap();
    assert_eq!(doc.body, "Keep. two One three. End.");
    assert_eq!(outcome.carried, vec![word.clone()]);
    let g = doc.ghost_at(to).unwrap();
    assert_eq!(g.id, outer);
    assert_eq!(
        g.anchor.text, "two One three.",
        "the reshuffled text stays ghosted"
    );
    assert_eq!(doc.span(&word).unwrap().anchor.start, to);
    assert_consistent(&doc);

    // The destination may be either boundary of the container.
    let ghost_end = doc.ghost_at(to).unwrap().anchor.end;
    let (start, end) = range(&doc, "two ");
    doc.move_range(start, end, ghost_end).unwrap();
    assert_eq!(doc.body, "Keep. One three.two  End.");
    assert_eq!(
        doc.ghost_at(at(&doc, "One")).unwrap().anchor.text,
        "One three.two "
    );
    assert_consistent(&doc);
}

#[test]
fn adjacent_ghosts_are_carried_together_and_touching_ghosts_are_not_merged() {
    let mut doc = Document::new("aa bb cc dd ee");
    let ga = ghost(&mut doc, "aa ");
    let gb = ghost(&mut doc, "bb");
    // `ghost` merges touching ranges, so the two above are one ghost.
    assert_eq!(ga, gb);
    let gd = ghost(&mut doc, "dd");
    assert_eq!(doc.annotations.ghosts.len(), 2);

    // Carry the merged ghost so that it ends exactly where `dd` starts.
    let (start, end) = range(&doc, "aa bb");
    let to = at(&doc, "dd");
    doc.move_range(start, end, to).unwrap();
    assert_eq!(doc.body, " cc aa bbdd ee");
    let ids: Vec<_> = doc
        .annotations
        .ghosts
        .iter()
        .map(|g| g.id.clone())
        .collect();
    assert_eq!(ids, vec![ga.clone(), gd.clone()], "touching, not merged");
    assert_eq!(
        doc.annotations.ghosts[0].anchor.end,
        doc.annotations.ghosts[1].anchor.start
    );
    assert_consistent(&doc);

    // A ghost ending at the destination, or starting at it, is outside.
    let (start, end) = range(&doc, " cc");
    let to = doc.annotations.ghosts[0].anchor.end;
    doc.move_range(start, end, to).unwrap();
    assert_eq!(doc.body, " aa bb ccdd ee");
    assert_consistent(&doc);
}

#[test]
fn moves_that_would_split_an_item_are_refused_by_name() {
    let mut doc = Document::new("Alpha beta gamma. Delta epsilon. Zeta eta.");
    let beta = span(&mut doc, "beta gamma", &["b g"]);
    let g = ghost(&mut doc, "Delta epsilon.");
    let before = doc.clone();
    let refuse = |doc: &mut Document, start: usize, end: usize, to: usize| {
        let error = doc.move_range(start, end, to).unwrap_err();
        assert_eq!(*doc, before, "unchanged after {error}");
        error
    };
    let straddles = |id: &str| EditError::Straddles(id.to_string());

    // Range partly overlapping a span, and partly overlapping a ghost.
    let (s, _) = range(&doc, "Alpha");
    let (_, e) = range(&doc, "beta");
    assert_eq!(
        refuse(&mut doc, s, e, utf16_len(&before.body)),
        straddles(&beta)
    );
    let (s, e) = range(&doc, ". Delta");
    assert_eq!(refuse(&mut doc, s, e, 0), straddles(&g));
    // Destination strictly inside a span, and inside a ghost.
    let (s, e) = range(&doc, "Zeta");
    let (gamma, epsilon) = (at(&doc, "gamma"), at(&doc, "epsilon"));
    assert_eq!(refuse(&mut doc, s, e, gamma), straddles(&beta));
    assert_eq!(refuse(&mut doc, s, e, epsilon), straddles(&g));
    // A span containing the range (its text would change).
    let (s, e) = range(&doc, "gamma");
    assert_eq!(refuse(&mut doc, s, e, 0), straddles(&beta));
    // A ghost containing the range but not the destination.
    let (s, e) = range(&doc, "epsilon");
    assert_eq!(refuse(&mut doc, s, e, 0), straddles(&g));
    let error = straddles(&g).to_string();
    assert!(error.contains("\"g") && error.contains("whole"), "{error}");
}

#[test]
fn invalid_ranges_and_destinations_are_refused() {
    let mut doc = Document::new("a😀b café");
    let before = doc.clone();
    let len = utf16_len(&doc.body);
    assert_eq!(
        doc.move_range(3, 3, 0),
        Err(EditError::InvalidRange { start: 3, end: 3 })
    );
    assert_eq!(
        doc.move_range(3, 1, 0),
        Err(EditError::InvalidRange { start: 3, end: 1 })
    );
    // Offset 2 splits the emoji's surrogate pair.
    assert_eq!(
        doc.move_range(0, 2, 5),
        Err(EditError::InvalidRange { start: 0, end: 2 })
    );
    assert_eq!(
        doc.move_range(3, 4, 2),
        Err(EditError::InvalidDestination {
            start: 3,
            end: 4,
            to: 2
        })
    );
    assert_eq!(
        doc.move_range(0, 1, len + 1),
        Err(EditError::InvalidDestination {
            start: 0,
            end: 1,
            to: len + 1
        })
    );
    assert_eq!(
        doc.move_range(4, 8, 6),
        Err(EditError::InvalidDestination {
            start: 4,
            end: 8,
            to: 6
        }),
        "inside the range"
    );
    assert_eq!(doc, before);

    // Moving to either end of the range is a no-op.
    for to in [1, 3] {
        let outcome = doc.move_range(1, 3, to).unwrap();
        assert_eq!((outcome.start, outcome.end, outcome.moved), (1, 3, false));
        assert_eq!(doc, before);
    }
}

#[test]
fn multi_byte_and_astral_text_moves_with_exact_offsets() {
    let mut doc = Document::new("😀 Café crème.\n\n𝒜 naïve idea.\n\n");
    let creme = span(&mut doc, "crème", &["crema"]);
    let g = ghost(&mut doc, "𝒜 naïve");
    let (start, end) = range(&doc, "𝒜 naïve idea.\n\n");
    let outcome = doc.move_range(start, end, 0).unwrap();
    assert_eq!(doc.body, "𝒜 naïve idea.\n\n😀 Café crème.\n\n");
    assert_eq!(outcome.carried, vec![g.clone()]);
    assert_eq!((outcome.start, outcome.end), (0, end - start));
    let ghost = doc.ghost_at(0).unwrap();
    assert_eq!((ghost.id.as_str(), ghost.anchor.end), (g.as_str(), 8));
    assert_eq!(doc.span(&creme).unwrap().anchor.start, at(&doc, "crème"));
    assert_consistent(&doc);

    doc.set_active(&creme, 1).unwrap();
    assert_eq!(doc.body, "𝒜 naïve idea.\n\n😀 Café crema.\n\n");
    assert_consistent(&doc);
}

#[test]
fn context_is_refreshed_for_moved_and_neighbouring_items() {
    let (mut doc, [s1, _, s2, g2]) = three_blocks();
    let old_s1 = doc.span(&s1).unwrap().anchor.clone();
    let old_s2 = doc.span(&s2).unwrap().anchor.clone();
    let (start, end) = range(&doc, "Pass me an eraser. Drop this.\n\n");
    let to = at(&doc, "A closing");
    doc.move_range(start, end, to).unwrap();

    let new_s1 = &doc.span(&s1).unwrap().anchor;
    assert_ne!(
        new_s1.before, old_s1.before,
        "the moved span sees new text before it"
    );
    assert!(
        new_s1
            .before
            .as_deref()
            .unwrap()
            .ends_with("high.\n\nPass me an ")
    );
    let new_s2 = &doc.span(&s2).unwrap().anchor;
    assert_ne!(new_s2.before, old_s2.before, "the span the move passed");
    assert_eq!(new_s2.before.as_deref(), Some("The "));
    let g2 = doc.annotations.ghosts.iter().find(|g| g.id == g2).unwrap();
    assert!(
        g2.anchor
            .before
            .as_deref()
            .unwrap()
            .ends_with("Drop this.\n\n")
    );
    assert_consistent(&doc);

    // A reload re-anchors every item where it is, with nothing unresolved.
    let mut reloaded = parse(&write(&doc)).unwrap();
    assert!(reloaded.reanchor().is_clean());
    assert_eq!(reloaded, doc);
}

#[test]
fn stale_items_the_move_would_touch_are_refused() {
    let (mut doc, [s1, _, _, g2]) = three_blocks();
    // Corrupt the stored text of a span the move would carry, then of a
    // ghost it would shift.
    let index = doc
        .annotations
        .spans
        .iter()
        .position(|s| s.id == s1)
        .unwrap();
    doc.annotations.spans[index].anchor.text = "rubber".into();
    let (start, end) = range(&doc, "Pass me an eraser.");
    let before = doc.clone();
    assert_eq!(
        doc.move_range(start, end, utf16_len(&doc.body)),
        Err(EditError::StaleAnchor(s1.clone()))
    );
    assert_eq!(doc, before);

    let (mut doc, _) = three_blocks();
    doc.annotations.ghosts[1].anchor.text = "Another line..".into();
    let (start, end) = range(&doc, "Pass me");
    let to = utf16_len(&doc.body);
    assert_eq!(
        doc.move_range(start, end, to),
        Err(EditError::StaleAnchor(g2))
    );
    // A stale item the move leaves alone does not block it.
    let (s, e) = range(&doc, "Pass me");
    let t = at(&doc, "The tension");
    assert!(doc.move_range(s, e, t).unwrap().moved);
}
