//! Public-fixture native diagnosis only; no JS/browser qualification or projection fix.
use terraphim_editor::{
    abort_history_transaction_json, begin_history_replay_json, begin_trim_ghost_transaction_json,
    with_session,
};
use terraphim_lab::{trim_plan, CutId, LabConfig, TrimLevel};

const BODY: &str = include_str!("fixtures/lab/lab.md");
const ROLE: &str =
    r#"{"name":"trim-public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#;
const OVERLAP_ERROR: &str = "independent ghost range overlaps existing ghost or another range";

fn probe_checkpoint(body: &str, expected_token: &str) {
    let before = with_session(|s| s.clone());
    let started = begin_history_replay_json(body, body, "[]").expect("empty checkpoint probe");
    assert!(started["ok"] == true);
    let token = started["token"].as_str().expect("opaque token");
    assert_eq!(token, expected_token, "refusal must not issue a token");
    let ack = abort_history_transaction_json(token).expect("real abort");
    assert!(ack["ok"] == true && ack["rolledBack"] == true && ack["token"] == token);
    assert!(
        with_session(|s| s.clone()) == before,
        "probe must restore exact session"
    );
}

fn fixture_case(first_keep: bool, manual: bool) {
    with_session(|s| {
        s.open(BODY);
        if manual {
            s.ghost_ranges_atomic(BODY, &[(0, 1)]).unwrap();
        }
    });
    terraphim_editor::load_thesaurus(ROLE).expect("public absent role");
    let role = serde_json::from_str(ROLE).unwrap();
    let protection =
        terraphim_editor::protection::ConceptProtection::from_thesaurus(&role).unwrap();
    assert!(protection.occurrences(BODY).unwrap().is_empty());
    let config = LabConfig::with_defaults().unwrap();
    let protected_plan = protection.plan(BODY, &config).unwrap();
    let original = trim_plan(BODY, &config);
    let initial = original.active(TrimLevel::Sharper, &[]);
    let kept: Vec<CutId> = if first_keep {
        vec![initial[0].id]
    } else {
        vec![]
    };
    let kept_json = serde_json::to_string(&kept).unwrap();
    let expected = original.active(TrimLevel::Sharper, &kept);
    let active = protected_plan.active(TrimLevel::Sharper, &kept);
    let tuples = |cuts: &[terraphim_lab::Cut]| {
        cuts.iter()
            .map(|c| (c.id.0, c.start, c.end))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tuples(&active),
        tuples(&expected),
        "protected preview preserves original oracle"
    );
    protection.validate_ranges(BODY, &active).unwrap();
    protection
        .validate_retention(BODY, &terraphim_lab::make_cuts(BODY, &active))
        .unwrap();
    let ranges: Vec<_> = active.iter().map(|c| (c.start, c.end)).collect();
    assert!(ranges.windows(2).all(|w| w[0] <= w[1]));
    let boundaries: Vec<usize> = std::iter::once(0)
        .chain(BODY.chars().scan(0, |offset, ch| {
            *offset += ch.len_utf16();
            Some(*offset)
        }))
        .collect();
    assert!(ranges
        .iter()
        .all(|&(s, e)| s < e && boundaries.contains(&s) && boundaries.contains(&e)));
    assert!(
        ranges.iter().all(|&(s, _)| s >= 1),
        "manual heading ghost is disjoint"
    );
    let mut overlaps = Vec::new();
    for (i, &(s, e)) in ranges.iter().enumerate() {
        for (j, &(a, b)) in ranges.iter().enumerate().take(i) {
            if s < b && a < e {
                let containment = (a <= s && e <= b) || (s <= a && b <= e);
                overlaps.push((
                    j,
                    i,
                    if containment {
                        "containment"
                    } else {
                        "crossing"
                    },
                ));
            }
        }
    }
    println!(
        "level=sharper kept={:?} manual={} active={:?} overlaps={:?}",
        kept.iter().map(|id| id.0).collect::<Vec<_>>(),
        manual,
        tuples(&active),
        overlaps
    );
    assert!(
        !overlaps.is_empty(),
        "exact fixture must exhibit the suspected overlap"
    );
    assert_eq!(overlaps.len(), 4, "exact fixture has four overlap pairs");
    assert!(overlaps.iter().all(|(_, _, kind)| *kind == "containment"));
    // An acknowledged empty probe proves checkpoint availability, establishes
    // the opaque watermark, and leaves the complete session unchanged.
    probe_checkpoint(BODY, "history-1");
    let before = with_session(|s| s.clone());
    let reservations = with_session(|s| s.ghost_snapshot().reserved_ids);
    let batch_error = with_session(|s| s.ghost_ranges_atomic(BODY, &ranges))
        .expect_err("exact protected active batch must refuse overlap");
    assert_eq!(batch_error, OVERLAP_ERROR);
    assert!(with_session(|s| s.clone()) == before);
    println!("direct_active_batch_refusal=overlap");
    // Cache publication is private; do not pretend the independently computed
    // real B plan above has entered the production protected-plan cache.
    let seam_error = terraphim_editor::protected_trim_ranges("sharper", &kept_json)
        .expect_err("native integration cannot publish private cached plan");
    assert_eq!(seam_error, "no trim plan: compute one first");
    let result = begin_trim_ghost_transaction_json(BODY, "sharper", &kept_json);
    let error = match result {
        Err(error) => error,
        Ok(Some(started)) => {
            let token = started["token"].as_str().expect("successful begin token");
            let ack = abort_history_transaction_json(token).expect("retire unexpected success");
            assert!(ack["ok"] == true && ack["rolledBack"] == true && ack["token"] == token);
            assert!(with_session(|s| s.clone()) == before);
            panic!(
                "fixture unexpectedly accepted overlapping promotion; actual abort acknowledged"
            );
        }
        Ok(None) => panic!("nonempty fixture unexpectedly produced no promotion"),
    };
    println!("refusal_kind={}", error.kind);
    assert_eq!(error.kind, "protection-refusal");
    assert_eq!(error.message, "no trim plan: compute one first");
    assert!(
        with_session(|s| s.clone()) == before,
        "refusal changes no session fields"
    );
    assert_eq!(
        with_session(|s| s.ghost_snapshot().reserved_ids),
        reservations
    );
    probe_checkpoint(BODY, "history-2");
}

