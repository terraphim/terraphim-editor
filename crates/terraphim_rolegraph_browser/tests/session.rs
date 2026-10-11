use serde_json::{Value, json};
use terraphim_rolegraph_browser::GraphSession;

fn call(
    s: &mut GraphSession,
    generation: &str,
    revision: Option<&str>,
    op: &str,
    payload: Value,
) -> Value {
    serde_json::from_str(&s.dispatch(&json!({"version":1,"requestId":"test","generation":generation,"expectedRevision":revision,"op":op,"payload":payload}).to_string()).unwrap()).unwrap()
}
fn role(s: &mut GraphSession, id: u64) {
    let text = json!({"name":"fixture","data":{"alpha":{"id":id,"nterm":"alpha"},"beta":{"id":2,"nterm":"beta"}}}).to_string();
    let p = call(s, "0", None, "prepare_role", json!({"thesaurusText":text}));
    assert_eq!(p["ok"], true, "{p}");
    let a = call(
        s,
        "0",
        None,
        "activate_role",
        json!({"token":p["result"]["token"],"nextGeneration":"1"}),
    );
    assert_eq!(a["ok"], true, "{a}");
}
#[test]
fn lifecycle_and_atomic_guards() {
    let mut s = GraphSession::default();
    assert_eq!(
        call(&mut s, "0", None, "stats", json!({}))["error"]["code"],
        "role_required"
    );
    role(&mut s, 1);
    assert_eq!(
        call(&mut s, "0", None, "stats", json!({}))["error"]["code"],
        "stale_generation"
    );
    let docs = json!({"documents":[{"id":"d","title":"","body":"alpha beta alpha beta"}]});
    assert_eq!(
        call(&mut s, "1", Some("0"), "index_documents", docs.clone())["revision"],
        "1"
    );
    assert_eq!(
        call(&mut s, "1", Some("0"), "index_documents", docs.clone())["error"]["code"],
        "stale_revision"
    );
    let before = call(&mut s, "1", None, "nodes", json!({}))["result"].clone();
    assert_eq!(call(&mut s, "1", None, "index_documents", docs)["ok"], true);
    assert_eq!(
        call(&mut s, "1", None, "nodes", json!({}))["result"],
        before
    );
    assert_eq!(
        call(&mut s, "1", None, "remove_document", json!({"id":"d"}))["ok"],
        true
    );
    assert_eq!(
        call(&mut s, "1", None, "stats", json!({}))["result"]["node_count"],
        0
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "clear_role",
            json!({"nextGeneration":"2"})
        )["generation"],
        "2"
    );
}
#[test]
fn all_read_query_trigger_and_learning_operations() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "index_documents",
            json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]})
        )["ok"],
        true
    );
    for (op, payload) in [
        ("match", json!({"text":"alpha beta"})),
        ("connected", json!({"text":"alpha beta"})),
        ("documents_for_term", json!({"text":"alpha"})),
        ("document", json!({"id":"d"})),
        ("nodes", json!({"limit":1})),
        ("edges", json!({})),
        ("stats", json!({})),
    ] {
        assert_eq!(call(&mut s, "1", None, op, payload)["ok"], true, "{op}");
    }
    for mode in ["graph", "fallback", "operators"] {
        let r = call(
            &mut s,
            "1",
            None,
            "query",
            json!({"text":"alpha","mode":mode,"operator":"and","terms":["alpha","beta"]}),
        );
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["result"][0]["id"], "d");
    }
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "set_triggers",
            json!({"descriptions":{"1":"special mechanism"},"pinnedNodeIds":["2"],"threshold":0.3})
        )["ok"],
        true
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "query",
            json!({"text":"special mechanism","mode":"fallback"})
        )["result"][0]["id"],
        "d"
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "index_learning",
            json!({"document":{"id":"learn","rank":"9007199254740993","nodes":["1"],"tags":["alpha"]}})
        )["ok"],
        true
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "adjust_learning_rank",
            json!({"id":"learn","increase":true,"adjustment":"2"})
        )["ok"],
        true
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "learning_documents",
            json!({"text":"alpha"})
        )["result"][0]["rank"],
        "9007199254740995"
    );
}
#[test]
fn snapshot_roundtrip_exact_large_edges_and_corruption() {
    let mut s = GraphSession::default();
    role(&mut s, 100_000_000);
    call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]}),
    );
    let edge = call(&mut s, "1", None, "edges", json!({}))["result"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(edge.parse::<u64>().unwrap() > 9_007_199_254_740_991);
    let snapshot = call(&mut s, "1", None, "export_snapshot", json!({}))["result"]["snapshotText"]
        .as_str()
        .unwrap()
        .to_owned();
    let p = call(
        &mut s,
        "1",
        None,
        "prepare_snapshot",
        json!({"snapshotText":snapshot}),
    );
    assert_eq!(p["ok"], true, "{p}");
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "activate_role",
            json!({"token":p["result"]["token"],"nextGeneration":"2"})
        )["revision"],
        "0"
    );
    assert_eq!(
        call(&mut s, "2", None, "edges", json!({}))["result"][0]["id"],
        edge
    );
    let mut wrapper: Value = serde_json::from_str(&snapshot).unwrap();
    let mut graph: Value = serde_json::from_str(wrapper["graphJson"].as_str().unwrap()).unwrap();
    graph["nodes"]["100000000"]["rank"] = json!(999);
    wrapper["graphJson"] = json!(graph.to_string());
    assert_eq!(
        call(
            &mut s,
            "2",
            None,
            "prepare_snapshot",
            json!({"snapshotText":wrapper.to_string()})
        )["error"]["code"],
        "invalid_snapshot"
    );
    assert_eq!(
        call(&mut s, "2", None, "edges", json!({}))["result"][0]["id"],
        edge
    );
}
#[test]
fn malformed_limits_and_unknown_ops_refuse_without_mutation() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "query",
            json!({"text":"x".repeat(4097),"mode":"graph"})
        )["error"]["code"],
        "resource_limit"
    );
    assert_eq!(
        call(&mut s, "1", None, "nodes", json!({"limit":101}))["error"]["code"],
        "resource_limit"
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "index_learning",
            json!({"document":{"id":"bad","rank":5,"nodes":["1"]}})
        )["ok"],
        false
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "index_documents",
            json!({"documents":[{"id":"bad","title":"","body":"x".repeat(1048577)}]})
        )["error"]["code"],
        "resource_limit"
    );
    assert_eq!(call(&mut s, "1", None, "bogus", json!({}))["ok"], false);
    assert_eq!(call(&mut s, "1", None, "stats", json!({}))["revision"], "0");
    assert!(s.dispatch("not json").is_err());
}

