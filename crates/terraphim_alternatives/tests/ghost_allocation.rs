use terraphim_alternatives::{Anchor, Document, EditError, Ghost};

#[test]
fn adjacent_independent_ghost_export_excludes_whitespace_prefixed_suffix() {
    let mut doc = Document::new("a bc");
    doc.create_independent_ghost(0, 1, || Ok("g1".into()))
        .unwrap();
    doc.create_independent_ghost(1, 4, || Ok("g2".into()))
        .unwrap();
    let before = doc.clone();
    assert_eq!(doc.annotations.ghosts.len(), 2);
    assert_eq!(doc.export(), "");
    assert_eq!(doc, before);
}

#[test]
fn export_union_preserves_visible_islands_unicode_and_storage_order() {
    for (body, ranges, expected) in [
        ("a\nbc", vec![(0, 1), (1, 4)], ""),
        ("a\n\n\nbc", vec![(0, 1), (1, 6)], ""),
        ("a  bc", vec![(0, 1), (2, 5)], ""),
        ("a\n\n\nbc", vec![(0, 1), (2, 6)], ""),
        ("a 😀X", vec![(0, 1), (1, 4)], "X"),
        (
            "a bc KEEP de f",
            vec![(0, 1), (1, 4), (10, 12), (12, 14)],
            "KEEP",
        ),
        ("a\n\nKEEP\n\nbc", vec![(0, 1), (9, 11)], "KEEP\n\n"),
    ] {
        let mut doc = Document::new(body);
        for (i, &(start, end)) in ranges.iter().rev().enumerate() {
            doc.create_independent_ghost(start, end, || Ok(format!("g{}", i + 1)))
                .unwrap();
        }
        let before = doc.clone();
        assert_eq!(doc.export(), expected, "{body:?}");
        assert_eq!(doc, before, "export must not merge stored identities");
        doc.annotations.ghosts.reverse();
        assert_eq!(
            doc.export(),
            expected,
            "export order is by original offsets"
        );
    }
}

#[test]
fn independent_overlap_is_refused_but_export_projection_unions_legacy_model_state() {
    let mut doc = Document::new("abcdef");
    doc.create_independent_ghost(0, 3, || Ok("g1".into()))
        .unwrap();
    let before = doc.clone();
    assert_eq!(
        doc.create_independent_ghost(2, 5, || panic!("overlap must not allocate")),
        Err(EditError::Overlap("g1".into()))
    );
    assert_eq!(doc, before);
    // Public mutable model state is not a supported independent promotion;
    // export must nevertheless omit its union rather than leak a suffix.
    doc.annotations.ghosts.push(Ghost {
        id: "legacy".into(),
        anchor: Anchor::new(2, 5, "cde"),
    });
    let before = doc.clone();
    assert_eq!(doc.export(), "f");
    assert_eq!(doc, before);
}

#[test]
fn allocator_preflights_merge_split_and_independent_paths() {
    let mut doc = Document::new("abcdef");
    doc.create_independent_ghost(0, 2, || Ok("g10".into()))
        .unwrap();
    doc.create_independent_ghost(2, 4, || Ok("g11".into()))
        .unwrap();
    assert_eq!(doc.annotations.ghosts.len(), 2);
    assert_eq!(
        doc.ghost_with_allocator(1, 3, || panic!("merge must not allocate"))
            .unwrap(),
        "g10"
    );
    let before = doc.clone();
    assert_eq!(
        doc.revive_with_allocator(1, 2, || Err(EditError::IdAllocation)),
        Err(EditError::IdAllocation)
    );
    assert_eq!(doc, before);
    assert_eq!(
        doc.revive_with_allocator(1, 2, || Ok("g10".into())),
        Err(EditError::IdAllocation)
    );
    assert_eq!(doc, before);
    doc.revive_with_allocator(1, 2, || Ok("g12".into()))
        .unwrap();
    assert_eq!(doc.annotations.ghosts[1].id, "g12");
}

#[test]
fn numeric_exhaustion_refuses_before_model_mutation() {
    let mut doc = Document::new("abcdef");
    doc.annotations.ghosts.push(Ghost {
        id: format!("g{}", u64::MAX),
        anchor: Anchor::new(0, 4, "abcd"),
    });
    let before = doc.clone();
    assert_eq!(doc.ghost(5, 6), Err(EditError::IdAllocation));
    assert_eq!(doc.revive(1, 2), Err(EditError::IdAllocation));
    assert_eq!(doc, before);
    // Exhaustion never blocks identity-preserving merge/trim.
    doc.ghost(0, 4).unwrap();
    doc.revive(0, 1).unwrap();
}
