//! Helpers shared by the integration tests.

use terraphim_alternatives::Document;

/// Drops every stored context, as in a file written before context existed
/// (decision 2026-10-05: context re-anchoring).
pub fn strip_context(doc: &mut Document) {
    let anchors = doc
        .annotations
        .spans
        .iter_mut()
        .map(|s| &mut s.anchor)
        .chain(doc.annotations.ghosts.iter_mut().map(|g| &mut g.anchor));
    for anchor in anchors {
        anchor.before = None;
        anchor.after = None;
    }
}
