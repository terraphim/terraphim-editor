//! The fixture rolegraph the engine is tested with is exactly what the real
//! `terraphim_rolegraph::RoleGraph` produces for the fixture thesaurus and
//! corpus, and the engine reads `SerializableRoleGraph` JSON unchanged.
//!
//! Native only: `terraphim_rolegraph` 2.0 enables tokio `full`, which does not
//! build for wasm32. Regenerate the committed fixture with
//! `LAB_REGENERATE_FIXTURES=1 cargo test -p terraphim_lab --test rolegraph_conformance`.
#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;

use terraphim_automata::load_thesaurus_from_json;
use terraphim_lab::{RoleGraphData, edge_id};
use terraphim_rolegraph::{RoleGraph, SerializableRoleGraph, magic_pair, magic_unpair};
use terraphim_types::{Document, RoleName};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Build the fixture rolegraph with the upstream crate.
fn upstream_rolegraph() -> SerializableRoleGraph {
    let json = std::fs::read_to_string(fixtures().join("role-thesaurus.json")).unwrap();
    let thesaurus = load_thesaurus_from_json(&json).unwrap();
    let mut graph = RoleGraph::new_sync(RoleName::new("Lab fixture role"), thesaurus).unwrap();
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures().join("corpus"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    for path in files {
        let id = path.file_stem().unwrap().to_string_lossy().to_string();
        let body = std::fs::read_to_string(&path).unwrap();
        let document = Document {
            id: id.clone(),
            title: id.clone(),
            body,
            ..Document::default()
        };
        graph.insert_document(&id, document);
    }
    graph.to_serializable()
}

#[test]
fn committed_fixture_matches_upstream_rolegraph() {
    let fresh = upstream_rolegraph();
    let path = fixtures().join("rolegraph.json");
    if std::env::var_os("LAB_REGENERATE_FIXTURES").is_some() {
        // serde_json's default Map is a BTreeMap, so the file has sorted keys
        // and is stable from run to run.
        let value = serde_json::to_value(&fresh).unwrap();
        let mut text = serde_json::to_string_pretty(&value).unwrap();
        text.push('\n');
        std::fs::write(&path, text).unwrap();
    }
    let committed = std::fs::read_to_string(&path).unwrap();
    let upstream = SerializableRoleGraph::from_json(&committed).unwrap();
    assert_eq!(upstream.nodes, fresh.nodes, "nodes drifted from upstream");
    assert_eq!(upstream.edges, fresh.edges, "edges drifted from upstream");
    assert!(!fresh.nodes.is_empty() && !fresh.edges.is_empty());

    // The engine reads the same JSON and sees the same graph.
    let ours = RoleGraphData::from_json(&committed).unwrap();
    assert_eq!(ours.nodes, fresh.nodes);
    assert_eq!(ours.edges, fresh.edges);
}

#[test]
fn edge_id_is_upstream_magic_pair() {
    for x in 0..40u64 {
        for y in 0..40u64 {
            assert_eq!(edge_id(x, y), Some(magic_pair(x, y)));
        }
    }
    // Every edge in the fixture decodes to two of its nodes.
    let fresh = upstream_rolegraph();
    for id in fresh.edges.keys() {
        let (a, b) = magic_unpair(*id);
        assert_eq!(edge_id(a, b), Some(*id));
        assert!(fresh.nodes.contains_key(&a) && fresh.nodes.contains_key(&b));
    }
}

#[test]
fn shared_contributions_ranking_and_removal_match_corpus_occurrences() {
    use std::collections::{BTreeMap, BTreeSet};

    let json = std::fs::read_to_string(fixtures().join("role-thesaurus.json")).unwrap();
    let terms: serde_json::Value = serde_json::from_str(&json).unwrap();
    let names = ["lsp-spans", "shared-schema", "thesaurus", "zed-adapter"];
    let documents: BTreeMap<_, _> = names
        .iter()
        .map(|&id| {
            let document = Document {
                id: id.into(),
                title: id.into(),
                body: std::fs::read_to_string(fixtures().join(format!("corpus/{id}.md"))).unwrap(),
                ..Document::default()
            };
            (id, document)
        })
        .collect();

    // Independent raw LeftmostLongest scanner: title + body, ASCII-insensitive,
    // no word boundaries and no concept-ID deduplication. Do not call the graph
    // matcher here: shared document counts must follow actual corpus occurrences.
    let sequences: BTreeMap<_, Vec<u64>> = documents
        .iter()
        .map(|(&id, document)| {
            let text = format!("{} {}", document.title, document.body).to_ascii_lowercase();
            assert!(
                text.is_ascii(),
                "this bounded oracle requires ASCII fixtures"
            );
            let mut position = 0;
            let mut ids = Vec::new();
            while position < text.len() {
                let longest = terms["data"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .filter(|(term, _)| text[position..].starts_with(term.as_str()))
                    .max_by_key(|(term, _)| term.len());
                if let Some((term, value)) = longest {
                    ids.push(value["id"].as_u64().unwrap());
                    position += term.len();
                } else {
                    position += 1;
                }
            }
            (id, ids)
        })
        .collect();
    assert_eq!(
        sequences["shared-schema"],
        [2, 2, 2, 2, 1, 2, 2, 2, 1, 2, 1]
    );

    let build = |selected: &[&str]| {
        let thesaurus = load_thesaurus_from_json(&json).unwrap();
        let mut graph = RoleGraph::new_sync(RoleName::new("Lab fixture role"), thesaurus).unwrap();
        for &id in selected {
            graph.insert_document(id, documents[id].clone());
        }
        graph
    };
    let assert_occurrences = |snapshot: &SerializableRoleGraph, selected: &[&str]| {
        let mut counts: BTreeMap<u64, BTreeMap<&str, u64>> = BTreeMap::new();
        let mut ranks: BTreeMap<u64, u64> = BTreeMap::new();
        let mut connected: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
        for &id in selected {
            for pair in sequences[id].windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let edge = if a >= b { a * a + a + b } else { b * b + a };
                *counts.entry(edge).or_default().entry(id).or_default() += 1;
                // A self-edge contributes to its endpoint twice.
                for node in [a, b] {
                    *ranks.entry(node).or_default() += 1;
                    connected.entry(node).or_default().insert(edge);
                }
            }
        }
        assert_eq!(snapshot.edges.len(), counts.len());
        for (id, expected) in counts {
            let edge = &snapshot.edges[&id];
            assert_eq!(edge.id, id);
            assert_eq!(
                edge.rank, 1,
                "edge weight is constant, not occurrence count"
            );
            assert_eq!(edge.doc_hash.len(), expected.len());
            for (document, count) in expected {
                assert_eq!(edge.doc_hash[document], count, "edge {id}, {document}");
            }
        }
        assert_eq!(snapshot.nodes.len(), ranks.len());
        for (id, rank) in ranks {
            let node = &snapshot.nodes[&id];
            assert_eq!(node.id, id);
            assert_eq!(node.rank, rank, "node {id} endpoint contributions");
            assert_eq!(
                node.connected_with.iter().copied().collect::<BTreeSet<_>>(),
                connected[&id]
            );
        }
    };

    let graph = build(&names);
    let baseline = graph.to_serializable();
    assert_occurrences(&baseline, &names);
    assert_eq!(baseline.edges[&5].doc_hash["shared-schema"], 2);
    assert_eq!(baseline.edges[&7].doc_hash["shared-schema"], 3);
    assert_eq!(baseline.edges[&8].doc_hash["shared-schema"], 5);
    let committed = SerializableRoleGraph::from_json(
        &std::fs::read_to_string(fixtures().join("rolegraph.json")).unwrap(),
    )
    .unwrap();
    assert_occurrences(&committed, &names);

    // Recompute query weights from occurrence counts and independent endpoint
    // ranks, not text-search relevance. Node 2 has rank 31 and edge weight is 1.
    let lsp_rank = (31 + 1 + 5) + (31 + 1 + 4) + (31 + 1 + 3);
    let shared_rank = (31 + 1 + 2) + (31 + 1 + 3) + (31 + 1 + 5);
    let thesaurus_rank = 31 + 1 + 1;
    assert_eq!((lsp_rank, shared_rank, thesaurus_rank), (108, 106, 33));
    let ranked: Vec<_> = graph
        .query_graph("span model", None, None)
        .unwrap()
        .into_iter()
        .map(|(id, document)| (id, document.rank))
        .collect();
    assert_eq!(
        ranked,
        vec![
            ("lsp-spans".into(), lsp_rank),
            ("shared-schema".into(), shared_rank),
            ("thesaurus".into(), thesaurus_rank),
        ]
    );

    let assert_same = |actual: &RoleGraph, expected: &SerializableRoleGraph| {
        let actual = actual.to_serializable();
        assert_eq!(actual.nodes, expected.nodes);
        assert_eq!(actual.edges, expected.edges);
    };
    let mut reversed = names;
    reversed.reverse();
    assert_same(&build(&reversed), &baseline);
    for id in names {
        let mut graph = build(&names);
        assert!(graph.remove_document(id));
        let remaining: Vec<_> = names.iter().copied().filter(|&name| name != id).collect();
        let rebuilt = build(&remaining).to_serializable();
        assert_occurrences(&rebuilt, &remaining);
        assert_occurrences(&graph.to_serializable(), &remaining);
        assert_same(&graph, &rebuilt);
        assert!(!graph.remove_document(id));
        assert_same(&graph, &rebuilt);
        graph.insert_document(id, documents[id].clone());
        assert_same(&graph, &baseline);
        graph.insert_document(id, documents[id].clone());
        assert_same(&graph, &baseline);
    }
}
