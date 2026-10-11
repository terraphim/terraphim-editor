use serde_json::{Value, json};
use terraphim_rolegraph_browser::GraphSession;

fn call(s: &mut GraphSession, op: &str, payload: Value) -> Value {
    serde_json::from_str(&s.dispatch(&json!({"version":1,"requestId":"review","generation":"1","expectedRevision":null,"op":op,"payload":payload}).to_string()).unwrap()).unwrap()
}
fn role() -> GraphSession {
    let mut s = GraphSession::default();
    let p:Value=serde_json::from_str(&s.dispatch(&json!({"version":1,"requestId":"prepare","generation":"0","expectedRevision":null,"op":"prepare_role","payload":{"thesaurusText":"{\"name\":\"test\",\"data\":{\"alpha\":{\"id\":1,\"nterm\":\"alpha\"},\"beta\":{\"id\":2,\"nterm\":\"beta\"}}}"}}).to_string()).unwrap()).unwrap();
    assert_eq!(p["ok"], true);
    let r=s.dispatch(&json!({"version":1,"requestId":"activate","generation":"0","expectedRevision":null,"op":"activate_role","payload":{"token":p["result"]["token"],"nextGeneration":"1"}}).to_string()).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&r).unwrap()["ok"], true);
    s
}
fn snapshot(s: &mut GraphSession) -> String {
    let r = call(s, "export_snapshot", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    r["result"]["snapshotText"].as_str().unwrap().to_owned()
}

#[test]
fn escaped_corpus_refuses_atomically_and_near_limit_roundtrips() {
    let mut s = role();
    let before = snapshot(&mut s);
    let docs: Vec<_> = (0..9)
        .map(|i| json!({"id":format!("d{i}"),"title":"","body":"\\".repeat(1048576)}))
        .collect();
    let r = call(&mut s, "index_documents", json!({"documents":docs}));
    assert_eq!(r["error"]["code"], "resource_limit", "{r}");
    assert_eq!(r["revision"], "0");
    assert_eq!(snapshot(&mut s), before);
    let docs: Vec<_> = (0..7)
        .map(|i| json!({"id":format!("d{i}"),"title":"","body":"\\".repeat(1048576)}))
        .collect();
    assert_eq!(
        call(&mut s, "index_documents", json!({"documents":docs}))["ok"],
        true
    );
    let near = snapshot(&mut s);
    assert!(near.len() > 14 * 1024 * 1024 && near.len() <= 16 * 1024 * 1024);
    assert_eq!(
        call(&mut s, "prepare_snapshot", json!({"snapshotText":near}))["ok"],
        true
    );
}

#[test]
fn combined_corpus_trigger_and_learning_sizes_remain_checkpointable() {
    let mut s = role();
    let docs: Vec<_> = (0..7)
        .map(|i| json!({"id":format!("d{i}"),"title":"","body":"\\".repeat(1048576)}))
        .collect();
    assert_eq!(
        call(&mut s, "index_documents", json!({"documents":docs}))["ok"],
        true
    );
    let before = snapshot(&mut s);
    let r = call(
        &mut s,
        "set_triggers",
        json!({"descriptions":{"1":"longword ".repeat(300000)},"pinnedNodeIds":[],"threshold":0.3}),
    );
    assert_eq!(r["error"]["code"], "resource_limit", "{r}");
    assert_eq!(r["revision"], "1");
    assert_eq!(snapshot(&mut s), before);
    let r = call(
        &mut s,
        "index_learning",
        json!({"document":{"id":"learn","rank":"1","nodes":["1"],"tags":["\\".repeat(1048576)]}}),
    );
    assert_eq!(r["error"]["code"], "resource_limit", "{r}");
    assert_eq!(r["revision"], "1");
    assert_eq!(snapshot(&mut s), before);
}

#[test]
fn all_collections_paginate_beyond_one_hundred() {
    let mut s = role();
    let docs: Vec<_> = (0..101)
        .map(|i| json!({"id":format!("d{i:03}"),"title":"","body":"alpha beta"}))
        .collect();
    assert_eq!(
        call(&mut s, "index_documents", json!({"documents":docs}))["ok"],
        true
    );
    for i in 0..101 {
        assert_eq!(
            call(
                &mut s,
                "index_learning",
                json!({"document":{"id":format!("l{i:03}"),"rank":"1","nodes":["1"]}})
            )["ok"],
            true
        );
    }
    for op in ["documents_for_term", "learning_documents", "match"] {
        let text = if op == "match" {
            "alpha ".repeat(101)
        } else {
            "alpha".to_owned()
        };
        let first = call(&mut s, op, json!({"text":text}));
        assert_eq!(first["ok"], true, "{first}");
        assert_eq!(first["result"].as_array().unwrap().len(), 100);
        let next = call(&mut s, op, json!({"text":text,"offset":100,"limit":100}));
        assert_eq!(next["ok"], true, "{next}");
        assert_eq!(next["result"].as_array().unwrap().len(), 1);
    }
}

#[test]
fn platform_independent_offset_boundaries() {
    let mut s = role();
    for op in [
        "nodes",
        "edges",
        "query",
        "match",
        "documents_for_term",
        "learning_documents",
    ] {
        let mut p = if op == "query" {
            json!({"text":"alpha","mode":"graph"})
        } else if ["nodes", "edges"].contains(&op) {
            json!({})
        } else {
            json!({"text":"alpha"})
        };
        p["offset"] = json!(4294967295u64);
        assert_eq!(call(&mut s, op, p.clone())["ok"], true);
        p["offset"] = json!(4294967296u64);
        let r = call(&mut s, op, p);
        assert_eq!(r["error"]["code"], "invalid_input", "{op}: {r}");
    }
}

#[test]
fn empty_tags_are_bounded_before_typed_allocation() {
    let mut s = role();
    let before = snapshot(&mut s);
    let r = call(
        &mut s,
        "index_learning",
        json!({"document":{"id":"bad","rank":"1","nodes":["1"],"tags":vec!["";1001]}}),
    );
    assert_eq!(r["error"]["code"], "resource_limit", "{r}");
    assert_eq!(snapshot(&mut s), before);
}

#[test]
fn learning_edge_metadata_is_preserved_without_graph_contributions() {
    let mut s = role();
    call(
        &mut s,
        "index_documents",
        json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]}),
    );
    let edge = call(&mut s, "edges", json!({}))["result"][0].clone();
    let graph_edges = call(&mut s, "edges", json!({}))["result"].clone();
    let r = call(
        &mut s,
        "index_learning",
        json!({"document":{"id":"learn","rank":"9007199254740993","nodes":["1"],"matched_edges":[edge.clone()]}}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(call(&mut s, "edges", json!({}))["result"], graph_edges);
    assert_eq!(
        call(&mut s, "document", json!({"id":"learn"}))["result"]["matched_edges"],
        json!([edge.clone()])
    );
    call(&mut s, "remove_document", json!({"id":"d"}));
    // Historical metadata remains independently valid after the corpus changes.
    let text = snapshot(&mut s);
    let p = call(&mut s, "prepare_snapshot", json!({"snapshotText":text}));
    assert_eq!(p["ok"], true, "{p}");
    assert_eq!(
        call(
            &mut s,
            "activate_role",
            json!({"token":p["result"]["token"],"nextGeneration":"1"})
        )["ok"],
        true
    );
    assert_eq!(
        call(&mut s, "learning_documents", json!({"text":"alpha"}))["result"][0]["matched_edges"],
        json!([edge])
    );
    let bad = json!({"id":"999","rank":"1","doc_hash":{"d":"1"}});
    assert_eq!(
        call(
            &mut s,
            "index_learning",
            json!({"document":{"id":"bad","rank":"1","nodes":["1"],"matched_edges":[bad]}})
        )["ok"],
        false
    );
}

#[test]
fn cumulative_tag_budget_and_dictionary_names_roundtrip() {
    let mut s = role();
    for i in 0..10 {
        let id = if i == 0 {
            "tags".to_owned()
        } else {
            format!("l{i}")
        };
        assert_eq!(
            call(
                &mut s,
                "index_learning",
                json!({"document":{"id":id,"rank":"1","nodes":["1"],"tags":vec!["";1000]}})
            )["ok"],
            true
        );
    }
    let text = snapshot(&mut s);
    assert_eq!(
        call(
            &mut s,
            "prepare_snapshot",
            json!({"snapshotText":text.clone()})
        )["ok"],
        true
    );
    let r = call(
        &mut s,
        "index_learning",
        json!({"document":{"id":"overflow","rank":"1","nodes":["1"],"tags":[""]}}),
    );
    assert_eq!(r["error"]["code"], "resource_limit");
    assert_eq!(r["revision"], "10");
    assert_eq!(snapshot(&mut s), text);
}
