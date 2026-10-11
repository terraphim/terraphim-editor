//! The Lab's mark actions for JavaScript (issue #14, spec R-8.1, R-8.2).
//!
//! A thin wasm-bindgen layer over the `terraphim_lab` engine. It reads the
//! body of the editor's single open document (see [`crate::with_session`]) and
//! returns the engine's marks unchanged: UTF-16 offsets into that body, which
//! are the surface's own offsets. **Nothing here changes text**; proposals are
//! returned for the UI to offer, never applied.
//!
//! * [`lab_actions`] lists `{ id, label }` in popover order. Ids are the
//!   engine's own serde names (`typos_and_punctuation`, ...), so there is no
//!   second table to keep in step.
//! * [`lab_mark`] runs one action and returns
//!   `[{ kind, start, end, score, reason, proposal }]`.
//!
//! * [`trim_plan_json`], [`trim_status`] and [`trim_make_cuts`] are the trim
//!   levels of issue #15 (see the delimited block below).
//!
//! The caller must align the model body with the surface first
//! (`MarkdownEditor.alignDocumentModel()`), exactly as `annotations()` does.
//!
//! The [`LabConfig`] (the embedded default style and typo lists, no role) is
//! built once, on the first [`lab_mark`] call, so plain editing and page load
//! pay nothing for it. Role support (`LabConfig::with_role`) is a follow-up:
//! it needs the active role's thesaurus and rolegraph in the browser.

use std::cell::{OnceCell, RefCell};

use crate::protection::ConceptProtection;
use serde_json::{json, Value};
#[cfg(test)]
use terraphim_lab::trim_plan;
use terraphim_lab::{
    make_cuts, mark, Cut, CutId, LabAction, LabConfig, LabError, TrimLevel, TrimPlan,
};
use wasm_bindgen::prelude::*;

thread_local! {
    static CONFIG: OnceCell<Result<LabConfig, LabError>> = const { OnceCell::new() };
    // Trim (issue #15): the last plan and the exact body it was computed for.
    static TRIM: RefCell<Option<ProtectedPlan>> = const { RefCell::new(None) };
}

struct ProtectedPlan {
    body: String,
    generation: u64,
    plan: TrimPlan,
    protection: ConceptProtection,
    count: usize,
}

/// Authoritative role publication invalidates cached plans before JS callbacks.
pub(crate) fn invalidate_trim_plan() {
    TRIM.with(|cell| *cell.borrow_mut() = None);
}

fn require_role() -> Result<crate::kg::ActiveRoleSnapshot, String> {
    crate::kg::active_role_snapshot().ok_or_else(|| {
        "role_required: load a nonempty local role/thesaurus to trim with KG protection".into()
    })
}

/// The serde id of an action (`"weakest_sentences"`, ...).
pub fn action_id(action: LabAction) -> String {
    match serde_json::to_value(action) {
        Ok(Value::String(id)) => id,
        // LabAction is a unit enum with snake_case names: always a string.
        _ => unreachable!("LabAction serialises to a string"),
    }
}

/// Parse an action id as produced by [`action_id`].
pub fn parse_action(id: &str) -> Result<LabAction, String> {
    serde_json::from_value(Value::String(id.to_string()))
        .map_err(|_| format!("unknown Lab action: {id}"))
}

/// `[{ id, label }]` for every action, in popover order.
pub fn actions_json() -> Value {
    Value::Array(
        LabAction::ALL
            .iter()
            .map(|&a| json!({ "id": action_id(a), "label": a.label() }))
            .collect(),
    )
}

/// The marks of action `id` over `body`, as JSON, with `config`.
pub fn marks_json(body: &str, config: &LabConfig, id: &str) -> Result<Value, String> {
    let action = parse_action(id)?;
    serde_json::to_value(mark(body, config, action)).map_err(|e| e.to_string())
}

/// Runs `f` with the shared default [`LabConfig`], building it on first use.
pub fn with_config<R>(f: impl FnOnce(&LabConfig) -> R) -> Result<R, String> {
    CONFIG.with(|cell| match cell.get_or_init(LabConfig::with_defaults) {
        Ok(config) => Ok(f(config)),
        Err(e) => Err(e.to_string()),
    })
}

/// The marks of action `id` over the open document's body, as JSON.
pub fn session_marks_json(id: &str) -> Result<Value, String> {
    let body = crate::with_session(|s| s.body().to_string());
    with_config(|config| marks_json(&body, config, id))?
}

fn to_js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

/// `[{ id, label }]` of the six Lab actions, in popover order.
#[wasm_bindgen]
pub fn lab_actions() -> JsValue {
    to_js(&actions_json())
}

/// Runs Lab action `action` (an id from [`lab_actions`]) over the current
/// document body and returns `[{ kind, start, end, score, reason, proposal }]`
/// in UTF-16 offsets. Throws for an unknown action id. Never changes text.
#[wasm_bindgen]
pub fn lab_mark(action: &str) -> Result<JsValue, JsValue> {
    session_marks_json(action)
        .map(|v| to_js(&v))
        .map_err(|e| JsValue::from_str(&e))
}

