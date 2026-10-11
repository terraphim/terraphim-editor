use serde_json::{Value, json};
use terraphim_rolegraph_browser::GraphSession;

#[test]
fn native_shared_browser_scenario() {
    let steps: Vec<Value> = serde_json::from_str(include_str!("fixtures/scenario.json")).unwrap();
    let mut session = GraphSession::default();
    let mut snapshot = Value::Null;
    for (i, step) in steps.into_iter().enumerate() {
        let mut payload = step["payload"].clone();
        if payload["snapshotText"] == "$snapshot" {
            payload["snapshotText"] = snapshot.clone();
        }
        if payload["snapshotText"] == "$corruptSnapshot" {
            let mut wrapper: Value = serde_json::from_str(snapshot.as_str().unwrap()).unwrap();
            wrapper["graphJson"] = json!(wrapper["graphJson"].as_str().unwrap().replacen(
                "\"rank\":1,",
                "\"rank\":999,",
                1
            ));
            payload["snapshotText"] = json!(wrapper.to_string());
        }
        let request = json!({"version":1,"requestId":i.to_string(),"generation":step["generation"],"expectedRevision":step["expectedRevision"],"op":step["op"],"payload":payload});
        let response: Value =
            serde_json::from_str(&session.dispatch(&request.to_string()).unwrap()).unwrap();
        if std::env::var_os("ROLEGRAPH_WIRE_EVIDENCE").is_some() {
            println!("ROLEGRAPH_WIRE {}", response);
        }
        for (pointer, expected) in step["expect"].as_object().unwrap() {
            assert_eq!(
                response.pointer(pointer),
                Some(expected),
                "step {i} {}: {response}",
                step["op"]
            );
        }
        if step["op"] == "export_snapshot" {
            snapshot = response["result"]["snapshotText"].clone();
        }
    }
}
