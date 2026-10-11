use serde_json::Value;
use terraphim_alternatives::{Source, SpanKind};
use terraphim_editor::DocumentSession;

fn graph(session: &DocumentSession) -> Value {
    serde_json::from_str(&session.export_knowledge_graph().unwrap()).unwrap()
}

#[test]
fn shared_graph_follows_swaps_reopen_snapshot_restoration_and_detachment() {
    let mut session = DocumentSession::new();
    session.open("Risk: supplier delay. Decision: retain a backup supplier.");
    let original = session.save();
    let id = session
        .alt_create_span(SpanKind::Word, 0, 4, "Concern", Source::Human)
        .unwrap()
        .span;
    let authored = session.save();
    let expected = graph(&session);
    assert_eq!(
        expected["data"]["concern"]["id"],
        expected["data"]["risk"]["id"]
    );
    session.set_active(&id, 1).unwrap();
    assert!(session.export().starts_with("Concern:"));
    assert_eq!(graph(&session), expected);
    session.set_active(&id, 0).unwrap();
    assert!(session.export().starts_with("Risk:"));
    let mut reopened = DocumentSession::new();
    reopened.open(&session.save());
    assert_eq!(graph(&reopened), expected);
    session.open(&original);
    assert_eq!(graph(&session)["data"], serde_json::json!({}));
    session.open(&authored);
    assert_eq!(graph(&session), expected);
    session.apply_edit(0, 4, "Issue").unwrap();
    assert_eq!(graph(&session)["data"], serde_json::json!({}));
    session.apply_edit(0, 5, "Risk").unwrap();
    assert_eq!(graph(&session), expected);
}

#[test]
fn malformed_annotations_cannot_be_exported_as_a_successful_empty_graph() {
    let source =
        include_str!("../crates/terraphim_alternatives/tests/fixtures/malformed/invalid_json.md");
    let mut session = DocumentSession::new();
    session.open(source);
    assert!(session.export_knowledge_graph().is_err());
    assert_eq!(session.save(), source);
}
