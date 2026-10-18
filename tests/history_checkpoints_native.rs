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

fn ghost_edit_fixture() -> String {
    let mut doc = Document::new("Risk needs review.");
    doc.annotations.ghosts.push(Ghost {
        id: "g1".into(),
        anchor: Anchor::new(0, 4, "Risk"),
    });
    write(&doc)
}

#[test]
fn issued_text_edit_receipt_restores_partial_ghost_and_tail_deletion_exactly() {
    for (start, deleted, inserted) in [(1, "isk", "isk-long"), (2, "sk", "")] {
        let original = ghost_edit_fixture();
        with_session(|s| {
            s.open(&original);
        });
        let receipt = with_session(|s| {
            s.apply_edit(start, deleted.len(), inserted)
                .unwrap()
                .to_json()["ghostEdit"]
                .clone()
        });
        assert!(
            receipt.is_object(),
            "actual edit must issue private Ghost mapping receipt"
        );
        let after = with_session(|s| s.save());
        let body_after = with_session(|s| s.body().to_owned());
        let undo = json!([{"kind":"edit","body":"Risk needs review.","start":start,"deletedText":inserted,"insertedText":deleted,"ghostEdit":{"receipt":receipt["receipt"],"undo":true}}]).to_string();
        let begun = begin_history_replay_json(&body_after, "Risk needs review.", &undo).unwrap();
        finish_history_transaction_json(begun["token"].as_str().unwrap()).unwrap();
        assert_eq!(with_session(|s| s.save()), original);
        let redo = json!([{"kind":"edit","body":body_after,"start":start,"deletedText":deleted,"insertedText":inserted,"ghostEdit":receipt}]).to_string();
        let begun = begin_history_replay_json("Risk needs review.", &body_after, &redo).unwrap();
        finish_history_transaction_json(begun["token"].as_str().unwrap()).unwrap();
        assert_eq!(with_session(|s| s.save()), after);
    }
}

#[test]
fn text_ghost_receipts_refuse_forgery_stale_state_cross_session_and_late_group_failure() {
    let original = ghost_edit_fixture();
    with_session(|s| {
        s.open(&original);
    });
    let receipt = with_session(|s| s.apply_edit(2, 2, "").unwrap().to_json()["ghostEdit"].clone());
    let changed = with_session(|s| s.clone());
    let undo_op = json!({"kind":"edit","body":"Risk needs review.","start":2,"deletedText":"","insertedText":"sk","ghostEdit":{"receipt":receipt["receipt"],"undo":true}});
    for field in ["receipt", "start", "insertedText"] {
        let mut forged = undo_op.clone();
        match field {
            "receipt" => forged["ghostEdit"]["receipt"] = json!("unissued-text-ghost"),
            "start" => forged["start"] = json!(1),
            _ => forged["insertedText"] = json!("SK"),
        }
        assert!(begin_history_replay_json(
            "Ri needs review.",
            "Risk needs review.",
            &json!([forged]).to_string()
        )
        .is_err());
        assert_eq!(with_session(|s| s.clone()), changed);
    }
    // A valid first receipt must not publish anything if a later operation fails.
    let group = json!([undo_op.clone(), {"kind":"edit","body":"wrong","start":0,"deletedText":"Risk","insertedText":"wrong"}]);
    assert!(begin_history_replay_json("Ri needs review.", "wrong", &group.to_string()).is_err());
    assert_eq!(with_session(|s| s.clone()), changed);
    with_session(|s| {
        s.revive(0, 2).unwrap();
    });
    let stale = with_session(|s| s.clone());
    assert!(begin_history_replay_json(
        "Ri needs review.",
        "Risk needs review.",
        &json!([undo_op.clone()]).to_string()
    )
    .is_err());
    assert_eq!(with_session(|s| s.clone()), stale);
    // Same serialized source in a new session has no old private issued registry.
    with_session(|s| {
        s.open(&changed.save());
    });
    let reopened = with_session(|s| s.clone());
    assert!(begin_history_replay_json(
        "Ri needs review.",
        "Risk needs review.",
        &json!([undo_op]).to_string()
    )
    .is_err());
    assert_eq!(with_session(|s| s.clone()), reopened);
    let next_receipt = with_session(|s| {
        s.open(&original);
        s.apply_edit(2, 2, "").unwrap().to_json()["ghostEdit"].clone()
    });
    assert_ne!(
        next_receipt["receipt"], receipt["receipt"],
        "open never reissues an old opaque name"
    );
}

#[test]
fn native_action_issues_the_same_actual_edit_metadata_and_abort_retires_it() {
    use sha2::{Digest, Sha256};
    use terraphim_editor::begin_document_action_json;
    let hash = |text: &str| format!("{:x}", Sha256::digest(text.as_bytes()));
    with_session(|s| {
        s.open(&ghost_edit_fixture());
    });
    let original = with_session(|s| s.clone());
    let meta = json!({"kind":"text_edit","start":2,"deleteUtf16":2,"insertedRef":0,"beforeBodyHash":hash("Risk needs review."),"afterBodyHash":hash("Ri needs review.")});
    let started =
        begin_document_action_json("Risk needs review.", &meta.to_string(), &[String::new()])
            .unwrap()
            .unwrap();
    let receipt = started["result"]["edit"]["ghostEdit"].clone();
    assert!(receipt.is_object());
    assert_eq!(receipt, started["result"]["ghostEdit"]);
    abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
    assert_eq!(with_session(|s| s.clone()), original);
    let redo = json!([{"kind":"edit","body":"Ri needs review.","start":2,"deletedText":"sk","insertedText":"","ghostEdit":receipt}]);
    assert!(
        begin_history_replay_json("Risk needs review.", "Ri needs review.", &redo.to_string())
            .is_err(),
        "aborted preparation never admits its receipt"
    );
}

#[test]
fn malformed_text_ghost_metadata_and_ghost_free_edits_keep_strict_contracts() {
    for reference in [
        json!({"receipt":"x"}),
        json!({"receipt":"x","undo":0}),
        json!({"receipt":"x","undo":true,"snapshot":{}}),
    ] {
        let op = json!([{"kind":"edit","body":"a","start":0,"deletedText":"","insertedText":"a","ghostEdit":reference}]);
        assert!(parse_history_operations(&op.to_string()).is_err());
    }
    with_session(|s| {
        s.open("abc");
        assert!(s
            .apply_edit(3, 0, "X")
            .unwrap()
            .to_json()
            .get("ghostEdit")
            .is_none());
    });
}
