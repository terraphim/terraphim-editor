//! The selected role's knowledge: its thesaurus (concept matcher) and its
//! rolegraph (concept rank and co-occurrence edges).
//!
//! `terraphim_rolegraph` 2.0 cannot be compiled for `wasm32-unknown-unknown`
//! (it enables `tokio` with `full`, which pulls in `mio`), and
//! `terraphim_rolegraph_core` 0.1.1 carries no graph types. So the engine
//! reads the rolegraph's **data** — the `terraphim_types::{Node, Edge}` maps
//! a `RoleGraph` serialises — and does the small amount of graph scoring it
//! needs here. [`RoleGraphData::from_json`] accepts the JSON of
//! `terraphim_rolegraph::SerializableRoleGraph` unchanged (fields the engine
//! does not need are ignored).

use std::collections::{BTreeMap, BTreeSet};

use ahash::AHashMap;
use serde::Deserialize;
use terraphim_types::{Edge, Node, Thesaurus};

use crate::LabError;
use crate::lists::{ConceptMatcher, Hit};

/// Edge id for the node pair `(x, y)`, exactly as `terraphim_rolegraph`
/// computes it (`magic_pair`, a Szudzik pairing). It is **ordered**:
/// `edge_id(3, 1) != edge_id(1, 3)`, because a rolegraph keys an edge by the
/// order in which the two concepts occurred. Returns `None` on overflow,
/// which can only mean "no such edge".
pub fn edge_id(x: u64, y: u64) -> Option<u64> {
    if x >= y {
        x.checked_mul(x)?.checked_add(x)?.checked_add(y)
    } else {
        y.checked_mul(y)?.checked_add(x)
    }
}

/// The parts of a serialised rolegraph the engine scores with.
///
/// Node ids are the thesaurus' concept ids (`NormalizedTerm::id`). A node's
/// `rank` counts its co-occurrences in the role's indexed documents; its
/// edges link it to the concepts it co-occurred with.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RoleGraphData {
    /// Concept nodes by id.
    #[serde(default)]
    pub nodes: AHashMap<u64, Node>,
    /// Co-occurrence edges by [`edge_id`].
    #[serde(default)]
    pub edges: AHashMap<u64, Edge>,
}

impl RoleGraphData {
    /// Parse `terraphim_rolegraph::SerializableRoleGraph` JSON (or any JSON
    /// object with `nodes` and `edges` in that shape).
    pub fn from_json(json: &str) -> Result<RoleGraphData, LabError> {
        serde_json::from_str(json).map_err(|e| LabError::RoleGraph(e.to_string()))
    }

    /// Build from node and edge maps directly.
    pub fn from_parts(nodes: AHashMap<u64, Node>, edges: AHashMap<u64, Edge>) -> RoleGraphData {
        RoleGraphData { nodes, edges }
    }

    /// The rank of concept `id` (0 when the graph has no such node).
    pub fn rank(&self, id: u64) -> u64 {
        self.nodes.get(&id).map(|n| n.rank).unwrap_or(0)
    }

    /// True when the graph has an edge between `a` and `b`, in either order.
    pub fn connected(&self, a: u64, b: u64) -> bool {
        a != b
            && [edge_id(a, b), edge_id(b, a)]
                .into_iter()
                .flatten()
                .any(|e| self.edges.contains_key(&e))
    }

    /// The highest node rank in the graph (0 for an empty graph).
    pub fn max_rank(&self) -> u64 {
        self.nodes.values().map(|n| n.rank).max().unwrap_or(0)
    }
}

/// The selected role: its thesaurus compiled into a concept matcher, plus its
/// rolegraph data.
#[derive(Debug, Clone)]
pub struct RoleKnowledge {
    pub(crate) concepts: ConceptMatcher,
    pub(crate) graph: RoleGraphData,
    max_rank_ln: f64,
}

impl RoleKnowledge {
    /// Compile the role's thesaurus and attach its rolegraph data. A role
    /// with a thesaurus but no indexed documents can pass
    /// `RoleGraphData::default()`: concepts then count by presence only.
    pub fn new(thesaurus: &Thesaurus, graph: RoleGraphData) -> Result<RoleKnowledge, LabError> {
        let max_rank_ln = (graph.max_rank() as f64).ln_1p();
        Ok(RoleKnowledge {
            concepts: ConceptMatcher::from_thesaurus(thesaurus)?,
            graph,
            max_rank_ln,
        })
    }

    /// Normalised rank of concept `id` in `[0, 1]`: `ln(1 + rank)` over
    /// `ln(1 + max rank in the graph)`.
    pub(crate) fn rank_norm(&self, id: u64) -> f64 {
        if self.max_rank_ln > 0.0 {
            (self.graph.rank(id) as f64).ln_1p() / self.max_rank_ln
        } else {
            0.0
        }
    }
}

