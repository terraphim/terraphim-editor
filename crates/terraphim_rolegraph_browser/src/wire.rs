use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use terraphim_types::{Edge, IndexedDocument};

pub const IMPORT_BYTES: usize = 16 * 1024 * 1024;
pub const DOCUMENT_BYTES: usize = 1024 * 1024;
pub const DOCUMENTS: usize = 1000;
pub const NODES: usize = 10_000;
pub const EDGES: usize = 100_000;
pub const QUERY_BYTES: usize = 4096;
pub const TAGS: usize = 1000;

#[derive(Debug, thiserror::Error)]
#[error("{code}: {message}")]
pub struct BridgeError {
    pub code: &'static str,
    pub message: &'static str,
}
pub(crate) type Result<T> = std::result::Result<T, BridgeError>;
pub(crate) fn error(code: &'static str, message: &'static str) -> BridgeError {
    BridgeError { code, message }
}
pub(crate) fn invalid() -> BridgeError {
    error("invalid_input", "Invalid request payload")
}
pub(crate) fn snapshot_error() -> BridgeError {
    error(
        "invalid_snapshot",
        "Invalid graph accounting or references; rebuild from original source",
    )
}
pub(crate) fn bound(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(error(
            "resource_limit",
            "Operation exceeds the supported resource limits",
        ))
    }
}
pub(crate) fn decimal(s: &str) -> Result<u64> {
    if s.is_empty() || (s.len() > 1 && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    s.parse().map_err(|_| invalid())
}
pub(crate) fn parse<T: serde::de::DeserializeOwned>(v: &serde_json::value::RawValue) -> Result<T> {
    crate::bounded_json::validate(v.get())?;
    serde_json::from_str(v.get()).map_err(|_| invalid())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Request<'a> {
    pub version: u32,
    pub request_id: String,
    pub generation: String,
    #[serde(deserialize_with = "required_revision")]
    pub expected_revision: Option<String>,
    pub op: String,
    #[serde(borrow)]
    pub payload: &'a serde_json::value::RawValue,
}

fn required_revision<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceDocument {
    pub id: String,
    pub title: String,
    pub body: String,
}
impl SourceDocument {
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(invalid());
        }
        bound(
            self.id.len() <= QUERY_BYTES
                && self.title.len().saturating_add(self.body.len()) <= DOCUMENT_BYTES,
        )
    }
    pub fn upstream(&self) -> terraphim_types::Document {
        terraphim_types::Document {
            id: self.id.clone(),
            title: self.title.clone(),
            body: self.body.clone(),
            ..Default::default()
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Snapshot {
    pub version: u32,
    pub rolegraph_version: String,
    pub graph_json: String,
    pub documents: Vec<SourceDocument>,
    pub generation: String,
    pub revision: String,
    // Upstream SerializableRoleGraph omits the configured threshold.
    #[serde(default = "default_threshold")]
    pub trigger_threshold: f64,
}
pub(crate) fn default_threshold() -> f64 {
    0.3
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Page {
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "page_limit")]
    pub limit: usize,
}
fn page_limit() -> usize {
    100
}
impl Page {
    pub fn validate(&self) -> Result<()> {
        bound(self.limit <= 100)
    }
}

pub(crate) fn edge_dto(edge: &Edge) -> Value {
    let docs: BTreeMap<_, _> = edge
        .doc_hash
        .iter()
        .map(|(id, rank)| (id.clone(), rank.to_string()))
        .collect();
    json!({"id":edge.id.to_string(),"rank":edge.rank.to_string(),"doc_hash":docs})
}
pub(crate) fn document_dto(doc: &IndexedDocument) -> Value {
    let mut edges: Vec<_> = doc.matched_edges.iter().collect();
    edges.sort_by_key(|e| e.id);
    json!({"id":doc.id,"rank":doc.rank.to_string(),"nodes":doc.nodes.iter().map(u64::to_string).collect::<Vec<_>>(),"tags":doc.tags,"matched_edges":edges.into_iter().map(edge_dto).collect::<Vec<_>>()})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LearningDocument {
    pub id: String,
    pub rank: String,
    pub nodes: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub matched_edges: Vec<LearningEdge>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct LearningEdge {
    pub id: String,
    pub rank: String,
    pub doc_hash: BTreeMap<String, String>,
}
impl LearningDocument {
    pub fn upstream(self) -> Result<IndexedDocument> {
        if self.id.is_empty() {
            return Err(invalid());
        }
        bound(
            self.id.len() <= QUERY_BYTES
                && self.nodes.len() <= NODES
                && self.tags.len() <= TAGS
                && self.tags.iter().map(String::len).sum::<usize>() <= DOCUMENT_BYTES
                && self.matched_edges.len() <= EDGES,
        )?;
        let mut edges = Vec::new();
        for e in self.matched_edges {
            edges.push(Edge {
                id: decimal(&e.id)?,
                rank: decimal(&e.rank)?,
                doc_hash: e
                    .doc_hash
                    .into_iter()
                    .map(|(id, r)| Ok((id, decimal(&r)?)))
                    .collect::<Result<_>>()?,
            });
        }
        Ok(IndexedDocument {
            id: self.id,
            rank: decimal(&self.rank)?,
            nodes: self
                .nodes
                .iter()
                .map(|n| decimal(n))
                .collect::<Result<_>>()?,
            tags: self.tags,
            matched_edges: edges,
            quality_score: None,
        })
    }
}
