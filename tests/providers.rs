//! Native tests for the alternatives provider seam (issue #83, R-8.7): the
//! thesaurus is the default and only provider, a second provider (a real
//! implementation of the trait, deterministic, no mock framework) is asked
//! after it in registry order, results are de-duplicated with the right
//! source and model, and the whole append stays one change.
#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;

use terraphim_alternatives::{Source, SpanKind};
use terraphim_editor::{
    append_from_providers, kg_append, load_thesaurus, parse_supplied, provider_ids,
    register_external_provider, register_provider, reset_providers, unregister_external_provider,
    unregister_provider, AlternativesProvider, DocumentSession, KgError, ProviderContext,
    Suggestion, KG_MODEL, THESAURUS_PROVIDER_ID,
};

const THESAURUS: &str = include_str!("fixtures/kg/thesaurus.json");
const DOC: &str = include_str!("fixtures/kg/doc.md");

/// Answers the target text reversed, as the model `reverser-1`. Real code,
/// deterministic, standing in for a host's LLM provider.
struct Reverser;

impl AlternativesProvider for Reverser {
    fn id(&self) -> &str {
        "reverser"
    }

    fn alternatives(&self, context: &ProviderContext<'_>) -> Vec<Suggestion> {
        vec![Suggestion::ai(
            context.text.chars().rev().collect::<String>(),
            "reverser-1",
        )]
    }
}

/// Offers one of the thesaurus's own synonyms again, then a new one, to
/// prove de-duplication keeps the first provider's provenance.
struct Echo;

impl AlternativesProvider for Echo {
    fn id(&self) -> &str {
        "echo"
    }

    fn alternatives(&self, context: &ProviderContext<'_>) -> Vec<Suggestion> {
        vec![
            Suggestion::ai("decision", "echo-1"),
            Suggestion::ai("verdict", "echo-1"),
            Suggestion::ai(context.text, "echo-1"),
            Suggestion::ai("", "echo-1"),
        ]
    }
}

fn session() -> DocumentSession {
    reset_providers();
    load_thesaurus(THESAURUS).expect("fixture thesaurus loads");
    let mut session = DocumentSession::new();
    session.open(DOC);
    session
}

fn at(text: &str, needle: &str) -> usize {
    let byte = text.find(needle).expect("needle present");
    text[..byte].encode_utf16().count()
}

/// `text:source/model` of every alternative of the first span.
fn lines(session: &DocumentSession) -> Vec<String> {
    session.document().annotations.spans[0]
        .alts
        .iter()
        .map(|a| {
            format!(
                "{}:{:?}/{}",
                a.text,
                a.source,
                a.model.as_deref().unwrap_or("-")
            )
        })
        .collect()
}

#[test]
fn the_thesaurus_is_the_default_and_only_provider() {
    reset_providers();
    assert_eq!(provider_ids(), [THESAURUS_PROVIDER_ID]);
}

#[test]
fn default_behaviour_is_unchanged() {
    let mut s = session();
    let choice = at(s.body(), "choice");
    let change = kg_append(&mut s, choice, choice + 6).unwrap().unwrap();
    assert_eq!(change.before, None);
    assert_eq!(
        lines(&s),
        [
            "choice:Original/-",
            "decision:Ai/kg",
            "judgment:Ai/kg",
            "option:Ai/kg"
        ]
    );
    assert_eq!(KG_MODEL, "kg");
    // No KG term and an empty selection: refused as before.
    let before = s.clone();
    assert_eq!(kg_append(&mut s, 0, 0).unwrap_err(), KgError::NoTerm);
    // A selection that is no KG term: nothing for the thesaurus to say.
    assert_eq!(kg_append(&mut s, 0, 5).unwrap_err(), KgError::NoTerm);
    assert_eq!(s, before);
}

#[test]
fn a_second_provider_is_asked_after_the_thesaurus_in_order() {
    let mut s = session();
    register_provider(Box::new(Reverser));
    assert_eq!(provider_ids(), [THESAURUS_PROVIDER_ID, "reverser"]);
    let choice = at(s.body(), "choice");
    let change = kg_append(&mut s, choice, choice + 6).unwrap().unwrap();
    // One change, one span, every line in provider order.
    assert_eq!(change.index, Some(1));
    assert_eq!(
        lines(&s),
        [
            "choice:Original/-",
            "decision:Ai/kg",
            "judgment:Ai/kg",
            "option:Ai/kg",
            "eciohc:Ai/reverser-1"
        ]
    );
    assert_eq!(
        change.after.as_ref(),
        Some(&s.document().annotations.spans[0])
    );
    // Asking again adds nothing.
    assert_eq!(kg_append(&mut s, choice, choice + 6).unwrap(), None);
    unregister_provider("reverser");
}

#[test]
fn providers_are_deduplicated_and_the_first_keeps_its_provenance() {
    let mut s = session();
    register_provider(Box::new(Echo));
    register_provider(Box::new(Reverser));
    let choice = at(s.body(), "choice");
    kg_append(&mut s, choice, choice + 6).unwrap().unwrap();
    assert_eq!(
        lines(&s),
        [
            "choice:Original/-",
            "decision:Ai/kg",
            "judgment:Ai/kg",
            "option:Ai/kg",
            "verdict:Ai/echo-1",
            "eciohc:Ai/reverser-1"
        ]
    );
    reset_providers();
}

