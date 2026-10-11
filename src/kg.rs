//! Knowledge-graph alternatives (issue #13).
//!
//! In a knowledge graph every synonym of a concept shares the concept's id,
//! so the synonyms of a matched word *are* its alternatives. All matching,
//! the concept -> synonyms index, capitalisation and the a/an fix-up come
//! from [`terraphim_lsp_core`] (consumed at an exact released version); this
//! module only adapts the core's results to the editor's document session.
//! Design notes: `docs/design/kg-alternatives.md`.
//!
//! # Derived spans
//!
//! [`kg_spans`] analyses the open document's body and returns one
//! [`KgSpan`] per KG term whose concept has at least two terms. KG spans are
//! **derived** from the text: they are never written to the annotation block
//! (the block keeps only what the KG cannot derive), so they cost nothing to
//! store and always agree with the text, including after undo.
//!
//! * **Dots.** A span's alternatives are every term of its concept in the
//!   core's stable concept order (concept name first, then the other
//!   synonyms sorted), with the text's own form at its place in that order
//!   and marked `active`. Swapping moves the lit dot along a row that does
//!   not reorder, so cycling walks the synonyms and wraps.
//! * **Ids.** `kg-<concept id>-<n>`, where `n` counts earlier matches of the
//!   same concept in the body, so an id survives a swap (the concept and the
//!   order do not change even when an article before the word does).
//! * **Precedence.** A KG term overlapping a live span from the annotation
//!   block is not offered: the writer's span wins.
//!
//! # Swapping
//!
//! [`DocumentSession`] stays the single owner of the body. [`kg_swap`]
//! applies the core's [`Replacement::edits`] (the article fix-up and the
//! term, already capitalised) as ONE contiguous edit through
//! [`DocumentSession::apply_edit`], model first, and returns the same shape
//! as `set_active_alternative`, so the editing surface applies it as one undo
//! step. Undo is a plain inverse edit; the KG spans re-derive from the text.
//!
//! # Appending to a span (R-8.6)
//!
//! [`kg_append`] ("AI alternatives for selection", Ctrl+Shift+G) looks up the
//! KG term at a selection and appends its synonyms to the span over that
//! term, creating a word span when there is none. That is the writer asking
//! for them, so they are persisted, as `source: "ai"` with `model: "kg"`
//! after any human alternatives already there.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{json, Value};
use terraphim_alternatives::utf16_to_byte;
use terraphim_lsp_core::{utf16_len, AlternativeSet, KgEngine, TermMatch};
use wasm_bindgen::prelude::*;

use crate::document::{
    with_session, AltChange, DocumentSession, EditOutcome, SwapEdit, SwapOutcome,
};
use crate::protection::ConceptProtection;
use crate::providers::{append_from_providers, AlternativesProvider, ProviderContext, Suggestion};
use terraphim_lab::types::Thesaurus;

/// The model name recorded on alternatives appended from the KG (R-8.6).
pub const KG_MODEL: &str = "kg";

/// Errors from the KG bridge. Every failing call changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KgError {
    /// The thesaurus could not be loaded; the previous one stays.
    #[error("{0}")]
    Thesaurus(String),
    /// No KG span has this id in the current body.
    #[error("no knowledge-graph span {0}")]
    UnknownSpan(String),
    /// The index is not one of the span's alternatives.
    #[error("knowledge-graph span {id} has no alternative {index}")]
    InvalidIndex {
        /// The span.
        id: String,
        /// The refused index.
        index: usize,
    },
    /// A span from the annotation block is in the way: a swap would change
    /// its text (an article inside the writer's span), or an append would
    /// overlap it (a sentence span around the word). The writer's span wins.
    #[error("span {0} from the annotation block overlaps the knowledge-graph term")]
    Overlaps(String),
    /// No KG term at the selection.
    #[error("no knowledge-graph term at the selection")]
    NoTerm,
    /// The document model refused the change.
    #[error("{0}")]
    Model(String),
    /// A role preparation token is stale, missing, or the limit was exceeded.
    #[error("{0}")]
    Preparation(String),
}

