//! Browser qualification of the authoritative role preparation API (real WASM).
#![cfg(target_arch = "wasm32")]

use terraphim_editor::{
    kg_clear_thesaurus, kg_commit_prepared_role, kg_discard_prepared_role, kg_load_thesaurus,
    kg_prepare_role, kg_role_snapshot_json, kg_set_role_change_listener,
};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

const ROLE: &str = include_str!("fixtures/kg/thesaurus.json");

#[wasm_bindgen_test]
fn observer_sees_published_state_without_borrow_and_only_success_notifies() {
    use std::{cell::RefCell, rc::Rc};
    use wasm_bindgen::{closure::Closure, JsCast};
    kg_set_role_change_listener(None);
    kg_clear_thesaurus();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let captured = seen.clone();
    let callback = Closure::<dyn FnMut()>::new(move || {
        captured.borrow_mut().push(kg_role_snapshot_json());
    });
    kg_set_role_change_listener(Some(
        callback
            .as_ref()
            .unchecked_ref::<js_sys::Function>()
            .clone(),
    ));
    kg_load_thesaurus(ROLE).unwrap();
    assert_eq!(seen.borrow().last().unwrap(), &kg_role_snapshot_json());
    let stale = kg_prepare_role(ROLE).unwrap();
    assert!(kg_load_thesaurus("invalid").is_err());
    assert!(kg_prepare_role("invalid").is_err());
    assert_eq!(seen.borrow().len(), 1);
    kg_clear_thesaurus();
    assert!(kg_commit_prepared_role(&stale).is_err());
    assert_eq!(seen.borrow().len(), 2);
    let token = kg_prepare_role(ROLE).unwrap();
    kg_commit_prepared_role(&token).unwrap();
    assert!(kg_commit_prepared_role(&token).is_err());
    assert_eq!(seen.borrow().len(), 3);
    assert_eq!(seen.borrow().last().unwrap(), &kg_role_snapshot_json());
    kg_set_role_change_listener(None);
    kg_clear_thesaurus();
    assert_eq!(seen.borrow().len(), 3);
}

#[wasm_bindgen_test]
fn throwing_observer_cannot_undo_publication_or_escape_successful_exports() {
    kg_set_role_change_listener(Some(js_sys::Function::new_no_args(
        "throw new Error('observer failed')",
    )));
    assert!(kg_load_thesaurus(ROLE).is_ok());
    let token = kg_prepare_role(ROLE).unwrap();
    assert!(kg_commit_prepared_role(&token).is_ok());
    kg_clear_thesaurus();
    let snapshot: serde_json::Value = serde_json::from_str(&kg_role_snapshot_json()).unwrap();
    assert!(snapshot["thesaurusText"].is_null());
    kg_set_role_change_listener(None);
}

#[wasm_bindgen_test]
fn preparation_does_not_publish_and_commit_is_single_use() {
    kg_clear_thesaurus();
    let before = kg_role_snapshot_json();
    let token = kg_prepare_role(ROLE).unwrap();
    assert_eq!(kg_role_snapshot_json(), before);
    let committed = kg_commit_prepared_role(&token).unwrap();
    let summary: serde_json::Value = serde_json::from_str(&committed).unwrap();
    assert_eq!(summary["concepts"], 7);
    assert!(summary["generation"].is_string());
    let after = kg_role_snapshot_json();
    assert_ne!(after, before);
    assert!(kg_commit_prepared_role(&token).is_err());
    assert_eq!(kg_role_snapshot_json(), after);
}

#[wasm_bindgen_test]
fn clear_reload_discard_and_invalid_role_cannot_publish_stale_state() {
    kg_load_thesaurus(ROLE).unwrap();
    let before = kg_role_snapshot_json();
    assert!(kg_prepare_role("not json").is_err());
    assert_eq!(kg_role_snapshot_json(), before);
    let old = kg_prepare_role(ROLE).unwrap();
    let latest = kg_prepare_role(ROLE).unwrap();
    kg_discard_prepared_role(&old);
    assert!(kg_commit_prepared_role(&old).is_err());
    assert!(kg_commit_prepared_role(&latest).is_ok());
    let stale = kg_prepare_role(ROLE).unwrap();
    kg_clear_thesaurus();
    let cleared = kg_role_snapshot_json();
    assert!(kg_commit_prepared_role(&stale).is_err());
    assert_eq!(kg_role_snapshot_json(), cleared);
    let stale = kg_prepare_role(ROLE).unwrap();
    kg_load_thesaurus(ROLE).unwrap();
    let reloaded = kg_role_snapshot_json();
    assert!(kg_commit_prepared_role(&stale).is_err());
    assert_eq!(kg_role_snapshot_json(), reloaded);
}
