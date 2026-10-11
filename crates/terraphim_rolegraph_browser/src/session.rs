use crate::{validation, wire::*};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use terraphim_rolegraph::{RoleGraph, SerializableRoleGraph};
use terraphim_types::{LogicalOperator, RoleName, Thesaurus};

struct Active {
    graph: RoleGraph,
    corpus: BTreeMap<String, SourceDocument>,
    threshold: f64,
}
struct Prepared {
    token: String,
    generation: u64,
    revision: u64,
    active: Active,
}

/// A single owner's graph and source corpus. All mutations publish only validated candidates.
#[derive(Default)]
pub struct GraphSession {
    active: Option<Active>,
    prepared: Option<Prepared>,
    generation: u64,
    revision: u64,
    token_counter: u64,
}

impl GraphSession {
    /// Dispatch a versioned request. Valid envelopes receive typed success/refusal JSON;
    /// unparseable/oversized envelopes return a transport-level BridgeError.
    pub fn dispatch(&mut self, request_json: &str) -> Result<String> {
        // Escaped import strings can occupy six times their decoded byte count.
        bound(request_json.len() <= IMPORT_BYTES * 6 + QUERY_BYTES)?;
        let request: Request = serde_json::from_str(request_json).map_err(|_| invalid())?;
        let outcome = self.execute(&request);
        let mut response = json!({"version":1,"requestId":request.request_id,"generation":self.generation.to_string(),"revision":self.revision.to_string(),"ok":outcome.is_ok()});
        match outcome {
            Ok(result) => response["result"] = result,
            Err(e) => response["error"] = json!({"code":e.code,"message":e.message}),
        }
        serde_json::to_string(&response).map_err(|_| invalid())
    }