/// A loaded thesaurus: the engine plus a generation counter so cached
/// analyses are invalidated on reload.
#[derive(Debug, Default)]
struct Kg {
    engine: KgEngine,
    name: Option<String>,
    generation: u64,
    /// Original text stays opaque at the JavaScript boundary (u64 IDs).
    thesaurus_json: Option<String>,
    thesaurus: Option<Thesaurus>,
    protection: Option<Result<ConceptProtection, String>>,
    prepared: Option<PreparedRole>,
    next_preparation: u64,
    /// The last [`kg_spans`] result, keyed by body and live spans.
    cache: Option<Cached>,
}

#[derive(Debug)]
struct PreparedRole {
    token: String,
    base_generation: u64,
    engine: KgEngine,
    name: Option<String>,
    thesaurus_json: String,
    thesaurus: Thesaurus,
    protection: Result<ConceptProtection, String>,
}

#[derive(Debug)]
struct Cached {
    generation: u64,
    body: String,
    live: Vec<(usize, usize)>,
    spans: Vec<KgSpan>,
}

thread_local! {
    static KG: RefCell<Kg> = RefCell::new(Kg::default());
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static ROLE_CHANGE_LISTENER: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
}

/// Browser observers run after publication and after releasing all KG borrows.
/// Their exceptions cannot undo a successful authoritative role mutation.
fn notify_role_change() {
    crate::lab::invalidate_trim_plan();
    #[cfg(target_arch = "wasm32")]
    {
        let listener = ROLE_CHANGE_LISTENER.with(|cell| cell.borrow().clone());
        if let Some(listener) = listener {
            let _ = listener.call0(&JsValue::NULL);
        }
    }
}

/// A derived knowledge-graph span (see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KgSpan {
    /// `kg-<concept id>-<n>`.
    pub id: String,
    /// Start of the term in the body (UTF-16).
    pub start: usize,
    /// End of the term in the body (UTF-16).
    pub end: usize,
    /// Index into `alts` of the form the text holds now.
    pub active: usize,
    /// Every term of the concept, capitalised like the text, in concept
    /// order; `alts[active]` is the text itself.
    pub alts: Vec<String>,
    /// The match as the core reported it.
    pub term: TermMatch,
}

impl KgSpan {
    /// The span in the shape of a block span, plus KG fields:
    /// `{ id, kind: "word", source: "kg", anchor: { start, end, text },
    /// active, alts: [{ text, source: "kg" }], conceptId, nterm, description,
    /// url }`. `alts[active]` is the text as it stands.
    pub fn to_json(&self) -> Value {
        let alts: Vec<Value> = self
            .alts
            .iter()
            .map(|text| json!({ "text": text, "source": "kg" }))
            .collect();
        json!({
            "id": self.id,
            "kind": "word",
            "source": "kg",
            "anchor": { "start": self.start, "end": self.end, "text": self.term.text },
            "active": self.active,
            "alts": alts,
            "conceptId": self.term.concept_id,
            "nterm": self.term.nterm,
            "description": self.term.description,
            "url": self.term.url,
        })
    }
}

/// Runs `f` against the editor's KG engine.
fn with_kg<R>(f: impl FnOnce(&mut Kg) -> R) -> R {
    KG.with(|cell| f(&mut cell.borrow_mut()))
}

/// Load the bundled standalone-page role using the existing Terraphim KG engine.
///
/// The same JSON bytes are shipped as `roles/default.json` for local CLI search.
/// Embedding hosts choose their own role and do not call this initializer.
///
/// # Errors
/// Returns an error if the bundled role cannot be parsed or published.
pub fn load_default_thesaurus() -> Result<Value, KgError> {
    load_thesaurus(include_str!("../public/roles/default.json"))
}