// ===== Trim levels (issue #15, spec R-8.3 to R-8.5) =====================
//
// Three calls over the open document's body, all on UTF-16 offsets:
//
// * [`trim_plan_json`] computes the engine's [`TrimPlan`] once per version
//   of the body and caches it with that body. It returns
//   `{ totalWords, levels: [{ id, label, target }], cuts: [Cut] }`, every
//   `Cut` in the engine's serde form (`id`, `start`, `end`, `tier`,
//   `first_level`, `score`, `reason`, `words`).
// * [`trim_status`] returns the status card for a level with the kept cut
//   ids excluded, plus the active pieces to fade (`TrimPlan::active`: keeps
//   applied, outer cuts split around kept ranges), so the UI needs one call
//   per step: `{ level, label, words_before, words_after, percent,
//   target_percent, cardText, active: [Cut] }`.
// * [`trim_make_cuts`] returns `{ text, edits: [{ start, end, insert, kind }] }`
//   (the engine's `MadeCuts`) for the editor to apply as one undo step.
//
// Status and make-cuts **refuse** (Err) when the body is no longer the one
// the cached plan was computed for, so stale cuts are never applied: the UI
// then recomputes the plan. Kept ids travel as a JSON array of integers.

/// The serde id of a trim level (`"slight"`, ...).
pub fn level_id(level: TrimLevel) -> String {
    match serde_json::to_value(level) {
        Ok(Value::String(id)) => id,
        // TrimLevel is a unit enum with snake_case names: always a string.
        _ => unreachable!("TrimLevel serialises to a string"),
    }
}

/// Parse a trim level id as produced by [`level_id`].
pub fn parse_level(id: &str) -> Result<TrimLevel, String> {
    serde_json::from_value(Value::String(id.to_string()))
        .map_err(|_| format!("unknown trim level: {id}"))
}

/// Parse kept cut ids: a JSON array of non-negative integers (or "").
pub fn parse_kept(kept: &str) -> Result<Vec<CutId>, String> {
    let kept = kept.trim();
    if kept.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<u32>>(kept)
        .map(|ids| ids.into_iter().map(CutId).collect())
        .map_err(|e| format!("invalid kept cut ids: {e}"))
}

/// The plan JSON for `plan` (see [`trim_plan_json`]).
pub fn plan_json(plan: &TrimPlan) -> Result<Value, String> {
    let levels: Vec<Value> = TrimLevel::ALL
        .iter()
        .map(|&l| {
            json!({
                "id": level_id(l),
                "label": l.label(),
                "target": (100.0 * l.target_fraction()).round() as i64,
            })
        })
        .collect();
    Ok(json!({
        "totalWords": plan.total_words(),
        "levels": levels,
        "cuts": serde_json::to_value(plan.cuts()).map_err(|e| e.to_string())?,
    }))
}

/// The status card and active pieces of `plan` at `level` (see [`trim_status`]).
pub fn status_json(plan: &TrimPlan, level: &str, kept: &str) -> Result<Value, String> {
    let level = parse_level(level)?;
    let kept = parse_kept(kept)?;
    let status = plan.status(level, &kept);
    let mut value = serde_json::to_value(status).map_err(|e| e.to_string())?;
    value["label"] = json!(level.label());
    value["cardText"] = json!(status.card_text());
    value["active"] = serde_json::to_value(plan.active(level, &kept)).map_err(|e| e.to_string())?;
    Ok(value)
}

/// "Make the cuts" over `body` with `plan` (see [`trim_make_cuts`]).
#[cfg(test)]
pub fn make_cuts_json(
    body: &str,
    plan: &TrimPlan,
    level: &str,
    kept: &str,
) -> Result<Value, String> {
    let level = parse_level(level)?;
    let kept = parse_kept(kept)?;
    let made = make_cuts(body, &plan.active(level, &kept));
    serde_json::to_value(made).map_err(|e| e.to_string())
}

/// Compute and cache the plan for the open document's body.
pub fn session_trim_plan_json() -> Result<Value, String> {
    let body = crate::with_session(|s| s.body().to_string());
    let role = require_role()?;
    let protection = role.protection?;
    let count = protection.occurrences(&body)?.len();
    let plan = with_config(|config| protection.plan(&body, config))??;
    let mut value = plan_json(&plan)?;
    value["roleGeneration"] = json!(role.generation.to_string());
    value["kgProtected"] = json!(true);
    value["protectionCount"] = json!(count);
    for (entry, level) in value["levels"]
        .as_array_mut()
        .expect("plan levels")
        .iter_mut()
        .zip(TrimLevel::ALL)
    {
        entry["achieved"] = json!(plan.status(level, &[]).percent);
    }
    TRIM.with(|cell| {
        *cell.borrow_mut() = Some(ProtectedPlan {
            body,
            generation: role.generation,
            plan,
            protection,
            count,
        })
    });
    Ok(value)
}

/// Runs `f` with the cached plan, refusing when the body has changed since.
fn with_fresh_plan<R>(f: impl FnOnce(&ProtectedPlan) -> Result<R, String>) -> Result<R, String> {
    let body = crate::with_session(|s| s.body().to_string());
    let role = require_role()?;
    role.protection?;
    TRIM.with(|cell| match &*cell.borrow() {
        None => Err("no trim plan: compute one first".to_string()),
        Some(planned) if planned.body != body || planned.generation != role.generation => {
            Err("stale trim plan: the text changed since it was computed".to_string())
        }
        Some(plan) => f(plan),
    })
}