#[test]
fn fixture_no_keeps() {
    fixture_case(false, false);
}
#[test]
fn fixture_first_keep() {
    fixture_case(true, false);
}
#[test]
fn fixture_no_keeps_manual_heading_ghost() {
    fixture_case(false, true);
}
#[test]
fn fixture_first_keep_manual_heading_ghost() {
    fixture_case(true, true);
}

#[test]
fn synthetic_strict_batch_nested_crossing_adjacent() {
    let body = "abcdefghij";
    for (label, ranges) in [
        ("nested", vec![(0, 10), (2, 4)]),
        ("crossing", vec![(0, 6), (4, 10)]),
    ] {
        with_session(|s| {
            s.open(body);
        });
        let before = with_session(|s| s.clone());
        let error = with_session(|s| s.ghost_ranges_atomic(body, &ranges))
            .expect_err("strict overlap refusal");
        assert_eq!(error, OVERLAP_ERROR);
        assert!(
            with_session(|s| s.clone()) == before,
            "strict batch refusal preserves exact session"
        );
        println!("synthetic={label} ranges={ranges:?} refusal=overlap");
    }
    with_session(|s| {
        s.open(body);
    });
    let before = with_session(|s| s.clone());
    let change = with_session(|s| s.ghost_ranges_atomic(body, &[(0, 2), (2, 4)]))
        .unwrap()
        .unwrap();
    assert_eq!(change.after.live.len(), 2);
    assert_ne!(change.after.live[0].id, change.after.live[1].id);
    assert_eq!(
        change
            .after
            .live
            .iter()
            .map(|g| (g.anchor.start, g.anchor.end))
            .collect::<Vec<_>>(),
        [(0, 2), (2, 4)]
    );
    let started = begin_history_replay_json(body, body, "[]").unwrap();
    let token = started["token"].as_str().unwrap();
    let ack = abort_history_transaction_json(token).unwrap();
    assert!(ack["ok"] == true && ack["rolledBack"] == true && ack["token"] == token);
    // This abort preserves the already-issued adjacent receipt, not a pretend
    // rollback of the standalone promotion.
    assert!(with_session(|s| s.ghost_snapshot()) == change.after);
    assert!(with_session(|s| s.clone()) != before);
    println!("synthetic=adjacent ranges=[(0,2),(2,4)] independent_ghosts=2 abort_ack=true");
}
