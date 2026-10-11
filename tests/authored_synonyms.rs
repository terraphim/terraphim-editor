//! Real document commands -> typed synonym registration -> actual RoleGraph.
use serde_json::{json, Value};
use terraphim_alternatives::{Source, SpanKind};
use terraphim_editor::{
    host::{source_hash, HostSession},
    DocumentSession, Suggestion,
};
use terraphim_rolegraph_browser::GraphSession;

fn call(graph: &mut GraphSession, generation: &str, op: &str, payload: Value) -> Value {
    let reply: Value = serde_json::from_str(&graph.dispatch(&json!({"version":1,"requestId":"actual-synonyms","generation":generation,"expectedRevision":null,"op":op,"payload":payload}).to_string()).unwrap()).unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    reply["result"].clone()
}
fn graph_for(group: &Value) -> GraphSession {
    let mut graph = GraphSession::default();
    let p = call(
        &mut graph,
        "0",
        "prepare_role",
        json!({"thesaurusText":group["thesaurus"].to_string()}),
    );
    call(
        &mut graph,
        "0",
        "activate_role",
        json!({"token":p["token"],"nextGeneration":"1"}),
    );
    call(
        &mut graph,
        "1",
        "index_documents",
        json!({"documents":[{"id":"authored","title":"","body":"Decision matters."}]}),
    );
    graph
}
fn request(s: &HostSession, id: u64, command: &str, args: Value) -> String {
    let current = s.snapshot();
    json!({"protocol":1,"resource":current["resource"],"session":current["session"],"revision":current["revision"],"sourceHash":current["sourceHash"],"id":id,"command":command,"args":args}).to_string()
}
#[test]
fn newly_created_human_and_ai_alternatives_query_actual_rolegraph_and_checkpoint() {
    let mut session = DocumentSession::new();
    session.open("Decision matters.");
    let id = session
        .alt_create_span(SpanKind::Word, 0, 8, "Choice", Source::Human)
        .unwrap()
        .span;
    session
        .append_suggestions(
            SpanKind::Word,
            0,
            8,
            &[Suggestion::ai("Judgment", "local-model")],
        )
        .unwrap();
    let group = session.synonym_groups_json()[0].clone();
    assert_eq!(group["alternatives"][2]["model"], "local-model");
    assert!(group["markdown"]
        .as_str()
        .unwrap()
        .contains("synonyms:: Decision, Choice, Judgment"));
    let mut graph = graph_for(&group);
    let original = call(&mut graph, "1", "match", json!({"text":"Decision"}));
    let authored = call(&mut graph, "1", "match", json!({"text":"Choice"}));
    let ai = call(&mut graph, "1", "match", json!({"text":"Judgment"}));
    assert_eq!(original, json!(["1"]));
    assert_eq!(authored, original);
    assert_eq!(ai, original);
    let checkpoint = call(&mut graph, "1", "export_snapshot", json!({}));
    let mut restored = GraphSession::default();
    let p = call(&mut restored, "0", "prepare_snapshot", checkpoint);
    call(
        &mut restored,
        "0",
        "activate_role",
        json!({"token":p["token"],"nextGeneration":"2"}),
    );
    assert_eq!(
        call(&mut restored, "2", "match", json!({"text":"Judgment"})),
        original
    );
    let saved = session.save();
    let mut reopened = DocumentSession::new();
    reopened.open(&saved);
    assert_eq!(
        reopened.synonym_groups_json(),
        session.synonym_groups_json()
    );
    assert_eq!(
        reopened.synonyms_for(&id, "Choice"),
        ["decision", "choice", "judgment"]
    );
    let mut reopened_graph = graph_for(&reopened.synonym_groups_json()[0]);
    assert_eq!(
        call(&mut reopened_graph, "1", "match", json!({"text":"Choice"})),
        original
    );
}
#[test]
fn host_ack_save_reopen_undo_redo_and_removal_track_exact_membership() {
    let input = "Decision matters.";
    let mut host = HostSession::new("file:///decision.md", "view", 1, input).unwrap();
    let p = host
        .propose(&request(
            &host,
            1,
            "alt_create",
            json!({"kind":"word","start":0,"end":8,"text":"Choice"}),
        ))
        .unwrap();
    let id = p["projection"]["annotations"]["spans"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        host.synonyms_for(&id, "Choice").is_empty(),
        "staging cannot register committed synonyms"
    );
    assert!(host.acknowledge(1, 2, "wrong").is_err());
    assert!(host.synonyms_for(&id, "Choice").is_empty());
    let output = p["source"].as_str().unwrap();
    host.acknowledge(1, 2, output).unwrap();
    assert_eq!(host.synonyms_for(&id, "Choice"), ["decision", "choice"]);
    let reopened = HostSession::new("file:///decision.md", "reopened", 1, output).unwrap();
    assert_eq!(
        reopened.synonyms_for(&id, "Choice"),
        host.synonyms_for(&id, "Choice")
    );
    host.synchronize(3, input).unwrap();
    assert!(host.synonyms_for(&id, "Choice").is_empty());
    host.synchronize(4, output).unwrap();
    assert_eq!(host.synonyms_for(&id, "Choice"), ["decision", "choice"]);
    let p = host
        .propose(&request(
            &host,
            2,
            "alt_edit",
            json!({"span":id,"index":1,"text":"Option"}),
        ))
        .unwrap();
    host.acknowledge(2, 5, p["source"].as_str().unwrap())
        .unwrap();
    assert!(host.synonyms_for(&id, "Choice").is_empty());
    assert_eq!(host.synonyms_for(&id, "Option"), ["decision", "option"]);
    host.synchronize(6, output).unwrap();
    assert_eq!(host.synonyms_for(&id, "Choice"), ["decision", "choice"]);
    assert!(host.synonyms_for(&id, "Option").is_empty());
    let p = host
        .propose(&request(
            &host,
            3,
            "alt_remove",
            json!({"span":id,"index":1}),
        ))
        .unwrap();
    host.acknowledge(3, 7, p["source"].as_str().unwrap())
        .unwrap();
    assert!(host.synonyms_for(&id, "Choice").is_empty());
    assert_eq!(host.snapshot()["sourceHash"], source_hash(input));
}
#[test]
fn rejection_external_edits_and_other_documents_cannot_leak_synonyms() {
    let mut a = HostSession::new("file:///a.md", "a", 1, "Bank").unwrap();
    let b = HostSession::new("file:///b.md", "b", 1, "Bank").unwrap();
    let p = a
        .propose(&request(
            &a,
            1,
            "alt_create",
            json!({"kind":"word","start":0,"end":4,"text":"Lender"}),
        ))
        .unwrap();
    let id = p["projection"]["annotations"]["spans"][0]["id"]
        .as_str()
        .unwrap();
    a.reject(1).unwrap();
    assert!(a.synonyms_for(id, "Lender").is_empty());
    assert!(b.synonyms_for(id, "Lender").is_empty());
    let p = a
        .propose(&request(
            &a,
            2,
            "alt_create",
            json!({"kind":"word","start":0,"end":4,"text":"Shore"}),
        ))
        .unwrap();
    a.synchronize(2, "External source").unwrap();
    assert!(a.acknowledge(2, 3, p["source"].as_str().unwrap()).is_err());
    assert!(a.snapshot()["projection"]["annotations"]["synonymGroups"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn detached_groups_are_preserved_but_not_used_as_live_matches() {
    let mut session = DocumentSession::new();
    session.open("Decision matters.");
    let id = session
        .alt_create_span(SpanKind::Word, 0, 8, "Choice", Source::Human)
        .unwrap()
        .span;
    session.apply_edit(0, 8, "Verdict").unwrap();
    assert!(session.synonyms_for(&id, "Choice").is_empty());
    assert_eq!(session.synonym_groups_json()[0]["attached"], false);
    session.apply_edit(0, 7, "Decision").unwrap();
    assert_eq!(session.synonyms_for(&id, "Choice"), ["decision", "choice"]);
}