/// Loads a thesaurus (the JSON written by terraphim's thesaurus builders),
/// replacing the current one. On error the previous thesaurus stays.
/// Returns `{ name, concepts, skipped }`.
pub fn load_thesaurus(json: &str) -> Result<Value, KgError> {
    let engine = KgEngine::from_json(json).map_err(|e| KgError::Thesaurus(e.to_string()))?;
    let thesaurus: Thesaurus =
        serde_json::from_str(json).map_err(|e| KgError::Thesaurus(e.to_string()))?;
    let protection = ConceptProtection::from_thesaurus(&thesaurus);
    let name = serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_owned));
    let summary = json!({
        "name": name,
        "concepts": engine.concept_index().len(),
        "skipped": engine.skipped_patterns(),
    });
    with_kg(|kg| -> Result<(), KgError> {
        let generation = kg
            .generation
            .checked_add(1)
            .ok_or_else(|| KgError::Preparation("role generation exhausted".into()))?;
        kg.engine = engine;
        kg.name = name;
        kg.generation = generation;
        kg.thesaurus_json = Some(json.to_owned());
        kg.thesaurus = Some(thesaurus);
        kg.protection = Some(protection);
        kg.prepared = None;
        kg.cache = None;
        Ok(())
    })?;
    notify_role_change();
    Ok(summary)
}

/// Forgets the thesaurus: no KG spans until the next load.
pub fn clear_thesaurus() {
    try_clear_thesaurus().expect("role generation exhausted; previous role retained");
}

fn try_clear_thesaurus() -> Result<(), KgError> {
    with_kg(|kg| -> Result<(), KgError> {
        let generation = kg
            .generation
            .checked_add(1)
            .ok_or_else(|| KgError::Preparation("role generation exhausted".into()))?;
        kg.engine = KgEngine::empty();
        kg.name = None;
        kg.generation = generation;
        kg.thesaurus_json = None;
        kg.thesaurus = None;
        kg.protection = None;
        kg.prepared = None;
        kg.cache = None;
        Ok(())
    })?;
    notify_role_change();
    Ok(())
}

/// Immutable authoritative capability snapshot; no ranked/UI matching involved.
pub(crate) struct ActiveRoleSnapshot {
    pub generation: u64,
    pub protection: Result<ConceptProtection, String>,
}

pub(crate) fn active_role_snapshot() -> Option<ActiveRoleSnapshot> {
    with_kg(|kg| {
        kg.thesaurus.as_ref().map(|_| ActiveRoleSnapshot {
            generation: kg.generation,
            protection: kg
                .protection
                .clone()
                .expect("loaded role has a capability result"),
        })
    })
}

/// Active role metadata with an exact decimal generation and opaque JSON text.
/// Reading the snapshot does not publish a role or change document annotations.
pub fn role_snapshot_json() -> String {
    with_kg(|kg| {
        json!({
            "generation": kg.generation.to_string(),
            "name": kg.name,
            "thesaurusText": kg.thesaurus_json,
            "loaded": kg.thesaurus.is_some(),
            "protectionReady": kg.protection.as_ref().is_some_and(Result::is_ok),
            "protectionError": kg.protection.as_ref().and_then(|p| p.as_ref().err()),
        })
        .to_string()
    })
}

/// Validate a role without publishing it. Only the latest preparation is kept.
/// Normal load/clear invalidates it, so late Worker responses cannot resurrect a role.
pub fn prepare_thesaurus(json: &str) -> Result<String, KgError> {
    if json.len() > 16 * 1024 * 1024 {
        return Err(KgError::Preparation(
            "role exceeds the 16 MiB import limit".into(),
        ));
    }
    let engine = KgEngine::from_json(json).map_err(|e| KgError::Thesaurus(e.to_string()))?;
    let thesaurus: Thesaurus =
        serde_json::from_str(json).map_err(|e| KgError::Thesaurus(e.to_string()))?;
    let protection = ConceptProtection::from_thesaurus(&thesaurus);
    let name = serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_owned));
    with_kg(|kg| {
        let next = kg
            .next_preparation
            .checked_add(1)
            .ok_or_else(|| KgError::Preparation("role preparation counter exhausted".into()))?;
        kg.next_preparation = next;
        let token = format!("role-{next}");
        kg.prepared = Some(PreparedRole {
            token: token.clone(),
            base_generation: kg.generation,
            engine,
            name,
            thesaurus_json: json.to_owned(),
            thesaurus,
            protection,
        });
        Ok(token)
    })
}

