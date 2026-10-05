//! Shared fixtures: real KG lists, a real thesaurus and the rolegraph JSON
//! produced by `terraphim_rolegraph` (see `rolegraph_conformance.rs`). No
//! mocks.
#![allow(dead_code)]

use terraphim_automata::load_thesaurus_from_json;
use terraphim_lab::{LabConfig, LabMark, RoleGraphData, RoleKnowledge};
use terraphim_types::Thesaurus;

pub const THREE_MEN: &str = include_str!("../fixtures/three-men-ch1.md");
pub const WALDEN: &str = include_str!("../fixtures/walden-economy.md");
pub const ZED: &str = include_str!("../fixtures/zed-plugin-fit.md");
pub const FIXTURES: [(&str, &str); 3] = [
    ("three-men-ch1", THREE_MEN),
    ("walden-economy", WALDEN),
    ("zed-plugin-fit", ZED),
];

pub fn role_thesaurus() -> Thesaurus {
    load_thesaurus_from_json(include_str!("../fixtures/role-thesaurus.json")).unwrap()
}

pub fn role_graph() -> RoleGraphData {
    RoleGraphData::from_json(include_str!("../fixtures/rolegraph.json")).unwrap()
}

/// Default lists plus the fixture role (thesaurus and rolegraph).
pub fn role_config() -> LabConfig {
    LabConfig::with_defaults()
        .unwrap()
        .with_role(RoleKnowledge::new(&role_thesaurus(), role_graph()).unwrap())
}

/// The text a mark covers, sliced by UTF-16 offsets.
pub fn covered(body: &str, m: &LabMark) -> String {
    let units: Vec<u16> = body.encode_utf16().collect();
    String::from_utf16(&units[m.start..m.end]).expect("mark on a char boundary")
}

/// UTF-16 range of the `nth` (0-based) occurrence of `needle` in `body`.
pub fn range16(body: &str, needle: &str, nth: usize) -> (usize, usize) {
    let (byte, _) = body
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle:?} occurrence {nth} not in body"));
    let start = body[..byte].encode_utf16().count();
    (start, start + needle.encode_utf16().count())
}

/// `(kind, covered text, proposal)` triples, in mark order.
pub fn summary(
    body: &str,
    marks: &[LabMark],
) -> Vec<(terraphim_lab::MarkKind, String, Option<String>)> {
    marks
        .iter()
        .map(|m| (m.kind, covered(body, m), m.proposal.clone()))
        .collect()
}
