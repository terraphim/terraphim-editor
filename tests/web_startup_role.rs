//! Real WASM startup branch checks; no JavaScript initialization doubles.
#![cfg(target_arch = "wasm32")]

use terraphim_editor::{kg_clear_thesaurus, kg_load_thesaurus, kg_role_snapshot_json, start};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn embed(value: bool) {
    js_sys::Reflect::set(
        &js_sys::global(),
        &JsValue::from_str("TE_EMBED"),
        &JsValue::from_bool(value),
    )
    .unwrap();
}

#[wasm_bindgen_test]
fn standalone_loads_shipped_role_but_embedding_and_absent_app_preserve_host_state() {
    let document = web_sys::window().unwrap().document().unwrap();
    assert!(document.get_element_by_id("app").is_none());
    embed(false);
    kg_clear_thesaurus();
    let empty = kg_role_snapshot_json();
    start().unwrap();
    assert_eq!(kg_role_snapshot_json(), empty);

    let app = document.create_element("div").unwrap();
    app.set_id("app");
    app.set_inner_html("<p>Host-selected content</p>");
    document.body().unwrap().append_child(&app).unwrap();
    embed(true);
    kg_load_thesaurus(include_str!("fixtures/kg/thesaurus.json")).unwrap();
    let host_role = kg_role_snapshot_json();
    start().unwrap();
    assert_eq!(kg_role_snapshot_json(), host_role);
    assert_eq!(app.inner_html(), "<p>Host-selected content</p>");

    embed(false);
    kg_clear_thesaurus();
    start().unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&kg_role_snapshot_json()).unwrap();
    assert_eq!(
        snapshot["thesaurusText"].as_str(),
        Some(include_str!("../public/roles/default.json"))
    );
    assert!(app.inner_html().contains("contenteditable"));
    app.remove();
    kg_clear_thesaurus();
}