/// Atomically publish a previously validated role, refusing stale tokens.
pub fn commit_prepared_thesaurus(token: &str) -> Result<Value, KgError> {
    let summary = with_kg(|kg| {
        let prepared = kg
            .prepared
            .as_ref()
            .ok_or_else(|| KgError::Preparation("no prepared role".into()))?;
        if prepared.token != token || prepared.base_generation != kg.generation {
            return Err(KgError::Preparation("stale role preparation".into()));
        }
        let generation = kg
            .generation
            .checked_add(1)
            .ok_or_else(|| KgError::Preparation("role generation exhausted".into()))?;
        let prepared = kg.prepared.take().expect("validated preparation");
        let summary = json!({
            "name": prepared.name,
            "concepts": prepared.engine.concept_index().len(),
            "skipped": prepared.engine.skipped_patterns(),
            "generation": generation.to_string(),
        });
        kg.engine = prepared.engine;
        kg.name = prepared.name;
        kg.thesaurus_json = Some(prepared.thesaurus_json);
        kg.thesaurus = Some(prepared.thesaurus);
        kg.protection = Some(prepared.protection);
        kg.generation = generation;
        kg.cache = None;
        Ok(summary)
    })?;
    notify_role_change();
    Ok(summary)
}

/// Discard this preparation without discarding a newer role or active state.
pub fn discard_prepared_thesaurus(token: &str) {
    with_kg(|kg| {
        if kg.prepared.as_ref().is_some_and(|p| p.token == token) {
            kg.prepared = None;
        }
    });
}

/// Live block spans as UTF-16 ranges.
fn live_ranges(session: &DocumentSession) -> Vec<(usize, usize)> {
    session
        .document()
        .annotations
        .spans
        .iter()
        .map(|s| (s.anchor.start, s.anchor.end))
        .collect()
}

fn overlapping(live: &[(usize, usize)], start: usize, end: usize) -> bool {
    live.iter().any(|&(s, e)| s < end && start < e)
}

/// The index of the text's own form and every alternative text.
type Row = (usize, Vec<String>);

/// The full alternatives row for `set`: the core's replacements (current form
/// excluded, concept order) with the text's own form put back at its place in
/// the concept's term list. `None` when the core's lists disagree (a term the
/// index does not hold), in which case the term is not offered.
fn row(engine: &KgEngine, set: &AlternativeSet) -> Option<Row> {
    let terms = engine.concept_index().synonyms_of(set.term.concept_id);
    let active = terms.iter().position(|t| t.as_str() == set.term.term)?;
    if terms.len() < 2 || set.replacements.len() + 1 != terms.len() {
        return None;
    }
    let mut alts: Vec<String> = set.replacements.iter().map(|r| r.text.clone()).collect();
    alts.insert(active, set.term.text.clone());
    Some((active, alts))
}

/// Derives the KG spans of `body` (see the module docs), skipping terms that
/// overlap `live` block spans.
///
/// The row of a term depends only on its concept and its exact text (the
/// capitalisation is the text's; the article changes the edits, not the
/// texts), so the core is asked once per distinct `(concept, text)` pair:
/// each `alternatives_for` call scans the body, and asking per match would
/// make a refresh quadratic in the document length (see
/// `benches/kg_bench.rs`).
fn derive(engine: &KgEngine, body: &str, live: &[(usize, usize)]) -> Vec<KgSpan> {
    let analysis = engine.analyse(body);
    let mut ordinals: HashMap<u64, usize> = HashMap::new();
    let mut rows: HashMap<(u64, String), Option<Row>> = HashMap::new();
    let mut spans = Vec::new();
    for term in analysis.matches {
        let ordinal = ordinals.entry(term.concept_id).or_insert(0);
        let id = format!("kg-{}-{}", term.concept_id, *ordinal);
        *ordinal += 1;
        if engine.concept_index().synonyms_of(term.concept_id).len() < 2 {
            continue;
        }
        let (start, end) = (term.range.start.utf16, term.range.end.utf16);
        if overlapping(live, start, end) {
            continue;
        }
        let Some((active, alts)) = rows
            .entry((term.concept_id, term.text.clone()))
            .or_insert_with(|| {
                engine
                    .alternatives_for(body, &term)
                    .and_then(|set| row(engine, &set))
            })
            .clone()
        else {
            continue;
        };
        spans.push(KgSpan {
            id,
            start,
            end,
            active,
            alts,
            term,
        });
    }
    spans
}

