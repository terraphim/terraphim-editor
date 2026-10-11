use serde_json::{Value, json};
use terraphim_rolegraph_browser::GraphSession;

fn send(s: &mut GraphSession, op: &str, payload: Value) -> Value {
    serde_json::from_str(&s.dispatch(&json!({"version":1,"requestId":"wire","generation":"0","expectedRevision":null,"op":op,"payload":payload}).to_string()).unwrap()).unwrap()
}
#[test]
fn protocol_and_decimal_identity_are_strict() {
    let mut s = GraphSession::default();
    for generation in [
        json!(0),
        json!("00"),
        json!("-1"),
        json!("18446744073709551616"),
    ] {
        let r = json!({"version":1,"requestId":"w","generation":generation,"expectedRevision":null,"op":"clear_role","payload":{"nextGeneration":"1"}});
        let response = s.dispatch(&r.to_string());
        assert!(
            response.is_err()
                || serde_json::from_str::<Value>(&response.unwrap()).unwrap()["ok"] == false
        );
    }
    let r = json!({"version":2,"requestId":"w","generation":"0","expectedRevision":null,"op":"stats","payload":{}});
    let v: Value = serde_json::from_str(&s.dispatch(&r.to_string()).unwrap()).unwrap();
    assert_eq!(v["error"]["code"], "unsupported_version");
    assert_eq!(
        send(&mut s, "clear_role", json!({"nextGeneration":"0"}))["ok"],
        true
    );
    let missing_revision =
        json!({"version":1,"requestId":"w","generation":"0","op":"stats","payload":{}});
    assert!(s.dispatch(&missing_revision.to_string()).is_err());
}
#[test]
fn unsafe_concept_pairing_and_conflicting_aliases_refuse() {
    for data in [
        json!({"x":{"id":u64::MAX,"nterm":"x"}}),
        json!({"x":{"id":1,"nterm":"x"},"y":{"id":1,"nterm":"y"}}),
        json!({"":{"id":1,"nterm":"x"}}),
    ] {
        let mut s = GraphSession::default();
        let r = send(
            &mut s,
            "prepare_role",
            json!({"thesaurusText":json!({"name":"bad","data":data}).to_string()}),
        );
        assert_eq!(r["ok"], false, "{r}");
        assert_eq!(r["generation"], "0");
    }
}
#[test]
fn schema_lists_the_complete_operation_allowlist() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../docs/contracts/schemas/rolegraph-browser.schema.json"
    ))
    .unwrap();
    let ops = schema["$defs"]["request"]["properties"]["op"]["enum"]
        .as_array()
        .unwrap();
    assert_eq!(ops.len(), 19);
    for op in [
        "prepare_role",
        "prepare_snapshot",
        "activate_role",
        "clear_role",
        "index_documents",
        "remove_document",
        "match",
        "connected",
        "query",
        "set_triggers",
        "document",
        "documents_for_term",
        "nodes",
        "edges",
        "index_learning",
        "learning_documents",
        "adjust_learning_rank",
        "stats",
        "export_snapshot",
    ] {
        assert!(ops.contains(&json!(op)));
    }
}
