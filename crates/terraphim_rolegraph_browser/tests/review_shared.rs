use serde_json::{Value, json};
use terraphim_rolegraph_browser::GraphSession;

#[test]
fn native_review_browser_scenario() {
    let steps: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/review_scenario.json")).unwrap();
    let mut session = GraphSession::default();
    let mut snapshot = Value::Null;
    let mut commands = 0;
    for step in steps {
        for i in 0..step["repeat"].as_u64().unwrap_or(1) {
            let mut p = step["payload"].clone();
            if p["documents"] == "$documents101" {
                p["documents"] = json!(
                    (0..101)
                        .map(|i| json!({"id":format!("d{i:03}"),"title":"","body":"alpha beta"}))
                        .collect::<Vec<_>>()
                );
            }
            if p["documents"] == "$escapedNine" {
                p["documents"]=json!((0..9).map(|i| json!({"id":format!("escaped{i}"),"title":"","body":"\\".repeat(1048576)})).collect::<Vec<_>>());
            }
            if p["document"]["id"] == "$learningId" {
                p["document"]["id"] = json!(format!("l{i:03}"));
            }
            if p["text"] == "$match101" {
                p["text"] = json!("alpha ".repeat(101));
            }
            if p["snapshotText"] == "$snapshot" {
                p["snapshotText"] = snapshot.clone();
            }
            let request = json!({"version":1,"requestId":commands.to_string(),"generation":step["generation"],"expectedRevision":null,"op":step["op"],"payload":p});
            let response: Value =
                serde_json::from_str(&session.dispatch(&request.to_string()).unwrap()).unwrap();
            for (pointer, expected) in step["expect"].as_object().unwrap() {
                assert_eq!(
                    response.pointer(pointer),
                    Some(expected),
                    "command {commands}: {response}"
                );
            }
            if let Some(length) = step.get("resultLength") {
                assert_eq!(
                    response["result"].as_array().unwrap().len() as u64,
                    length.as_u64().unwrap()
                );
            }
            if step["op"] == "export_snapshot" {
                snapshot = response["result"]["snapshotText"].clone();
            }
            commands += 1;
        }
    }
    assert_eq!(commands, 123);
}

#[test]
fn five_million_empty_tags_do_not_allocate_a_typed_vector() {
    let mut session = GraphSession::default();
    let prepare = json!({"version":1,"requestId":"p","generation":"0","expectedRevision":null,"op":"prepare_role","payload":{"thesaurusText":"{\"name\":\"test\",\"data\":{\"alpha\":{\"id\":1,\"nterm\":\"alpha\"}}}"}});
    session.dispatch(&prepare.to_string()).unwrap();
    session.dispatch(r#"{"version":1,"requestId":"a","generation":"0","expectedRevision":null,"op":"activate_role","payload":{"token":"prepared-1","nextGeneration":"1"}}"#).unwrap();
    // Construct raw JSON, NOT five million Value/String objects in the test host.
    let request = format!(
        r#"{{"version":1,"requestId":"tags","generation":"1","expectedRevision":null,"op":"index_learning","payload":{{"document":{{"id":"bad","rank":"1","nodes":["1"],"tags":[{}""]}}}}}}"#,
        "\"\",".repeat(4_999_999)
    );
    let result: Value = serde_json::from_str(&session.dispatch(&request).unwrap()).unwrap();
    assert_eq!(result["error"]["code"], "resource_limit");
    assert_eq!(result["revision"], "0");
}
