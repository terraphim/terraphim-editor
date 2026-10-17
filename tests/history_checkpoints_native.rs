//! Real public native checkpoint APIs; not JS history/browser acceptance.
use serde_json::json;
use terraphim_alternatives::{parse, write, Anchor, Document, Ghost};
use terraphim_editor::{
    abort_history_transaction_json, begin_history_replay_json, finish_history_transaction_json,
    parse_history_operations, with_session,
};

#[test]
fn public_mixed_checkpoint_preserves_large_string_ids_and_exact_abort_state() {
    let mut doc = Document::new("abcdef");
    doc.annotations.ghosts.push(Ghost {
        id: "g9007199254740993".into(),
        anchor: Anchor::new(4, 6, "ef"),
    });
    let change = with_session(|s| {
        s.open(&write(&doc));
        let change = s.ghost_ranges_atomic("abcdef", &[(0, 2)]).unwrap().unwrap();
        s.apply_edit(6, 0, "X").unwrap();
        change
    });
    assert_eq!(change.after.live[0].id, "g9007199254740994");
    let before = with_session(|s| s.clone());
    let operations = json!([
        {"kind":"edit","body":"abcdef","start":6,"deletedText":"X","insertedText":""},
        {"kind":"ghost","body":"abcdef","change":change.to_json(),"undo":true}
    ])
    .to_string();
    let started = begin_history_replay_json("abcdefX", "abcdef", &operations).unwrap();
    assert_eq!(
        with_session(|s| s.document().annotations.ghosts[0].id.clone()),
        "g9007199254740993"
    );
    let token = started["token"].as_str().unwrap();
    assert_eq!(
        abort_history_transaction_json(token).unwrap()["rolledBack"],
        true
    );
    assert_eq!(with_session(|s| s.clone()), before);
    let started = begin_history_replay_json("abcdefX", "abcdef", &operations).unwrap();
    let token = started["token"].as_str().unwrap();
    assert_eq!(
        finish_history_transaction_json(token).unwrap()["finalized"],
        true
    );
    assert!(abort_history_transaction_json(token).is_err());
    assert!(with_session(|s| s
        .ghost_snapshot()
        .reserved_ids
        .contains(&"g9007199254740994".into())));
}

fn alternative_target(id: &str) -> serde_json::Value {
    json!({"id":id,"kind":"word","anchor":{"start":1,"end":2,"text":"b"},"active":0,
        "alts":[{"text":"b","source":"original"},{"text":"bee","source":"human"}]})
}

#[test]
fn public_issued_ghost_then_colliding_alternative_refuses_entire_group() {
    let receipt = with_session(|s| {
        s.open("abc");
        s.set_overflow("notes").unwrap();
        let receipt = s.ghost_ranges_atomic("abc", &[(0, 1)]).unwrap().unwrap();
        s.replay_ghost_change(&receipt, true).unwrap();
        receipt
    });
    let before = with_session(|s| s.clone());
    let operations = json!([
        {"kind":"edit","body":"abcX","start":3,"deletedText":"","insertedText":"X"},
        {"kind":"edit","body":"abc","start":3,"deletedText":"X","insertedText":""},
        {"kind":"ghost","body":"abc","change":receipt.to_json(),"undo":false},
        {"kind":"alt","body":"abc","start":0,"deletedText":"","insertedText":"","span":"g1","target":alternative_target("g1")}
    ]).to_string();
    let result = begin_history_replay_json("abc", "abc", &operations);
    if let Ok(started) = &result {
        let saved = with_session(|s| s.save());
        let error = parse(&saved).expect_err("reproduction publishes duplicate IDs");
        eprintln!("P1 reproduced: accepted group saves an invalid annotation block: {error}");
        abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
    }
    assert!(
        result.is_err(),
        "P1: mixed replay must refuse cross-layer duplicate identity"
    );
    assert_eq!(with_session(|s| s.clone()), before);
    assert!(parse(&with_session(|s| s.save())).is_ok());
    // Failed simulation must not issue or leave a pending checkpoint.
    let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
    assert_eq!(started["token"], "history-1");
    abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
}

