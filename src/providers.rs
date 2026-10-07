//! Pluggable alternatives providers (issue #83, R-8.7).
//!
//! "AI alternatives for selection" asks an ordered list of providers for
//! suggestions rather than calling the knowledge graph directly. The
//! thesaurus ([`crate::kg::ThesaurusProvider`]) is the default and, out of
//! the box, the only provider; a host page can add more (an LLM behind
//! `terraphim_server`'s `/chat`, say) through the JavaScript hook described
//! in `docs/design/provider-seam.md`. No LLM call lives here (decision 12.4).
//!
//! # The seam
//!
//! * [`AlternativesProvider`] is the trait: an id and a function from a
//!   [`ProviderContext`] (the target text and where it is) to
//!   [`Suggestion`]s.
//! * The registry is a per-thread ordered list. [`register_provider`] adds
//!   a provider (replacing one with the same id in place), and
//!   [`unregister_provider`] removes one.
//! * [`append_from_providers`] resolves the target, asks every provider in
//!   order and appends the merged suggestions as ONE change, so one undo
//!   step. Suggestions the span already offers, and repeats between
//!   providers, are skipped: the earlier provider wins, and its `source`
//!   and `model` are kept.
//!
//! # Asynchronous providers
//!
//! The WASM document API is synchronous, but a host provider is not (it
//! calls a network service). The JavaScript side therefore awaits its
//! providers first and hands the answers in as `supplied` results keyed by
//! provider id; an [`ExternalProvider`] is the registry entry that replays
//! them in its place in the order.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;
use terraphim_alternatives::{Source, SpanKind};
use wasm_bindgen::prelude::*;

use crate::document::{with_session, AltChange, DocumentSession};
use crate::kg::{self, KgError, ThesaurusProvider};

/// One suggested alternative text and where it came from (R-4.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// The suggested text.
    pub text: String,
    /// Provenance. Providers answer [`Source::Ai`]: the writer asked.
    pub source: Source,
    /// The model that produced it (`"kg"` for the thesaurus).
    pub model: Option<String>,
}

impl Suggestion {
    /// An AI-sourced suggestion from `model`.
    pub fn ai(text: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            source: Source::Ai,
            model: Some(model.into()),
        }
    }
}

/// What a provider is asked about: the target span's text and place.
#[derive(Debug, Clone, Copy)]
pub struct ProviderContext<'a> {
    /// The whole body.
    pub body: &'a str,
    /// Start of the target (UTF-16).
    pub start: usize,
    /// End of the target (UTF-16).
    pub end: usize,
    /// The target's text (`body[start..end]`).
    pub text: &'a str,
    /// Answers already obtained from asynchronous host providers, by
    /// provider id (see the module docs).
    pub supplied: &'a HashMap<String, Vec<Suggestion>>,
}

/// A source of alternatives for a piece of text (R-8.7).
///
/// Providers are synchronous and single-threaded (the editor runs on the
/// browser's main thread, and the registry is per thread).
pub trait AlternativesProvider {
    /// A stable identifier, unique in the registry.
    fn id(&self) -> &str;

    /// Suggestions for the target in `context`, best first. An empty list
    /// means "nothing to offer", never an error.
    fn alternatives(&self, context: &ProviderContext<'_>) -> Vec<Suggestion>;
}

/// The registry entry for a provider that lives in JavaScript: its answers
/// arrive in [`ProviderContext::supplied`], already awaited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalProvider {
    id: String,
    model: String,
}

impl ExternalProvider {
    /// A host provider `id` whose suggestions default to `model`.
    pub fn new(id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            model: model.into(),
        }
    }

    /// The model recorded when a suggestion names none.
    pub fn model(&self) -> &str {
        &self.model
    }
}

impl AlternativesProvider for ExternalProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn alternatives(&self, context: &ProviderContext<'_>) -> Vec<Suggestion> {
        context.supplied.get(&self.id).cloned().unwrap_or_default()
    }
}

thread_local! {
    static REGISTRY: RefCell<Vec<Box<dyn AlternativesProvider>>> =
        RefCell::new(default_providers());
}