/// Status card JSON for the open document (refuses a stale plan).
pub fn session_trim_status(level: &str, kept: &str) -> Result<Value, String> {
    with_fresh_plan(|cached| {
        let active = cached.plan.active(parse_level(level)?, &parse_kept(kept)?);
        cached.protection.validate_ranges(&cached.body, &active)?;
        cached
            .protection
            .validate_retention(&cached.body, &make_cuts(&cached.body, &active))?;
        let mut value = status_json(&cached.plan, level, kept)?;
        value["roleGeneration"] = json!(cached.generation.to_string());
        value["kgProtected"] = json!(true);
        value["protectionCount"] = json!(cached.count);
        Ok(value)
    })
}

/// Make-the-cuts JSON for the open document (refuses a stale plan).
pub fn session_trim_make_cuts(level: &str, kept: &str) -> Result<Value, String> {
    with_fresh_plan(|cached| {
        let active = cached.plan.active(parse_level(level)?, &parse_kept(kept)?);
        cached.protection.validate_ranges(&cached.body, &active)?;
        let made = make_cuts(&cached.body, &active);
        cached.protection.validate_retention(&cached.body, &made)?;
        serde_json::to_value(made).map_err(|e| e.to_string())
    })
}

/// Fresh, fully validated UTF-16 fades for the later atomic Done transaction.
/// Callers cannot supply arbitrary ranges or bypass the active role/body guard.
pub fn protected_trim_ranges(level: &str, kept: &str) -> Result<Vec<(usize, usize)>, String> {
    with_fresh_plan(|cached| {
        let active = cached.plan.active(parse_level(level)?, &parse_kept(kept)?);
        cached.protection.validate_ranges(&cached.body, &active)?;
        cached
            .protection
            .validate_retention(&cached.body, &make_cuts(&cached.body, &active))?;
        Ok(active.iter().map(|c| (c.start, c.end)).collect())
    })
}

/// Promotion-only coverage projection. Preview/walk cuts and Make-cuts retain
/// their original identities and nesting. Stored ghosts are never normalized.
fn promotion_fragments(body: &str, active: &[Cut]) -> Result<Vec<Cut>, String> {
    let mut boundary = 0;
    let mut boundaries = std::collections::BTreeSet::from([0]);
    for ch in body.chars() {
        boundary += ch.len_utf16();
        boundaries.insert(boundary);
    }
    for (i, cut) in active.iter().enumerate() {
        if cut.start >= cut.end
            || !boundaries.contains(&cut.start)
            || !boundaries.contains(&cut.end)
        {
            return Err("unsafe_promotion: invalid UTF-16 fragment".into());
        }
        for prior in &active[..i] {
            let overlap = cut.start < prior.end && prior.start < cut.end;
            let contained = (cut.start <= prior.start && prior.end <= cut.end)
                || (prior.start <= cut.start && cut.end <= prior.end);
            if overlap && !contained {
                return Err("unsafe_promotion: crossing active fragments".into());
            }
        }
    }
    let mut ordered = active.to_vec();
    ordered.sort_by_key(|c| (c.start, std::cmp::Reverse(c.end), c.id.0));
    let mut projected: Vec<Cut> = Vec::new();
    for cut in ordered {
        if projected
            .last()
            .is_some_and(|outer| outer.start <= cut.start && cut.end <= outer.end)
        {
            continue;
        }
        projected.push(cut);
    }
    // Interval union is used only as a proof of exact coverage, never to create
    // stored ghosts. In particular, touching projected fragments stay separate.
    fn coverage(cuts: &[Cut]) -> Vec<(usize, usize)> {
        let mut ranges: Vec<_> = cuts.iter().map(|c| (c.start, c.end)).collect();
        ranges.sort_unstable();
        let mut union: Vec<(usize, usize)> = Vec::new();
        for (start, end) in ranges {
            if let Some(last) = union.last_mut().filter(|last| start <= last.1) {
                last.1 = last.1.max(end);
            } else {
                union.push((start, end));
            }
        }
        union
    }
    if coverage(active) != coverage(&projected) {
        return Err("unsafe_promotion: coverage changed".into());
    }
    Ok(projected)
}

/// Private protected-begin seam: fresh B guards on both original and projected
/// fragments. No public arbitrary-range binding or change to preview semantics.
pub(crate) fn protected_trim_promotion_ranges(
    level: &str,
    kept: &str,
) -> Result<Vec<(usize, usize)>, String> {
    with_fresh_plan(|cached| {
        let active = cached.plan.active(parse_level(level)?, &parse_kept(kept)?);
        cached.protection.validate_ranges(&cached.body, &active)?;
        let original = make_cuts(&cached.body, &active);
        cached
            .protection
            .validate_retention(&cached.body, &original)?;
        let projected = promotion_fragments(&cached.body, &active)?;
        cached
            .protection
            .validate_ranges(&cached.body, &projected)?;
        let made = make_cuts(&cached.body, &projected);
        cached.protection.validate_retention(&cached.body, &made)?;
        if original != made {
            return Err("unsafe_promotion: Make-cuts result changed".into());
        }
        Ok(projected.iter().map(|c| (c.start, c.end)).collect())
    })
}