    fn execute(&mut self, r: &Request) -> Result<Value> {
        if r.version != 1 {
            return Err(error(
                "unsupported_version",
                "Only protocol version 1 is supported",
            ));
        }
        if r.request_id.len() > QUERY_BYTES || !r.payload.get().trim_start().starts_with('{') {
            return Err(invalid());
        }
        if decimal(&r.generation)? != self.generation {
            return Err(error("stale_generation", "Role generation has changed"));
        }
        if let Some(expected) = &r.expected_revision
            && decimal(expected)? != self.revision
        {
            return Err(error("stale_revision", "Graph revision has changed"));
        }
        match r.op.as_str() {
            "prepare_role" => {
                let p: PrepareRole = parse(r.payload)?;
                bound(p.thesaurus_text.len() <= IMPORT_BYTES)?;
                crate::bounded_json::validate(&p.thesaurus_text)?;
                let t: Thesaurus =
                    serde_json::from_str(&p.thesaurus_text).map_err(|_| invalid())?;
                validation::thesaurus(&t)?;
                let name = p.role_name.unwrap_or_else(|| t.name().to_owned());
                bound(name.len() <= QUERY_BYTES)?;
                let graph = RoleGraph::new_sync(RoleName::new(&name), t).map_err(|_| invalid())?;
                self.prepare(Active {
                    graph,
                    corpus: BTreeMap::new(),
                    threshold: default_threshold(),
                })
            }
            "prepare_snapshot" => {
                let p: PrepareSnapshot = parse(r.payload)?;
                bound(p.snapshot_text.len() <= IMPORT_BYTES)?;
                crate::bounded_json::validate(&p.snapshot_text).map_err(|e| {
                    if e.code == "resource_limit" {
                        e
                    } else {
                        snapshot_error()
                    }
                })?;
                let s: Snapshot =
                    serde_json::from_str(&p.snapshot_text).map_err(|_| snapshot_error())?;
                if s.version != 1 || s.rolegraph_version != "2.0.0" {
                    return Err(error(
                        "unsupported_version",
                        "Unsupported snapshot or RoleGraph version",
                    ));
                }
                decimal(&s.generation).map_err(|_| snapshot_error())?;
                decimal(&s.revision).map_err(|_| snapshot_error())?;
                crate::bounded_json::validate(&s.graph_json).map_err(|e| {
                    if e.code == "resource_limit" {
                        e
                    } else {
                        snapshot_error()
                    }
                })?;
                let graph: SerializableRoleGraph =
                    serde_json::from_str(&s.graph_json).map_err(|_| snapshot_error())?;
                let mut corpus = BTreeMap::new();
                for d in s.documents {
                    if corpus.insert(d.id.clone(), d).is_some() {
                        return Err(snapshot_error());
                    }
                }
                validation::graph(&graph, &corpus, s.trigger_threshold).map_err(|e| {
                    if e.code == "invalid_input" {
                        snapshot_error()
                    } else {
                        e
                    }
                })?;
                let descriptions = graph.trigger_descriptions.clone();
                let pinned = graph.pinned_node_ids.clone();
                let mut graph =
                    RoleGraph::from_serializable_sync(graph).map_err(|_| snapshot_error())?;
                graph.load_trigger_index(descriptions, pinned, s.trigger_threshold);
                self.prepare(Active {
                    graph,
                    corpus,
                    threshold: s.trigger_threshold,
                })
            }
            "activate_role" => {
                let p: Activate = parse(r.payload)?;
                let next = self.next_generation(&p.next_generation)?;
                let prepared = self
                    .prepared
                    .as_ref()
                    .ok_or_else(|| error("not_found", "Preparation token not found"))?;
                if prepared.token != p.token {
                    return Err(error("not_found", "Preparation token not found"));
                }
                if prepared.generation != self.generation || prepared.revision != self.revision {
                    return Err(error(
                        "stale_revision",
                        "Preparation is stale; prepare again",
                    ));
                }
                crate::checkpoint::validate(
                    &prepared.active.graph.to_serializable(),
                    &prepared.active.corpus,
                    prepared.active.threshold,
                    next,
                    0,
                )?;
                let prepared = self.prepared.take().ok_or_else(invalid)?;
                let name = prepared.active.graph.role.to_string();
                self.active = Some(prepared.active);
                self.generation = next;
                self.revision = 0;
                Ok(json!({"name":name,"generation":next.to_string()}))
            }
            "clear_role" => {
                let p: Clear = parse(r.payload)?;
                let next = self.next_generation(&p.next_generation)?;
                self.active = None;
                self.prepared = None;
                self.generation = next;
                self.revision = 0;
                Ok(json!({"generation":next.to_string()}))
            }
            _ => self.active_command(r),
        }
    }

    fn next_generation(&self, s: &str) -> Result<u64> {
        // The main engine owns generations. Recreated workers may synchronize to
        // any valid main generation, including an initial no-role clear at zero.
        decimal(s)
    }
    fn prepare(&mut self, active: Active) -> Result<Value> {
        let serializable = active.graph.to_serializable();
        validation::graph(&serializable, &active.corpus, active.threshold)?;
        crate::checkpoint::validate(
            &serializable,
            &active.corpus,
            active.threshold,
            self.generation,
            0,
        )?;
        self.token_counter = self.token_counter.checked_add(1).ok_or_else(invalid)?;
        let token = format!("prepared-{}", self.token_counter);
        let text = serde_json::to_string(&active.graph.thesaurus).map_err(|_| invalid())?;
        self.prepared = Some(Prepared {
            token: token.clone(),
            generation: self.generation,
            revision: self.revision,
            active,
        });
        Ok(json!({"token":token,"thesaurusText":text}))
    }