/// The KG spans of the session's body, cached until the body, the live spans
/// or the thesaurus change.
pub fn kg_spans(session: &DocumentSession) -> Vec<KgSpan> {
    with_kg(|kg| {
        let live = live_ranges(session);
        let body = session.body();
        if let Some(cached) = &kg.cache {
            if cached.generation == kg.generation && cached.body == body && cached.live == live {
                return cached.spans.clone();
            }
        }
        let spans = derive(&kg.engine, body, &live);
        kg.cache = Some(Cached {
            generation: kg.generation,
            body: body.to_string(),
            live,
            spans: spans.clone(),
        });
        spans
    })
}

/// [`kg_spans`] as a JSON array.
pub fn kg_spans_json(session: &DocumentSession) -> Value {
    Value::Array(kg_spans(session).iter().map(KgSpan::to_json).collect())
}

/// Makes alternative `index` of KG span `id` the text (see the module docs).
///
/// The core's edits for the chosen synonym (an `a`/`an` fix-up when the
/// article changes, then the term) are applied to the model as one
/// contiguous edit; the returned [`SwapOutcome`] carries that edit so the
/// surface can apply exactly the same change. Choosing the active form is a
/// no-op (`edit` is `None`). An unknown id, an invalid index, or an edit
/// that would change the text of a block span is refused and changes
/// nothing.
pub fn kg_swap(
    session: &mut DocumentSession,
    id: &str,
    index: usize,
) -> Result<SwapOutcome, KgError> {
    let span = kg_spans(session)
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| KgError::UnknownSpan(id.to_string()))?;
    if index >= span.alts.len() {
        return Err(KgError::InvalidIndex {
            id: id.to_string(),
            index,
        });
    }
    let mut swap = SwapOutcome {
        span: id.to_string(),
        from: span.active,
        to: index,
        edit: None,
        range: (span.start, span.end),
        outcome: EditOutcome::default(),
    };
    if index == span.active {
        swap.outcome.set_aside = session.set_aside().len();
        swap.outcome.notice = session.set_aside_notice();
        return Ok(swap);
    }
    let body = session.body().to_string();
    let set = with_kg(|kg| kg.engine.alternatives_for(&body, &span.term))
        .ok_or_else(|| KgError::UnknownSpan(id.to_string()))?;
    let replacement = &set.replacements[if index < span.active {
        index
    } else {
        index - 1
    }];
    // The edits are sorted and non-overlapping, the term last: collapse them
    // into one region, keeping the text between them.
    let (first, last) = match (replacement.edits.first(), replacement.edits.last()) {
        (Some(first), Some(last)) => (first.range.start, last.range.end),
        _ => return Err(KgError::Model("the core returned no edit".into())),
    };
    let mut inserted = String::new();
    let mut copied = first.byte;
    for edit in &replacement.edits {
        inserted.push_str(&body[copied..edit.range.start.byte]);
        inserted.push_str(&edit.new_text);
        copied = edit.range.end.byte;
    }
    if let Some(&(s, e)) = live_ranges(session)
        .iter()
        .find(|&&(s, e)| s < last.utf16 && first.utf16 < e)
    {
        let owner = session
            .document()
            .annotations
            .spans
            .iter()
            .find(|sp| sp.anchor.start == s && sp.anchor.end == e)
            .map_or_else(String::new, |sp| sp.id.clone());
        return Err(KgError::Overlaps(owner));
    }
    let deleted_text = body[first.byte..last.byte].to_string();
    let outcome = session
        .apply_edit(first.utf16, last.utf16 - first.utf16, &inserted)
        .map_err(|e| KgError::Model(e.to_string()))?;
    let inserted_end = first.utf16 + utf16_len(&inserted);
    let term_len = utf16_len(&replacement.text);
    swap.range = (inserted_end - term_len, inserted_end);
    swap.edit = Some(SwapEdit {
        start: first.utf16,
        deleted_text,
        inserted_text: inserted,
    });
    swap.outcome = outcome;
    Ok(swap)
}

