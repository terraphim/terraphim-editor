//! Count the exact exported JSON wrapper without allocating its encoded bodies.
use crate::wire::*;
use serde::Serialize;
use std::{collections::BTreeMap, io::Write};
use terraphim_rolegraph::SerializableRoleGraph;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SnapshotView<'a> {
    version: u32,
    rolegraph_version: &'static str,
    graph_json: &'static str,
    documents: Vec<&'a SourceDocument>,
    generation: String,
    revision: String,
    trigger_threshold: f64,
}

struct Counter {
    bytes: usize,
    limit: usize,
    escaped: bool,
    exceeded: bool,
}
impl Write for Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let additional = if self.escaped {
            bytes
                .iter()
                .map(|b| if *b == b'"' || *b == b'\\' { 2 } else { 1 })
                .sum()
        } else {
            bytes.len()
        };
        if additional > self.limit.saturating_sub(self.bytes) {
            self.exceeded = true;
            return Err(std::io::Error::other("snapshot byte limit"));
        }
        self.bytes += additional;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn count<T: Serialize>(value: &T, limit: usize, escaped: bool) -> Result<usize> {
    let mut counter = Counter {
        bytes: 0,
        limit,
        escaped,
        exceeded: false,
    };
    let result = serde_json::to_writer(&mut counter, value);
    if counter.exceeded {
        bound(false)?;
    }
    result.map_err(|_| invalid())?;
    Ok(counter.bytes)
}

pub(crate) fn validate(
    graph: &SerializableRoleGraph,
    corpus: &BTreeMap<String, SourceDocument>,
    threshold: f64,
    generation: u64,
    revision: u64,
) -> Result<()> {
    let wrapper = SnapshotView {
        version: 1,
        rolegraph_version: "2.0.0",
        graph_json: "",
        documents: corpus.values().collect(),
        generation: generation.to_string(),
        revision: revision.to_string(),
        trigger_threshold: threshold,
    };
    let base = count(&wrapper, IMPORT_BYTES, false)?;
    // Upstream serde JSON contains no raw control characters. Embedding it into
    // graphJson adds exactly one byte per quote/backslash; UTF-8 stays unchanged.
    count(graph, IMPORT_BYTES - base, true)?;
    Ok(())
}