/// Computes the trim plan for the current document body and returns
/// `{ totalWords, levels, cuts }` (UTF-16 offsets). Align the model with the
/// surface first. Never changes text.
#[wasm_bindgen]
pub fn trim_plan_json() -> Result<JsValue, JsValue> {
    session_trim_plan_json()
        .map(|v| to_js(&v))
        .map_err(|e| JsValue::from_str(&e))
}

/// The status card for trim level `level` (`"slight"`, ...) with the cut
/// ids in `kept` (a JSON array) kept, plus the active pieces to fade. Throws
/// for an unknown level, bad ids, no plan or a stale plan.
#[wasm_bindgen]
pub fn trim_status(level: &str, kept: &str) -> Result<JsValue, JsValue> {
    session_trim_status(level, kept)
        .map(|v| to_js(&v))
        .map_err(|e| JsValue::from_str(&e))
}

/// The typed edits "Make the cuts" applies at `level` with `kept` kept:
/// `{ text, edits: [{ start, end, insert, kind }] }` on the current body's
/// UTF-16 offsets. Throws for a stale plan; never changes text itself.
#[wasm_bindgen]
pub fn trim_make_cuts(level: &str, kept: &str) -> Result<JsValue, JsValue> {
    session_trim_make_cuts(level, kept)
        .map(|v| to_js(&v))
        .map_err(|e| JsValue::from_str(&e))
}

// ===== end trim levels (issue #15) =======================================

#[cfg(test)]
mod promotion_tests {
    use super::*;
    use crate::{
        abort_history_transaction_json, begin_trim_ghost_transaction_json,
        finish_history_transaction_json, with_session,
    };

    const BODY: &str = include_str!("../tests/fixtures/lab/lab.md");
    const ROLE: &str =
        r#"{"name":"trim-public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#;

    // Internal test-only cache publication: the real private production function,
    // not a new public native/WASM API or a separately compiled session model.
    fn publish(body: &str) -> Value {
        with_session(|s| {
            s.open(body);
        });
        crate::kg::load_thesaurus(ROLE).unwrap();
        session_trim_plan_json().unwrap()
    }

    fn cut(start: usize, end: usize, id: u32) -> Cut {
        let mut c = trim_plan(BODY, &LabConfig::with_defaults().unwrap()).cuts()[0].clone();
        c.start = start;
        c.end = end;
        c.id = CutId(id);
        c
    }

    #[test]
    fn containment_duplicates_adjacency_crossing_and_scalar_boundaries() {
        let active = vec![cut(2, 4, 2), cut(0, 10, 1), cut(0, 10, 0), cut(10, 12, 3)];
        let projected = promotion_fragments("abcdefghijkl", &active).unwrap();
        assert_eq!(
            projected
                .iter()
                .map(|c| (c.id.0, c.start, c.end))
                .collect::<Vec<_>>(),
            [(0, 0, 10), (3, 10, 12)]
        );
        for ranges in [
            vec![cut(0, 6, 0), cut(4, 10, 1)],
            vec![cut(0, 12, 0), cut(0, 6, 1), cut(4, 10, 2)],
        ] {
            assert_eq!(
                promotion_fragments("abcdefghijkl", &ranges).unwrap_err(),
                "unsafe_promotion: crossing active fragments"
            );
        }
        assert!(promotion_fragments("a😀中b", &[cut(1, 3, 0), cut(3, 4, 1)]).is_ok());
        assert!(promotion_fragments("a😀中b", &[cut(1, 2, 0)]).is_err());
        assert!(promotion_fragments("abc", &[cut(1, 1, 0)]).is_err());
        assert!(promotion_fragments("abc", &[cut(0, 4, 0)]).is_err());
    }

