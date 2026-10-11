use terraphim_alternatives::{Document, Source, SpanKind, parse, write};
use terraphim_types::NormalizedTermValue;

fn add(doc: &mut Document, start: usize, end: usize, text: &str) -> String {
    let id = doc.add_span(SpanKind::Word, start, end).unwrap();
    doc.add_alternative(&id, text, Source::Human, None).unwrap();
    id
}

#[test]
fn exported_graph_uses_actual_registered_terms_and_distinct_concepts() {
    let mut doc = Document::new("Risk Decision");
    add(&mut doc, 0, 4, "Concern");
    add(&mut doc, 5, 13, "Choice");
    let graph = doc.export_synonym_thesaurus().unwrap();
    let risk = graph.get(&NormalizedTermValue::from("risk")).unwrap();
    assert_eq!(
        risk.id,
        graph.get(&NormalizedTermValue::from("concern")).unwrap().id
    );
    assert_ne!(
        risk.id,
        graph.get(&NormalizedTermValue::from("choice")).unwrap().id
    );
    assert_eq!(graph.len(), 4);
    assert_eq!(
        parse(&write(&doc))
            .unwrap()
            .export_synonym_thesaurus()
            .unwrap(),
        graph
    );
    let encoded = serde_json::to_string(&graph).unwrap();
    let decoded: terraphim_types::Thesaurus = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, graph);
}

#[test]
fn conflicting_groups_are_refused_without_merging_or_changing_document() {
    let mut doc = Document::new("Bank bank");
    add(&mut doc, 0, 4, "Lender");
    add(&mut doc, 5, 9, "Shore");
    let before = write(&doc);
    assert!(
        doc.export_synonym_thesaurus()
            .unwrap_err()
            .to_string()
            .contains("bank")
    );
    assert_eq!(write(&doc), before);
}

#[test]
fn export_preserves_typed_terms_and_removal_and_empty_state() {
    let mut doc = Document::new("Risk");
    assert!(doc.export_synonym_thesaurus().unwrap().is_empty());
    let id = add(&mut doc, 0, 4, "Concern, uncertainty");
    doc.add_alternative(&id, "First\nSecond", Source::Human, None)
        .unwrap();
    let graph = doc.export_synonym_thesaurus().unwrap();
    assert!(
        graph
            .get(&NormalizedTermValue::from("Concern, uncertainty"))
            .is_some()
    );
    assert!(graph.get(&NormalizedTermValue::from("concern")).is_none());
    assert!(
        graph
            .get(&NormalizedTermValue::from("First\nSecond"))
            .is_some()
    );
    doc.remove_alternative(&id, 2).unwrap();
    doc.remove_alternative(&id, 1).unwrap();
    assert!(doc.export_synonym_thesaurus().unwrap().is_empty());
}

#[test]
fn parsed_external_edits_cannot_export_stale_attached_terms() {
    let mut doc = Document::new("Risk matters.");
    add(&mut doc, 0, 4, "Concern");
    let external = write(&doc).replacen("Risk matters.", "Issue matters.", 1);
    let parsed = parse(&external).unwrap();
    let before = write(&parsed);
    assert!(parsed.export_synonym_thesaurus().unwrap().is_empty());
    assert_eq!(
        write(&parsed),
        before,
        "export must not mutate saved provenance"
    );
}