/// The KG term at the selection `start..end` (UTF-16) of the session's body:
/// the term covering `start` (a caret just after a word counts), provided it
/// intersects a non-empty selection.
fn term_at(session: &DocumentSession, start: usize, end: usize) -> Option<AlternativeSet> {
    let body = session.body();
    let set = with_kg(|kg| kg.engine.alternatives_at_utf16(body, start))?;
    let range = set.term.range;
    let intersects = start == end || (range.start.utf16 < end && start < range.end.utf16);
    (intersects && !set.replacements.is_empty()).then_some(set)
}

/// The id of a live block span that overlaps `start..end` without being
/// exactly over it (a sentence or paragraph span around a word, say). The
/// writer's span wins there, so the range is not offered.
pub(crate) fn blocking_range(
    session: &DocumentSession,
    start: usize,
    end: usize,
) -> Option<String> {
    session
        .document()
        .annotations
        .spans
        .iter()
        .find(|sp| {
            sp.anchor.start < end
                && start < sp.anchor.end
                && !(sp.anchor.start == start && sp.anchor.end == end)
        })
        .map(|sp| sp.id.clone())
}

fn blocking_span(session: &DocumentSession, set: &AlternativeSet) -> Option<String> {
    blocking_range(
        session,
        set.term.range.start.utf16,
        set.term.range.end.utf16,
    )
}

/// The range of the KG term that "AI alternatives" would target at the
/// selection (see [`term_at`]), for the provider seam.
pub(crate) fn term_range_at(
    session: &DocumentSession,
    start: usize,
    end: usize,
) -> Option<(usize, usize)> {
    term_at(session, start, end).map(|set| (set.term.range.start.utf16, set.term.range.end.utf16))
}

/// `body[start..end]` for UTF-16 offsets.
pub(crate) fn slice_utf16(body: &str, start: usize, end: usize) -> Option<&str> {
    let a = utf16_to_byte(body, start)?;
    let b = utf16_to_byte(body, end)?;
    body.get(a..b)
}

/// The thesaurus as an [`AlternativesProvider`] (R-8.7): the synonyms of the
/// KG term at the target, as `source: "ai"`, `model: "kg"` (R-8.6). It is the
/// default, and out of the box the only, provider.
#[derive(Debug, Clone, Copy, Default)]
pub struct ThesaurusProvider;

/// The thesaurus provider's registry id.
pub const THESAURUS_PROVIDER_ID: &str = "thesaurus";

impl AlternativesProvider for ThesaurusProvider {
    fn id(&self) -> &str {
        THESAURUS_PROVIDER_ID
    }

    fn alternatives(&self, context: &ProviderContext<'_>) -> Vec<Suggestion> {
        let Some(set) = with_kg(|kg| kg.engine.alternatives_at_utf16(context.body, context.start))
        else {
            return Vec::new();
        };
        // Only the term the target is over: a selection that is not a KG
        // term is not the thesaurus's to answer.
        if (set.term.range.start.utf16, set.term.range.end.utf16) != (context.start, context.end) {
            return Vec::new();
        }
        set.replacements
            .iter()
            .map(|r| Suggestion::ai(r.text.clone(), KG_MODEL))
            .collect()
    }
}

/// `{ term, conceptId, nterm, range: { start, end }, alternatives: [{ text,
/// source: "kg" }], span }` for the KG term at the selection, where `span` is
/// the id of the block span exactly over the term (or `null`); `null` when
/// there is no term, or when another block span overlaps it (see
/// [`kg_append`]). Read-only.
pub fn kg_lookup(session: &DocumentSession, start: usize, end: usize) -> Value {
    let Some(set) = term_at(session, start, end) else {
        return Value::Null;
    };
    if blocking_span(session, &set).is_some() {
        return Value::Null;
    }
    let (s, e) = (set.term.range.start.utf16, set.term.range.end.utf16);
    let span = session
        .document()
        .annotations
        .spans
        .iter()
        .find(|sp| sp.anchor.start == s && sp.anchor.end == e)
        .map(|sp| sp.id.clone());
    let alternatives: Vec<Value> = set
        .replacements
        .iter()
        .map(|r| json!({ "text": r.text, "source": "kg" }))
        .collect();
    json!({
        "term": set.term.text,
        "conceptId": set.term.concept_id,
        "nterm": set.term.nterm,
        "range": { "start": s, "end": e },
        "alternatives": alternatives,
        "span": span,
    })
}

