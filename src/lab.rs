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
//! The caller must align the model body with the surface first
//! (`MarkdownEditor.alignDocumentModel()`), exactly as `annotations()` does.
//!
//! The [`LabConfig`] (the embedded default style and typo lists, no role) is
//! built once, on the first [`lab_mark`] call, so plain editing and page load
//! pay nothing for it. Role support (`LabConfig::with_role`) is a follow-up:
//! it needs the active role's thesaurus and rolegraph in the browser.

use std::cell::OnceCell;

use serde_json::{json, Value};
use terraphim_lab::{mark, LabAction, LabConfig, LabError};
use wasm_bindgen::prelude::*;

thread_local! {
    static CONFIG: OnceCell<Result<LabConfig, LabError>> = const { OnceCell::new() };
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
}