#[test]
fn stale_preparations_and_bad_roles_keep_active_graph() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    let exported =
        call(&mut s, "1", None, "export_snapshot", json!({}))["result"]["snapshotText"].clone();
    let p = call(
        &mut s,
        "1",
        None,
        "prepare_snapshot",
        json!({"snapshotText":exported}),
    );
    call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]}),
    );
    let a = call(
        &mut s,
        "1",
        None,
        "activate_role",
        json!({"token":p["result"]["token"],"nextGeneration":"2"}),
    );
    assert_eq!(a["error"]["code"], "stale_revision");
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "prepare_role",
            json!({"thesaurusText":"{}"})
        )["ok"],
        false
    );
    assert_eq!(
        call(&mut s, "1", None, "document", json!({"id":"d"}))["ok"],
        true
    );
    assert_eq!(
        call(&mut s, "1", Some("0"), "stats", json!({}))["error"]["code"],
        "stale_revision"
    );
}

#[test]
fn learning_replacement_overflow_and_source_unavailability() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]}),
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "index_learning",
            json!({"document":{"id":"d","rank":u64::MAX.to_string(),"nodes":["1"]}})
        )["ok"],
        true
    );
    let before = call(&mut s, "1", None, "stats", json!({}))["revision"].clone();
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "adjust_learning_rank",
            json!({"id":"d","increase":true,"adjustment":"1"})
        )["ok"],
        false
    );
    assert_eq!(
        call(&mut s, "1", None, "document", json!({"id":"d"}))["result"]["rank"],
        u64::MAX.to_string()
    );
    assert_eq!(
        call(&mut s, "1", None, "stats", json!({}))["revision"],
        before
    );
    call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]}),
    );
    assert_eq!(
        call(
            &mut s,
            "1",
            None,
            "learning_documents",
            json!({"text":"alpha"})
        )["result"],
        json!([])
    );
    let mut wrapper: Value = serde_json::from_str(
        call(&mut s, "1", None, "export_snapshot", json!({}))["result"]["snapshotText"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    wrapper["documents"] = json!([]);
    let p = call(
        &mut s,
        "1",
        None,
        "prepare_snapshot",
        json!({"snapshotText":wrapper.to_string()}),
    );
    assert_eq!(p["ok"], true, "{p}");
    call(
        &mut s,
        "1",
        None,
        "activate_role",
        json!({"token":p["result"]["token"],"nextGeneration":"2"}),
    );
    assert_eq!(
        call(&mut s, "2", None, "document", json!({"id":"d"}))["result"]["sourceAvailable"],
        false
    );
}

#[test]
fn unicode_empty_single_and_self_edges_survive_roundtrip() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    let r = call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"empty","title":"","body":""},{"id":"single","title":"","body":"alpha"},{"id":"unicode","title":"","body":"🦀 中文 alpha alpha beta café"}]}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        call(&mut s, "1", None, "stats", json!({}))["result"]["document_count"],
        3
    );
    assert_eq!(
        call(&mut s, "1", None, "export_snapshot", json!({}))["ok"],
        true
    );
}

