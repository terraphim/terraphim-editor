use crate::wire::*;
use std::collections::{BTreeMap, BTreeSet};
use terraphim_rolegraph::{RoleGraph, SerializableRoleGraph, checked_magic_pair, magic_unpair};
use terraphim_types::Thesaurus;

pub(crate) fn insert_document(graph: &mut RoleGraph, source: &SourceDocument) -> Result<()> {
    let document = source.upstream();
    let matches = graph.find_matching_node_ids(&document.to_string());
    // Work only on candidates: remove old membership before counting final edges.
    graph.remove_document(&source.id);
    let mut prospective: BTreeSet<_> = graph.edges_map().keys().copied().collect();
    for pair in matches.windows(2) {
        prospective.insert(checked_magic_pair(pair[0], pair[1]).ok_or_else(invalid)?);
        bound(prospective.len() <= EDGES)?;
    }
    graph.insert_document(&source.id, document);
    Ok(())
}

pub(crate) fn thesaurus(t: &Thesaurus) -> Result<BTreeSet<u64>> {
    bound(t.len() <= NODES)?;
    let mut ids = BTreeSet::new();
    let mut terms = BTreeMap::new();
    for (key, term) in t {
        if key.as_str().is_empty() || term.value.as_str().is_empty() {
            return Err(invalid());
        }
        // Maximum ID paired with itself bounds every ordered pair without O(n²).
        if checked_magic_pair(term.id, term.id).is_none() {
            return Err(invalid());
        }
        if let Some(previous) = terms.insert(term.id, term.value.as_str())
            && previous != term.value.as_str()
        {
            return Err(invalid());
        }
        ids.insert(term.id);
    }
    Ok(ids)
}
pub(crate) fn corpus(c: &BTreeMap<String, SourceDocument>) -> Result<()> {
    bound(c.len() <= DOCUMENTS)?;
    let mut bytes = 0usize;
    for d in c.values() {
        d.validate()?;
        bytes = bytes
            .checked_add(d.id.len() + d.title.len() + d.body.len())
            .ok_or_else(invalid)?;
    }
    bound(bytes <= IMPORT_BYTES)
}