    fn active_command(&mut self, r: &Request) -> Result<Value> {
        let a = self
            .active
            .as_ref()
            .ok_or_else(|| error("role_required", "Load a role before this operation"))?;
        let p = &r.payload;
        match r.op.as_str() {
            "stats" => {
                let _: Empty = parse(p)?;
                Ok(json!(a.graph.get_graph_stats()))
            }
            "connected" => {
                let p: Text = parse(p)?;
                bound(p.text.len() <= QUERY_BYTES)?;
                Ok(json!(a.graph.is_all_terms_connected_by_path(&p.text)))
            }
            "match" | "documents_for_term" | "learning_documents" => {
                let p: TextPage = parse(p)?;
                bound(p.text.len() <= QUERY_BYTES)?;
                bound(p.limit <= 100)?;
                match r.op.as_str() {
                    "match" => Ok(json!(
                        a.graph
                            .find_matching_node_ids(&p.text)
                            .into_iter()
                            .skip(p.offset as usize)
                            .take(p.limit)
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                    )),
                    "documents_for_term" => {
                        let mut ids = a.graph.find_document_ids_for_term(&p.text);
                        ids.sort();
                        Ok(json!(
                            ids.into_iter()
                                .skip(p.offset as usize)
                                .take(p.limit)
                                .collect::<Vec<_>>()
                        ))
                    }
                    _ => {
                        let mut docs = a.graph.get_learning_documents(&p.text);
                        docs.sort_by(|a, b| a.id.cmp(&b.id));
                        Ok(json!(
                            docs.into_iter()
                                .skip(p.offset as usize)
                                .take(p.limit)
                                .map(document_dto)
                                .collect::<Vec<_>>()
                        ))
                    }
                }
            }
            "document" => {
                let p: Id = parse(p)?;
                p.validate()?;
                let d = a
                    .graph
                    .get_document(&p.id)
                    .ok_or_else(|| error("not_found", "Document not found"))?;
                let mut dto = document_dto(d);
                dto["sourceAvailable"] = json!(a.corpus.contains_key(&p.id));
                if let Some(source) = a.corpus.get(&p.id) {
                    dto["source"] = json!(source);
                }
                Ok(dto)
            }
            "nodes" => {
                let page: Page = parse(p)?;
                page.validate()?;
                let mut nodes: Vec<_> = a.graph.nodes_map().values().collect();
                nodes.sort_by_key(|n| n.id);
                Ok(json!(nodes.into_iter().skip(page.offset as usize).take(page.limit).map(|n| {
                    let mut edges:Vec<_>=n.connected_with.iter().copied().collect(); edges.sort();
                    json!({"id":n.id.to_string(),"rank":n.rank.to_string(),"connected_with":edges.iter().map(u64::to_string).collect::<Vec<_>>()})
                }).collect::<Vec<_>>()))
            }
            "edges" => {
                let page: Page = parse(p)?;
                page.validate()?;
                let mut edges: Vec<_> = a.graph.edges_map().values().collect();
                edges.sort_by_key(|e| e.id);
                Ok(json!(
                    edges
                        .into_iter()
                        .skip(page.offset as usize)
                        .take(page.limit)
                        .map(edge_dto)
                        .collect::<Vec<_>>()
                ))
            }
            "query" => {
                let p: Query = parse(p)?;
                if p.operator
                    .as_deref()
                    .is_some_and(|op| op != "and" && op != "or")
                {
                    return Err(invalid());
                }
                bound(
                    p.text.len() <= QUERY_BYTES
                        && p.limit <= 100
                        && p.terms.len() <= 100
                        && p.terms.iter().map(String::len).sum::<usize>() <= QUERY_BYTES,
                )?;
                if p.skip_direct {
                    return Err(error(
                        "invalid_input",
                        "skipDirect=true is not supported by the upstream query API",
                    ));
                }
                let docs = match p.mode.as_str() {
                    "graph" => a
                        .graph
                        .query_graph(&p.text, Some(p.offset as usize), Some(p.limit)),
                    "fallback" => a.graph.query_graph_with_trigger_fallback(
                        &p.text,
                        Some(p.offset as usize),
                        Some(p.limit),
                        true,
                    ),
                    "operators" => {
                        let op = match p.operator.as_deref() {
                            Some("and") => LogicalOperator::And,
                            Some("or") => LogicalOperator::Or,
                            _ => return Err(invalid()),
                        };
                        let terms: Vec<_> = p.terms.iter().map(String::as_str).collect();
                        a.graph.query_graph_with_operators(
                            &terms,
                            &op,
                            Some(p.offset as usize),
                            Some(p.limit),
                        )
                    }
                    _ => return Err(invalid()),
                }
                .map_err(|_| invalid())?;
                Ok(json!(
                    docs.iter()
                        .map(|(_, d)| document_dto(d))
                        .collect::<Vec<_>>()
                ))
            }
            "export_snapshot" => {
                let _: Empty = parse(p)?;
                let s = Snapshot {
                    version: 1,
                    rolegraph_version: "2.0.0".into(),
                    graph_json: a.graph.to_serializable().to_json().map_err(|_| invalid())?,
                    documents: a.corpus.values().cloned().collect(),
                    generation: self.generation.to_string(),
                    revision: self.revision.to_string(),
                    trigger_threshold: a.threshold,
                };
                let text = serde_json::to_string(&s).map_err(|_| invalid())?;
                bound(text.len() <= IMPORT_BYTES)?;
                Ok(json!({"snapshotText":text}))
            }
            "index_documents"
            | "remove_document"
            | "set_triggers"
            | "index_learning"
            | "adjust_learning_rank" => self.mutate(r),
            _ => Err(error("invalid_input", "Unsupported operation")),
        }
    }

