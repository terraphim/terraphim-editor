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