#[test]
fn registering_an_id_again_replaces_in_place_and_unregister_removes() {
    reset_providers();
    register_provider(Box::new(Reverser));
    register_provider(Box::new(Echo));
    register_provider(Box::new(Reverser));
    assert_eq!(provider_ids(), [THESAURUS_PROVIDER_ID, "reverser", "echo"]);
    assert!(unregister_provider("reverser"));
    assert!(!unregister_provider("reverser"));
    assert_eq!(provider_ids(), [THESAURUS_PROVIDER_ID, "echo"]);
    reset_providers();
}

#[test]
fn a_provider_can_offer_alternatives_for_a_selection_that_is_no_kg_term() {
    let mut s = session();
    register_provider(Box::new(Reverser));
    let none = HashMap::new();
    let change = append_from_providers(&mut s, 2, 9, &none).unwrap().unwrap();
    let span = change.after.unwrap();
    assert_eq!(span.kind, SpanKind::Word);
    assert_eq!((span.anchor.start, span.anchor.end), (2, 9));
    assert_eq!(
        span.alts[1].text.chars().rev().collect::<String>(),
        span.anchor.text
    );
    assert_eq!(span.alts[1].source, Source::Ai);
    assert_eq!(span.alts[1].model.as_deref(), Some("reverser-1"));
    // A multi-word selection becomes a sentence span.
    let mut s = session();
    register_provider(Box::new(Reverser));
    let change = append_from_providers(&mut s, 2, 16, &none)
        .unwrap()
        .unwrap();
    assert_eq!(change.after.unwrap().kind, SpanKind::Sentence);
    reset_providers();
}

#[test]
fn a_different_span_in_the_way_wins_over_every_provider() {
    let mut s = session();
    register_provider(Box::new(Reverser));
    // A sentence span around "Every choice": a selection inside is refused.
    let end = at(s.body(), ".") + 1;
    s.append_alternatives(
        SpanKind::Sentence,
        0,
        end,
        &["x".into()],
        Source::Human,
        None,
    )
    .unwrap();
    let before = s.clone();
    let err = append_from_providers(&mut s, 2, 6, &HashMap::new()).unwrap_err();
    assert!(matches!(err, KgError::Overlaps(_)), "{err:?}");
    assert_eq!(s, before);
    reset_providers();
}

#[test]
fn supplied_host_answers_take_their_place_in_the_order() {
    let mut s = session();
    register_external_provider("host", "llama-3").unwrap();
    register_provider(Box::new(Reverser));
    assert_eq!(provider_ids(), [THESAURUS_PROVIDER_ID, "host", "reverser"]);
    let supplied = parse_supplied(
        r#"{ "host": ["selection", { "text": "pick", "model": "llama-3-70b" }, "", 3, { "x": 1 }],
             "gone": ["ignored"] }"#,
    )
    .unwrap();
    assert_eq!(supplied.len(), 1, "unregistered providers are dropped");
    let choice = at(s.body(), "choice");
    append_from_providers(&mut s, choice, choice + 6, &supplied)
        .unwrap()
        .unwrap();
    assert_eq!(
        lines(&s),
        [
            "choice:Original/-",
            "decision:Ai/kg",
            "judgment:Ai/kg",
            "option:Ai/kg",
            "selection:Ai/llama-3",
            "pick:Ai/llama-3-70b",
            "eciohc:Ai/reverser-1"
        ]
    );
    // Without supplied answers the host provider offers nothing.
    let mut s = session();
    register_external_provider("host", "llama-3").unwrap();
    let choice = at(s.body(), "choice");
    kg_append(&mut s, choice, choice + 6).unwrap().unwrap();
    assert_eq!(lines(&s).len(), 4);
    reset_providers();
}

#[test]
fn bad_supplied_json_is_refused() {
    assert!(parse_supplied("[1]").is_err());
    assert!(parse_supplied("{ nope").is_err());
    assert!(parse_supplied("").unwrap().is_empty());
}

#[test]
fn a_host_only_provider_works_without_a_thesaurus() {
    reset_providers();
    terraphim_editor::clear_thesaurus();
    let mut s = DocumentSession::new();
    s.open("Plain words here.");
    register_external_provider("host", "m").unwrap();
    let supplied = parse_supplied(r#"{ "host": ["terms"] }"#).unwrap();
    let change = append_from_providers(&mut s, 6, 11, &supplied)
        .unwrap()
        .unwrap();
    assert_eq!(change.after.unwrap().alts[1].text, "terms");
    // Nothing supplied and no term: refused.
    let err = append_from_providers(&mut s, 0, 5, &HashMap::new()).unwrap_err();
    assert_eq!(err, KgError::NoTerm);
    reset_providers();
}

#[test]
fn the_thesaurus_id_is_reserved() {
    reset_providers();
    assert!(register_external_provider(THESAURUS_PROVIDER_ID, "x").is_err());
    assert!(!unregister_external_provider(THESAURUS_PROVIDER_ID));
    assert_eq!(provider_ids(), [THESAURUS_PROVIDER_ID]);
    let mut s = session();
    let choice = at(s.body(), "choice");
    assert_eq!(lines_after(&mut s, choice), 4, "the default still answers");
}

fn lines_after(s: &mut DocumentSession, at: usize) -> usize {
    kg_append(s, at, at + 6).unwrap().unwrap();
    lines(s).len()
}