    #[test]
    fn exact_cached_fixture_begin_abort_finish_manual_and_kept_oracles() {
        for manual in [false, true] {
            for keep in [false, true] {
                let plan_before = publish(BODY);
                if manual {
                    with_session(|s| {
                        s.ghost_ranges_atomic(BODY, &[(0, 1)]).unwrap();
                    });
                }
                let initial = session_trim_status("sharper", "[]").unwrap();
                let kept = if keep {
                    serde_json::to_string(&vec![initial["active"][0]["id"].as_u64().unwrap()])
                        .unwrap()
                } else {
                    "[]".into()
                };
                let preview_before = session_trim_status("sharper", &kept).unwrap();
                let made_before = session_trim_make_cuts("sharper", &kept).unwrap();
                let raw = protected_trim_ranges("sharper", &kept).unwrap();
                let projected = protected_trim_promotion_ranges("sharper", &kept).unwrap();
                let expected: Vec<_> = [
                    (152, 179),
                    (218, 247),
                    (323, 331),
                    (365, 426),
                    (441, 450),
                    (700, 752),
                    (752, 801),
                    (862, 868),
                ]
                .into_iter()
                .filter(|r| !keep || *r != (152, 179))
                .collect();
                assert_eq!(projected, expected);
                assert_eq!(raw.len(), if keep { 11 } else { 12 });
                assert!(projected.windows(2).all(|w| w[0].1 <= w[1].0));
                let before = with_session(|s| s.clone());
                let reservations = with_session(|s| s.ghost_snapshot().reserved_ids);
                // Raw primitive stays strict even on these original active cuts.
                assert_eq!(
                    with_session(|s| s.ghost_ranges_atomic(BODY, &raw))
                        .err()
                        .unwrap(),
                    "independent ghost range overlaps existing ghost or another range"
                );
                assert!(with_session(|s| s.clone()) == before);
                let started = begin_trim_ghost_transaction_json(BODY, "sharper", &kept)
                    .unwrap()
                    .unwrap();
                let token = started["token"].as_str().unwrap();
                let after = with_session(|s| s.clone());
                let ghosts = with_session(|s| s.ghost_snapshot().live);
                assert_eq!(ghosts.len(), projected.len() + usize::from(manual));
                let promoted: Vec<_> = ghosts
                    .iter()
                    .filter(|g| !manual || g.id != "g1")
                    .map(|g| (g.anchor.start, g.anchor.end))
                    .collect();
                assert_eq!(promoted, projected);
                if manual {
                    assert_eq!(
                        (
                            ghosts[0].id.as_str(),
                            ghosts[0].anchor.start,
                            ghosts[0].anchor.end
                        ),
                        ("g1", 0, 1)
                    );
                }
                assert_eq!(
                    session_trim_status("sharper", &kept).unwrap(),
                    preview_before
                );
                assert_eq!(
                    session_trim_make_cuts("sharper", &kept).unwrap(),
                    made_before
                );
                let ack = abort_history_transaction_json(token).unwrap();
                assert!(ack["ok"] == true && ack["rolledBack"] == true && ack["token"] == token);
                assert!(with_session(|s| s.clone()) == before);
                assert_eq!(
                    with_session(|s| s.ghost_snapshot().reserved_ids),
                    reservations
                );
                // No pending checkpoint after acknowledged retirement. Fresh begin
                // allocates the same ghost IDs despite a distinct opaque token.
                let next = begin_trim_ghost_transaction_json(BODY, "sharper", &kept)
                    .unwrap()
                    .unwrap();
                assert_ne!(next["token"], started["token"]);
                assert!(with_session(|s| s.clone()) == after);
                let token = next["token"].as_str().unwrap();
                let ack = finish_history_transaction_json(token).unwrap();
                assert!(ack["ok"] == true && ack["finalized"] == true && ack["token"] == token);
                assert!(with_session(|s| s.clone()) == after);
                assert!(abort_history_transaction_json(token).is_err());
                assert_eq!(session_trim_plan_json().unwrap(), plan_before);
            }
        }
    }

    #[test]
    fn all_levels_all_keeps_preserve_preview_make_cuts_and_split_coverage() {
        publish(BODY);
        TRIM.with(|cell| {
            let borrowed = cell.borrow();
            let cached = borrowed.as_ref().unwrap();
            for level in TrimLevel::ALL {
                for kept in
                    std::iter::once(vec![]).chain(cached.plan.cuts().iter().map(|c| vec![c.id]))
                {
                    let active = cached.plan.active(level, &kept);
                    let crossing = active.iter().enumerate().any(|(i, c)| {
                        active[..i].iter().any(|p| {
                            c.start < p.end
                                && p.start < c.end
                                && !((c.start <= p.start && p.end <= c.end)
                                    || (p.start <= c.start && c.end <= p.end))
                        })
                    });
                    let result = promotion_fragments(BODY, &active);
                    if crossing {
                        assert_eq!(
                            result.unwrap_err(),
                            "unsafe_promotion: crossing active fragments"
                        );
                        let kept_json = serde_json::to_string(&kept).unwrap();
                        let before = with_session(|s| s.clone());
                        let error =
                            begin_trim_ghost_transaction_json(BODY, &level_id(level), &kept_json)
                                .unwrap_err();
                        assert_eq!(error.kind, "protection-refusal");
                        assert_eq!(error.message, "unsafe_promotion: crossing active fragments");
                        assert!(with_session(|s| s.clone()) == before);
                        println!(
                            "strict_crossing level={} kept={:?}",
                            level_id(level),
                            kept.iter().map(|id| id.0).collect::<Vec<_>>()
                        );
                        continue;
                    }
                    let projected = result.unwrap();
                    assert_eq!(make_cuts(BODY, &active), make_cuts(BODY, &projected));
                    cached.protection.validate_ranges(BODY, &projected).unwrap();
                    cached
                        .protection
                        .validate_retention(BODY, &make_cuts(BODY, &projected))
                        .unwrap();
                    for id in &kept {
                        let retained = cached.plan.get(*id).unwrap();
                        assert!(projected
                            .iter()
                            .all(|c| c.end <= retained.start || retained.end <= c.start));
                    }
                }
            }
        });
    }

