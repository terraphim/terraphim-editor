//! Atomic role publication for the browser graph coordinator; real KG engine.
#![cfg(not(target_arch = "wasm32"))]

use serde_json::{json, Value};
use terraphim_editor::{
    clear_thesaurus, commit_prepared_thesaurus, discard_prepared_thesaurus, load_thesaurus,
    prepare_thesaurus, role_snapshot_json,
};

const ROLE: &str = include_str!("fixtures/kg/thesaurus.json");

fn snapshot() -> Value {
    serde_json::from_str(&role_snapshot_json()).unwrap()
}

#[test]
fn preparation_is_read_only_and_publication_is_single_use() {
    clear_thesaurus();
    let before = snapshot();
    let token = prepare_thesaurus(ROLE).unwrap();
    assert_eq!(snapshot(), before);
    let summary = commit_prepared_thesaurus(&token).unwrap();
    assert_eq!(summary["concepts"], 7);
    let after = snapshot();
    assert_eq!(after["name"], "writing-fixture");
    assert_eq!(after["thesaurusText"], ROLE);
    assert_ne!(after["generation"], before["generation"]);
    assert!(after["generation"].is_string());
    assert!(commit_prepared_thesaurus(&token).is_err());
    assert_eq!(snapshot(), after);
}

#[test]
fn bad_load_discard_and_stale_preparation_keep_current_role() {
    load_thesaurus(ROLE).unwrap();
    let before = snapshot();
    assert!(load_thesaurus("{not json").is_err());
    assert_eq!(snapshot(), before);
    assert!(prepare_thesaurus("{not json").is_err());
    assert_eq!(snapshot(), before);
    let discarded = prepare_thesaurus(ROLE).unwrap();
    discard_prepared_thesaurus(&discarded);
    assert!(commit_prepared_thesaurus(&discarded).is_err());
    assert_eq!(snapshot(), before);

    let stale = prepare_thesaurus(ROLE).unwrap();
    load_thesaurus(ROLE).unwrap();
    let reloaded = snapshot();
    assert!(commit_prepared_thesaurus(&stale).is_err());
    assert_eq!(snapshot(), reloaded);
    let stale = prepare_thesaurus(ROLE).unwrap();
    clear_thesaurus();
    let cleared = snapshot();
    assert_eq!(cleared["thesaurusText"], json!(null));
    assert!(commit_prepared_thesaurus(&stale).is_err());
    assert_eq!(snapshot(), cleared);
}

#[test]
fn only_the_latest_preparation_can_commit() {
    clear_thesaurus();
    let old = prepare_thesaurus(ROLE).unwrap();
    let latest = prepare_thesaurus(ROLE).unwrap();
    discard_prepared_thesaurus(&old);
    assert!(commit_prepared_thesaurus(&old).is_err());
    assert!(commit_prepared_thesaurus(&latest).is_ok());
}
