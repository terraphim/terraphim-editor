//! Native public-session contract; no browser or history-capacity claim.
use serde_json::json;
use terraphim_editor::DocumentSession;

#[test]
fn adjacent_independent_ghost_export_excludes_whitespace_prefixed_suffix() {
    let mut session = DocumentSession::new();
    session.open("a bc");
    session
        .ghost_ranges_atomic("a bc", &[(0, 1), (1, 4)])
        .unwrap()
        .unwrap();
    let before = session.clone();
    assert_eq!(session.document().annotations.ghosts.len(), 2);
    assert_eq!(session.export(), "");
    assert_eq!(session, before);
}

#[test]
fn manual_and_promoted_whitespace_adjacency_round_trips_without_identity_merge() {
    for (body, manual, promoted, exported, undo_export) in [
        ("a bc", (1, 4), vec![(0, 1)], "", "a"),
        ("a bc", (0, 1), vec![(1, 4)], "", "bc"),
        ("a\n\nbc", (1, 5), vec![(0, 1)], "", "a"),
        ("a 😀X", (1, 4), vec![(0, 1)], "X", "aX"),
        (
            "a bc KEEP de f",
            (0, 1),
            vec![(1, 4), (10, 12), (12, 14)],
            "KEEP",
            "bc KEEP de f",
        ),
    ] {
        let mut session = DocumentSession::new();
        session.open(body);
        session.ghost(manual.0, manual.1).unwrap();
        let manual_ghosts = session.document().annotations.ghosts.clone();
        let change = session
            .ghost_ranges_atomic(body, &promoted)
            .unwrap()
            .unwrap();
        let after = session.clone();
        let receipt = change.to_json();
        let saved = session.save();
        assert_eq!(session.export(), exported, "{body:?}");
        assert_eq!(session, after, "export/save do not mutate session state");
        assert_eq!(
            session.document().annotations.ghosts.len(),
            promoted.len() + 1
        );
        let mut reopened = DocumentSession::new();
        reopened.open(&saved);
        assert_eq!(reopened.export(), exported);
        assert_eq!(reopened.body(), body);
        assert_eq!(reopened.save(), saved);
        assert_eq!(
            reopened.document().annotations.ghosts,
            session.document().annotations.ghosts
        );
        session.replay_ghost_change(&change, true).unwrap();
        assert_eq!(session.document().annotations.ghosts, manual_ghosts);
        assert_eq!(session.export(), undo_export);
        assert_eq!(
            session.ghost_snapshot().reserved_ids,
            change.after.reserved_ids
        );
        session.replay_ghost_change(&change, false).unwrap();
        assert_eq!(session, after);
        assert_eq!(session.export(), exported);
        assert_eq!(session.save(), saved);
        assert_eq!(change.to_json(), receipt);
        let before = session.clone();
        assert!(session
            .ghost_ranges_atomic(body, &[(manual.0, manual.1)])
            .is_err());
        assert_eq!(
            session, before,
            "overlap is refused, not merged to hide export bugs"
        );
    }
}

#[test]
fn public_transaction_receipt_is_exact_and_survives_annotation_save() {
    let mut session = DocumentSession::new();
    session.open("abcdef");
    session.ghost(4, 6).unwrap();
    let manual =
        json!({"id":"g1", "anchor":{"start":4,"end":6,"text":"ef","before":"abcd","after":""}});
    let first =
        json!({"id":"g2", "anchor":{"start":0,"end":2,"text":"ab","before":"","after":"cdef"}});
    let second =
        json!({"id":"g3", "anchor":{"start":2,"end":4,"text":"cd","before":"ab","after":"ef"}});
    let change = session
        .ghost_ranges_atomic("abcdef", &[(0, 2), (2, 4)])
        .unwrap()
        .unwrap();
    assert_eq!(
        change.to_json(),
        json!({
            "body":"abcdef",
            "before":{"live":[manual.clone()],"setAside":[],"reservedIds":["g1"]},
            "after":{"live":[first,second,manual],"setAside":[],"reservedIds":["g1","g2","g3"]}
        })
    );
    let saved = session.save();
    assert!(saved.contains("\"version\": 1"));
    let mut reopened = DocumentSession::new();
    reopened.open(&saved);
    assert_eq!(reopened.body(), "abcdef");
    assert_eq!(reopened.document().annotations.ghosts.len(), 3);
    assert_eq!(reopened.export(), "");
    assert_eq!(reopened.save(), saved);
    session.preflight_ghost_change(&change, true).unwrap();
    session.replay_ghost_change(&change, true).unwrap();
    assert_eq!(session.export(), "abcd");
    session.replay_ghost_change(&change, false).unwrap();
    assert_eq!(session.save(), saved);
    let before = reopened.clone();
    assert!(
        reopened.replay_ghost_change(&change, true).is_err(),
        "receipt belongs to issuing open session"
    );
    assert_eq!(reopened, before);
}

#[test]
fn failed_public_commit_and_guard_leave_session_exactly_unchanged() {
    let mut session = DocumentSession::new();
    session.open("a😀bc");
    let before = session.clone();
    assert!(session
        .ghost_ranges_atomic("a😀bc", &[(0, 1), (2, 3)])
        .is_err());
    assert_eq!(session, before);
    let change = session
        .ghost_ranges_atomic("a😀bc", &[(1, 3)])
        .unwrap()
        .unwrap();
    session.ghost(4, 5).unwrap();
    let before = session.clone();
    assert!(session.preflight_ghost_change(&change, true).is_err());
    assert!(session.replay_ghost_change(&change, true).is_err());
    assert_eq!(session, before);
}