    fn mutate(&mut self, r: &Request) -> Result<Value> {
        let active = self.active.as_ref().ok_or_else(invalid)?;
        // Stream collection bounds, deserialize and validate before cloning state.
        let mutation = match r.op.as_str() {
            "index_documents" => {
                let p: Documents = parse(r.payload)?;
                for d in &p.documents {
                    d.validate()?;
                }
                Mutation::Documents(p)
            }
            "remove_document" => {
                let p: Id = parse(r.payload)?;
                p.validate()?;
                Mutation::Remove(p)
            }
            "set_triggers" => {
                let p: Triggers = parse(r.payload)?;
                if !p.threshold.is_finite() || !(0.0..=1.0).contains(&p.threshold) {
                    return Err(invalid());
                }
                bound(p.descriptions.values().map(String::len).sum::<usize>() <= IMPORT_BYTES)?;
                Mutation::Triggers(p)
            }
            "index_learning" => {
                let p: Learning = parse(r.payload)?;
                let doc = p.document.upstream()?;
                let ids = validation::thesaurus(&active.graph.thesaurus)?;
                validation::learning_metadata(&doc, &ids).map_err(|e| {
                    if e.code == "invalid_snapshot" {
                        invalid()
                    } else {
                        e
                    }
                })?;
                Mutation::Learning(doc)
            }
            "adjust_learning_rank" => {
                let p: Adjust = parse(r.payload)?;
                bound(p.id.len() <= QUERY_BYTES)?;
                if p.id.is_empty() {
                    return Err(invalid());
                }
                decimal(&p.adjustment)?;
                Mutation::Adjust(p)
            }
            _ => return Err(invalid()),
        };
        let next = self.revision.checked_add(1).ok_or_else(invalid)?;
        let mut candidate = Active {
            graph: active.graph.clone(),
            corpus: active.corpus.clone(),
            threshold: active.threshold,
        };
        let result = match mutation {
            Mutation::Documents(p) => {
                bound(p.documents.len() <= DOCUMENTS)?;
                let mut seen = std::collections::BTreeSet::new();
                for d in &p.documents {
                    d.validate()?;
                    if !seen.insert(d.id.clone()) {
                        return Err(invalid());
                    }
                    candidate.corpus.insert(d.id.clone(), d.clone());
                }
                validation::corpus(&candidate.corpus)?;
                for d in p.documents {
                    validation::insert_document(&mut candidate.graph, &d)?;
                }
                json!({"indexed":seen.len()})
            }
            Mutation::Remove(p) => {
                p.validate()?;
                candidate.corpus.remove(&p.id);
                json!({"removed":candidate.graph.remove_document(&p.id)})
            }
            Mutation::Triggers(p) => {
                if !p.threshold.is_finite() || !(0.0..=1.0).contains(&p.threshold) {
                    return Err(invalid());
                }
                bound(
                    p.descriptions.len() <= NODES
                        && p.pinned_node_ids.len() <= NODES
                        && p.descriptions.values().map(String::len).sum::<usize>() <= IMPORT_BYTES,
                )?;
                let ids = validation::thesaurus(&candidate.graph.thesaurus)?;
                let descriptions = p
                    .descriptions
                    .into_iter()
                    .map(|(id, text)| Ok((decimal(&id)?, text)))
                    .collect::<Result<ahash::AHashMap<_, _>>>()?;
                let pinned = p
                    .pinned_node_ids
                    .iter()
                    .map(|id| decimal(id))
                    .collect::<Result<Vec<_>>>()?;
                if descriptions
                    .keys()
                    .chain(pinned.iter())
                    .any(|id| !ids.contains(id))
                {
                    return Err(invalid());
                }
                candidate
                    .graph
                    .load_trigger_index(descriptions, pinned, p.threshold);
                candidate.threshold = p.threshold;
                json!({"updated":true})
            }
            Mutation::Learning(doc) => {
                candidate.corpus.remove(&doc.id);
                candidate
                    .graph
                    .index_learning_document(doc)
                    .map_err(|_| invalid())?;
                json!({"indexed":true})
            }
            Mutation::Adjust(p) => {
                bound(p.id.len() <= QUERY_BYTES)?;
                if p.id.is_empty() {
                    return Err(invalid());
                }
                let adjustment = decimal(&p.adjustment)?;
                let d = candidate
                    .graph
                    .get_document(&p.id)
                    .ok_or_else(|| error("not_found", "Learning document not found"))?;
                if p.increase {
                    d.rank.checked_add(adjustment)
                } else {
                    d.rank.checked_sub(adjustment)
                }
                .ok_or_else(invalid)?;
                if !candidate
                    .graph
                    .adjust_learning_document_rank(&p.id, p.increase, adjustment)
                {
                    return Err(error("not_found", "Learning document not found"));
                }
                json!({"adjusted":true})
            }
        };
        let serializable = candidate.graph.to_serializable();
        validation::graph(&serializable, &candidate.corpus, candidate.threshold)?;
        crate::checkpoint::validate(
            &serializable,
            &candidate.corpus,
            candidate.threshold,
            self.generation,
            next,
        )?;
        self.active = Some(candidate);
        self.revision = next;
        Ok(result)
    }
}

enum Mutation {
    Documents(Documents),
    Remove(Id),
    Triggers(Triggers),
    Learning(terraphim_types::IndexedDocument),
    Adjust(Adjust),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Text {
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextPage {
    text: String,
    #[serde(default)]
    offset: u32,
    #[serde(default = "query_limit")]
    limit: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Id {
    id: String,
}
impl Id {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(invalid());
        }
        bound(self.id.len() <= QUERY_BYTES)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PrepareRole {
    thesaurus_text: String,
    role_name: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PrepareSnapshot {
    snapshot_text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Activate {
    token: String,
    next_generation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Clear {
    next_generation: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Documents {
    documents: Vec<SourceDocument>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Triggers {
    descriptions: BTreeMap<String, String>,
    pinned_node_ids: Vec<String>,
    threshold: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Learning {
    document: LearningDocument,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Adjust {
    id: String,
    increase: bool,
    adjustment: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Query {
    text: String,
    mode: String,
    operator: Option<String>,
    #[serde(default)]
    terms: Vec<String>,
    #[serde(default)]
    offset: u32,
    #[serde(default = "query_limit")]
    limit: usize,
    #[serde(default)]
    skip_direct: bool,
}
fn query_limit() -> usize {
    100
}