fn default_providers() -> Vec<Box<dyn AlternativesProvider>> {
    vec![Box::new(ThesaurusProvider)]
}

/// Adds `provider` at the end of the order. A provider with the same id is
/// replaced in place, keeping its position.
pub fn register_provider(provider: Box<dyn AlternativesProvider>) {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        match registry.iter().position(|p| p.id() == provider.id()) {
            Some(at) => registry[at] = provider,
            None => registry.push(provider),
        }
    });
}

/// Removes the provider `id`; `false` when there was none.
pub fn unregister_provider(id: &str) -> bool {
    REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        let before = registry.len();
        registry.retain(|p| p.id() != id);
        registry.len() != before
    })
}

/// The registered provider ids, in the order they are asked.
pub fn provider_ids() -> Vec<String> {
    REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .map(|p| p.id().to_owned())
            .collect()
    })
}

/// Back to the default: the thesaurus provider alone.
pub fn reset_providers() {
    EXTERNAL_MODELS.with(|m| m.borrow_mut().clear());
    REGISTRY.with(|registry| *registry.borrow_mut() = default_providers());
}

/// The span kind for a selection that is not a knowledge-graph term.
fn kind_of(text: &str) -> SpanKind {
    if text.contains('\n') {
        SpanKind::Paragraph
    } else if text.split_whitespace().nth(1).is_some() {
        SpanKind::Sentence
    } else {
        SpanKind::Word
    }
}

/// "AI alternatives for selection" through the providers (R-8.6, R-8.7).
///
/// The target is the knowledge-graph term at the selection when there is
/// one (the term covering `start`, a caret just after a word counting), and
/// otherwise the selection itself when it is not empty. Every provider is
/// asked in registry order; the suggestions are merged and appended to the
/// span exactly over the target, creating it when there is none, after the
/// alternatives already there, as one [`AltChange`]. `Ok(None)` when
/// nothing new is left to add.
///
/// Refused, changing nothing, when there is no target or no provider has a
/// suggestion and no knowledge-graph term is there ([`KgError::NoTerm`]), or
/// when a different span overlaps the target ([`KgError::Overlaps`]).
pub fn append_from_providers(
    session: &mut DocumentSession,
    start: usize,
    end: usize,
    supplied: &HashMap<String, Vec<Suggestion>>,
) -> Result<Option<AltChange>, KgError> {
    let term = kg::term_range_at(session, start, end);
    let (target_start, target_end, term_found) = match term {
        Some((s, e)) => (s, e, true),
        None if start != end => (start.min(end), start.max(end), false),
        None => return Err(KgError::NoTerm),
    };
    let body = session.body().to_string();
    let Some(text) = kg::slice_utf16(&body, target_start, target_end) else {
        return Err(KgError::NoTerm);
    };
    let context = ProviderContext {
        body: &body,
        start: target_start,
        end: target_end,
        text,
        supplied,
    };
    // Ask first, append after: a provider cannot hold the registry while
    // the session changes.
    let suggestions: Vec<Suggestion> = REGISTRY.with(|registry| {
        registry
            .borrow()
            .iter()
            .flat_map(|provider| provider.alternatives(&context))
            .collect()
    });
    if suggestions.is_empty() && !term_found {
        return Err(KgError::NoTerm);
    }
    if let Some(id) = kg::blocking_range(session, target_start, target_end) {
        return Err(KgError::Overlaps(id));
    }
    session
        .append_suggestions(
            if term_found {
                SpanKind::Word
            } else {
                kind_of(text)
            },
            target_start,
            target_end,
            &suggestions,
        )
        .map_err(KgError::Model)
}