/// Weight of mere presence of a role concept in a sentence, before rank and
/// connectivity are considered.
pub const CONCEPT_PRESENCE: f64 = 0.25;
/// Share of the remaining concept strength that comes from concept rank.
pub const CONCEPT_RANK_WEIGHT: f64 = 0.5;
/// Share of the remaining concept strength that comes from connectivity to
/// the document's other concepts.
pub const CONCEPT_CONNECTIVITY_WEIGHT: f64 = 0.5;

/// Per-sentence rolegraph strength.
#[derive(Debug, Clone, Default)]
pub(crate) struct GraphScore {
    /// Noisy-OR of the strengths of the sentence's distinct concepts, in
    /// `[0, 1]`; 0 for a sentence touching no role concept.
    pub strength: f64,
    /// Distinct concepts in the sentence, by name (sorted).
    pub concepts: Vec<String>,
}

/// Score every sentence by its concepts in the role's rolegraph.
///
/// `hits` are the role concept matches over the whole body (original byte
/// offsets, sorted), and `sentences` the sorted, disjoint sentence ranges.
pub(crate) fn graph_scores(
    role: &RoleKnowledge,
    hits: &[Hit],
    sentences: &[(usize, usize)],
) -> Vec<GraphScore> {
    let mut per_sentence: Vec<BTreeMap<u64, String>> = vec![BTreeMap::new(); sentences.len()];
    for hit in hits {
        // Sentences are sorted and disjoint: find the one holding the hit.
        let i = sentences.partition_point(|s| s.1 <= hit.start);
        if let Some(&(s, e)) = sentences.get(i)
            && s <= hit.start
            && hit.end <= e
        {
            per_sentence[i]
                .entry(hit.term.id)
                .or_insert_with(|| hit.term.value.as_str().to_string());
        }
    }
    let doc_concepts: BTreeSet<u64> = per_sentence
        .iter()
        .flat_map(|m| m.keys().copied())
        .collect();
    let others = doc_concepts.len().saturating_sub(1);
    let strength_of = |c: u64| -> f64 {
        let conn = if others == 0 {
            0.0
        } else {
            doc_concepts
                .iter()
                .filter(|&&d| role.graph.connected(c, d))
                .count() as f64
                / others as f64
        };
        let quality = CONCEPT_RANK_WEIGHT * role.rank_norm(c) + CONCEPT_CONNECTIVITY_WEIGHT * conn;
        CONCEPT_PRESENCE + (1.0 - CONCEPT_PRESENCE) * quality
    };
    let strengths: BTreeMap<u64, f64> = doc_concepts.iter().map(|&c| (c, strength_of(c))).collect();
    per_sentence
        .into_iter()
        .map(|m| {
            let miss: f64 = m.keys().map(|c| 1.0 - strengths[c]).product();
            let mut concepts: Vec<String> = m.into_values().collect();
            concepts.sort();
            concepts.dedup();
            GraphScore {
                strength: if concepts.is_empty() { 0.0 } else { 1.0 - miss },
                concepts,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_id_matches_upstream_magic_pair() {
        // terraphim_rolegraph::magic_pair: x >= y ? x*x + x + y : y*y + x
        assert_eq!(edge_id(3, 1), Some(13));
        assert_eq!(edge_id(1, 3), Some(10));
        assert_eq!(edge_id(0, 0), Some(0));
        assert_eq!(edge_id(u64::MAX, 1), None);
    }

    #[test]
    fn connectivity_checks_both_orders() {
        let mut edges = AHashMap::new();
        let id = edge_id(5, 2).unwrap();
        edges.insert(id, Edge::new(id, "doc".into()));
        let g = RoleGraphData::from_parts(AHashMap::new(), edges);
        assert!(g.connected(5, 2));
        assert!(g.connected(2, 5));
        assert!(!g.connected(2, 2));
        assert!(!g.connected(1, 2));
    }

    #[test]
    fn serialisable_rolegraph_extra_fields_are_ignored() {
        let json = r#"{"role":"r","nodes":{"1":{"id":1,"rank":2,"connected_with":[10]}},
            "edges":{"10":{"id":10,"rank":1,"doc_hash":{"d":1}}},"thesaurus":{"name":"x","data":{}},
            "aho_corasick_values":[],"ac_reverse_nterm":{},"trigger_descriptions":{},"pinned_node_ids":[]}"#;
        let g = RoleGraphData::from_json(json).unwrap();
        assert_eq!(g.rank(1), 2);
        assert_eq!(g.max_rank(), 2);
        assert_eq!(g.rank(9), 0);
    }
}
