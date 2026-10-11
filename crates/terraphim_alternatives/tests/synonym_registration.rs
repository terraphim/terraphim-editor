use terraphim_alternatives::{Document, Source, SpanKind, parse, write};
use terraphim_types::NormalizedTermValue;

fn terms(doc: &Document, id: &str, text: &str) -> Vec<String> {
    let groups = doc.synonym_groups();
    let group = groups.iter().find(|g| g.span_id == id).unwrap();
    group
        .synonyms_of(text)
        .iter()
        .map(ToString::to_string)
        .collect()
}
#[test]
fn authored_alternatives_register_with_real_types_and_directive_parser() {
    let mut doc = Document::new("Decision matters.");
    let id = doc.add_span(SpanKind::Word, 0, 8).unwrap();
    doc.add_alternative(&id, "Choice", Source::Human, None)
        .unwrap();
    doc.add_alternative(&id, "Judgment", Source::Ai, Some("local".into()))
        .unwrap();
    let groups = doc.synonym_groups();
    let group = &groups[0];
    assert!(
        group
            .markdown
            .contains("synonyms:: Decision, Choice, Judgment")
    );
    let parsed = terraphim_automata::parse_markdown_directives_str("alternative", &group.markdown);
    assert_eq!(
        parsed.directives["alternative"].synonyms,
        ["decision", "choice", "judgment"]
    );
    let choice = group
        .thesaurus
        .get(&NormalizedTermValue::from("Choice"))
        .unwrap();
    let decision = group
        .thesaurus
        .get(&NormalizedTermValue::from("Decision"))
        .unwrap();
    assert_eq!(choice.id, decision.id);
    assert_eq!(
        group
            .synonyms_of("CHOICE")
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["decision", "choice", "judgment"]
    );
    assert_eq!(group.alternatives[2].source, Source::Ai);
    assert_eq!(group.alternatives[2].model.as_deref(), Some("local"));
}
#[test]
fn save_reopen_edit_remove_and_ghost_preserve_exact_current_membership() {
    let mut doc = Document::new("Decision matters.");
    let id = doc.add_span(SpanKind::Word, 0, 8).unwrap();
    doc.add_alternative(&id, "Choice", Source::Human, None)
        .unwrap();
    doc.ghost(0, 8).unwrap();
    let reopened = parse(&write(&doc)).unwrap();
    assert_eq!(terms(&reopened, &id, "choice"), ["decision", "choice"]);
    doc.add_alternative(&id, "Option", Source::Human, None)
        .unwrap();
    doc.remove_alternative(&id, 1).unwrap();
    assert!(terms(&doc, &id, "choice").is_empty());
    assert_eq!(terms(&doc, &id, "option"), ["decision", "option"]);
    doc.remove_alternative(&id, 1).unwrap();
    assert!(doc.synonym_groups().is_empty());
    assert_eq!(doc.annotations.ghosts.len(), 1);
}
#[test]
fn identical_terms_in_different_spans_do_not_merge_concepts() {
    let mut doc = Document::new("Bank bank");
    let a = doc.add_span(SpanKind::Word, 0, 4).unwrap();
    let b = doc.add_span(SpanKind::Word, 5, 9).unwrap();
    doc.add_alternative(&a, "Lender", Source::Human, None)
        .unwrap();
    doc.add_alternative(&b, "Shore", Source::Human, None)
        .unwrap();
    assert_eq!(terms(&doc, &a, "bank"), ["bank", "lender"]);
    assert_eq!(terms(&doc, &b, "bank"), ["bank", "shore"]);
    assert!(terms(&doc, &a, "shore").is_empty());
}
#[test]
fn commas_newlines_unicode_and_directive_text_are_lossless_typed_synonyms() {
    let mut doc = Document::new("Café");
    let id = doc.add_span(SpanKind::Paragraph, 0, 4).unwrap();
    for text in [
        "Hello, world",
        "First\nSecond",
        "synonyms:: injected",
        "CAFÉ",
    ] {
        doc.add_alternative(&id, text, Source::Human, None).unwrap();
    }
    let groups = doc.synonym_groups();
    let group = &groups[0];
    for text in [
        "Café",
        "Hello, world",
        "First\nSecond",
        "synonyms:: injected",
    ] {
        assert!(!group.synonyms_of(text).is_empty(), "{text}");
    }
    assert!(group.synonyms_of("Hello").is_empty());
    assert!(group.synonyms_of("world").is_empty());
    assert!(group.synonyms_of("Second").is_empty());
    assert_eq!(group.thesaurus.len(), 4);
    assert_eq!(group.alternatives.len(), 5);
    assert_eq!(group.typed_synonyms, ["Hello, world", "First\nSecond"]);
    assert_eq!(parse(&write(&doc)).unwrap().synonym_groups(), groups);
}
#[test]
fn whitespace_original_does_not_create_a_blank_concept_or_synonym() {
    let mut doc = Document::new(" ");
    let id = doc.add_span(SpanKind::Word, 0, 1).unwrap();
    doc.add_alternative(&id, "Choice", Source::Human, None)
        .unwrap();
    doc.add_alternative(&id, "\t", Source::Human, None).unwrap();
    let groups = doc.synonym_groups();
    let group = &groups[0];
    assert_eq!(
        group
            .synonyms_of("choice")
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["choice"]
    );
    assert!(group.synonyms_of(" ").is_empty());
    assert_eq!(group.alternatives[0].text, " ");
    assert_eq!(group.alternatives[2].text, "\t");
    assert_eq!(
        group
            .thesaurus
            .get(&NormalizedTermValue::from("choice"))
            .unwrap()
            .value
            .as_str(),
        "choice"
    );
}
#[test]
fn entirely_blank_group_keeps_provenance_without_fake_searchable_terms() {
    let mut doc = Document::new(" ");
    let id = doc.add_span(SpanKind::Word, 0, 1).unwrap();
    doc.add_alternative(&id, "\t", Source::Human, None).unwrap();
    let groups = doc.synonym_groups();
    assert!(groups[0].thesaurus.is_empty());
    assert!(groups[0].index.is_empty());
    assert!(groups[0].synonyms_of(" ").is_empty());
    assert_eq!(groups[0].alternatives.len(), 2);
}