/// "AI alternatives for selection" (R-8.6): appends the KG synonyms of the
/// term at the selection to the block span over that term (creating a word
/// span when there is none), as `source: "ai"`, `model: "kg"`, after the
/// alternatives already there; synonyms the span already offers are skipped
/// (`Ok(None)` when nothing is left to add). Refused, changing nothing, when
/// there is no KG term at the selection or a different span overlaps it.
///
/// It asks the registered providers (R-8.7, [`crate::providers`]), of which
/// the thesaurus is the default, so a provider registered with
/// [`crate::register_provider`] adds its suggestions after the thesaurus's.
pub fn kg_append(
    session: &mut DocumentSession,
    start: usize,
    end: usize,
) -> Result<Option<AltChange>, KgError> {
    append_from_providers(session, start, end, &HashMap::new())
}

/// The thesaurus name, if one is loaded.
pub fn thesaurus_name() -> Option<String> {
    with_kg(|kg| kg.name.clone())
}

// ---------------------------------------------------------------------------
// JavaScript exports (`window.wasmBindings.<name>` in the Trunk build).
// ---------------------------------------------------------------------------

/// Replace the single opt-in browser role observer; `None` unregisters it.
/// Called synchronously, with no arguments, only after successful load, clear,
/// or prepared commit. Read the authoritative snapshot inside the callback.
#[wasm_bindgen]
pub fn kg_set_role_change_listener(listener: Option<js_sys::Function>) {
    #[cfg(target_arch = "wasm32")]
    ROLE_CHANGE_LISTENER.with(|cell| *cell.borrow_mut() = listener);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = listener;
}

pub(crate) fn to_js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