#[test]
fn public_alternative_edit_reattachment_collision_and_standalone_restore_are_atomic() {
    with_session(|s| {
        s.open("abc");
        s.ghost_ranges_atomic("abc", &[(0, 1)]).unwrap().unwrap();
        s.apply_edit(0, 1, "").unwrap();
        s.set_overflow("notes").unwrap();
        assert_eq!(s.set_aside().ghosts[0].id, "g1");
    });
    let before = with_session(|s| s.clone());
    let operations = json!([
        {"kind":"edit","body":"bcX","start":2,"deletedText":"","insertedText":"X"},
        {"kind":"edit","body":"bc","start":2,"deletedText":"X","insertedText":""},
        {"kind":"alt","body":"abc","start":0,"deletedText":"","insertedText":"a","span":"g1","target":alternative_target("g1")}
    ]).to_string();
    assert_eq!(
        begin_history_replay_json("bc", "abc", &operations)
            .unwrap_err()
            .kind,
        "model-refusal"
    );
    assert_eq!(with_session(|s| s.clone()), before);
    let target = serde_json::from_value(alternative_target("g1")).unwrap();
    assert!(with_session(|s| s.alt_restore("g1", Some(target), 0, 0, "a")).is_err());
    assert_eq!(with_session(|s| s.clone()), before);
    assert!(parse(&with_session(|s| s.save())).is_ok());
    let started = begin_history_replay_json("bc", "bc", "[]").unwrap();
    assert_eq!(started["token"], "history-1");
    abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
}

#[test]
fn public_distinct_historical_alternative_and_ghost_round_trip() {
    let (receipt, target) = with_session(|s| {
        s.open("abc");
        let target = serde_json::from_value(alternative_target("historical-alt")).unwrap();
        s.alt_restore("historical-alt", Some(target), 0, 0, "")
            .unwrap();
        let target = s.document().annotations.spans[0].clone();
        let receipt = s.ghost_ranges_atomic("abc", &[(0, 1)]).unwrap().unwrap();
        s.replay_ghost_change(&receipt, true).unwrap();
        s.alt_restore("historical-alt", None, 0, 0, "").unwrap();
        assert!(s
            .ghost_snapshot()
            .reserved_ids
            .contains(&"historical-alt".into()));
        (receipt, target)
    });
    let before = with_session(|s| s.clone());
    let operations = json!([
        {"kind":"ghost","body":"abc","change":receipt.to_json(),"undo":false},
        {"kind":"alt","body":"abc","start":0,"deletedText":"","insertedText":"","span":"historical-alt","target":target}
    ]).to_string();
    let started = begin_history_replay_json("abc", "abc", &operations).unwrap();
    let doc = parse(&with_session(|s| s.save())).unwrap();
    assert_eq!(doc.annotations.spans[0].id, "historical-alt");
    assert_eq!(doc.annotations.ghosts[0].id, "g1");
    abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
    assert_eq!(with_session(|s| s.clone()), before);
    let started = begin_history_replay_json("abc", "abc", &operations).unwrap();
    finish_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
    assert_eq!(parse(&with_session(|s| s.save())).unwrap(), doc);
}

#[test]
fn public_strict_group_refusal_preserves_every_session_field() {
    with_session(|s| {
        s.open("a😀b");
    });
    let before = with_session(|s| s.clone());
    let operations = json!([
        {"kind":"edit","body":"a😀bX","start":4,"deletedText":"","insertedText":"X"},
        {"kind":"edit","body":"bad","start":2,"deletedText":"","insertedText":"Y"}
    ])
    .to_string();
    assert!(begin_history_replay_json("a😀b", "bad", &operations).is_err());
    assert_eq!(with_session(|s| s.clone()), before);
    assert!(parse_history_operations(
        r#"[{"kind":"edit","body":"a😀b","start":0,"deletedText":"","insertedText":"","ghost":{}}]"#
    )
    .is_err());
    assert_eq!(with_session(|s| s.clone()), before);
    let started = begin_history_replay_json("a😀b", "a😀b", "[]").unwrap();
    abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
}