pub(crate) fn learning_metadata(
    doc: &terraphim_types::IndexedDocument,
    ids: &BTreeSet<u64>,
) -> Result<()> {
    bound(doc.tags.len() <= TAGS && doc.nodes.len() <= NODES && doc.matched_edges.len() <= EDGES)?;
    if doc.nodes.iter().any(|id| !ids.contains(id)) {
        return Err(snapshot_error());
    }
    for edge in &doc.matched_edges {
        let (a, b) = magic_unpair(edge.id);
        if checked_magic_pair(a, b) != Some(edge.id) || !ids.contains(&a) || !ids.contains(&b) {
            return Err(snapshot_error());
        }
        bound(edge.doc_hash.len() <= DOCUMENTS)?;
        let mut total = 0u64;
        for (id, count) in &edge.doc_hash {
            if id.is_empty() || *count == 0 {
                return Err(snapshot_error());
            }
            bound(id.len() <= QUERY_BYTES)?;
            total = total.checked_add(*count).ok_or_else(snapshot_error)?;
        }
        bound(total <= IMPORT_BYTES as u64)?;
    }
    Ok(())
}
pub(crate) fn graph(
    s: &SerializableRoleGraph,
    c: &BTreeMap<String, SourceDocument>,
    threshold: f64,
) -> Result<()> {
    let ids = thesaurus(&s.thesaurus)?;
    let mut expected_values: Vec<_> = (&s.thesaurus)
        .into_iter()
        .map(|(_, term)| term.id)
        .collect();
    let mut actual_values = s.aho_corasick_values.clone();
    expected_values.sort();
    actual_values.sort();
    if expected_values != actual_values
        || s.ac_reverse_nterm.len() != ids.len()
        || (&s.thesaurus)
            .into_iter()
            .any(|(_, term)| s.ac_reverse_nterm.get(&term.id) != Some(&term.value))
    {
        return Err(snapshot_error());
    }
    corpus(c)?;
    bound(s.nodes.len() <= NODES && s.edges.len() <= EDGES && s.documents.len() <= DOCUMENTS)?;
    bound(s.documents.values().map(|d| d.tags.len()).sum::<usize>() <= 10_000)?;
    bound(s.nodes.len() + s.documents.values().map(|d| d.nodes.len()).sum::<usize>() <= 100_000)?;
    bound(
        s.documents
            .values()
            .map(|d| d.matched_edges.len())
            .sum::<usize>()
            <= 100_000,
    )?;
    // Conservative container-member accounting keeps the streaming import
    // budget attainable for every published graph, not just each mutation input.
    let provenance = s.edges.values().map(|e| e.doc_hash.len()).sum::<usize>()
        + s.documents
            .values()
            .flat_map(|d| &d.matched_edges)
            .map(|e| e.doc_hash.len())
            .sum::<usize>();
    let document_members = s
        .documents
        .values()
        .map(|d| d.nodes.len() + d.tags.len() + d.matched_edges.len() * 4)
        .sum::<usize>();
    let members = 32
        + s.edges.len() * 4
        + s.nodes.len() * 4
        + s.nodes
            .values()
            .map(|n| n.connected_with.len())
            .sum::<usize>()
        + provenance
        + s.documents.len() * 11
        + document_members
        + s.thesaurus.len() * 14
        + s.aho_corasick_values.len()
        + s.ac_reverse_nterm.len()
        + s.trigger_descriptions.len()
        + s.pinned_node_ids.len()
        + s.learning_document_ids.len();
    bound(members <= 1_000_000)?;
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return Err(snapshot_error());
    }
    if s.pinned_node_ids.iter().any(|id| !ids.contains(id))
        || s.trigger_descriptions.keys().any(|id| !ids.contains(id))
    {
        return Err(snapshot_error());
    }
    bound(
        s.trigger_descriptions
            .values()
            .map(String::len)
            .sum::<usize>()
            <= IMPORT_BYTES,
    )?;
    let learning: BTreeSet<_> = s.learning_document_ids.iter().collect();
    if learning.len() != s.learning_document_ids.len()
        || learning.iter().any(|id| !s.documents.contains_key(*id))
    {
        return Err(snapshot_error());
    }
    let mut ranks = BTreeMap::<u64, u64>::new();
    let mut adjacency = BTreeMap::<u64, BTreeSet<u64>>::new();
    let mut total_contributions = 0u64;
    for (id, e) in &s.edges {
        let (a, b) = magic_unpair(*id);
        if e.id != *id
            || e.rank != 1
            || e.doc_hash.is_empty()
            || checked_magic_pair(a, b) != Some(*id)
            || !ids.contains(&a)
            || !ids.contains(&b)
        {
            return Err(snapshot_error());
        }
        let mut count = 0u64;
        for (doc, rank) in &e.doc_hash {
            if *rank == 0 || !s.documents.contains_key(doc) || learning.contains(doc) {
                return Err(snapshot_error());
            }
            count = count.checked_add(*rank).ok_or_else(snapshot_error)?;
        }
        // At most the bounded corpus's byte count can contribute matches.
        total_contributions = total_contributions
            .checked_add(count)
            .ok_or_else(snapshot_error)?;
        bound(total_contributions <= IMPORT_BYTES as u64)?;
        for endpoint in [a, b] {
            let rank = ranks.entry(endpoint).or_default();
            *rank = rank.checked_add(count).ok_or_else(snapshot_error)?;
            adjacency.entry(endpoint).or_default().insert(*id);
        }
    }
    if ranks.len() != s.nodes.len() {
        return Err(snapshot_error());
    }
    for (id, n) in &s.nodes {
        if n.id != *id
            || ranks.get(id) != Some(&n.rank)
            || adjacency.get(id) != Some(&n.connected_with.iter().copied().collect())
        {
            return Err(snapshot_error());
        }
    }
    for (id, d) in &s.documents {
        if id.is_empty() || d.id != *id || d.nodes.iter().any(|n| !ids.contains(n)) {
            return Err(snapshot_error());
        }
        bound(
            id.len() <= QUERY_BYTES
                && d.nodes.len() <= NODES
                && d.tags.len() <= TAGS
                && d.tags.iter().map(String::len).sum::<usize>() <= DOCUMENT_BYTES
                && d.matched_edges.len() <= EDGES,
        )?;
        if learning.contains(id) {
            // IndexedDocument edges are historical result metadata, not pointers
            // into current graph edges or contributions to edge.doc_hash.
            learning_metadata(d, &ids)?;
        } else if d.rank != 0 || !d.nodes.is_empty() || !d.matched_edges.is_empty() {
            return Err(snapshot_error());
        }
    }
    if c.keys()
        .any(|id| !s.documents.contains_key(id) || learning.contains(id))
    {
        return Err(snapshot_error());
    }
    // Validate original source's exact per-document contributions when available.
    // Missing source remains explicitly unavailable; no invented rebuild corpus.
    if !c.is_empty() {
        let mut rebuilt = RoleGraph::new_sync(s.role.clone(), s.thesaurus.clone())
            .map_err(|_| snapshot_error())?;
        for (id, d) in c {
            debug_assert_eq!(id, &d.id);
            insert_document(&mut rebuilt, d)?;
        }
        let r = rebuilt.to_serializable();
        for (id, e) in &s.edges {
            for (doc, count) in &e.doc_hash {
                if c.contains_key(doc)
                    && r.edges.get(id).and_then(|edge| edge.doc_hash.get(doc)) != Some(count)
                {
                    return Err(snapshot_error());
                }
            }
        }
        for (id, e) in &r.edges {
            for (doc, count) in &e.doc_hash {
                if s.edges.get(id).and_then(|edge| edge.doc_hash.get(doc)) != Some(count) {
                    return Err(snapshot_error());
                }
            }
        }
    }
    Ok(())
}