/// Loads a thesaurus JSON for KG alternatives, replacing the current one.
/// Returns `{ name, concepts, skipped }`; throws (keeping the previous
/// thesaurus) if the JSON is not a thesaurus.
#[wasm_bindgen]
pub fn kg_load_thesaurus(json: &str) -> Result<JsValue, JsValue> {
    load_thesaurus(json)
        .map(|v| to_js(&v))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Forgets the thesaurus: no KG spans until the next load.
#[wasm_bindgen]
pub fn kg_clear_thesaurus() {
    if let Err(error) = try_clear_thesaurus() {
        wasm_bindgen::throw_str(&error.to_string());
    }
}

/// Read-only active role snapshot for the Worker coordinator.
#[wasm_bindgen]
pub fn kg_role_snapshot_json() -> String {
    role_snapshot_json()
}

/// Prepare local role JSON without replacing the current KG engine.
#[wasm_bindgen]
pub fn kg_prepare_role(json: &str) -> Result<String, JsValue> {
    prepare_thesaurus(json).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Publish a prepared role and return its summary as JSON text.
#[wasm_bindgen]
pub fn kg_commit_prepared_role(token: &str) -> Result<String, JsValue> {
    commit_prepared_thesaurus(token)
        .map(|v| v.to_string())
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Discard a prepared role; active state is untouched.
#[wasm_bindgen]
pub fn kg_discard_prepared_role(token: &str) {
    discard_prepared_thesaurus(token);
}

/// The derived KG spans of the open document (see [`KgSpan::to_json`]).
/// `document_annotations` returns the same list as its `kg` field.
#[wasm_bindgen]
pub fn kg_document_spans() -> JsValue {
    to_js(&with_session(|s| kg_spans_json(s)))
}

/// Makes alternative `index` of KG span `span_id` the text. Returns the same
/// shape as `set_active_alternative` (`{ span, from, to, edit, range, ... }`,
/// `edit` the exact UTF-16 change made to the body, or `null`); throws,
/// changing nothing, for an unknown span, an invalid index or an edit that
/// would change a block span's text.
#[wasm_bindgen]
pub fn kg_swap_alternative(span_id: &str, index: u32) -> Result<JsValue, JsValue> {
    with_session(|s| kg_swap(s, span_id, index as usize))
        .map(|outcome| to_js(&outcome.to_json()))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// The KG term at the selection `start..end` with its alternatives, or
/// `null` (see [`kg_lookup`]). Read-only.
#[wasm_bindgen]
pub fn kg_lookup_selection(start: u32, end: u32) -> JsValue {
    to_js(&with_session(|s| {
        kg_lookup(s, start as usize, end as usize)
    }))
}

/// "AI alternatives for selection" (R-8.6, see [`kg_append`]), in the shape
/// of the #10 panel operations so `MarkdownEditor.alternativeOp('kg_append',
/// start, end)` records it as one undo step: returns the change (see
/// `AltChange::to_json`), `null` when the span already offers every synonym,
/// and throws, changing nothing, when there is no KG term at the selection
/// or another span overlaps it.
#[wasm_bindgen]
pub fn alt_kg_append(start: u32, end: u32) -> Result<JsValue, JsValue> {
    match with_session(|s| kg_append(s, start as usize, end as usize)) {
        Ok(Some(change)) => Ok(to_js(&change.to_json())),
        Ok(None) => Ok(JsValue::NULL),
        Err(error) => Err(JsValue::from_str(&error.to_string())),
    }
}

#[cfg(test)]
mod protection_tests {
    use super::*;
    const ROLE: &str = r#"{"name":"public","data":{"x":{"id":1,"nterm":"x"}}}"#;

    #[test]
    fn bundled_default_role_registers_real_synonyms_and_uses_shipped_bytes() {
        clear_thesaurus();
        let summary = load_default_thesaurus().unwrap();
        assert_eq!(summary["name"], "terraphim-editor-default");
        assert_eq!(
            thesaurus_name().as_deref(),
            Some("terraphim-editor-default")
        );
        let mut session = DocumentSession::new();
        session.open("Decision: launch the pilot.");
        let spans = kg_spans(&session);
        let decision = spans
            .iter()
            .find(|span| span.term.text == "Decision")
            .unwrap();
        assert!(decision.alts.iter().any(|term| term == "Judgment"));
        assert!(decision.alts.iter().any(|term| term == "Choice"));
        with_kg(|kg| {
            assert_eq!(
                kg.thesaurus_json.as_deref(),
                Some(include_str!("../public/roles/default.json"))
            )
        });
        clear_thesaurus();
    }

    #[test]
    fn generation_exhaustion_refuses_all_publication_paths_without_mutation() {
        load_thesaurus(ROLE).unwrap();
        with_kg(|kg| kg.generation = u64::MAX);
        let token = prepare_thesaurus(ROLE).unwrap();
        let before = role_snapshot_json();
        assert!(load_thesaurus(ROLE)
            .unwrap_err()
            .to_string()
            .contains("exhausted"));
        assert_eq!(role_snapshot_json(), before);
        assert!(try_clear_thesaurus()
            .unwrap_err()
            .to_string()
            .contains("exhausted"));
        assert_eq!(role_snapshot_json(), before);
        assert!(commit_prepared_thesaurus(&token)
            .unwrap_err()
            .to_string()
            .contains("exhausted"));
        assert_eq!(role_snapshot_json(), before);
        with_kg(|kg| *kg = Kg::default());
    }

    #[test]
    fn unsupported_and_empty_roles_remain_ordinary_kg_but_not_trim_capable() {
        for role in [
            r#"{"name":"public","data":{"Straße":{"id":1,"nterm":"strasse"}}}"#,
            r#"{"name":"public","data":{}}"#,
        ] {
            let token = prepare_thesaurus(role).unwrap();
            commit_prepared_thesaurus(&token).unwrap();
            let snapshot: Value = serde_json::from_str(&role_snapshot_json()).unwrap();
            assert_eq!(snapshot["loaded"], true);
            assert_eq!(snapshot["protectionReady"], false);
            assert!(snapshot["protectionError"].is_string());
            assert!(active_role_snapshot().unwrap().protection.is_err());
        }
        load_thesaurus(ROLE).unwrap();
        assert!(active_role_snapshot().unwrap().protection.is_ok());
    }
}