    #[test]
    fn nine_occurrences_and_unicode_remain_protected() {
        let role = r#"{"name":"B9","data":{"async":{"id":1,"nterm":"async"},"rust async":{"id":2,"nterm":"rust async"},"α":{"id":3,"nterm":"α"},"я":{"id":4,"nterm":"я"},"中":{"id":5,"nterm":"中"}}}"#;
        let body = "Rust\r\n async rust async Rust Async α я 中\n\nThe editor is basically quite useful. It is really just a bit too much for one small team to read.";
        with_session(|s| {
            s.open(body);
        });
        crate::kg::load_thesaurus(role).unwrap();
        let plan = session_trim_plan_json().unwrap();
        assert_eq!(plan["protectionCount"], 9);
        for level in TrimLevel::ALL {
            let id = level_id(level);
            let preview = session_trim_status(&id, "[]").unwrap();
            let made = session_trim_make_cuts(&id, "[]").unwrap();
            let projected = protected_trim_promotion_ranges(&id, "[]").unwrap();
            assert!(projected.windows(2).all(|w| w[0].1 <= w[1].0));
            assert_eq!(session_trim_status(&id, "[]").unwrap(), preview);
            assert_eq!(session_trim_make_cuts(&id, "[]").unwrap(), made);
        }
    }

