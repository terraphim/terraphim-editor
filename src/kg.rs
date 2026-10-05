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
use terraphim_alternatives::{Source, SpanKind};
use terraphim_lsp_core::{utf16_len, AlternativeSet, KgEngine, TermMatch};
use wasm_bindgen::prelude::*;

use crate::document::{with_session, DocumentSession, EditOutcome, SwapEdit, SwapOutcome};

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
    /// The replacement would change the text of a span from the annotation
    /// block (an article inside the writer's span).
    #[error("the replacement would change the text of span {0}")]
    Overlaps(String),
    /// No KG term at the selection.
    #[error("no knowledge-graph term at the selection")]
    NoTerm,
    /// The document model refused the change.
    #[error("{0}")]
    Model(String),
}

/// A loaded thesaurus: the engine plus a generation counter so cached
/// analyses are invalidated on reload.
#[derive(Debug, Default)]
struct Kg {
    engine: KgEngine,
    name: Option<String>,
    generation: u64,
    /// The last [`kg_spans`] result, keyed by body and live spans.
    cache: Option<Cached>,
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

/// Loads a thesaurus (the JSON written by terraphim's thesaurus builders),
/// replacing the current one. On error the previous thesaurus stays.
/// Returns `{ name, concepts, skipped }`.
pub fn load_thesaurus(json: &str) -> Result<Value, KgError> {
    let engine = KgEngine::from_json(json).map_err(|e| KgError::Thesaurus(e.to_string()))?;
    let name = serde_json::from_str::<Value>(json)
        .ok()
        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_owned));
    let summary = json!({
        "name": name,
        "concepts": engine.concept_index().len(),
        "skipped": engine.skipped_patterns(),
    });
    with_kg(|kg| {
        kg.engine = engine;
        kg.name = name;
        kg.generation += 1;
        kg.cache = None;
    });
    Ok(summary)
}

/// Forgets the thesaurus: no KG spans until the next load.
pub fn clear_thesaurus() {
    with_kg(|kg| {
        kg.engine = KgEngine::empty();
        kg.name = None;
        kg.generation += 1;
        kg.cache = None;
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

/// `{ term, conceptId, nterm, range: { start, end }, alternatives: [{ text,
/// source: "kg" }], span }` for the KG term at the selection, where `span` is
/// the id of the block span exactly over the term (or `null`); `null` when
/// there is no term. Read-only.
pub fn kg_lookup(session: &DocumentSession, start: usize, end: usize) -> Value {
    let Some(set) = term_at(session, start, end) else {
        return Value::Null;
    };
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
/// alternatives already there; synonyms the span already offers are skipped.
/// Returns `(span id, number appended)`. Refused, changing nothing, when
/// there is no KG term at the selection or a different span overlaps it.
pub fn kg_append(
    session: &mut DocumentSession,
    start: usize,
    end: usize,
) -> Result<(String, usize), KgError> {
    let set = term_at(session, start, end).ok_or(KgError::NoTerm)?;
    let texts: Vec<String> = set.replacements.iter().map(|r| r.text.clone()).collect();
    session
        .append_alternatives(
            SpanKind::Word,
            set.term.range.start.utf16,
            set.term.range.end.utf16,
            &texts,
            Source::Ai,
            Some(KG_MODEL),
        )
        .map_err(|e| KgError::Model(e.to_string()))
}

/// The thesaurus name, if one is loaded.
pub fn thesaurus_name() -> Option<String> {
    with_kg(|kg| kg.name.clone())
}

// ---------------------------------------------------------------------------
// JavaScript exports (`window.wasmBindings.<name>` in the Trunk build).
// ---------------------------------------------------------------------------

fn to_js(value: &Value) -> JsValue {
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
    clear_thesaurus();
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

/// "AI alternatives for selection" (R-8.6, see [`kg_append`]). Returns
/// `{ ok: true, span, added, annotations }` or `{ ok: false, error }` with the
/// document unchanged. Never throws.
#[wasm_bindgen]
pub fn kg_append_alternatives(start: u32, end: u32) -> JsValue {
    to_js(&with_session(|s| {
        match kg_append(s, start as usize, end as usize) {
            Ok((span, added)) => json!({
                "ok": true,
                "span": span,
                "added": added,
                "annotations": s.annotations_json(),
            }),
            Err(error) => json!({ "ok": false, "error": error.to_string() }),
        }
    }))
}