/// Parses the answers a host passed in: `{ "<provider id>": [ "text" |
/// { "text": ..., "model": ... } ] }`. A suggestion without a model takes
/// its provider's registered model, and the provider's id when it has none.
/// Entries that are neither a string nor an object with a non-empty `text`
/// are dropped, as are answers for providers that are not registered.
pub fn parse_supplied(json: &str) -> Result<HashMap<String, Vec<Suggestion>>, String> {
    if json.trim().is_empty() {
        return Ok(HashMap::new());
    }
    let value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let Value::Object(map) = value else {
        return Err("supplied alternatives must be an object keyed by provider id".into());
    };
    let known = provider_ids();
    let mut out = HashMap::new();
    for (id, answers) in map {
        if !known.contains(&id) {
            continue;
        }
        let model = EXTERNAL_MODELS.with(|m| m.borrow().get(&id).cloned());
        let default_model = model.unwrap_or_else(|| id.clone());
        let list: Vec<Suggestion> = answers
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or_default()
            .iter()
            .filter_map(|item| {
                let (text, model) = match item {
                    Value::String(text) => (text.as_str(), None),
                    Value::Object(o) => (
                        o.get("text").and_then(Value::as_str)?,
                        o.get("model").and_then(Value::as_str),
                    ),
                    _ => return None,
                };
                (!text.trim().is_empty())
                    .then(|| Suggestion::ai(text, model.unwrap_or(&default_model)))
            })
            .collect();
        out.insert(id, list);
    }
    Ok(out)
}

thread_local! {
    /// The registered model of each external provider, by id.
    static EXTERNAL_MODELS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

/// Registers a host (JavaScript) provider `id` whose suggestions default to
/// `model`, after the providers already there.
///
/// The built-in thesaurus id is reserved: registering it is refused, so a
/// host can never replace the default provider.
pub fn register_external_provider(id: &str, model: &str) -> Result<(), String> {
    if id == crate::kg::THESAURUS_PROVIDER_ID {
        return Err(format!("the provider id \"{id}\" is reserved"));
    }
    EXTERNAL_MODELS.with(|m| m.borrow_mut().insert(id.to_owned(), model.to_owned()));
    register_provider(Box::new(ExternalProvider::new(id, model)));
    Ok(())
}

/// Removes a host provider and forgets its model; the reserved thesaurus id
/// is never removed this way (`false`).
pub fn unregister_external_provider(id: &str) -> bool {
    if id == crate::kg::THESAURUS_PROVIDER_ID {
        return false;
    }
    EXTERNAL_MODELS.with(|m| m.borrow_mut().remove(id));
    unregister_provider(id)
}

// ---------------------------------------------------------------------------
// JavaScript exports (`window.wasmBindings.<name>` in the Trunk build).
// ---------------------------------------------------------------------------

/// Registers a host provider `id` (default model `model`) after the ones
/// already there, so its answers can be supplied to
/// [`alt_append_alternatives`]. Re-registering keeps its place; the
/// `thesaurus` id is reserved and throws.
#[wasm_bindgen]
pub fn alternatives_provider_register(id: &str, model: &str) -> Result<(), JsValue> {
    register_external_provider(id, model).map_err(|e| JsValue::from_str(&e))
}

/// Removes the provider `id`; `false` when there was none.
#[wasm_bindgen]
pub fn alternatives_provider_unregister(id: &str) -> bool {
    unregister_external_provider(id)
}

/// The provider ids in the order they are asked, as a JSON array.
#[wasm_bindgen]
pub fn alternatives_provider_ids() -> JsValue {
    crate::kg::to_js(&Value::from(provider_ids()))
}

/// "AI alternatives for selection" through every provider, in the shape of
/// the #10 panel operations so `MarkdownEditor.alternativeOp(
/// 'append_alternatives', start, end, supplied)` records it as one undo step.
/// `supplied` is the JSON the host's awaited providers produced (see
/// [`parse_supplied`]); `""` for none. Returns the change, `null` when
/// nothing new was left to add, and throws, changing nothing, when there is
/// nothing to offer for the selection or another span overlaps it.
#[wasm_bindgen]
pub fn alt_append_alternatives(start: u32, end: u32, supplied: &str) -> Result<JsValue, JsValue> {
    let supplied = parse_supplied(supplied).map_err(|e| JsValue::from_str(&e))?;
    match with_session(|s| append_from_providers(s, start as usize, end as usize, &supplied)) {
        Ok(Some(change)) => Ok(crate::kg::to_js(&change.to_json())),
        Ok(None) => Ok(JsValue::NULL),
        Err(error) => Err(JsValue::from_str(&error.to_string())),
    }
}