    #[test]
    fn cached_body_role_finish_guards_and_manual_overlap_preserve_state() {
        publish(BODY);
        let before = with_session(|s| s.clone());
        assert_eq!(
            begin_trim_ghost_transaction_json("wrong", "sharper", "[]")
                .unwrap_err()
                .kind,
            "body-conflict"
        );
        assert!(with_session(|s| s.clone()) == before);
        with_session(|s| {
            s.apply_edit(BODY.encode_utf16().count(), 0, "X").unwrap();
        });
        let changed = with_session(|s| s.clone());
        let changed_body = format!("{BODY}X");
        assert_eq!(
            begin_trim_ghost_transaction_json(&changed_body, "sharper", "[]")
                .unwrap_err()
                .kind,
            "protection-refusal"
        );
        assert!(with_session(|s| s.clone()) == changed);
        publish(BODY);
        let before = with_session(|s| s.clone());
        crate::kg::load_thesaurus(ROLE).unwrap();
        assert_eq!(
            begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
                .unwrap_err()
                .kind,
            "protection-refusal"
        );
        assert!(with_session(|s| s.clone()) == before);
        session_trim_plan_json().unwrap();
        let started = begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
            .unwrap()
            .unwrap();
        crate::kg::clear_thesaurus();
        let token = started["token"].as_str().unwrap();
        assert_eq!(
            finish_history_transaction_json(token).unwrap_err().kind,
            "role-conflict"
        );
        let ack = abort_history_transaction_json(token).unwrap();
        assert!(ack["ok"] == true && ack["rolledBack"] == true && ack["token"] == token);
        assert!(with_session(|s| s.clone()) == before);
        assert!(crate::kg::active_role_snapshot().is_none());
        publish(BODY);
        with_session(|s| {
            s.ghost_ranges_atomic(BODY, &[(365, 366)]).unwrap();
        });
        let before = with_session(|s| s.clone());
        assert_eq!(
            begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
                .unwrap_err()
                .kind,
            "model-refusal"
        );
        assert!(with_session(|s| s.clone()) == before);
        // A refusal leaves the bridge available, with exact acknowledged abort.
        let probe = crate::begin_history_replay_json(BODY, BODY, "[]").unwrap();
        let token = probe["token"].as_str().unwrap();
        let ack = abort_history_transaction_json(token).unwrap();
        assert!(ack["ok"] == true && ack["rolledBack"] == true && ack["token"] == token);
        assert!(with_session(|s| s.clone()) == before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_listed_in_popover_order_with_spec_labels() {
        let actions = actions_json();
        let list = actions.as_array().unwrap();
        assert_eq!(list.len(), 6);
        let ids: Vec<&str> = list.iter().map(|a| a["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            [
                "typos_and_punctuation",
                "weakest_sentences",
                "long_sentences",
                "convoluted_sentences",
                "off_tone",
                "hedges_and_filler",
            ]
        );
        assert_eq!(list[0]["label"], "Fix punctuation and typos");
        assert_eq!(list[4]["label"], "Mark words that don't fit the tone");
        assert_eq!(list[5]["label"], "Mark hedges and filler");
    }

    #[test]
    fn ids_round_trip_and_unknown_ids_are_rejected() {
        for action in LabAction::ALL {
            assert_eq!(parse_action(&action_id(action)), Ok(action));
        }
        assert!(parse_action("rewrite_everything").is_err());
        assert!(parse_action("").is_err());
        assert!(parse_action("WeakestSentences").is_err());
    }

    #[test]
    fn marks_match_the_engine_and_use_utf16_offsets() {
        let body = "Caf\u{e9} owners recieve it.  It is basically fine.";
        let marks = with_config(|c| marks_json(body, c, "typos_and_punctuation"))
            .unwrap()
            .unwrap();
        let expected = with_config(|c| mark(body, c, LabAction::TyposAndPunctuation)).unwrap();
        assert_eq!(marks, serde_json::to_value(&expected).unwrap());
        let first = &marks.as_array().unwrap()[0];
        assert_eq!(first["kind"], "typo");
        assert_eq!(
            (first["start"].as_u64(), first["end"].as_u64()),
            (Some(12), Some(19))
        );
        assert_eq!(first["proposal"], "receive");
        assert!(first["reason"].as_str().unwrap().starts_with("typo:"));
    }

    #[test]
    fn marks_never_change_the_body_and_protected_text_is_skipped() {
        let body = "# Maybe a heading\n\n```\nrecieve\n```\n\nIt is basically fine.\n";
        let hedges = with_config(|c| marks_json(body, c, "hedges_and_filler"))
            .unwrap()
            .unwrap();
        let list = hedges.as_array().unwrap();
        assert_eq!(list.len(), 1, "only the prose filler: {hedges}");
        assert_eq!(list[0]["kind"], "filler");
        let typos = with_config(|c| marks_json(body, c, "typos_and_punctuation"))
            .unwrap()
            .unwrap();
        assert_eq!(typos, json!([]), "code is protected");
    }

    #[test]
    fn unknown_action_is_an_error_not_a_panic() {
        let err = with_config(|c| marks_json("Text.", c, "nope"))
            .unwrap()
            .unwrap_err();
        assert!(err.contains("unknown Lab action"));
    }

    #[test]
    fn session_marks_read_the_open_document() {
        crate::with_session(|s| {
            s.open("We recieve it.\n");
        });
        let marks = session_marks_json("typos_and_punctuation").unwrap();
        assert_eq!(marks[0]["proposal"], "receive");
        assert_eq!(marks[0]["start"], 3);
        assert!(session_marks_json("bogus").is_err());
        // The body is untouched by marking.
        assert_eq!(
            crate::with_session(|s| s.body().to_string()),
            "We recieve it.\n"
        );
    }

    /// The browser fixture: every action marks something, and nothing in
    /// the heading or the fenced block (both hold listed words) is marked.
    #[test]
    fn fixture_exercises_every_action_outside_protected_text() {
        let body = include_str!("../tests/fixtures/lab/lab.md");
        let units: Vec<u16> = body.encode_utf16().collect();
        let utf16 = |s: &str| s.encode_utf16().count();
        let heading_end = utf16(&body[..body.find('\n').unwrap()]);
        let fence_start = utf16(&body[..body.find("```").unwrap()]);
        let fence_end = utf16(&body[..body.rfind("```").unwrap() + 3]);
        for action in LabAction::ALL {
            let marks = with_config(|c| mark(body, c, action)).unwrap();
            assert!(!marks.is_empty(), "{action:?} marks nothing");
            for m in &marks {
                let text = String::from_utf16(&units[m.start..m.end]).unwrap();
                assert!(
                    m.start >= heading_end,
                    "{action:?} marked the heading: {text}"
                );
                assert!(
                    m.end <= fence_start || m.start >= fence_end,
                    "{action:?} marked code: {text}"
                );
            }
        }
        let typos = with_config(|c| mark(body, c, LabAction::TyposAndPunctuation)).unwrap();
        let proposals: Vec<_> = typos.iter().map(|m| m.proposal.as_deref()).collect();
        assert_eq!(proposals, [Some("receive"), Some(",")]);
    }
    // ---- Trim (issue #15) ----

    const TRIM_BODY: &str = "The editor, which owns its DOM, is the only target here today.";

    #[test]
    fn trim_levels_round_trip_and_bad_input_is_an_error() {
        for level in TrimLevel::ALL {
            assert_eq!(parse_level(&level_id(level)), Ok(level));
        }
        assert_eq!(level_id(TrimLevel::Half), "half");
        assert!(parse_level("quarter").is_err());
        assert_eq!(parse_kept(""), Ok(vec![]));
        assert_eq!(parse_kept("[2, 0]"), Ok(vec![CutId(2), CutId(0)]));
        assert!(parse_kept("[-1]").is_err());
        assert!(parse_kept("{}").is_err());
    }

    #[test]
    fn trim_plan_json_lists_levels_and_engine_cuts() {
        let plan = with_config(|c| trim_plan(TRIM_BODY, c)).unwrap();
        let value = plan_json(&plan).unwrap();
        assert_eq!(value["totalWords"], 12);
        let levels: Vec<(&str, &str, i64)> = value["levels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                (
                    l["id"].as_str().unwrap(),
                    l["label"].as_str().unwrap(),
                    l["target"].as_i64().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            levels,
            [
                ("original", "Original", 0),
                ("slight", "Slight trim", 10),
                ("tighten", "Tighten more", 20),
                ("sharper", "Even sharper", 30),
                ("half", "Cut in half", 50),
            ]
        );
        assert_eq!(value["cuts"], serde_json::to_value(plan.cuts()).unwrap());
        let first = &value["cuts"][0];
        assert!(first["tier"].is_string() && first["first_level"].is_string());
        assert!(first["reason"].is_string());
    }

    #[test]
    fn trim_status_and_make_cuts_equal_the_engine() {
        let plan = with_config(|c| trim_plan(TRIM_BODY, c)).unwrap();
        let status = status_json(&plan, "sharper", "[]").unwrap();
        assert_eq!(status["cardText"], "12 \u{2192} 8 words \u{b7} \u{2212}33%");
        assert_eq!(status["words_before"], 12);
        assert_eq!(status["words_after"], 8);
        assert_eq!(status["label"], "Even sharper");
        assert_eq!(
            status["active"],
            serde_json::to_value(plan.active(TrimLevel::Sharper, &[])).unwrap()
        );
        let made = make_cuts_json(TRIM_BODY, &plan, "sharper", "[]").unwrap();
        assert_eq!(made["text"], "The editor is the only target here today.");
        let edits = made["edits"].as_array().unwrap();
        assert!(!edits.is_empty());
        assert_eq!(edits[0]["kind"], "cut");
        // Keeping every active cut leaves nothing to cut.
        let ids: Vec<u32> = plan
            .active(TrimLevel::Sharper, &[])
            .iter()
            .map(|c| c.id.0)
            .collect();
        let kept = serde_json::to_string(&ids).unwrap();
        let status = status_json(&plan, "sharper", &kept).unwrap();
        assert_eq!(status["words_after"], 12);
        assert_eq!(status["active"], json!([]));
        let made = make_cuts_json(TRIM_BODY, &plan, "sharper", &kept).unwrap();
        assert_eq!(made["text"], TRIM_BODY);
        assert_eq!(made["edits"], json!([]));
        assert!(status_json(&plan, "nope", "[]").is_err());
        assert!(make_cuts_json(TRIM_BODY, &plan, "slight", "not json").is_err());
    }

    #[test]
    fn session_trim_refuses_a_stale_plan_and_recomputes() {
        crate::kg::load_thesaurus(
            r#"{"name":"public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#,
        )
        .unwrap();
        crate::with_session(|s| {
            s.open(TRIM_BODY);
        });
        TRIM.with(|cell| *cell.borrow_mut() = None);
        assert!(session_trim_status("slight", "[]")
            .unwrap_err()
            .contains("no trim plan"));
        session_trim_plan_json().unwrap();
        let status = session_trim_status("sharper", "[]").unwrap();
        assert_eq!(status["words_after"], 8);
        // The body changes: both calls refuse until the plan is recomputed.
        crate::with_session(|s| {
            s.sync_body("The editor is here.");
        });
        assert!(session_trim_status("sharper", "[]")
            .unwrap_err()
            .contains("stale"));
        assert!(session_trim_make_cuts("sharper", "[]")
            .unwrap_err()
            .contains("stale"));
        session_trim_plan_json().unwrap();
        assert!(session_trim_make_cuts("sharper", "[]").is_ok());
        // Planning never changes the body.
        assert_eq!(
            crate::with_session(|s| s.body().to_string()),
            "The editor is here."
        );
    }

    /// The browser fixture has cuts at every level, including a cut nested
    /// inside a larger cut (the outer-cut resolution the keep test relies on).
    #[test]
    fn lab_fixture_has_trim_cuts_at_every_level_and_a_nested_cut() {
        let body = include_str!("../tests/fixtures/lab/lab.md");
        let plan = with_config(|c| trim_plan(body, c)).unwrap();
        for level in &TrimLevel::ALL[1..] {
            assert!(plan.faded(*level).count() > 0, "{level:?} fades nothing");
        }
        let half: Vec<_> = plan.faded(TrimLevel::Half).collect();
        let nested = half.iter().any(|inner| {
            half.iter().any(|outer| {
                outer.id != inner.id && outer.start <= inner.start && inner.end <= outer.end
            })
        });
        assert!(nested, "no nested cut at Half");
    }

    #[test]
    fn protection_role_and_body_guards_are_atomic() {
        const ROLE: &str =
            r#"{"name":"public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#;
        crate::kg::clear_thesaurus();
        crate::with_session(|s| s.open(TRIM_BODY));
        assert!(session_trim_plan_json()
            .unwrap_err()
            .contains("role_required"));
        crate::kg::load_thesaurus(ROLE).unwrap();
        let planned = session_trim_plan_json().unwrap();
        assert_eq!(planned["kgProtected"], true);
        assert_eq!(planned["protectionCount"], 0);
        assert_eq!(
            session_trim_make_cuts("sharper", "[]").unwrap()["text"],
            "The editor is the only target here today."
        );
        assert!(crate::kg::load_thesaurus("invalid").is_err());
        assert!(session_trim_status("sharper", "[]").is_ok());
        crate::kg::load_thesaurus(ROLE).unwrap();
        assert!(session_trim_make_cuts("sharper", "[]").is_err());
        let changed = session_trim_plan_json().unwrap();
        assert_ne!(planned["roleGeneration"], changed["roleGeneration"]);
        let token = crate::kg::prepare_thesaurus(ROLE).unwrap();
        crate::kg::commit_prepared_thesaurus(&token).unwrap();
        assert!(protected_trim_ranges("half", "[]").is_err());
        crate::with_session(|s| s.sync_body("Straße"));
        assert_eq!(session_trim_plan_json().unwrap()["kgProtected"], true);
        assert_eq!(
            session_trim_make_cuts("sharper", "[]").unwrap()["text"],
            "Straße"
        );
        assert_eq!(crate::with_session(|s| s.body().to_owned()), "Straße");
        crate::kg::load_thesaurus(
            r#"{"name":"unsupported","data":{"Straße":{"id":2,"nterm":"strasse"}}}"#,
        )
        .unwrap();
        assert_eq!(session_trim_plan_json().unwrap()["protectionCount"], 1);
        assert_eq!(
            session_trim_make_cuts("sharper", "[]").unwrap()["text"],
            "Straße"
        );
        crate::kg::clear_thesaurus();
        assert!(protected_trim_ranges("half", "[]")
            .unwrap_err()
            .contains("role_required"));
    }
}