#[test]
fn edge_growth_limit_is_atomic_even_for_a_valid_sized_document() {
    let mut s = GraphSession::default();
    let mut data = serde_json::Map::new();
    for id in 0..320 {
        data.insert(format!("n{id}"), json!({"id":id,"nterm":format!("n{id}")}));
    }
    let p = call(
        &mut s,
        "0",
        None,
        "prepare_role",
        json!({"thesaurusText":json!({"name":"limits","data":data}).to_string()}),
    );
    assert_eq!(p["ok"], true);
    call(
        &mut s,
        "0",
        None,
        "activate_role",
        json!({"token":p["result"]["token"],"nextGeneration":"1"}),
    );
    let mut body = String::new();
    for a in 0..320 {
        for b in 0..320 {
            body.push_str(&format!("n{a} n{b}\n"));
        }
    }
    assert!(body.len() < 1048576);
    let r = call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"dense","title":"","body":body}]}),
    );
    assert_eq!(r["error"]["code"], "resource_limit", "{r}");
    let stats = call(&mut s, "1", None, "stats", json!({}));
    assert_eq!(stats["revision"], "0");
    assert_eq!(stats["result"]["document_count"], 0);
}

#[test]
fn corrupt_snapshot_references_membership_and_edge_rank_are_refused() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    call(
        &mut s,
        "1",
        None,
        "index_documents",
        json!({"documents":[{"id":"d","title":"","body":"alpha beta"}]}),
    );
    let wrapper: Value = serde_json::from_str(
        call(&mut s, "1", None, "export_snapshot", json!({}))["result"]["snapshotText"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for kind in 0..5 {
        let mut w = wrapper.clone();
        let mut g: Value = serde_json::from_str(w["graphJson"].as_str().unwrap()).unwrap();
        match kind {
            0 => g["edges"]["5"]["rank"] = json!(2),
            1 => g["edges"]["5"]["doc_hash"] = json!({"missing":1}),
            2 => g["learning_document_ids"] = json!(["missing"]),
            3 => g["nodes"]["1"]["connected_with"] = json!(["999"]),
            _ => g["aho_corasick_values"] = json!([1]),
        }
        w["graphJson"] = json!(g.to_string());
        let r = call(
            &mut s,
            "1",
            None,
            "prepare_snapshot",
            json!({"snapshotText":w.to_string()}),
        );
        assert_eq!(r["error"]["code"], "invalid_snapshot", "kind={kind} {r}");
        assert_eq!(r["revision"], "1");
    }
}

#[test]
fn learning_metadata_has_a_cumulative_limit() {
    let mut s = GraphSession::default();
    role(&mut s, 1);
    let tag = "x".repeat(1048576);
    let mut accepted = 0;
    for i in 0..17 {
        let r = call(
            &mut s,
            "1",
            None,
            "index_learning",
            json!({"document":{"id":format!("learn-{i}"),"rank":"1","nodes":["1"],"tags":[tag]}}),
        );
        if r["ok"] == true {
            accepted += 1;
        } else {
            assert_eq!(r["error"]["code"], "resource_limit", "{r}");
            assert_eq!(r["revision"], accepted.to_string());
            break;
        }
    }
    assert!(accepted > 0 && accepted < 17);
    assert_eq!(
        call(&mut s, "1", None, "stats", json!({}))["result"]["document_count"],
        accepted
    );
}
