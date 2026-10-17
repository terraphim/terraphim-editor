//! Persistence bridge between the browser editor and
//! [`terraphim_alternatives`] (issue #6).
//!
//! The editor keeps exactly one open document in memory. Its body is what the
//! editing surface shows; its annotations (alternatives, the ghost layer and
//! the overflow stash) live only in the model and are written back as the
//! trailing `terraphim-alternatives` block on save. The block never reaches
//! the editing surface or the preview.
//!
//! # Offsets
//!
//! Every offset is a **UTF-16 code unit** into the body, the same unit the
//! `EditorSurface` in `public/js/editor.js` reports, so surface edits can be
//! passed to [`apply_edit`] unchanged.
//!
//! # Layers
//!
//! * [`DocumentSession`] is plain Rust with no `web-sys` dependency, so it is
//!   unit-tested natively and reused by the golden-file tests.
//! * The `#[wasm_bindgen]` functions below wrap one thread-local session for
//!   JavaScript. In the Trunk build they are reachable as
//!   `window.wasmBindings.<name>`; `MarkdownEditor` wraps them as
//!   `openDocument`, `saveDocument`, `exportDocument`, `counts` and
//!   `annotations`; the ghost layer (`public/js/selection-menu.js`, issue
//!   #11) calls [`ghost_range`] and [`revive_range`].
//!
//! # Nothing is dropped
//!
//! * **Malformed block.** The body opens for editing, one warning is
//!   reported, and the raw block is kept verbatim: [`DocumentSession::save`]
//!   writes `body + raw_block`, so the block survives untouched until it is
//!   repaired by hand (the crate's `body + raw_block == source` contract).
//! * **Set-aside annotations.** A span whose text an edit changes is detached
//!   by [`Document::apply_edit`], and a span or ghost that re-anchoring cannot
//!   place is unresolved. Both are kept whole in [`SetAside`] and are
//!   **saved**: they are written into the block's ordinary `spans` and
//!   `ghosts` lists with their last known anchors, so the schema and the crate
//!   are unchanged. The block only checks structure (it never checks anchors
//!   against the body), so a stale anchor is legal; the one rule a stale
//!   anchor could break is "no overlaps within a list", so a set-aside item
//!   whose range would overlap a live (or earlier set-aside) item of the same
//!   list is moved past the end of the body, keeping its text, and an id that
//!   is now taken gets a fresh one. The next open re-anchors every item by its
//!   text: what is found is live again, what is not is set aside again and
//!   still preserved.
//! * **Re-attaching.** While a document is open, set-aside anchors follow
//!   edits made before them, and after every body change (typing, undo, redo)
//!   each set-aside item whose text is back exactly at its anchor, without
//!   overlapping a live item, is re-attached. Undoing the edit that detached a
//!   span therefore brings its alternatives back.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::providers::Suggestion;

use serde_json::{json, Value};
use terraphim_alternatives::{
    parse, utf16_len, utf16_to_byte, write, Anchor, Counts, Document, EditError, Ghost,
    ReanchorReport, Span,
};
use wasm_bindgen::prelude::*;

/// Outcome of [`DocumentSession::open`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The text to show on the editing surface (never contains the block).
    pub body: String,
    /// One human-readable, non-blocking warning, if the file needs attention.
    pub warning: Option<String>,
    /// Number of spans and ghosts whose anchor text could no longer be found
    /// in the body when the file was opened (see [`DocumentSession::open`]).
    pub unresolved: usize,
    /// What `warning` is about: `"malformed"` (the block could not be read;
    /// the warning stays until dismissed because the raw block is still
    /// preserved) or `"set-aside"` (annotations out of the model; the warning
    /// can clear once they re-attach).
    pub kind: Option<&'static str>,
}

/// Outcome of [`DocumentSession::sync_body`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Synced {
    /// Whether the body differed from the model and was replaced.
    pub changed: bool,
    /// Spans and ghosts set aside after the sync (all of them, not only new
    /// ones).
    pub unresolved: usize,
}

/// Outcome of [`DocumentSession::apply_edit`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EditOutcome {
    /// Ids of spans this edit detached (now set aside).
    pub detached: Vec<String>,
    /// Ids of ghosts whose whole text this edit removed. The live layer
    /// drops them (R-5.3), and the session sets them aside with their
    /// pre-edit anchors, so undoing the edit (or pasting the text back)
    /// restores them, and a save in between keeps them.
    pub detached_ghosts: Vec<String>,
    /// Ids of set-aside spans and ghosts this edit re-attached.
    pub reattached: Vec<String>,
    /// Spans and ghosts set aside after the edit.
    pub set_aside: usize,
    /// A non-blocking notice when the edit detached alternatives.
    pub warning: Option<String>,
    /// The current set-aside notice (see
    /// [`DocumentSession::set_aside_notice`]), `None` when nothing is set
    /// aside.
    pub notice: Option<String>,
}

/// One open document: the span model plus, after a malformed open, the raw
/// annotation block kept for saving.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentSession {
    /// Clones retain identity; an explicit open always starts a new session,
    /// even when all serialized/model values happen to be identical.
    identity: SessionIdentity,
    doc: Document,
    /// The unreadable block exactly as found, written back verbatim on save.
    raw_block: Option<String>,
    warning: Option<String>,
    /// Spans and ghosts out of the model, kept whole and saved (see the
    /// module docs).
    set_aside: SetAside,
    /// Session-only identities, including ghosts removed by edits/history.
    reserved_ids: HashSet<String>,
    /// Exact receipts issued by this open session; rejects forged replay targets.
    ghost_receipts: HashSet<String>,
}

#[derive(Debug, Clone)]
struct SessionIdentity(Rc<()>);

impl Default for SessionIdentity {
    fn default() -> Self {
        Self(Rc::new(()))
    }
}

impl PartialEq for SessionIdentity {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SessionIdentity {}

/// Annotations temporarily out of the model: spans an edit detached (it
/// changed their text) and spans or ghosts that re-anchoring could not place.
/// They are saved with the document and re-attached when their text returns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetAside {
    /// Detached or unplaceable spans, with their last known anchors.
    pub spans: Vec<Span>,
    /// Unplaceable ghosts, with their last known anchors.
    pub ghosts: Vec<Ghost>,
}

impl SetAside {
    /// Keeps everything a re-anchoring pass could not place.
    fn keep_unresolved(&mut self, report: ReanchorReport) {
        self.spans
            .extend(report.unresolved.into_iter().map(|u| u.span));
        self.ghosts
            .extend(report.unresolved_ghosts.into_iter().map(|u| u.ghost));
    }

    /// Number of set-aside spans and ghosts.
    pub fn len(&self) -> usize {
        self.spans.len() + self.ghosts.len()
    }

    /// Whether nothing is set aside.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Moves every anchor hint through an edit replacing `start..end` with
    /// `inserted`: an anchor wholly after the edit (an insertion exactly at
    /// its start included) shifts; any other anchor keeps its position, which
    /// is where its text would come back on undo.
    ///
    /// One exception: when the inserted text holds the anchor's own text at
    /// exactly the anchor's position, that is the text coming back (undoing
    /// the deletion of a ghost's whole text, or pasting it back), so the
    /// anchor stays where the text now is.
    fn shift_hints(&mut self, start: usize, end: usize, inserted: &str) {
        let inserted_len = utf16_len(inserted);
        let anchors = self
            .spans
            .iter_mut()
            .map(|s| &mut s.anchor)
            .chain(self.ghosts.iter_mut().map(|g| &mut g.anchor));
        for anchor in anchors {
            let returning = !anchor.text.is_empty()
                && end <= anchor.start
                && utf16_to_byte(inserted, anchor.start - start)
                    .is_some_and(|b| inserted[b..].starts_with(anchor.text.as_str()));
            if end <= anchor.start && !returning {
                let len = anchor.end - anchor.start;
                anchor.start = anchor.start - (end - start) + inserted_len;
                anchor.end = anchor.start + len;
            }
        }
    }
}

/// Whether `body` holds `anchor.text` exactly at the anchor's offsets.
fn text_at(body: &str, anchor: &Anchor) -> bool {
    match (
        utf16_to_byte(body, anchor.start),
        utf16_to_byte(body, anchor.end),
    ) {
        (Some(start), Some(end)) => body[start..end] == anchor.text,
        _ => false,
    }
}

/// Whether `anchor` touches `window` (always true without a window).
fn near(anchor: &Anchor, window: Option<(usize, usize)>) -> bool {
    window.is_none_or(|(start, end)| anchor.start <= end && start <= anchor.end)
}

fn overlaps(a: &Anchor, start: usize, end: usize) -> bool {
    a.start < end && start < a.end
}

/// `<prefix><n>` with `n` one past the highest number used with that prefix.
fn fresh_id(prefix: char, used: &HashSet<String>) -> String {
    let max = used
        .iter()
        .filter_map(|id| id.strip_prefix(prefix)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    if let Some(next) = max.checked_add(1) {
        return format!("{prefix}{next}");
    }
    // Save/reattach collision repair must remain lossless even when the
    // numeric namespace is exhausted. New ghost allocation instead refuses.
    let mut suffix = String::from("~");
    while used.contains(&format!("{prefix}{suffix}")) {
        suffix.push('~');
    }
    format!("{prefix}{suffix}")
}

impl DocumentSession {
    /// An empty session (empty body, no annotations).
    pub fn new() -> Self {
        Self::default()
    }

    /// Opens a `.md` source, replacing the current document.
    ///
    /// * No block: the whole source is the body, as plain Markdown.
    /// * A valid block: annotations are loaded and re-anchored against the
    ///   body, so a file edited outside the editor still finds its spans by
    ///   their text. Anything that cannot be placed is set aside (and still
    ///   saved), and reported in `warning` and `unresolved`.
    /// * A malformed block: the body opens, `warning` explains the problem
    ///   once, and the raw block is preserved for [`save`](Self::save).
    pub fn open(&mut self, source: &str) -> Opened {
        *self = Self::default();
        match parse(source) {
            Ok(mut doc) => {
                self.set_aside.keep_unresolved(doc.reanchor());
                self.doc = doc;
                let unresolved = self.set_aside.len();
                if unresolved > 0 {
                    self.warning = Some(format!(
                        "{unresolved} saved {} no longer {} the text. {} preserved and saved \
                         with the document, and {} re-attached if the text returns.",
                        plural(
                            unresolved,
                            "annotation (alternatives or ghost)",
                            "annotations (alternatives or ghosts)"
                        ),
                        plural(unresolved, "matches", "match"),
                        plural(unresolved, "It is", "They are"),
                        plural(unresolved, "is", "are"),
                    ));
                }
            }
            Err(error) => {
                self.warning = Some(format!(
                    "The alternatives block at the end of this file could not be read \
                     ({}). The text opened normally and the block will be saved back \
                     unchanged.",
                    error.kind
                ));
                self.doc = Document::new(error.body);
                self.raw_block = Some(error.raw_block);
            }
        }
        self.reserve_current_ids();
        let kind = if self.raw_block.is_some() {
            Some("malformed")
        } else if self.set_aside.is_empty() {
            None
        } else {
            Some("set-aside")
        };
        Opened {
            body: self.doc.body.clone(),
            warning: self.warning.clone(),
            unresolved: self.set_aside.len(),
            kind,
        }
    }

    /// A notice describing what is currently set aside, or `None` when
    /// nothing is. The editor shows it when the set-aside count changes for a
    /// reason other than an edit detaching spans (for example a full re-sync
    /// of the body), and clears its notice once this becomes `None`.
    pub fn set_aside_notice(&self) -> Option<String> {
        let n = self.set_aside.len();
        (n > 0).then(|| {
            format!(
                "{n} {} out of step with the text. {} preserved and saved with the document, \
                 and {} re-attached if the text returns.",
                plural(
                    n,
                    "annotation (alternatives or ghost) is",
                    "annotations (alternatives or ghosts) are"
                ),
                plural(n, "It is", "They are"),
                plural(n, "is", "are"),
            )
        })
    }

    /// The `.md` text to write to disk: the body followed by the annotation
    /// block (or, after a malformed open, by the preserved raw block). Set-aside
    /// annotations are included (see the module docs). A document with nothing
    /// to persist saves as plain Markdown.
    pub fn save(&self) -> String {
        match &self.raw_block {
            Some(raw) => format!("{}{raw}", self.doc.body),
            None => write(&self.persisted()),
        }
    }

    /// The document as written on save: live annotations, then the set-aside
    /// ones with their last known anchors, moved past the end of the body only
    /// where they would overlap an item of the same list, and renamed only
    /// where their id is taken.
    fn persisted(&self) -> Document {
        let mut doc = self.doc.clone();
        if self.set_aside.is_empty() {
            return doc;
        }
        let mut used: HashSet<String> = doc
            .annotations
            .spans
            .iter()
            .map(|s| s.id.clone())
            .chain(doc.annotations.ghosts.iter().map(|g| g.id.clone()))
            .collect();
        let anchors = doc
            .annotations
            .spans
            .iter()
            .map(|s| &s.anchor)
            .chain(doc.annotations.ghosts.iter().map(|g| &g.anchor))
            .chain(self.set_aside.spans.iter().map(|s| &s.anchor))
            .chain(self.set_aside.ghosts.iter().map(|g| &g.anchor));
        let mut past_end = anchors
            .map(|a| a.end)
            .fold(utf16_len(&doc.body), usize::max);

        let mut place = |anchor: &mut Anchor, taken: &[(usize, usize)]| {
            if taken.iter().any(|&(s, e)| overlaps(anchor, s, e)) {
                let len = anchor.end - anchor.start;
                anchor.start = past_end;
                anchor.end = past_end + len;
                past_end += len;
            }
        };

        let mut taken: Vec<(usize, usize)> = doc
            .annotations
            .spans
            .iter()
            .map(|s| (s.anchor.start, s.anchor.end))
            .collect();
        for span in &self.set_aside.spans {
            let mut span = span.clone();
            if used.contains(&span.id) {
                span.id = fresh_id('s', &used);
            }
            used.insert(span.id.clone());
            place(&mut span.anchor, &taken);
            taken.push((span.anchor.start, span.anchor.end));
            doc.annotations.spans.push(span);
        }

        let mut taken: Vec<(usize, usize)> = doc
            .annotations
            .ghosts
            .iter()
            .map(|g| (g.anchor.start, g.anchor.end))
            .collect();
        for ghost in &self.set_aside.ghosts {
            let mut ghost = ghost.clone();
            if used.contains(&ghost.id) {
                ghost.id = fresh_id('g', &used);
            }
            used.insert(ghost.id.clone());
            place(&mut ghost.anchor, &taken);
            taken.push((ghost.anchor.start, ghost.anchor.end));
            doc.annotations.ghosts.push(ghost);
        }
        doc
    }

    /// Validate the complete session namespace with the real persisted schema.
    /// Unresolved hints can overlap legitimately: move only their validation
    /// anchors to checked, nonoverlapping slots, never rename an identity.
    /// This also bounds the arithmetic used by the existing save projection.
    fn validate_history_annotations(&self) -> Result<(), String> {
        if self.raw_block.is_some() {
            return Err(
                "cannot validate annotations while a raw annotation block is preserved".into(),
            );
        }
        let mut complete = self.doc.clone();
        let mut next = complete
            .annotations
            .spans
            .iter()
            .map(|s| &s.anchor)
            .chain(complete.annotations.ghosts.iter().map(|g| &g.anchor))
            .chain(self.set_aside.spans.iter().map(|s| &s.anchor))
            .chain(self.set_aside.ghosts.iter().map(|g| &g.anchor))
            .map(|a| a.end)
            .fold(utf16_len(self.body()), usize::max);
        let mut place = |anchor: &mut Anchor| -> Result<(), String> {
            let len = anchor
                .end
                .checked_sub(anchor.start)
                .ok_or_else(|| "invalid set-aside anchor range".to_string())?;
            let end = next
                .checked_add(len)
                .ok_or_else(|| "set-aside persistence offset exhausted".to_string())?;
            anchor.start = next;
            anchor.end = end;
            next = end;
            Ok(())
        };
        for span in &self.set_aside.spans {
            let mut span = span.clone();
            place(&mut span.anchor)?;
            complete.annotations.spans.push(span);
        }
        for ghost in &self.set_aside.ghosts {
            let mut ghost = ghost.clone();
            place(&mut ghost.anchor)?;
            complete.annotations.ghosts.push(ghost);
        }
        // Full schema checks shared IDs, alternatives/provenance, anchors,
        // within-layer overlaps and overflow. Reservations are NOT live IDs.
        parse(&write(&complete)).map_err(|e| e.to_string())?;
        parse(&write(&self.persisted())).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Clean Markdown for export (R-9.3): active alternatives only, ghosted
    /// text dropped, no overflow and no block.
    pub fn export(&self) -> String {
        self.doc.export()
    }

    /// Word and character counts of the body, ghosted text included
    /// (decision 3).
    pub fn counts(&self) -> Counts {
        self.doc.counts()
    }

    /// The body as the model holds it.
    pub fn body(&self) -> &str {
        &self.doc.body
    }

    /// The warning reported by the last [`open`](Self::open), if any.
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// Whether a malformed block is being preserved for saving.
    pub fn has_preserved_block(&self) -> bool {
        self.raw_block.is_some()
    }

    /// Annotations currently out of the model (saved all the same).
    pub fn set_aside(&self) -> &SetAside {
        &self.set_aside
    }

    /// Read-only access to the live span model.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Mirrors one editing-surface edit: replaces `deleted_len` UTF-16 code
    /// units at `start` with `inserted`, moving every anchor exactly.
    ///
    /// Spans whose text the edit changes are detached (see
    /// [`Document::apply_edit`]) and set aside with a warning; ghosts resize
    /// with the text. Set-aside items whose text is back at their anchor are
    /// re-attached. On error the model is unchanged; the caller should
    /// resynchronise with [`sync_body`](Self::sync_body).
    pub fn apply_edit(
        &mut self,
        start: usize,
        deleted_len: usize,
        inserted: &str,
    ) -> Result<EditOutcome, EditError> {
        let end = start
            .checked_add(deleted_len)
            .ok_or(EditError::InvalidRange {
                start,
                end: usize::MAX,
            })?;
        let ghosts_before = self.doc.annotations.ghosts.clone();
        let detached = self.doc.apply_edit(start, end, inserted)?;
        let inserted_len = utf16_len(inserted);
        self.set_aside.shift_hints(start, end, inserted);
        // Ghosts the edit removed (all of their text was deleted or replaced).
        let live: HashSet<&str> = self
            .doc
            .annotations
            .ghosts
            .iter()
            .map(|g| g.id.as_str())
            .collect();
        let removed: Vec<Ghost> = ghosts_before
            .into_iter()
            .filter(|g| !live.contains(g.id.as_str()))
            .collect();
        let detached_ids: Vec<String> = detached.iter().map(|s| s.id.clone()).collect();
        let detached_ghosts: Vec<String> = removed.iter().map(|g| g.id.clone()).collect();
        let warning = detach_warning(&detached);
        // Freshly detached spans and removed ghosts keep their pre-edit
        // anchors: that is exactly where their text returns if the edit is
        // undone. Only an item touching the changed text can have its text
        // come back, unless the edit removed a live item that was blocking it.
        let freed = !detached.is_empty() || !removed.is_empty();
        self.set_aside.spans.extend(detached);
        self.set_aside.ghosts.extend(removed);
        let window = (!freed).then_some((start, start + inserted_len));
        let reattached = self.reattach(window);
        Ok(EditOutcome {
            detached: detached_ids,
            detached_ghosts,
            reattached,
            set_aside: self.set_aside.len(),
            warning,
            notice: self.set_aside_notice(),
        })
    }

    /// Re-attaches every set-aside span whose text is back exactly at its
    /// anchor and that does not overlap a live span, and every set-aside
    /// ghost whose text is back exactly at its anchor. A ghost that touches
    /// or overlaps a live ghost is merged with it by [`Document::ghost`] (the
    /// crate's rule; the merged ghost keeps the id of the one starting first),
    /// so ghosts never overlap. Returns the re-attached ids (a re-attached
    /// item whose id was taken meanwhile gets a fresh one).
    ///
    /// With a `window` (the text just changed, in body offsets), only items
    /// touching it are checked: any other item's text and hint moved
    /// together, so it still does not fit. This keeps typing cheap while
    /// something is set aside.
    fn reattach(&mut self, window: Option<(usize, usize)>) -> Vec<String> {
        self.reserve_current_ids();
        if self.set_aside.is_empty() {
            return Vec::new();
        }
        let mut reattached = Vec::new();
        let body = &self.doc.body;
        let annotations = &mut self.doc.annotations;
        let mut used: HashSet<String> = annotations
            .spans
            .iter()
            .map(|s| s.id.clone())
            .chain(annotations.ghosts.iter().map(|g| g.id.clone()))
            .collect();

        let mut waiting = Vec::new();
        for mut span in std::mem::take(&mut self.set_aside.spans) {
            let a = &span.anchor;
            let fits = near(a, window)
                && text_at(body, a)
                && !annotations
                    .spans
                    .iter()
                    .any(|s| overlaps(&s.anchor, a.start, a.end));
            if fits {
                if used.contains(&span.id) {
                    span.id = fresh_id('s', &used);
                }
                used.insert(span.id.clone());
                reattached.push(span.id.clone());
                annotations.spans.push(span);
            } else {
                waiting.push(span);
            }
        }
        self.set_aside.spans = waiting;

        let mut waiting = Vec::new();
        for mut ghost in std::mem::take(&mut self.set_aside.ghosts) {
            let (start, end) = (ghost.anchor.start, ghost.anchor.end);
            if !(near(&ghost.anchor, window) && text_at(&self.doc.body, &ghost.anchor)) {
                waiting.push(ghost);
                continue;
            }
            let touches = self
                .doc
                .annotations
                .ghosts
                .iter()
                .any(|g| g.anchor.start <= end && start <= g.anchor.end);
            if touches {
                match self.ghost(start, end) {
                    Ok(id) => {
                        used.insert(id.clone());
                        reattached.push(id);
                    }
                    Err(_) => waiting.push(ghost),
                }
            } else {
                if used.contains(&ghost.id) {
                    match allocate_ghost_id(&mut self.reserved_ids) {
                        Ok(id) => ghost.id = id,
                        Err(_) => {
                            waiting.push(ghost);
                            continue;
                        }
                    }
                }
                used.insert(ghost.id.clone());
                reattached.push(ghost.id.clone());
                self.doc.annotations.ghosts.push(ghost);
            }
        }
        self.set_aside.ghosts = waiting;
        self.doc.annotations.ghosts.sort_by_key(|g| g.anchor.start);
        reattached
    }

    /// Makes the model body equal to `text` when the two have drifted apart
    /// (for example after line-ending normalisation on the surface, or a
    /// failed [`apply_edit`](Self::apply_edit)). Live spans and ghosts are
    /// re-anchored by their text (what cannot be placed is set aside), then
    /// set-aside items are re-attached where their text is back. A no-op when
    /// the bodies already match.
    pub fn sync_body(&mut self, text: &str) -> Synced {
        if self.doc.body == text {
            return Synced::default();
        }
        self.doc.body = text.to_string();
        self.set_aside.keep_unresolved(self.doc.reanchor());
        self.reattach(None);
        Synced {
            changed: true,
            unresolved: self.set_aside.len(),
        }
    }

    /// Spans, ghosts and overflow as JSON, for decorations and inspection:
    /// `{ spans, ghosts, overflow, setAside: { spans, ghosts }, preservedBlock }`.
    /// Spans and ghosts use the block's own schema (UTF-16 anchors).
    pub fn annotations_json(&self) -> Value {
        let annotations = &self.doc.annotations;
        json!({
            "spans": annotations.spans,
            "ghosts": annotations.ghosts,
            "overflow": annotations.overflow,
            "setAside": { "spans": self.set_aside.spans, "ghosts": self.set_aside.ghosts },
            "preservedBlock": self.raw_block.is_some(),
            "synonymGroups": self.synonym_groups_json(),
        })
    }

    /// Current authored KG synonym registrations, including set-aside groups.
    /// Exact annotations remain the durable source; no shared role is mutated.
    pub fn synonym_groups_json(&self) -> Value {
        let groups: Vec<_> = self
            .doc
            .annotations
            .spans
            .iter()
            .map(|span| (span, true))
            .chain(self.set_aside.spans.iter().map(|span| (span, false)))
            .filter(|(span, _)| !span.is_inert())
            .map(|(span, attached)| {
                let group = terraphim_alternatives::SynonymGroup::from_span(span);
                json!({"spanId":group.span_id,"attached":attached,
                    "markdown":group.markdown,"typedSynonyms":group.typed_synonyms,
                    "thesaurus":group.thesaurus,"synonyms":group.index.synonyms_of(1),
                    "alternatives":group.alternatives})
            })
            .collect();
        json!(groups)
    }

    /// Share attached committed alternatives using the existing Terraphim types.
    ///
    /// # Errors
    /// Refuses malformed preserved annotations or ambiguous cross-group terms.
    pub fn export_knowledge_graph(&self) -> Result<String, String> {
        if self.raw_block.is_some() {
            return Err(
                "Repair the document's malformed annotations before sharing its graph.".into(),
            );
        }
        let graph = self
            .doc
            .export_synonym_thesaurus()
            .map_err(|e| e.to_string())?;
        serde_json::to_string_pretty(&serde_json::to_value(graph).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }

    /// Query a live authored concept using Terraphim's registered synonym index.
    /// Scope is this document plus `span_id`; detached/unknown groups return [].
    pub fn synonyms_for(&self, span_id: &str, text: &str) -> Vec<String> {
        self.doc
            .annotations
            .spans
            .iter()
            .find(|span| span.id == span_id && !span.is_inert())
            .map(|span| {
                terraphim_alternatives::SynonymGroup::from_span(span)
                    .synonyms_of(text)
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Ghosts `start..end` (R-5.1, R-5.3) and returns the id of the ghost now
    /// covering it. Ghosts are an independent layer: the range may cover or
    /// partly overlap spans, and a ghost overlapping or touching it is merged
    /// in (see [`Document::ghost`]). The text is unchanged, so it stays
    /// counted and editable; export drops it. On error nothing changes.
    pub fn ghost(&mut self, start: usize, end: usize) -> Result<String, EditError> {
        let saved = self.clone();
        self.reserve_current_ids();
        let reserved = &mut self.reserved_ids;
        let result = self
            .doc
            .ghost_with_allocator(start, end, || allocate_ghost_id(reserved));
        if result.is_err() {
            *self = saved;
        }
        result
    }

    /// Revives `start..end` (R-5.2): ghosts inside it are removed, ghosts
    /// partly inside are trimmed and a ghost strictly containing it is split
    /// (see [`Document::revive`]). Returns whether any ghost changed. On
    /// error nothing changes.
    pub fn revive(&mut self, start: usize, end: usize) -> Result<bool, EditError> {
        let saved = self.clone();
        self.reserve_current_ids();
        let reserved = &mut self.reserved_ids;
        let result = self
            .doc
            .revive_with_allocator(start, end, || allocate_ghost_id(reserved));
        if result.is_err() {
            *self = saved;
        }
        result
    }
}

fn allocate_ghost_id(reserved: &mut HashSet<String>) -> Result<String, EditError> {
    allocate_reserved_id('g', reserved)
}

fn allocate_reserved_id(prefix: char, reserved: &mut HashSet<String>) -> Result<String, EditError> {
    let max = reserved
        .iter()
        .filter_map(|id| id.strip_prefix(prefix)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let next = max.checked_add(1).ok_or(EditError::IdAllocation)?;
    let id = format!("{prefix}{next}");
    if !reserved.insert(id.clone()) {
        return Err(EditError::IdAllocation);
    }
    Ok(id)
}

/// Ghost-only history state. Reservations are decimal-string IDs, never JS
/// numbers. Replay does not roll this reservation set back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostSnapshot {
    pub live: Vec<Ghost>,
    pub set_aside: Vec<Ghost>,
    pub reserved_ids: Vec<String>,
}

impl GhostSnapshot {
    pub fn to_json(&self) -> Value {
        json!({ "live": self.live, "setAside": self.set_aside, "reservedIds": self.reserved_ids })
    }

    fn from_json(value: &Value) -> Result<Self, String> {
        Ok(Self {
            live: serde_json::from_value(value["live"].clone()).map_err(|e| e.to_string())?,
            set_aside: serde_json::from_value(value["setAside"].clone())
                .map_err(|e| e.to_string())?,
            reserved_ids: serde_json::from_value(value["reservedIds"].clone())
                .map_err(|e| e.to_string())?,
        })
    }
}

/// Exact receipt for independent ghost promotion; body is only a guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostChange {
    pub body: String,
    pub before: GhostSnapshot,
    pub after: GhostSnapshot,
}

impl GhostChange {
    pub fn to_json(&self) -> Value {
        json!({ "body": self.body, "before": self.before.to_json(), "after": self.after.to_json() })
    }

    pub fn from_json(value: &Value) -> Result<Self, String> {
        Ok(Self {
            body: value["body"]
                .as_str()
                .ok_or("receipt body must be text")?
                .to_string(),
            before: GhostSnapshot::from_json(&value["before"])?,
            after: GhostSnapshot::from_json(&value["after"])?,
        })
    }
}

impl DocumentSession {
    fn reserve_current_ids(&mut self) {
        self.reserved_ids.extend(
            self.doc
                .annotations
                .spans
                .iter()
                .map(|s| s.id.clone())
                .chain(self.doc.annotations.ghosts.iter().map(|g| g.id.clone()))
                .chain(self.set_aside.spans.iter().map(|s| s.id.clone()))
                .chain(self.set_aside.ghosts.iter().map(|g| g.id.clone())),
        );
    }

    pub fn ghost_snapshot(&self) -> GhostSnapshot {
        let mut reserved_ids: Vec<_> = self.reserved_ids.iter().cloned().collect();
        reserved_ids.sort();
        GhostSnapshot {
            live: self.doc.annotations.ghosts.clone(),
            set_aside: self.set_aside.ghosts.clone(),
            reserved_ids,
        }
    }

    /// Creates independent ghosts over all ranges atomically. Adjacent ranges
    /// remain distinct; overlap with another range or a manual ghost is refused.
    /// Spans may coexist. Callers must validate their protected plan/role first.
    pub fn ghost_ranges_atomic(
        &mut self,
        expected_body: &str,
        ranges: &[(usize, usize)],
    ) -> Result<Option<GhostChange>, String> {
        if self.raw_block.is_some() {
            return Err("preserved malformed block cannot save new ghosts".into());
        }
        if self.body() != expected_body {
            return Err("ghost transaction body conflict".into());
        }
        for (i, &(start, end)) in ranges.iter().enumerate() {
            if start >= end
                || utf16_to_byte(self.body(), start).is_none()
                || utf16_to_byte(self.body(), end).is_none()
            {
                return Err(EditError::InvalidRange { start, end }.to_string());
            }
            if ranges[..i].iter().any(|&(s, e)| start < e && s < end)
                || self
                    .doc
                    .annotations
                    .ghosts
                    .iter()
                    .any(|g| overlaps(&g.anchor, start, end))
            {
                return Err(
                    "independent ghost range overlaps existing ghost or another range".into(),
                );
            }
        }
        if ranges.is_empty() {
            return Ok(None);
        }
        let saved = self.clone();
        self.reserve_current_ids();
        let before = self.ghost_snapshot();
        for &(start, end) in ranges {
            let reserved = &mut self.reserved_ids;
            if let Err(error) = self
                .doc
                .create_independent_ghost(start, end, || allocate_ghost_id(reserved))
            {
                *self = saved;
                return Err(error.to_string());
            }
        }
        let change = GhostChange {
            body: expected_body.to_string(),
            before,
            after: self.ghost_snapshot(),
        };
        self.ghost_receipts.insert(change.to_json().to_string());
        Ok(Some(change))
    }

    /// Read-only single-step preflight. JS must preflight its whole replay group
    /// against a clone before changing history cursor or surface.
    pub fn preflight_ghost_change(&self, change: &GhostChange, undo: bool) -> Result<(), String> {
        let mut trial = self.clone();
        trial.replay_ghost_change(change, undo)
    }

    /// Preflights an entire ordered history replay group against one clone.
    /// A later conflict cannot leave earlier steps applied to the live session.
    pub fn preflight_ghost_changes(&self, changes: &[(GhostChange, bool)]) -> Result<(), String> {
        let mut trial = self.clone();
        trial.replay_ghost_changes(changes)
    }

    /// Atomically replays an ordered ghost-only group after all guards pass.
    /// Mixed text/annotation groups must be simulated by their controller on
    /// a session clone; this method does not pretend to validate surface edits.
    pub fn replay_ghost_changes(&mut self, changes: &[(GhostChange, bool)]) -> Result<(), String> {
        let mut trial = self.clone();
        for (change, undo) in changes {
            trial.replay_ghost_change(change, *undo)?;
        }
        *self = trial;
        Ok(())
    }

    /// Guarded ghost-only undo/redo. Never restores a stale whole document,
    /// alternatives or overflow, and never releases historical identities.
    pub fn replay_ghost_change(&mut self, change: &GhostChange, undo: bool) -> Result<(), String> {
        if self.raw_block.is_some()
            || self.body() != change.body
            || !self.ghost_receipts.contains(&change.to_json().to_string())
        {
            return Err("unknown or stale ghost transaction receipt".into());
        }
        let (expected, target) = if undo {
            (&change.after, &change.before)
        } else {
            (&change.before, &change.after)
        };
        if self.doc.annotations.ghosts != expected.live
            || self.set_aside.ghosts != expected.set_aside
        {
            return Err("ghost transaction state conflict".into());
        }
        let mut ids = HashSet::new();
        for ghost in target.live.iter().chain(target.set_aside.iter()) {
            if !ids.insert(&ghost.id)
                || !self.reserved_ids.contains(&ghost.id)
                || self
                    .doc
                    .annotations
                    .spans
                    .iter()
                    .chain(self.set_aside.spans.iter())
                    .any(|s| s.id == ghost.id)
            {
                return Err("ghost transaction identity conflict".into());
            }
        }
        if target
            .live
            .iter()
            .any(|g| g.anchor.start >= g.anchor.end || !text_at(self.body(), &g.anchor))
        {
            return Err("ghost transaction anchor conflict".into());
        }
        self.doc.annotations.ghosts = target.live.clone();
        self.set_aside.ghosts = target.set_aside.clone();
        Ok(())
    }
}

/// Replays an issued receipt, never arbitrary promotion ranges. Protected
/// promotion is wired by Lab separately, not exposed as a JS bypass here.
#[wasm_bindgen]
pub fn replay_document_ghost_change(change_json: &str, undo: bool) -> Result<JsValue, JsValue> {
    let value = serde_json::from_str(change_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let change = GhostChange::from_json(&value).map_err(|e| JsValue::from_str(&e))?;
    with_session(|s| {
        s.replay_ghost_change(&change, undo)
            .map(|()| to_js(&s.annotations_json()))
    })
    .map_err(|e| JsValue::from_str(&e))
}

/// Synchronous model preflight for one issued receipt, before JS cursor/surface
/// movement. Whole mixed replay groups require controller-level clone preflight.
#[wasm_bindgen]
pub fn preflight_document_ghost_change(change_json: &str, undo: bool) -> Result<(), JsValue> {
    let value = serde_json::from_str(change_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let change = GhostChange::from_json(&value).map_err(|e| JsValue::from_str(&e))?;
    with_session(|s| s.preflight_ghost_change(&change, undo)).map_err(|e| JsValue::from_str(&e))
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

/// The notice shown when an edit detaches spans, naming how many spans and
/// alternatives were detached.
fn detach_warning(detached: &[Span]) -> Option<String> {
    if detached.is_empty() {
        return None;
    }
    let spans = detached.len();
    let alternatives: usize = detached.iter().map(|s| s.alts.len()).sum();
    Some(format!(
        "This edit detached {spans} {} with {alternatives} {}. {} preserved and saved with the \
         document; undo the edit to re-attach {}.",
        plural(spans, "span", "spans"),
        plural(alternatives, "alternative", "alternatives"),
        plural(alternatives, "It is", "They are"),
        plural(alternatives, "it", "them"),
    ))
}

impl Opened {
    /// `{ body, warning, unresolved }` for JavaScript.
    pub fn to_json(&self) -> Value {
        json!({
            "body": self.body,
            "warning": self.warning,
            "unresolved": self.unresolved,
            "kind": self.kind,
        })
    }
}

impl EditOutcome {
    /// `{ detached, detachedGhosts, reattached, setAside, warning, notice }`
    /// for JavaScript.
    pub fn to_json(&self) -> Value {
        json!({
            "detached": self.detached,
            "detachedGhosts": self.detached_ghosts,
            "reattached": self.reattached,
            "setAside": self.set_aside,
            "warning": self.warning,
            "notice": self.notice,
        })
    }
}

/// `{ words, chars }` for JavaScript.
pub fn counts_json(counts: Counts) -> Value {
    json!({ "words": counts.words, "chars": counts.chars })
}

thread_local! {
    static SESSION: RefCell<DocumentSession> = RefCell::new(DocumentSession::new());
}

/// Runs `f` against the editor's single open document.
pub fn with_session<R>(f: impl FnOnce(&mut DocumentSession) -> R) -> R {
    SESSION.with(|cell| f(&mut cell.borrow_mut()))
}

/// Converts a JSON value to a plain JavaScript object.
fn to_js(value: &Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

/// Opens a `.md` source as the current document and returns
/// `{ body, warning, unresolved }`: put `body` on the editing surface and
/// show `warning` (a string or `null`) as a non-blocking notice.
#[wasm_bindgen]
pub fn open_document(source: &str) -> JsValue {
    to_js(&with_session(|s| s.open(source)).to_json())
}

/// The current document as `.md` text: body plus annotation block, set-aside
/// annotations included.
#[wasm_bindgen]
pub fn save_document() -> String {
    with_session(|s| s.save())
}

/// The current document as clean Markdown: active alternatives, no ghosted
/// text, no overflow, no block.
#[wasm_bindgen]
pub fn export_document() -> String {
    with_session(|s| s.export())
}

/// Downloadable Terraphim graph of the current document's authored synonyms.
///
/// # Errors
/// Refuses malformed annotations and ambiguous cross-group terms.
#[wasm_bindgen]
pub fn export_knowledge_graph() -> Result<String, JsValue> {
    with_session(|s| s.export_knowledge_graph()).map_err(|e| JsValue::from_str(&e))
}

/// `{ words, chars }` of the body, ghosted text included.
#[wasm_bindgen]
pub fn document_counts() -> JsValue {
    to_js(&counts_json(with_session(|s| s.counts())))
}

/// Mirrors a surface edit (`start`, `deletedLength`, `insertedText`, UTF-16)
/// in the model. Returns `{ detached, detachedGhosts, reattached, setAside, warning, notice }`; throws
/// if the edit does not fit the model, in which case call
/// [`sync_document_body`].
#[wasm_bindgen]
pub fn apply_edit(start: u32, deleted_len: u32, inserted: &str) -> Result<JsValue, JsValue> {
    with_session(|s| s.apply_edit(start as usize, deleted_len as usize, inserted))
        .map(|outcome| to_js(&outcome.to_json()))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Replaces the model body with `text` if it differs and re-anchors; returns
/// `{ changed, unresolved, notice }` (`notice` as for [`apply_edit`]).
#[wasm_bindgen]
pub fn sync_document_body(text: &str) -> JsValue {
    to_js(&with_session(|s| sync_json(s, text)))
}

/// [`DocumentSession::sync_body`] plus the current notice, as JSON.
pub fn sync_json(session: &mut DocumentSession, text: &str) -> Value {
    let synced = session.sync_body(text);
    json!({
        "changed": synced.changed,
        "unresolved": synced.unresolved,
        "notice": session.set_aside_notice(),
    })
}

/// The body as the model holds it (should always equal the surface text).
#[wasm_bindgen]
pub fn document_body() -> String {
    with_session(|s| s.body().to_string())
}

/// `{ spans, ghosts, overflow, setAside: { spans, ghosts }, preservedBlock,
/// kg }` of the current document, in UTF-16 offsets. `kg` holds the derived
/// knowledge-graph spans (issue #13, see [`crate::kg::KgSpan::to_json`]);
/// they are never saved.
#[wasm_bindgen]
pub fn document_annotations() -> JsValue {
    to_js(&with_session(|s| {
        let mut value = s.annotations_json();
        if let Value::Object(map) = &mut value {
            map.insert("kg".into(), crate::kg::kg_spans_json(s));
        }
        value
    }))
}

/// Query the current browser document's scoped authored KG synonym concept.
#[wasm_bindgen]
pub fn document_synonyms(span_id: &str, text: &str) -> String {
    with_session(|s| json!(s.synonyms_for(span_id, text)).to_string())
}

/// An error object for JavaScript: `{ ok: false, error, kind }`, where `kind`
/// is `"invalid-range"`, `"stale"` or `"other"`.
fn edit_error_json(error: &EditError) -> Value {
    let kind = match error {
        EditError::InvalidRange { .. } => "invalid-range",
        EditError::StaleAnchor(_) => "stale",
        _ => "other",
    };
    json!({ "ok": false, "error": error.to_string(), "kind": kind })
}

/// [`DocumentSession::ghost`] as JSON: `{ ok: true, id, annotations }` or an
/// error object `{ ok: false, error, kind }`.
pub fn ghost_json(session: &mut DocumentSession, start: usize, end: usize) -> Value {
    match session.ghost(start, end) {
        Ok(id) => json!({ "ok": true, "id": id, "annotations": session.annotations_json() }),
        Err(error) => edit_error_json(&error),
    }
}

/// [`DocumentSession::revive`] as JSON: `{ ok: true, changed, annotations }`
/// or an error object `{ ok: false, error, kind }`.
pub fn revive_json(session: &mut DocumentSession, start: usize, end: usize) -> Value {
    match session.revive(start, end) {
        Ok(changed) => json!({
            "ok": true,
            "changed": changed,
            "annotations": session.annotations_json(),
        }),
        Err(error) => edit_error_json(&error),
    }
}

/// Ghosts `start..end` (UTF-16 offsets into the body) in the current
/// document. Returns `{ ok: true, id, annotations }` with the updated
/// annotations (as [`document_annotations`]), or `{ ok: false, error, kind }`
/// with the document unchanged. Never throws.
#[wasm_bindgen]
pub fn ghost_range(start: u32, end: u32) -> JsValue {
    to_js(&with_session(|s| {
        ghost_json(s, start as usize, end as usize)
    }))
}

/// Revives `start..end` (UTF-16 offsets into the body) in the current
/// document. Returns `{ ok: true, changed, annotations }`, or
/// `{ ok: false, error, kind }` with the document unchanged. Never throws.
#[wasm_bindgen]
pub fn revive_range(start: u32, end: u32) -> JsValue {
    to_js(&with_session(|s| {
        revive_json(s, start as usize, end as usize)
    }))
}

// ---------------------------------------------------------------------------
// #10 The alternatives side panel. Kept in one block, apart from the other
// exports, so parallel additions to this file merge cleanly.
//
// Every panel operation runs model first and returns an [`AltChange`]: the
// one span it touched before and after (`None` when the span did not exist,
// or no longer exists), plus the single contiguous text edit it made to the
// body (only when the active alternative's text changed, a/an fix-up
// included). The editor records that as ONE undo step and replays it with
// [`DocumentSession::alt_restore`], which puts the span back exactly as it
// was snapshotted, so undo and redo never re-run the operation itself.
// ---------------------------------------------------------------------------

use terraphim_alternatives::{Source, SpanKind};

/// The one text edit a panel operation made to the body, in UTF-16 code
/// units of the body before it: `deleted_text` at `start` became
/// `inserted_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltEdit {
    /// Start of the replaced region (UTF-16).
    pub start: usize,
    /// The body text removed.
    pub deleted_text: String,
    /// The body text inserted.
    pub inserted_text: String,
}

/// Outcome of a panel operation (see the block comment above).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltChange {
    /// Id of the span operated on (for a create, the new span's id).
    pub span: String,
    /// The alternative the operation is about afterwards (added, edited or
    /// moved line, newly active line), if any.
    pub index: Option<usize>,
    /// The span before the operation, `None` if it did not exist.
    pub before: Option<Span>,
    /// The span after the operation, `None` if it no longer exists (removing
    /// its last non-original alternative prunes it, R-2.7).
    pub after: Option<Span>,
    /// The body edit made, `None` when only annotations changed.
    pub edit: Option<AltEdit>,
    /// Set-aside bookkeeping, as for [`DocumentSession::apply_edit`].
    pub outcome: EditOutcome,
}

impl AltChange {
    /// `{ span, index, before, after, edit, detached, reattached, setAside,
    /// warning, notice }` for JavaScript. `before`/`after` use the block's
    /// span schema (or `null`); `edit` is `{ start, deletedLength,
    /// deletedText, insertedText }` (UTF-16) or `null`.
    pub fn to_json(&self) -> Value {
        let mut value = self.outcome.to_json();
        let edit = self.edit.as_ref().map(|e| {
            json!({
                "start": e.start,
                "deletedLength": utf16_len(&e.deleted_text),
                "deletedText": e.deleted_text,
                "insertedText": e.inserted_text,
            })
        });
        if let Value::Object(map) = &mut value {
            map.insert("span".into(), json!(self.span));
            map.insert("index".into(), json!(self.index));
            map.insert("before".into(), json!(self.before));
            map.insert("after".into(), json!(self.after));
            map.insert("edit".into(), json!(edit));
        }
        value
    }
}

/// `"word"`, `"sentence"` or `"paragraph"`.
pub fn parse_span_kind(kind: &str) -> Result<SpanKind, String> {
    match kind {
        "word" => Ok(SpanKind::Word),
        "sentence" => Ok(SpanKind::Sentence),
        "paragraph" => Ok(SpanKind::Paragraph),
        other => Err(format!("unknown span kind {other:?}")),
    }
}

/// `"human"` or `"ai"`; the original is never added through the panel.
pub fn parse_alt_source(source: &str) -> Result<Source, String> {
    match source {
        "human" => Ok(Source::Human),
        "ai" => Ok(Source::Ai),
        other => Err(format!("unknown alternative source {other:?}")),
    }
}

/// The single edit turning `old` into `new`: the longest common prefix and
/// then the longest common suffix not overlapping it, on character
/// boundaries. `None` when the two are equal.
pub fn body_edit(old: &str, new: &str) -> Option<AltEdit> {
    if old == new {
        return None;
    }
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let max_suffix = old.len().min(new.len()) - prefix;
    let mut suffix = old
        .bytes()
        .rev()
        .zip(new.bytes().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    Some(AltEdit {
        start: utf16_len(&old[..prefix]),
        deleted_text: old[prefix..old.len() - suffix].to_string(),
        inserted_text: new[prefix..new.len() - suffix].to_string(),
    })
}

impl DocumentSession {
    fn live_span(&self, id: &str) -> Option<&Span> {
        self.doc.annotations.spans.iter().find(|s| s.id == id)
    }

    fn live_span_mut(&mut self, id: &str) -> Result<&mut Span, String> {
        self.doc
            .annotations
            .spans
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| EditError::UnknownSpan(id.to_string()).to_string())
    }

    /// Runs `op` atomically and describes what it did. `op` returns the id of
    /// the span it worked on and the alternative index to report. On error
    /// the session is restored and the error returned.
    fn alt_change(
        &mut self,
        known: Option<&str>,
        op: impl FnOnce(&mut Self) -> Result<(String, Option<usize>), String>,
    ) -> Result<AltChange, String> {
        let saved = self.clone();
        let before = known.and_then(|id| self.live_span(id).cloned());
        let old_body = self.doc.body.clone();
        let (span, index) = match op(self) {
            Ok(done) => done,
            Err(error) => {
                *self = saved;
                return Err(error);
            }
        };
        let edit = body_edit(&old_body, &self.doc.body);
        let mut outcome = EditOutcome::default();
        if let Some(edit) = &edit {
            let start = edit.start;
            let end = start + utf16_len(&edit.deleted_text);
            let inserted = utf16_len(&edit.inserted_text);
            self.set_aside.shift_hints(start, end, &edit.inserted_text);
            outcome.reattached = self.reattach(Some((start, start + inserted)));
        }
        outcome.set_aside = self.set_aside.len();
        outcome.notice = self.set_aside_notice();
        Ok(AltChange {
            after: self.live_span(&span).cloned(),
            span,
            index,
            before,
            edit,
            outcome,
        })
    }

    // A detached span still owns its identity, including while undo can
    // restore it. The document allocator sees only live annotations.
    fn create_reserved_span(
        &mut self,
        kind: SpanKind,
        start: usize,
        end: usize,
    ) -> Result<String, String> {
        self.reserve_current_ids();
        let id = allocate_reserved_id('s', &mut self.reserved_ids).map_err(|e| e.to_string())?;
        let temporary = self
            .doc
            .add_span(kind, start, end)
            .map_err(|e| e.to_string())?;
        self.live_span_mut(&temporary)?.id = id.clone();
        Ok(id)
    }

    /// Creates a span of `kind` over `start..end` together with its first
    /// alternative, as one operation (the panel never leaves an inert span
    /// behind, R-2.7). The new alternative is not made active.
    pub fn alt_create_span(
        &mut self,
        kind: SpanKind,
        start: usize,
        end: usize,
        text: &str,
        source: Source,
    ) -> Result<AltChange, String> {
        self.alt_change(None, |s| {
            let id = s.create_reserved_span(kind, start, end)?;
            if s.live_span(&id).is_some_and(|sp| sp.alts[0].text == text) {
                return Err("the alternative is the same as the original".into());
            }
            let index = s
                .doc
                .add_alternative(&id, text, source, None)
                .map_err(|e| e.to_string())?;
            Ok((id, Some(index)))
        })
    }

    /// Appends an alternative to span `id`. `Ok(None)` when an alternative
    /// with that text already exists (nothing changes).
    pub fn alt_add(
        &mut self,
        id: &str,
        text: &str,
        source: Source,
    ) -> Result<Option<AltChange>, String> {
        let span = self
            .live_span(id)
            .ok_or_else(|| EditError::UnknownSpan(id.to_string()).to_string())?;
        if span.alts.iter().any(|a| a.text == text) {
            return Ok(None);
        }
        self.alt_change(Some(id), |s| {
            let index = s
                .doc
                .add_alternative(id, text, source, None)
                .map_err(|e| e.to_string())?;
            Ok((id.to_string(), Some(index)))
        })
        .map(Some)
    }

    /// Replaces the text of alternative `index` (not the original). Editing
    /// the active alternative rewrites the body through
    /// [`Document::set_active`], so the a/an fix-up applies. `Ok(None)` when
    /// the text is unchanged. Empty text, or text another alternative of the
    /// span already has, is refused.
    pub fn alt_edit(
        &mut self,
        id: &str,
        index: usize,
        text: &str,
    ) -> Result<Option<AltChange>, String> {
        let span = self
            .live_span(id)
            .ok_or_else(|| EditError::UnknownSpan(id.to_string()).to_string())?;
        if index == 0 {
            return Err(EditError::OriginalImmutable.to_string());
        }
        if index >= span.alts.len() {
            return Err(EditError::InvalidIndex {
                id: id.to_string(),
                index,
            }
            .to_string());
        }
        if text.is_empty() {
            return Err(EditError::EmptyText.to_string());
        }
        if span.alts[index].text == text {
            return Ok(None);
        }
        if span.alts.iter().any(|a| a.text == text) {
            return Err(format!("span {id:?} already has the alternative {text:?}"));
        }
        let active = span.active == index;
        self.alt_change(Some(id), |s| {
            s.live_span_mut(id)?.alts[index].text = text.to_string();
            if active {
                s.doc.set_active(id, index).map_err(|e| e.to_string())?;
            }
            Ok((id.to_string(), Some(index)))
        })
        .map(Some)
    }

    /// Removes alternative `index` (not the original). Removing the active
    /// one shows the original again; removing the last non-original one
    /// removes the span (R-2.7), so `after` is `None`.
    pub fn alt_remove(&mut self, id: &str, index: usize) -> Result<AltChange, String> {
        self.alt_change(Some(id), |s| {
            s.doc
                .remove_alternative(id, index)
                .map_err(|e| e.to_string())?;
            Ok((id.to_string(), None))
        })
    }

    /// Moves alternative `from` to position `to` (both past the original,
    /// which stays at index 0). The active alternative stays active, so the
    /// body is unchanged. `Ok(None)` when `from == to`.
    pub fn alt_move(
        &mut self,
        id: &str,
        from: usize,
        to: usize,
    ) -> Result<Option<AltChange>, String> {
        let span = self
            .live_span(id)
            .ok_or_else(|| EditError::UnknownSpan(id.to_string()).to_string())?;
        let len = span.alts.len();
        for index in [from, to] {
            if index == 0 {
                return Err(EditError::OriginalImmutable.to_string());
            }
            if index >= len {
                return Err(EditError::InvalidIndex {
                    id: id.to_string(),
                    index,
                }
                .to_string());
            }
        }
        if from == to {
            return Ok(None);
        }
        self.alt_change(Some(id), |s| {
            let span = s.live_span_mut(id)?;
            let active_text = span.alts[span.active].text.clone();
            let alt = span.alts.remove(from);
            span.alts.insert(to, alt);
            span.active = span
                .alts
                .iter()
                .position(|a| a.text == active_text)
                .expect("alternative texts are unique within a span");
            Ok((id.to_string(), Some(to)))
        })
        .map(Some)
    }

    /// Replays a recorded panel operation (undo or redo): takes span `id` out
    /// of the model (if live), applies the body edit `start`, `deleted_len`,
    /// `inserted` (skipped when empty) as an ordinary edit, so other anchors,
    /// ghosts and set-aside hints follow it, then puts `target` back verbatim
    /// (when `Some`) and refreshes every anchor's context.
    ///
    /// Atomic: if the edit does not fit the body, or `target`'s text is not
    /// at its anchor afterwards or overlaps a live span, nothing changes and
    /// the error is returned (the editor then re-syncs from the surface).
    pub fn alt_restore(
        &mut self,
        id: &str,
        target: Option<Span>,
        start: usize,
        deleted_len: usize,
        inserted: &str,
    ) -> Result<EditOutcome, String> {
        let saved = self.clone();
        let result = self
            .alt_restore_inner(id, target, start, deleted_len, inserted)
            .and_then(|outcome| {
                self.validate_history_annotations()?;
                Ok(outcome)
            });
        if result.is_err() {
            *self = saved;
        }
        result
    }

    fn alt_restore_inner(
        &mut self,
        id: &str,
        target: Option<Span>,
        start: usize,
        deleted_len: usize,
        inserted: &str,
    ) -> Result<EditOutcome, String> {
        let spans = &mut self.doc.annotations.spans;
        let position = spans.iter().position(|s| s.id == id);
        if let Some(i) = position {
            spans.remove(i);
        }
        let mut outcome = if deleted_len > 0 || !inserted.is_empty() {
            self.apply_edit(start, deleted_len, inserted)
                .map_err(|e| e.to_string())?
        } else {
            EditOutcome::default()
        };
        if let Some(span) = target {
            let a = &span.anchor;
            if span.id != id {
                return Err(format!("snapshot is of span {:?}, not {id:?}", span.id));
            }
            if !text_at(&self.doc.body, a) {
                return Err(EditError::StaleAnchor(id.to_string()).to_string());
            }
            let spans = &mut self.doc.annotations.spans;
            if let Some(other) = spans
                .iter()
                .find(|s| s.id == id || overlaps(&s.anchor, a.start, a.end))
            {
                return Err(EditError::Overlap(other.id.clone()).to_string());
            }
            let at = position.unwrap_or(spans.len()).min(spans.len());
            spans.insert(at, span);
        }
        self.doc.refresh_context();
        outcome.set_aside = self.set_aside.len();
        outcome.notice = self.set_aside_notice();
        Ok(outcome)
    }
}

/// Maps a panel result to JavaScript: the change as JSON, `null` when
/// nothing changed, or a thrown error string.
fn alt_js(result: Result<Option<AltChange>, String>) -> Result<JsValue, JsValue> {
    match result {
        Ok(Some(change)) => Ok(to_js(&change.to_json())),
        Ok(None) => Ok(JsValue::NULL),
        Err(error) => Err(JsValue::from_str(&error)),
    }
}

/// Creates a span of `kind` (`"word"`, `"sentence"`, `"paragraph"`) over
/// `start..end` (UTF-16) with its first alternative `text` from `source`
/// (`"human"` or `"ai"`). Returns the change (see [`AltChange::to_json`]);
/// throws, changing nothing, for an invalid or overlapping range, empty text
/// or text equal to the original.
#[wasm_bindgen]
pub fn alt_create_span(
    kind: &str,
    start: u32,
    end: u32,
    text: &str,
    source: &str,
) -> Result<JsValue, JsValue> {
    alt_js(parse_span_kind(kind).and_then(|kind| {
        let source = parse_alt_source(source)?;
        with_session(|s| s.alt_create_span(kind, start as usize, end as usize, text, source))
            .map(Some)
    }))
}

/// Appends alternative `text` from `source` to span `span_id`. Returns the
/// change, or `null` if the span already has that text; throws on error.
#[wasm_bindgen]
pub fn alt_add(span_id: &str, text: &str, source: &str) -> Result<JsValue, JsValue> {
    alt_js(
        parse_alt_source(source)
            .and_then(|source| with_session(|s| s.alt_add(span_id, text, source))),
    )
}

/// Replaces the text of alternative `index` of span `span_id`. Returns the
/// change (with the body edit when it was the active one), or `null` when the
/// text is unchanged; throws on error.
#[wasm_bindgen]
pub fn alt_edit(span_id: &str, index: u32, text: &str) -> Result<JsValue, JsValue> {
    alt_js(with_session(|s| s.alt_edit(span_id, index as usize, text)))
}

/// Removes alternative `index` of span `span_id`. Returns the change (its
/// `after` is `null` when the span was removed); throws on error.
#[wasm_bindgen]
pub fn alt_remove(span_id: &str, index: u32) -> Result<JsValue, JsValue> {
    alt_js(with_session(|s| s.alt_remove(span_id, index as usize)).map(Some))
}

/// Moves alternative `from` of span `span_id` to position `to`. Returns the
/// change, or `null` when `from == to`; throws on error.
#[wasm_bindgen]
pub fn alt_move(span_id: &str, from: u32, to: u32) -> Result<JsValue, JsValue> {
    alt_js(with_session(|s| {
        s.alt_move(span_id, from as usize, to as usize)
    }))
}

/// Replays a recorded panel operation (see [`DocumentSession::alt_restore`]):
/// `target_json` is the span snapshot to put back, or `"null"` to leave the
/// span out. Returns `{ detached, reattached, setAside, warning, notice, ... }`;
/// throws, changing nothing, when the replay does not fit.
#[wasm_bindgen]
pub fn alt_restore(
    span_id: &str,
    target_json: &str,
    start: u32,
    deleted_len: u32,
    inserted: &str,
) -> Result<JsValue, JsValue> {
    let target: Option<Span> =
        serde_json::from_str(target_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    with_session(|s| {
        s.alt_restore(
            span_id,
            target,
            start as usize,
            deleted_len as usize,
            inserted,
        )
    })
    .map(|outcome| to_js(&outcome.to_json()))
    .map_err(|e| JsValue::from_str(&e))
}

// ---------------------------------------------------------------------------
// End of #10 (alternatives side panel).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Moving text (issue #44). Kept in one block, apart from the other exports,
// so parallel additions to this file merge cleanly.
// ---------------------------------------------------------------------------

impl SetAside {
    /// Moves every anchor hint through a move of `start..end` to `to` (pre-move
    /// offsets), mirroring [`Document::move_range`]: a hint wholly inside the
    /// range travels with it, one wholly between the range and `to` shifts by
    /// the moved length, and any other hint keeps its position.
    fn move_hints(&mut self, start: usize, end: usize, to: usize) {
        if to == start || to == end {
            return;
        }
        let len = end - start;
        let forward = to > end;
        let anchors = self
            .spans
            .iter_mut()
            .map(|s| &mut s.anchor)
            .chain(self.ghosts.iter_mut().map(|g| &mut g.anchor));
        for anchor in anchors {
            let (a, b) = (anchor.start, anchor.end);
            let new_start = if start <= a && b <= end {
                if forward {
                    a + (to - end)
                } else {
                    a - (start - to)
                }
            } else if forward && end <= a && b <= to {
                a - len
            } else if !forward && to <= a && b <= start {
                a + len
            } else {
                continue;
            };
            anchor.start = new_start;
            anchor.end = new_start + (b - a);
        }
    }
}

impl DocumentSession {
    /// Moves the body text `start..end` to `to` (UTF-16, an insertion point
    /// in the pre-move body outside the range), carrying the spans and ghosts
    /// inside it. See [`Document::move_range`] for the rules; a move that
    /// would split a span or ghost is refused with
    /// [`EditError::Straddles`] and nothing changes.
    ///
    /// Set-aside hints follow the move the same way, and set-aside items whose
    /// text is back at their anchor are re-attached. The outcome has the shape
    /// of [`apply_edit`](Self::apply_edit)'s: a move never detaches anything,
    /// so `detached` is empty and `warning` is `None`.
    pub fn move_range(
        &mut self,
        start: usize,
        end: usize,
        to: usize,
    ) -> Result<EditOutcome, EditError> {
        let outcome = self.doc.move_range(start, end, to)?;
        let reattached = if outcome.moved {
            self.set_aside.move_hints(start, end, to);
            self.reattach(None)
        } else {
            Vec::new()
        };
        Ok(EditOutcome {
            detached: Vec::new(),
            detached_ghosts: Vec::new(),
            reattached,
            set_aside: self.set_aside.len(),
            warning: None,
            notice: self.set_aside_notice(),
        })
    }
}

/// Moves the body text `start..end` to `to` (UTF-16 code units; `to` is an
/// insertion point in the pre-move body, outside the range), carrying the
/// spans and ghosts inside the range. Returns
/// `{ detached, detachedGhosts, reattached, setAside, warning, notice }` like [`apply_edit`];
/// throws (changing nothing) if the range or destination is invalid or the
/// move would split a span or ghost.
///
/// `MarkdownEditor.moveRange` calls this and then updates the editing surface
/// without mirroring the change through [`apply_edit`].
#[wasm_bindgen]
pub fn move_document_range(start: u32, end: u32, to: u32) -> Result<JsValue, JsValue> {
    with_session(|s| s.move_range(start as usize, end as usize, to as usize))
        .map(|outcome| to_js(&outcome.to_json()))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ---------------------------------------------------------------------------
// End of moving text (issue #44).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// In-place cycling of alternatives (issue #9). Kept in one block, apart from
// the other exports, so parallel additions to this file merge cleanly.
// ---------------------------------------------------------------------------

/// The one text edit a swap made to the body, in UTF-16 code units of the
/// pre-swap body: `deleted_text` at `start` was replaced by `inserted_text`.
/// It covers the a/an fix-up (if any) and the span text as one contiguous
/// region; everything before and after it is unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapEdit {
    /// Start of the replaced region (UTF-16).
    pub start: usize,
    /// The body text the swap removed.
    pub deleted_text: String,
    /// The body text the swap inserted.
    pub inserted_text: String,
}

/// Outcome of [`DocumentSession::set_active`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapOutcome {
    /// The span swapped.
    pub span: String,
    /// The previously active alternative.
    pub from: usize,
    /// The newly active alternative.
    pub to: usize,
    /// The text edit made, or `None` when `to` was already active.
    pub edit: Option<SwapEdit>,
    /// The span's range after the call (UTF-16, `start..end`).
    pub range: (usize, usize),
    /// The usual bookkeeping. A swap never detaches anything, so `detached`
    /// is empty and `warning` is `None`.
    pub outcome: EditOutcome,
}

impl SwapOutcome {
    /// `{ span, from, to, edit, range, detached, reattached, setAside,
    /// warning, notice }` for JavaScript; `edit` is `{ start, deletedLength,
    /// deletedText, insertedText }` (UTF-16) or `null`, and `range` is
    /// `{ start, end }`.
    pub fn to_json(&self) -> Value {
        let mut value = self.outcome.to_json();
        let edit = self.edit.as_ref().map(|e| {
            json!({
                "start": e.start,
                "deletedLength": utf16_len(&e.deleted_text),
                "deletedText": e.deleted_text,
                "insertedText": e.inserted_text,
            })
        });
        if let Value::Object(map) = &mut value {
            map.insert("span".into(), json!(self.span));
            map.insert("from".into(), json!(self.from));
            map.insert("to".into(), json!(self.to));
            map.insert("edit".into(), json!(edit));
            map.insert(
                "range".into(),
                json!({ "start": self.range.0, "end": self.range.1 }),
            );
        }
        value
    }
}

impl DocumentSession {
    /// Makes alternative `index` of span `id` active through
    /// [`Document::set_active`], which swaps the span's text, applies the
    /// a/an fix-up to an immediately preceding article (R-2.6) and refreshes
    /// the anchors' context. Returns the exact text edit it made, so the
    /// editing surface can apply the same change without mirroring it back.
    ///
    /// Atomic: an unknown id, an invalid index or a stale anchor returns the
    /// crate's error and changes nothing. Making the active alternative
    /// active again is a no-op (`edit` is `None` and the article is left
    /// alone). Set-aside hints after the edit shift, and set-aside items
    /// whose text is back are re-attached, as for
    /// [`apply_edit`](Self::apply_edit).
    pub fn set_active(&mut self, id: &str, index: usize) -> Result<SwapOutcome, EditError> {
        let span = self
            .doc
            .annotations
            .spans
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| EditError::UnknownSpan(id.to_string()))?;
        if index >= span.alts.len() {
            return Err(EditError::InvalidIndex {
                id: id.to_string(),
                index,
            });
        }
        let from = span.active;
        let (old_start, old_end) = (span.anchor.start, span.anchor.end);
        let mut swap = SwapOutcome {
            span: id.to_string(),
            from,
            to: index,
            edit: None,
            range: (old_start, old_end),
            outcome: EditOutcome::default(),
        };
        if index != from {
            let old_body = self.doc.body.clone();
            self.doc.set_active(id, index)?;
            let edit = swap_edit(&self.doc, id, &old_body, old_start, old_end);
            let start = edit.start;
            let end = start + utf16_len(&edit.deleted_text);
            let inserted = utf16_len(&edit.inserted_text);
            self.set_aside.shift_hints(start, end, &edit.inserted_text);
            swap.outcome.reattached = self.reattach(Some((start, start + inserted)));
            swap.edit = Some(edit);
            // The span's new text ends where the edit's inserted text ends.
            let len = self
                .doc
                .annotations
                .spans
                .iter()
                .find(|s| s.id == id)
                .map_or(0, |s| s.anchor.end - s.anchor.start);
            swap.range = (start + inserted - len, start + inserted);
        }
        swap.outcome.set_aside = self.set_aside.len();
        swap.outcome.notice = self.set_aside_notice();
        Ok(swap)
    }
}

/// The single edit that turned `old_body` (span `id` at `old_start..old_end`,
/// UTF-16) into `doc.body` after a successful [`Document::set_active`]. Only
/// an article before the span and the span itself can have changed, so the
/// bodies agree up to the first difference (at most the span start) and
/// after the span.
fn swap_edit(
    doc: &Document,
    id: &str,
    old_body: &str,
    old_start: usize,
    old_end: usize,
) -> SwapEdit {
    let anchor = &doc
        .annotations
        .spans
        .iter()
        .find(|s| s.id == id)
        .expect("set_active keeps the span")
        .anchor;
    let body = &doc.body;
    // set_active checked the old anchor and wrote the new one, so all four
    // offsets fall on character boundaries.
    let old_span_byte = utf16_to_byte(old_body, old_start).expect("checked anchor");
    let old_end_byte = utf16_to_byte(old_body, old_end).expect("checked anchor");
    let new_span_byte = utf16_to_byte(body, anchor.start).expect("fresh anchor");
    let new_end_byte = utf16_to_byte(body, anchor.end).expect("fresh anchor");
    let cap = old_span_byte.min(new_span_byte);
    let mut prefix = old_body
        .bytes()
        .zip(body.bytes())
        .take(cap)
        .take_while(|(a, b)| a == b)
        .count();
    while !old_body.is_char_boundary(prefix) || !body.is_char_boundary(prefix) {
        prefix -= 1;
    }
    SwapEdit {
        start: utf16_len(&old_body[..prefix]),
        deleted_text: old_body[prefix..old_end_byte].to_string(),
        inserted_text: body[prefix..new_end_byte].to_string(),
    }
}

/// Makes alternative `index` of span `span_id` active (R-2.4): the span's
/// text is swapped and a preceding "a"/"an" fixed up (R-2.6) in the model.
/// Returns `{ span, from, to, edit, range, detached, reattached, setAside,
/// warning, notice }`, where `edit` (`{ start, deletedLength, deletedText,
/// insertedText }` in UTF-16 code units, or `null` when `index` was already
/// active) is the exact change made to the body and `range` (`{ start, end }`)
/// is the span's text afterwards. Throws, changing nothing,
/// for an unknown span, an invalid index or a stale anchor.
///
/// `MarkdownEditor.swapAlternative` calls this and then applies `edit` to
/// the editing surface without mirroring it through [`apply_edit`].
#[wasm_bindgen]
pub fn set_active_alternative(span_id: &str, index: u32) -> Result<JsValue, JsValue> {
    with_session(|s| s.set_active(span_id, index as usize))
        .map(|outcome| to_js(&outcome.to_json()))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

// ---------------------------------------------------------------------------
// End of in-place cycling (issue #9).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Appending alternatives from a provider (issue #13, R-8.6). Kept in one
// block, apart from the other exports, so parallel additions to this file
// merge cleanly.
//
// It reuses the #10 panel's [`AltChange`] (span before and after), so the
// editor records an append as ONE undo step and replays it with
// [`DocumentSession::alt_restore`] like any panel operation.
// ---------------------------------------------------------------------------

impl DocumentSession {
    /// Appends `texts` as alternatives (with `source` and `model`) to the
    /// live span exactly over `start..end` (UTF-16), after the alternatives
    /// it already has, creating a `kind` span there when there is none.
    /// Texts the span already offers (its original included) are skipped,
    /// so repeating the call adds nothing: `Ok(None)`. The active
    /// alternative is unchanged, so the body is never edited.
    ///
    /// Atomic: a range that partly overlaps another span or is invalid is
    /// refused and changes nothing.
    pub fn append_alternatives(
        &mut self,
        kind: SpanKind,
        start: usize,
        end: usize,
        texts: &[String],
        source: Source,
        model: Option<&str>,
    ) -> Result<Option<AltChange>, String> {
        let suggestions: Vec<Suggestion> = texts
            .iter()
            .map(|text| Suggestion {
                text: text.clone(),
                source,
                model: model.map(str::to_owned),
            })
            .collect();
        self.append_suggestions(kind, start, end, &suggestions)
    }

    /// [`append_alternatives`](Self::append_alternatives) for suggestions
    /// that each carry their own `source` and `model` (the provider seam,
    /// R-8.7). They are appended in order as ONE change; a text the span
    /// already offers, or an earlier suggestion repeats, is skipped, so the
    /// first provider to offer a text keeps its provenance.
    pub fn append_suggestions(
        &mut self,
        kind: SpanKind,
        start: usize,
        end: usize,
        suggestions: &[Suggestion],
    ) -> Result<Option<AltChange>, String> {
        let existing = self
            .doc
            .annotations
            .spans
            .iter()
            .find(|s| s.anchor.start == start && s.anchor.end == end);
        let known = existing.map(|s| s.id.clone());
        let mut offered: HashSet<String> = match existing {
            Some(span) => span.alts.iter().map(|a| a.text.clone()).collect(),
            None => utf16_to_byte(&self.doc.body, start)
                .zip(utf16_to_byte(&self.doc.body, end))
                .map(|(a, b)| self.doc.body[a..b].to_string())
                .into_iter()
                .collect(),
        };
        let fresh: Vec<Suggestion> = suggestions
            .iter()
            .filter(|s| !s.text.is_empty() && offered.insert(s.text.clone()))
            .cloned()
            .collect();
        if fresh.is_empty() {
            return Ok(None);
        }
        self.alt_change(known.as_deref(), |s| {
            let id = match known.clone() {
                Some(id) => id,
                None => s.create_reserved_span(kind, start, end)?,
            };
            let mut first = None;
            for suggestion in fresh {
                let index = s
                    .doc
                    .add_alternative(&id, suggestion.text, suggestion.source, suggestion.model)
                    .map_err(|e| e.to_string())?;
                first.get_or_insert(index);
            }
            Ok((id, first))
        })
        .map(Some)
    }
}

// ---------------------------------------------------------------------------
// End of appending alternatives (issue #13).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Overflow panel: stash, pull back, panel text (issue #12). Kept in one
// block, apart from the other exports, so parallel additions to this file
// merge cleanly.
//
// The overflow text lives in `Annotations::overflow` and is written into the
// annotation block on save (omitted when empty); export never includes it.
// The panel (`public/js/overflow.js`) writes what the author types with
// `set_document_overflow`. A stash removes a body range through the normal
// edit path (`apply_edit`) and appends the removed text to the overflow in
// one call; the editor records both in one undo step and replays the
// overflow half with `replay_document_overflow`.
// ---------------------------------------------------------------------------

/// Why an overflow change was refused. Nothing changes on any error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowError {
    /// A malformed annotation block is being preserved verbatim, so the
    /// overflow could not be saved (save writes the raw block back, not the
    /// model's overflow). Stashing would lose the stashed text.
    PreservedBlock,
    /// A stash needs a non-empty range.
    EmptyRange,
    /// A pull back found no text: the overflow selection (or the line at the
    /// overflow caret) is empty or blank.
    NothingToPull,
    /// The range does not fit the body (see [`EditError`]).
    Edit(EditError),
}

impl std::fmt::Display for OverflowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PreservedBlock => f.write_str(
                "the annotation block could not be read and is preserved unchanged, so \
                 Overflow cannot be saved until it is repaired",
            ),
            Self::EmptyRange => f.write_str("nothing selected to stash"),
            Self::NothingToPull => f.write_str("no Overflow text to pull back"),
            Self::Edit(error) => write!(f, "{error}"),
        }
    }
}

impl OverflowError {
    /// `"preserved-block"`, `"empty"` (also for nothing to pull back),
    /// `"invalid-range"`, `"stale"` or `"other"`, for JavaScript.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::PreservedBlock => "preserved-block",
            Self::EmptyRange | Self::NothingToPull => "empty",
            Self::Edit(EditError::InvalidRange { .. }) => "invalid-range",
            Self::Edit(EditError::StaleAnchor(_)) => "stale",
            Self::Edit(_) => "other",
        }
    }
}

/// Outcome of [`DocumentSession::stash`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashOutcome {
    /// Start of the removed body text (UTF-16).
    pub start: usize,
    /// The body text removed (and appended to the overflow).
    pub text: String,
    /// The overflow before the stash.
    pub overflow_before: String,
    /// The overflow after the stash.
    pub overflow_after: String,
    /// The bookkeeping of the removal, as for
    /// [`apply_edit`](DocumentSession::apply_edit).
    pub outcome: EditOutcome,
}

impl StashOutcome {
    /// `{ edit: { start, deletedLength, deletedText, insertedText },
    /// overflow: { before, after }, detached, detachedGhosts, reattached,
    /// setAside, warning, notice }` for JavaScript.
    pub fn to_json(&self) -> Value {
        let mut value = self.outcome.to_json();
        if let Value::Object(map) = &mut value {
            map.insert(
                "edit".into(),
                json!({
                    "start": self.start,
                    "deletedLength": utf16_len(&self.text),
                    "deletedText": self.text,
                    "insertedText": "",
                }),
            );
            map.insert(
                "overflow".into(),
                json!({ "before": self.overflow_before, "after": self.overflow_after }),
            );
        }
        value
    }
}

/// The text a pull back inserts, and where (see
/// [`DocumentSession::pull_back_text`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullBack {
    /// Where the text goes into the body (UTF-16).
    pub start: usize,
    /// The text to insert: the pulled overflow text with its separators.
    pub text: String,
}

/// `paragraph` as a pull back inserts it into `body` at byte `at`, exactly
/// as terraphim_lsp_core's `pull_back` does (`separated`, terraphim-ai
/// #3472), so the editor and terraphim-lsp agree. In the middle of a line
/// it goes in as it is. At the start or end of a line, or on a blank line,
/// it becomes a paragraph of its own: set off from the text before and
/// after it by one blank line (counting the blank lines already there), in
/// the document's line ending.
pub fn pull_back_separated(body: &str, at: usize, paragraph: &str) -> String {
    let (before, after) = body.split_at(at);
    let at_start = before.is_empty() || before.ends_with('\n');
    let at_end = after.is_empty() || after.starts_with(['\r', '\n']);
    if !at_start && !at_end {
        return paragraph.to_string();
    }
    let eol = if body.contains("\r\n") { "\r\n" } else { "\n" };
    let lead = match (before.is_empty(), at_start) {
        (true, _) => 0,
        (false, true) => 2usize.saturating_sub(line_breaks(before.chars().rev())),
        (false, false) => 2,
    };
    let trail = match (after.is_empty(), at_end) {
        (true, _) => 0,
        (false, true) => 2usize.saturating_sub(line_breaks(after.chars())),
        (false, false) => 2,
    };
    format!("{}{paragraph}{}", eol.repeat(lead), eol.repeat(trail))
}

/// The line breaks at the start of `chars`, up to two (`\r` is part of a
/// `\r\n` break, not a break of its own).
fn line_breaks(chars: impl Iterator<Item = char>) -> usize {
    let mut breaks = 0;
    for c in chars {
        match c {
            '\n' => breaks += 1,
            '\r' => {}
            _ => break,
        }
        if breaks == 2 {
            break;
        }
    }
    breaks
}

/// `current` with `text` appended as a new entry: directly when `current`
/// is empty, otherwise after a blank line (completing a trailing newline).
pub fn append_overflow(current: &str, text: &str) -> String {
    format!("{current}{}{text}", overflow_separator(current))
}

/// The separator [`append_overflow`] puts between `current` and a new entry:
/// nothing at the start or after a blank line, otherwise enough newlines to
/// leave one blank line.
fn overflow_separator(current: &str) -> &'static str {
    if current.is_empty() || current.ends_with("\n\n") {
        ""
    } else if current.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    }
}

/// Outcome of [`rebase_overflow`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rebased {
    /// The overflow after the replay.
    pub text: String,
    /// Whether the recorded change was applied. `false` means it could not be
    /// placed safely and `text` is `current` unchanged.
    pub applied: bool,
}

/// Replays a recorded overflow change `from` -> `to` (an undo or redo of a
/// stash) onto `current`, which may hold text typed in the panel since.
///
/// A stash only ever appends, so one of the two texts is the other plus a
/// chunk (separator and stashed text), and the step itself records the
/// chunk's position: it starts at the length of the shorter text (the
/// pre-stash overflow). The chunk is identified by that position, never by
/// searching for its text, so an identical copy typed by the author is never
/// mistaken for it.
///
/// * `current == from`: exactly `to`.
/// * Undo (`to` is `from` minus the chunk): the chunk is removed only when
///   everything before its recorded position is unchanged (`current` starts
///   with `to`) and the chunk is still there, intact, right after it. Text
///   typed after the chunk is kept. Otherwise (text changed before the
///   chunk, or the chunk itself edited or deleted) nothing is removed:
///   `current` is kept and `applied` is `false`, so the editor can say so.
/// * Redo (`to` is `from` plus the chunk): the chunk is inserted at its
///   recorded position when everything before it is unchanged (`current`
///   starts with `from`), otherwise appended at the end. Always applied.
/// * Anything else: `current` is kept, not applied.
pub fn rebase_overflow(current: &str, from: &str, to: &str) -> Rebased {
    let kept = || Rebased {
        text: current.to_string(),
        applied: false,
    };
    if current == from {
        return Rebased {
            text: to.to_string(),
            applied: true,
        };
    }
    if let Some(chunk) = from.strip_prefix(to).filter(|c| !c.is_empty()) {
        return match current
            .strip_prefix(to)
            .and_then(|rest| rest.strip_prefix(chunk))
        {
            Some(after_chunk) => Rebased {
                text: format!("{to}{after_chunk}"),
                applied: true,
            },
            None => kept(),
        };
    }
    if let Some(chunk) = to.strip_prefix(from).filter(|c| !c.is_empty()) {
        let text = match current.strip_prefix(from) {
            Some(rest) => format!("{from}{chunk}{rest}"),
            // The chunk carries the separator chosen for `from`; re-append
            // the entry itself so the separator suits `current`.
            None => append_overflow(
                current,
                chunk
                    .strip_prefix(overflow_separator(from))
                    .unwrap_or(chunk),
            ),
        };
        return Rebased {
            text,
            applied: true,
        };
    }
    kept()
}

impl DocumentSession {
    /// The overflow (stash) text.
    pub fn overflow(&self) -> &str {
        &self.doc.annotations.overflow
    }

    /// Replaces the overflow text (the panel's content). Refused, changing
    /// nothing, while a malformed block is preserved.
    pub fn set_overflow(&mut self, text: &str) -> Result<(), OverflowError> {
        if self.raw_block.is_some() {
            return Err(OverflowError::PreservedBlock);
        }
        self.doc.annotations.overflow = text.to_string();
        Ok(())
    }

    /// Stash (R-6.3): removes the body text `start..end` (UTF-16) through
    /// [`apply_edit`](Self::apply_edit), so spans whose text it touches are
    /// detached and set aside, and ghosts wholly inside it are set aside
    /// (both re-attach when the text comes back, for example on undo), then
    /// appends the removed text to the overflow with [`append_overflow`].
    ///
    /// Atomic: an empty, reversed or out-of-bounds range, a range splitting a
    /// character, a refused edit, or a preserved malformed block (whose save
    /// would drop the overflow) changes neither the body nor the overflow.
    pub fn stash(&mut self, start: usize, end: usize) -> Result<StashOutcome, OverflowError> {
        if self.raw_block.is_some() {
            return Err(OverflowError::PreservedBlock);
        }
        if start >= end {
            return Err(if start == end {
                OverflowError::EmptyRange
            } else {
                OverflowError::Edit(EditError::InvalidRange { start, end })
            });
        }
        let body = &self.doc.body;
        let (Some(a), Some(b)) = (utf16_to_byte(body, start), utf16_to_byte(body, end)) else {
            return Err(OverflowError::Edit(EditError::InvalidRange { start, end }));
        };
        let text = body[a..b].to_string();
        let outcome = self
            .apply_edit(start, end - start, "")
            .map_err(OverflowError::Edit)?;
        let before = std::mem::take(&mut self.doc.annotations.overflow);
        let after = append_overflow(&before, &text);
        self.doc.annotations.overflow = after.clone();
        Ok(StashOutcome {
            start,
            text,
            overflow_before: before,
            overflow_after: after,
            outcome,
        })
    }

    /// Pull back (R-6.4): the text that copying overflow text into the body
    /// at `at` (UTF-16) inserts, and where. The overflow text is the range
    /// `from..to` (UTF-16; the panel selection) or, when it is empty, the
    /// line at `from`, without its own line breaks at either end. It is
    /// separated as terraphim-lsp separates a pulled paragraph (see
    /// [`pull_back_separated`]); `at` inside a `\r\n` pair counts as after
    /// it. A copy: neither the body nor the overflow changes here; the
    /// editor inserts the text through its normal edit path, one undo step.
    pub fn pull_back_text(
        &self,
        at: usize,
        from: usize,
        to: usize,
    ) -> Result<PullBack, OverflowError> {
        let overflow = &self.doc.annotations.overflow;
        let (Some(mut start), Some(mut end)) = (
            utf16_to_byte(overflow, from.min(to)),
            utf16_to_byte(overflow, from.max(to)),
        ) else {
            return Err(OverflowError::Edit(EditError::InvalidRange {
                start: from,
                end: to,
            }));
        };
        if start == end {
            start = overflow[..start].rfind('\n').map_or(0, |i| i + 1);
            end += overflow[end..].find('\n').unwrap_or(overflow.len() - end);
        }
        let picked = overflow[start..end].trim_matches(['\r', '\n']);
        if picked.trim().is_empty() {
            return Err(OverflowError::NothingToPull);
        }
        let body = &self.doc.body;
        let Some(mut at_byte) = utf16_to_byte(body, at) else {
            return Err(OverflowError::Edit(EditError::InvalidRange {
                start: at,
                end: at,
            }));
        };
        if at_byte > 0 && body[..at_byte].ends_with('\r') && body[at_byte..].starts_with('\n') {
            at_byte += 1;
        }
        Ok(PullBack {
            start: utf16_len(&body[..at_byte]),
            text: pull_back_separated(body, at_byte, picked),
        })
    }

    /// Replays a recorded overflow change (see [`rebase_overflow`]) and
    /// returns the outcome. Refused while a malformed block is
    /// preserved.
    pub fn replay_overflow(&mut self, from: &str, to: &str) -> Result<Rebased, OverflowError> {
        let next = rebase_overflow(self.overflow(), from, to);
        self.set_overflow(&next.text)?;
        Ok(next)
    }
}

fn overflow_error_json(error: &OverflowError) -> Value {
    json!({ "ok": false, "error": error.to_string(), "kind": error.kind() })
}

/// The overflow (stash) text of the current document.
#[wasm_bindgen]
pub fn document_overflow() -> String {
    with_session(|s| s.overflow().to_string())
}

/// [`DocumentSession::set_overflow`] as JSON: `{ ok: true, overflow }` or
/// `{ ok: false, error, kind }`.
pub fn set_overflow_json(session: &mut DocumentSession, text: &str) -> Value {
    match session.set_overflow(text) {
        Ok(()) => json!({ "ok": true, "overflow": session.overflow() }),
        Err(error) => overflow_error_json(&error),
    }
}

/// Replaces the overflow text (what the Overflow panel holds). Returns
/// `{ ok: true, overflow }`, or `{ ok: false, error, kind }` (kind
/// `"preserved-block"`) with nothing changed. Never throws.
#[wasm_bindgen]
pub fn set_document_overflow(text: &str) -> JsValue {
    to_js(&with_session(|s| set_overflow_json(s, text)))
}

/// Stashes the body text `start..end` (UTF-16) in the overflow: removed
/// from the body through the normal edit path and appended to the overflow
/// (R-6.3). Returns `{ edit, overflow: { before, after }, detached,
/// detachedGhosts, reattached, setAside, warning, notice }`; throws,
/// changing nothing, for an empty or invalid range or while a malformed
/// block is preserved.
///
/// `MarkdownEditor.stashRange` calls this and then applies `edit` to the
/// editing surface without mirroring it through [`apply_edit`].
#[wasm_bindgen]
pub fn stash_document_range(start: u32, end: u32) -> Result<JsValue, JsValue> {
    with_session(|s| s.stash(start as usize, end as usize))
        .map(|outcome| to_js(&outcome.to_json()))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// The text a pull back (R-6.4, a copy) inserts at `at` (UTF-16) and where:
/// the overflow range `from..to` (UTF-16; when empty, the line at `from`),
/// separated as terraphim-lsp separates it. Returns `{ start, text }` and
/// changes nothing; throws when there is nothing to pull back or for an
/// invalid offset or range. `TeOverflowPanel.pullBack` inserts `text` at
/// `start` through the normal edit path.
#[wasm_bindgen]
pub fn overflow_pull_back_text(at: u32, from: u32, to: u32) -> Result<JsValue, JsValue> {
    with_session(|s| s.pull_back_text(at as usize, from as usize, to as usize))
        .map(|pull| to_js(&json!({ "start": pull.start, "text": pull.text })))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// [`DocumentSession::replay_overflow`] as JSON: `{ ok: true, overflow,
/// applied }` or `{ ok: false, error, kind }`.
pub fn replay_overflow_json(session: &mut DocumentSession, from: &str, to: &str) -> Value {
    match session.replay_overflow(from, to) {
        Ok(next) => json!({ "ok": true, "overflow": next.text, "applied": next.applied }),
        Err(error) => overflow_error_json(&error),
    }
}

/// Replays a recorded overflow change `from` -> `to` (undo or redo of a
/// stash) onto the current overflow, keeping text typed in the panel since
/// (see [`rebase_overflow`]). Returns `{ ok: true, overflow, applied }`
/// (`applied: false` when an undo could not find the stashed text intact at
/// its recorded position and left the overflow unchanged) or
/// `{ ok: false, error, kind }`. Never throws.
#[wasm_bindgen]
pub fn replay_document_overflow(from: &str, to: &str) -> JsValue {
    to_js(&with_session(|s| replay_overflow_json(s, from, to)))
}

// ---------------------------------------------------------------------------
// End of overflow panel (issue #12).
// ---------------------------------------------------------------------------

/// Limits apply only to transaction transports, not ordinary document editing.
pub const HISTORY_JSON_LIMIT: usize = 16 * 1024 * 1024;
pub const HISTORY_OPERATION_LIMIT: usize = 4096;

/// Typed refusal; no failed operation changes the pending checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryTransactionError {
    pub kind: &'static str,
    pub message: String,
}

impl std::fmt::Display for HistoryTransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for HistoryTransactionError {}

fn history_error(kind: &'static str, message: impl Into<String>) -> HistoryTransactionError {
    HistoryTransactionError {
        kind,
        message: message.into(),
    }
}

fn history_bytes(text: &str) -> Result<(), HistoryTransactionError> {
    if text.len() > HISTORY_JSON_LIMIT {
        return Err(history_error(
            "limit",
            "transaction argument exceeds 16 MiB",
        ));
    }
    Ok(())
}

/// Allocation-free top-level array item budget check before serde decoding.
/// This is not a JSON parser: serde remains the syntax/recursion authority.
fn bounded_history_array(text: &str) -> Result<Value, HistoryTransactionError> {
    history_bytes(text)?;
    if !text.trim_start().starts_with('[') {
        return Err(history_error("invalid-payload", "expected an array"));
    }
    let (mut depth, mut quoted, mut escaped, mut item, mut count) =
        (0usize, false, false, false, 0usize);
    for byte in text.bytes() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        if depth == 1 && byte == b',' {
            item = false;
        } else if depth == 1 && !item && !byte.is_ascii_whitespace() && byte != b']' {
            count = count
                .checked_add(1)
                .ok_or_else(|| history_error("limit", "item count exhausted"))?;
            if count > HISTORY_OPERATION_LIMIT {
                return Err(history_error("limit", "more than 4096 items"));
            }
            item = true;
        }
        match byte {
            b'"' => quoted = true,
            b'[' | b'{' => {
                depth = depth
                    .checked_add(1)
                    .ok_or_else(|| history_error("limit", "nesting exhausted"))?
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    let value: Value = serde_json::from_str(text)
        .map_err(|_| history_error("invalid-payload", "invalid JSON or excessive nesting"))?;
    let array = value
        .as_array()
        .ok_or_else(|| history_error("invalid-payload", "expected an array"))?;
    if array.len() > HISTORY_OPERATION_LIMIT {
        return Err(history_error("limit", "more than 4096 items"));
    }
    Ok(value)
}

fn history_fields(value: &Value, fields: &[&str]) -> Result<(), HistoryTransactionError> {
    let object = value
        .as_object()
        .ok_or_else(|| history_error("invalid-payload", "expected an object"))?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err(history_error(
            "invalid-payload",
            "unknown or missing history fields",
        ));
    }
    Ok(())
}

fn history_string(value: &Value, field: &str) -> Result<String, HistoryTransactionError> {
    let text = value[field]
        .as_str()
        .ok_or_else(|| history_error("invalid-payload", "expected a string"))?;
    history_bytes(text)?;
    Ok(text.to_string())
}

fn history_integer(value: &Value, field: &str) -> Result<usize, HistoryTransactionError> {
    value[field]
        .as_u64()
        .filter(|n| *n <= u32::MAX as u64)
        .map(|n| n as usize)
        .ok_or_else(|| {
            history_error(
                "invalid-payload",
                "expected an exact nonnegative 32-bit integer",
            )
        })
}

#[derive(Debug, Clone)]
struct HistoryTextEdit {
    start: usize,
    deleted: String,
    inserted: String,
}

#[derive(Debug, Clone)]
enum HistoryKind {
    Edit,
    Ghost {
        change: GhostChange,
        undo: bool,
    },
    Alt {
        span: String,
        target: Option<Span>,
    },
    Move {
        start: usize,
        end: usize,
        to: usize,
    },
    Swap {
        span: String,
        from: usize,
        to: usize,
    },
    Overflow {
        before: String,
        after: String,
    },
}

/// A strictly decoded operation. Private fields prevent unchecked construction.
#[derive(Debug, Clone)]
pub struct HistoryOperation {
    body: String,
    edit: Option<HistoryTextEdit>,
    kind: HistoryKind,
}

/// Decode the six real history kinds, rejecting unknown/conflicting metadata.
pub fn parse_history_operations(
    text: &str,
) -> Result<Vec<HistoryOperation>, HistoryTransactionError> {
    let value = bounded_history_array(text)?;
    let mut operations = Vec::new();
    for value in value.as_array().expect("checked array") {
        let kind = value["kind"]
            .as_str()
            .ok_or_else(|| history_error("invalid-payload", "history kind is required"))?;
        let fields: &[&str] = match kind {
            "edit" => &["kind", "body", "start", "deletedText", "insertedText"],
            "ghost" => &["kind", "body", "change", "undo"],
            "alt" => &[
                "kind",
                "body",
                "start",
                "deletedText",
                "insertedText",
                "span",
                "target",
            ],
            "move" => &[
                "kind",
                "body",
                "start",
                "deletedText",
                "insertedText",
                "move",
            ],
            "swap" => &[
                "kind",
                "body",
                "start",
                "deletedText",
                "insertedText",
                "span",
                "from",
                "to",
            ],
            "overflow" => &[
                "kind",
                "body",
                "start",
                "deletedText",
                "insertedText",
                "before",
                "after",
            ],
            _ => return Err(history_error("invalid-payload", "unknown history kind")),
        };
        history_fields(value, fields)?;
        let body = history_string(value, "body")?;
        let edit = if kind == "ghost" {
            None
        } else {
            Some(HistoryTextEdit {
                start: history_integer(value, "start")?,
                deleted: history_string(value, "deletedText")?,
                inserted: history_string(value, "insertedText")?,
            })
        };
        let kind = match kind {
            "edit" => HistoryKind::Edit,
            "ghost" => {
                let change = &value["change"];
                history_fields(change, &["body", "before", "after"])?;
                for side in ["before", "after"] {
                    history_fields(&change[side], &["live", "setAside", "reservedIds"])?;
                }
                HistoryKind::Ghost {
                    change: GhostChange::from_json(change)
                        .map_err(|_| history_error("invalid-payload", "invalid ghost receipt"))?,
                    undo: value["undo"].as_bool().ok_or_else(|| {
                        history_error("invalid-payload", "ghost direction must be boolean")
                    })?,
                }
            }
            "alt" => {
                let target: Option<Span> = serde_json::from_value(value["target"].clone())
                    .map_err(|_| {
                        history_error("invalid-payload", "invalid alternative snapshot")
                    })?;
                if let Some(span) = &target {
                    // Reuse the schema's semantic validation, not a duplicate
                    // alternative/anchor validator or serialized session clone.
                    let mut fixture = Document::new("");
                    fixture.annotations.spans.push(span.clone());
                    parse(&write(&fixture)).map_err(|_| {
                        history_error("invalid-payload", "invalid alternative snapshot")
                    })?;
                }
                HistoryKind::Alt {
                    span: history_string(value, "span")?,
                    target,
                }
            }
            "move" => {
                history_fields(&value["move"], &["start", "end", "to"])?;
                HistoryKind::Move {
                    start: history_integer(&value["move"], "start")?,
                    end: history_integer(&value["move"], "end")?,
                    to: history_integer(&value["move"], "to")?,
                }
            }
            "swap" => HistoryKind::Swap {
                span: history_string(value, "span")?,
                from: history_integer(value, "from")?,
                to: history_integer(value, "to")?,
            },
            "overflow" => HistoryKind::Overflow {
                before: history_string(value, "before")?,
                after: history_string(value, "after")?,
            },
            _ => unreachable!("kind checked above"),
        };
        operations.push(HistoryOperation { body, edit, kind });
    }
    Ok(operations)
}

impl HistoryTextEdit {
    fn checked_body(&self, body: &str) -> Result<String, HistoryTransactionError> {
        let end = self
            .start
            .checked_add(utf16_len(&self.deleted))
            .filter(|end| *end <= u32::MAX as usize)
            .ok_or_else(|| history_error("invalid-range", "history edit end overflow"))?;
        let (a, b) = utf16_to_byte(body, self.start)
            .zip(utf16_to_byte(body, end))
            .ok_or_else(|| {
                history_error(
                    "invalid-range",
                    "history edit splits a scalar or is out of bounds",
                )
            })?;
        if body[a..b] != self.deleted {
            return Err(history_error(
                "body-conflict",
                "history deleted text does not match",
            ));
        }
        let size = body
            .len()
            .checked_sub(b - a)
            .and_then(|n| n.checked_add(self.inserted.len()))
            .filter(|n| *n <= HISTORY_JSON_LIMIT)
            .ok_or_else(|| history_error("limit", "history result exceeds 16 MiB"))?;
        let mut result = String::with_capacity(size);
        result.push_str(&body[..a]);
        result.push_str(&self.inserted);
        result.push_str(&body[b..]);
        Ok(result)
    }
}

impl DocumentSession {
    /// Simulate the complete ordered group on one clone, publish only after
    /// every metadata/body guard passes. Ghost-free legacy APIs are unchanged.
    pub fn replay_history_group_atomic(
        &mut self,
        expected_body: &str,
        target_body: &str,
        operations: &[HistoryOperation],
    ) -> Result<(), HistoryTransactionError> {
        history_bytes(expected_body)?;
        history_bytes(target_body)?;
        if operations.len() > HISTORY_OPERATION_LIMIT {
            return Err(history_error("limit", "more than 4096 operations"));
        }
        if self.body() != expected_body {
            return Err(history_error(
                "body-conflict",
                "initial history body does not match",
            ));
        }
        let mut trial = self.clone();
        for op in operations {
            let recorded = op
                .edit
                .as_ref()
                .map(|edit| edit.checked_body(trial.body()))
                .transpose()?;
            let model_error = |message: String| history_error("model-refusal", message);
            match &op.kind {
                HistoryKind::Ghost { change, undo } => trial
                    .replay_ghost_change(change, *undo)
                    .map_err(model_error)?,
                HistoryKind::Edit => {
                    let e = op.edit.as_ref().expect("decoded edit");
                    trial
                        .apply_edit(e.start, utf16_len(&e.deleted), &e.inserted)
                        .map_err(|e| model_error(e.to_string()))?;
                }
                HistoryKind::Alt { span, target } => {
                    let e = op.edit.as_ref().expect("decoded edit");
                    trial
                        .alt_restore(
                            span,
                            target.clone(),
                            e.start,
                            utf16_len(&e.deleted),
                            &e.inserted,
                        )
                        .map_err(model_error)?;
                }
                HistoryKind::Move { start, end, to } => {
                    trial
                        .move_range(*start, *end, *to)
                        .map_err(|e| model_error(e.to_string()))?;
                }
                HistoryKind::Swap { span, from, to } => {
                    if trial.live_span(span).map(|s| s.active) != Some(*from) {
                        return Err(history_error(
                            "model-refusal",
                            "swap source index conflicts",
                        ));
                    }
                    trial
                        .set_active(span, *to)
                        .map_err(|e| model_error(e.to_string()))?;
                }
                HistoryKind::Overflow { before, after } => {
                    let e = op.edit.as_ref().expect("decoded edit");
                    trial
                        .apply_edit(e.start, utf16_len(&e.deleted), &e.inserted)
                        .map_err(|e| model_error(e.to_string()))?;
                    if !trial
                        .replay_overflow(before, after)
                        .map_err(|e| model_error(e.to_string()))?
                        .applied
                    {
                        return Err(history_error(
                            "model-refusal",
                            "overflow replay could not be applied",
                        ));
                    }
                }
            }
            if trial.body() != op.body || recorded.as_ref().is_some_and(|body| body != trial.body())
            {
                return Err(history_error(
                    "body-conflict",
                    "intermediate history body or replacement envelope conflicts",
                ));
            }
        }
        if trial.body() != target_body {
            return Err(history_error(
                "body-conflict",
                "final history body does not match",
            ));
        }
        trial
            .validate_history_annotations()
            .map_err(|e| history_error("model-refusal", e))?;
        *self = trial;
        Ok(())
    }
}

// The ordinary dispatcher shares the native checkpoint, never a reopened mirror.
fn action_generation(value: &Value, field: &str) -> Result<String, HistoryTransactionError> {
    let text = history_string(value, field)?;
    if text.is_empty()
        || !text.bytes().all(|b| b.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return Err(history_error(
            "invalid-payload",
            "generation must be a canonical decimal string",
        ));
    }
    Ok(text)
}

fn current_action_role_generation() -> String {
    let snapshot: Value = serde_json::from_str(&crate::kg::role_snapshot_json())
        .expect("native role snapshot is JSON");
    snapshot["generation"]
        .as_str()
        .expect("native generation string")
        .to_owned()
}

fn action_edit_fields(value: &Value, with_kind: bool) -> Result<(), HistoryTransactionError> {
    if with_kind {
        history_fields(
            value,
            &[
                "kind",
                "start",
                "deleteUtf16",
                "insertedRef",
                "beforeBodyHash",
                "afterBodyHash",
            ],
        )?;
    } else {
        history_fields(
            value,
            &[
                "start",
                "deleteUtf16",
                "insertedRef",
                "beforeBodyHash",
                "afterBodyHash",
            ],
        )?;
    }
    for field in ["start", "deleteUtf16", "insertedRef"] {
        history_integer(value, field)?;
    }
    for field in ["beforeBodyHash", "afterBodyHash"] {
        let hash = history_string(value, field)?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(history_error(
                "invalid-payload",
                "expected lowercase SHA-256",
            ));
        }
    }
    Ok(())
}

fn action_ref(
    value: &Value,
    field: &str,
    used: &mut [bool],
) -> Result<(), HistoryTransactionError> {
    let index = history_integer(value, field)?;
    let slot = used
        .get_mut(index)
        .ok_or_else(|| history_error("invalid-payload", "text reference out of range"))?;
    if *slot {
        return Err(history_error("invalid-payload", "text reference reused"));
    }
    *slot = true;
    Ok(())
}

fn parse_action_metadata(meta: &str, args: &[String]) -> Result<Value, HistoryTransactionError> {
    history_bytes(meta)?;
    if args.len() > HISTORY_OPERATION_LIMIT {
        return Err(history_error("limit", "more than 4096 text arguments"));
    }
    let mut bytes = 0usize;
    for arg in args {
        history_bytes(arg)?;
        bytes = bytes
            .checked_add(arg.len())
            .ok_or_else(|| history_error("limit", "text budget exhausted"))?;
        if bytes > HISTORY_JSON_LIMIT {
            return Err(history_error(
                "limit",
                "combined text arguments exceed 16 MiB",
            ));
        }
    }
    let value: Value = serde_json::from_str(meta)
        .map_err(|_| history_error("invalid-payload", "invalid action JSON or nesting"))?;
    let kind = value["kind"]
        .as_str()
        .ok_or_else(|| history_error("invalid-payload", "missing action kind"))?;
    let fields: &[&str] = match kind {
        "text_edit" => &[
            "kind",
            "start",
            "deleteUtf16",
            "insertedRef",
            "beforeBodyHash",
            "afterBodyHash",
        ],
        "text_edits" => &["kind", "edits"],
        "ghost_range" | "revive_range" | "stash" => &["kind", "start", "end"],
        "alt_create" => &["kind", "spanKind", "start", "end", "textRef", "source"],
        "alt_add" => &["kind", "spanId", "textRef", "source"],
        "alt_edit" => &["kind", "spanId", "index", "textRef"],
        "alt_remove" | "alt_active" => &["kind", "spanId", "index"],
        "kg_swap" => &["kind", "spanId", "index", "roleGeneration"],
        "alt_move" => &["kind", "spanId", "from", "to"],
        "append_alternatives" => &[
            "kind",
            "start",
            "end",
            "suppliedJsonRef",
            "roleGeneration",
            "providerGeneration",
        ],
        "pull" => &["kind", "at", "from", "to"],
        "overflow_set" => &["kind", "textRef"],
        _ => {
            return Err(history_error(
                "invalid-payload",
                "unsupported ordinary action",
            ))
        }
    };
    history_fields(&value, fields)?;
    for field in fields {
        match *field {
            "start" | "end" | "at" | "from" | "to" | "index" | "deleteUtf16" => {
                history_integer(&value, field)?;
            }
            "spanId" | "spanKind" | "source" => {
                history_string(&value, field)?;
            }
            "roleGeneration" | "providerGeneration" => {
                action_generation(&value, field)?;
            }
            _ => {}
        }
    }
    let mut used = vec![false; args.len()];
    match kind {
        "text_edit" => {
            action_edit_fields(&value, true)?;
            action_ref(&value, "insertedRef", &mut used)?;
        }
        "text_edits" => {
            let edits = value["edits"]
                .as_array()
                .ok_or_else(|| history_error("invalid-payload", "edits must be an array"))?;
            if edits.is_empty() || edits.len() > HISTORY_OPERATION_LIMIT {
                return Err(history_error("limit", "batch must contain 1–4096 edits"));
            }
            for edit in edits {
                action_edit_fields(edit, false)?;
                action_ref(edit, "insertedRef", &mut used)?;
            }
        }
        "alt_create" | "alt_add" | "alt_edit" | "overflow_set" => {
            action_ref(&value, "textRef", &mut used)?
        }
        "append_alternatives" => action_ref(&value, "suppliedJsonRef", &mut used)?,
        _ => {}
    }
    if used.iter().any(|used| !used) {
        return Err(history_error("invalid-payload", "unused text argument"));
    }
    Ok(value)
}

fn apply_action_edit(
    trial: &mut DocumentSession,
    edit: &Value,
    args: &[String],
) -> Result<Value, HistoryTransactionError> {
    if crate::host::source_hash(trial.body())
        != edit["beforeBodyHash"].as_str().expect("checked hash")
    {
        return Err(history_error(
            "body-conflict",
            "intermediate edit body does not match",
        ));
    }
    let start = history_integer(edit, "start")?;
    let deleted_len = history_integer(edit, "deleteUtf16")?;
    let end = start
        .checked_add(deleted_len)
        .ok_or_else(|| history_error("invalid-payload", "edit range overflow"))?;
    let (a, b) = (
        utf16_to_byte(trial.body(), start),
        utf16_to_byte(trial.body(), end),
    );
    let (Some(a), Some(b)) = (a, b) else {
        return Err(history_error(
            "model-refusal",
            "edit range does not fit UTF-16 body",
        ));
    };
    let deleted = trial.body()[a..b].to_owned();
    let inserted = &args[history_integer(edit, "insertedRef")?];
    let outcome = trial
        .apply_edit(start, deleted_len, inserted)
        .map_err(|e| history_error("model-refusal", e.to_string()))?;
    history_bytes(trial.body())?;
    if crate::host::source_hash(trial.body())
        != edit["afterBodyHash"].as_str().expect("checked hash")
    {
        return Err(history_error(
            "body-conflict",
            "result edit body does not match",
        ));
    }
    let mut result = outcome.to_json();
    result.as_object_mut().expect("outcome object").insert("edit".into(), json!({"start":start,"deletedLength":deleted_len,"deletedText":deleted,"insertedText":inserted}));
    Ok(result)
}

/// Begin an ordinary typed action on the existing native session authority.
///
/// # Errors
/// Refuses stale bodies/roles, malformed or oversized arguments, domain errors,
/// and outstanding transactions. Failure leaves the session/checkpoint intact.
/// A returned token must be finished or guarded-aborted exactly once.
pub fn begin_document_action_json(
    expected_body: &str,
    action_meta_json: &str,
    text_args: &[String],
) -> Result<Option<Value>, HistoryTransactionError> {
    history_bytes(expected_body)?;
    let meta = parse_action_metadata(action_meta_json, text_args)?;
    history_available()?;
    let role = if meta.get("roleGeneration").is_some() {
        Some(action_generation(&meta, "roleGeneration")?)
    } else {
        None
    };
    if role
        .as_ref()
        .is_some_and(|g| *g != current_action_role_generation())
    {
        return Err(history_error("role-conflict", "displayed role changed"));
    }
    let started = begin_history_checkpoint(None, |trial| {
        if trial.body() != expected_body {
            return Err(history_error("body-conflict", "action body does not match"));
        }
        let before = trial.clone();
        let n = |field| history_integer(&meta, field);
        let s = |field: &str| meta[field].as_str().expect("checked string");
        let text = |field| -> Result<&str, HistoryTransactionError> { Ok(&text_args[n(field)?]) };
        let domain = |e: String| history_error("model-refusal", e);
        let kind = s("kind");
        let result = match kind {
            "text_edit" => apply_action_edit(trial, &meta, text_args)?,
            "text_edits" => {
                let mut results = Vec::new();
                for edit in meta["edits"].as_array().expect("checked edits") {
                    results.push(apply_action_edit(trial, edit, text_args)?);
                }
                json!({"edits":results})
            }
            "ghost_range" => {
                let id = trial
                    .ghost(n("start")?, n("end")?)
                    .map_err(|e| domain(e.to_string()))?;
                json!({"ok":true,"id":id,"annotations":trial.annotations_json()})
            }
            "revive_range" => {
                let changed = trial
                    .revive(n("start")?, n("end")?)
                    .map_err(|e| domain(e.to_string()))?;
                json!({"ok":true,"changed":changed,"annotations":trial.annotations_json()})
            }
            "alt_create" => trial
                .alt_create_span(
                    parse_span_kind(s("spanKind")).map_err(domain)?,
                    n("start")?,
                    n("end")?,
                    text("textRef")?,
                    parse_alt_source(s("source")).map_err(domain)?,
                )
                .map_err(domain)?
                .to_json(),
            "alt_add" => match trial
                .alt_add(
                    s("spanId"),
                    text("textRef")?,
                    parse_alt_source(s("source")).map_err(domain)?,
                )
                .map_err(domain)?
            {
                Some(v) => v.to_json(),
                None => return Ok(None),
            },
            "alt_edit" => match trial
                .alt_edit(s("spanId"), n("index")?, text("textRef")?)
                .map_err(domain)?
            {
                Some(v) => v.to_json(),
                None => return Ok(None),
            },
            "alt_remove" => trial
                .alt_remove(s("spanId"), n("index")?)
                .map_err(domain)?
                .to_json(),
            "alt_move" => match trial
                .alt_move(s("spanId"), n("from")?, n("to")?)
                .map_err(domain)?
            {
                Some(v) => v.to_json(),
                None => return Ok(None),
            },
            "alt_active" => trial
                .set_active(s("spanId"), n("index")?)
                .map_err(|e| domain(e.to_string()))?
                .to_json(),
            "kg_swap" => crate::kg::kg_swap(trial, s("spanId"), n("index")?)
                .map_err(|e| domain(e.to_string()))?
                .to_json(),
            "append_alternatives" => {
                let supplied =
                    crate::providers::parse_supplied(text("suppliedJsonRef")?).map_err(domain)?;
                match crate::providers::append_from_providers(
                    trial,
                    n("start")?,
                    n("end")?,
                    &supplied,
                )
                .map_err(|e| domain(e.to_string()))?
                {
                    Some(v) => v.to_json(),
                    None => return Ok(None),
                }
            }
            "stash" => trial
                .stash(n("start")?, n("end")?)
                .map_err(|e| domain(e.to_string()))?
                .to_json(),
            "pull" => {
                let pull = trial
                    .pull_back_text(n("at")?, n("from")?, n("to")?)
                    .map_err(|e| domain(e.to_string()))?;
                let outcome = trial
                    .apply_edit(pull.start, 0, &pull.text)
                    .map_err(|e| domain(e.to_string()))?;
                json!({"start":pull.start,"text":pull.text,"outcome":outcome.to_json(),
                    "edit":{"start":pull.start,"deletedLength":0,"deletedText":"","insertedText":pull.text},
                    "overflow":{"before":trial.overflow(),"after":trial.overflow()}})
            }
            "overflow_set" => {
                trial
                    .set_overflow(text("textRef")?)
                    .map_err(|e| domain(e.to_string()))?;
                json!({"ok":true,"overflow":trial.overflow()})
            }
            _ => unreachable!("strict allowlist"),
        };
        if *trial == before {
            return Ok(None);
        }
        let source = trial.save();
        history_bytes(&source)?;
        Ok(Some(
            json!({"ok":true,"result":result,"annotations":trial.annotations_json(),"source":source}),
        ))
    })?;
    if started.is_some() {
        HISTORY_BRIDGE.with(|b| {
            b.borrow_mut()
                .pending
                .as_mut()
                .expect("issued checkpoint")
                .action_role_generation = role
        });
    }
    Ok(started)
}

/// WASM transport for the strict ordinary-action checkpoint dispatcher.
///
/// # Errors
/// Uses the native typed refusals. Response conversion failure guarded-aborts
/// the issued checkpoint rather than leaving an undelivered action accepted.
#[wasm_bindgen]
pub fn begin_document_action(
    expected_body: &str,
    action_meta_json: &str,
    text_args: js_sys::Array,
) -> Result<JsValue, JsValue> {
    if text_args.length() as usize > HISTORY_OPERATION_LIMIT {
        return Err(history_js_error(history_error(
            "limit",
            "more than 4096 text arguments",
        )));
    }
    let mut args = Vec::with_capacity(text_args.length() as usize);
    let mut total = 0usize;
    for value in text_args.iter() {
        if !value.is_string() {
            return Err(history_js_error(history_error(
                "invalid-payload",
                "text arguments must be strings",
            )));
        }
        let js_text = js_sys::JsString::from(value.clone());
        if js_text.length() as usize > HISTORY_JSON_LIMIT {
            return Err(history_js_error(history_error(
                "limit",
                "text argument exceeds limit",
            )));
        }
        let text = value.as_string().expect("checked string");
        total = total
            .checked_add(text.len())
            .ok_or_else(|| history_js_error(history_error("limit", "text budget exhausted")))?;
        if total > HISTORY_JSON_LIMIT {
            return Err(history_js_error(history_error(
                "limit",
                "combined text arguments exceed 16 MiB",
            )));
        }
        args.push(text);
    }
    match begin_document_action_json(expected_body, action_meta_json, &args)
        .map_err(history_js_error)?
    {
        Some(v) => history_begin_js(v),
        None => Ok(JsValue::NULL),
    }
}

#[derive(Debug)]
struct HistoryCheckpoint {
    token: String,
    before: DocumentSession,
    after: DocumentSession,
    role_generation: Option<u64>,
    action_role_generation: Option<String>,
}

#[derive(Debug, Default)]
struct HistoryBridge {
    watermark: u64,
    pending: Option<HistoryCheckpoint>,
}

thread_local! { static HISTORY_BRIDGE: RefCell<HistoryBridge> = RefCell::new(HistoryBridge::default()); }

fn history_available() -> Result<(), HistoryTransactionError> {
    HISTORY_BRIDGE.with(|cell| {
        if cell.borrow().pending.is_some() {
            Err(history_error(
                "pending",
                "finish, abort or explicitly recover the outstanding transaction",
            ))
        } else {
            Ok(())
        }
    })
}

fn begin_history_checkpoint(
    role_generation: Option<u64>,
    op: impl FnOnce(&mut DocumentSession) -> Result<Option<Value>, HistoryTransactionError>,
) -> Result<Option<Value>, HistoryTransactionError> {
    HISTORY_BRIDGE.with(|cell| {
        let mut bridge = cell.borrow_mut();
        if bridge.pending.is_some() {
            return Err(history_error(
                "pending",
                "an outstanding transaction requires acknowledgement or recovery",
            ));
        }
        let next = bridge.watermark.checked_add(1).ok_or_else(|| {
            history_error("token-exhausted", "transaction token allocation exhausted")
        })?;
        with_session(|session| {
            let mut trial = session.clone();
            let Some(payload) = op(&mut trial)? else {
                return Ok(None);
            };
            trial
                .validate_history_annotations()
                .map_err(|e| history_error("model-refusal", e))?;
            let token = format!("history-{next}");
            let mut response = payload;
            response
                .as_object_mut()
                .expect("internal response object")
                .insert("token".into(), json!(token));
            // Prepare response and checkpoint before publishing any state.
            let checkpoint = HistoryCheckpoint {
                token,
                before: session.clone(),
                after: trial.clone(),
                role_generation,
                action_role_generation: None,
            };
            bridge.watermark = next;
            bridge.pending = Some(checkpoint);
            *session = trial;
            Ok(Some(response))
        })
    })
}

/// Native bridge for the future JS adapter. No document clones cross the bridge.
pub fn begin_history_replay_json(
    expected_body: &str,
    target_body: &str,
    operations_json: &str,
) -> Result<Value, HistoryTransactionError> {
    history_bytes(expected_body)?;
    history_bytes(target_body)?;
    let operations = parse_history_operations(operations_json)?;
    history_available()?;
    begin_history_checkpoint(None, |trial| {
        trial.replay_history_group_atomic(expected_body, target_body, &operations)?;
        Ok(Some(
            json!({"ok":true,"annotations":trial.annotations_json()}),
        ))
    })?
    .ok_or_else(|| history_error("model-refusal", "missing replay response"))
}

/// Protected-only promotion: exact native body guard before B's fresh range
/// seam, called without a SESSION borrow (B reads SESSION itself). No sync or
/// arbitrary-range WASM bypass. Finish also checks authoritative role state.
pub fn begin_trim_ghost_transaction_json(
    expected_body: &str,
    level: &str,
    kept_json: &str,
) -> Result<Option<Value>, HistoryTransactionError> {
    history_bytes(expected_body)?;
    history_bytes(level)?;
    let kept = bounded_history_array(kept_json)?;
    if kept
        .as_array()
        .expect("checked array")
        .iter()
        .any(|id| id.as_u64().is_none_or(|n| n > u32::MAX as u64))
    {
        return Err(history_error(
            "invalid-payload",
            "kept IDs must be exact nonnegative 32-bit integers",
        ));
    }
    history_available()?;
    if !with_session(|s| s.body() == expected_body) {
        return Err(history_error(
            "body-conflict",
            "promotion body does not match",
        ));
    }
    let ranges = crate::lab::protected_trim_promotion_ranges(level, kept_json)
        .map_err(|e| history_error("protection-refusal", e))?;
    let role = crate::kg::active_role_snapshot()
        .filter(|r| r.protection.is_ok())
        .ok_or_else(|| history_error("role-conflict", "protected role is unavailable"))?;
    if ranges.is_empty() {
        return Ok(None);
    }
    begin_history_checkpoint(Some(role.generation), |trial| {
        let change = trial
            .ghost_ranges_atomic(expected_body, &ranges)
            .map_err(|e| history_error("model-refusal", e))?;
        Ok(change.map(|change| json!({"ok":true,"change":change.to_json(),"annotations":trial.annotations_json()})))
    })
}

fn acknowledge_history(token: &str, abort: bool) -> Result<Value, HistoryTransactionError> {
    history_bytes(token)?;
    HISTORY_BRIDGE.with(|cell| {
        let mut bridge = cell.borrow_mut();
        let checkpoint = bridge
            .pending
            .as_ref()
            .filter(|p| p.token == token)
            .ok_or_else(|| {
                history_error("unknown-token", "unknown or consumed transaction token")
            })?;
        if !with_session(|s| *s == checkpoint.after) {
            return Err(history_error(
                "session-conflict",
                "document interleaved; explicit canonical recovery required",
            ));
        }
        if !abort
            && checkpoint.role_generation.is_some_and(|generation| {
                !crate::kg::active_role_snapshot()
                    .is_some_and(|role| role.generation == generation && role.protection.is_ok())
            })
        {
            return Err(history_error(
                "role-conflict",
                "protected role changed; guarded document abort is still available",
            ));
        }
        if !abort
            && checkpoint
                .action_role_generation
                .as_ref()
                .is_some_and(|g| *g != current_action_role_generation())
        {
            return Err(history_error(
                "role-conflict",
                "displayed role changed; guarded document abort remains available",
            ));
        }
        let response = if abort {
            json!({"ok":true,"rolledBack":true,"token":token})
        } else {
            json!({"ok":true,"finalized":true,"token":token})
        };
        let checkpoint = bridge.pending.take().expect("validated pending token");
        if abort {
            with_session(|s| *s = checkpoint.before);
        }
        Ok(response)
    })
}

pub fn finish_history_transaction_json(token: &str) -> Result<Value, HistoryTransactionError> {
    acknowledge_history(token, false)
}
pub fn abort_history_transaction_json(token: &str) -> Result<Value, HistoryTransactionError> {
    acknowledge_history(token, true)
}

/// Explicit canonical reopen/recovery, not a successful abort. Retires only
/// the named pending checkpoint; does not restore interleaved/global role data
/// or reset the token watermark. Ordinary open never silently retires it.
pub fn recover_history_transaction_json(
    token: &str,
    source: &str,
) -> Result<Value, HistoryTransactionError> {
    history_bytes(token)?;
    history_bytes(source)?;
    HISTORY_BRIDGE.with(|cell| {
        let mut bridge = cell.borrow_mut();
        if bridge.pending.as_ref().is_none_or(|p| p.token != token) { return Err(history_error("unknown-token", "unknown recovery token")); }
        let mut trial = DocumentSession::new();
        let opened = trial.open(source);
        let response = json!({"ok":true,"recovered":true,"rolledBack":false,"token":token,"opened":opened.to_json(),"annotations":trial.annotations_json()});
        with_session(|s| *s = trial);
        bridge.pending = None;
        Ok(response)
    })
}

fn history_js_error(error: HistoryTransactionError) -> JsValue {
    js_sys::JSON::parse(&json!({"ok":false,"kind":error.kind,"error":error.message}).to_string())
        .unwrap_or_else(|_| JsValue::from_str(&error.to_string()))
}

fn history_js_value(value: &Value) -> Result<JsValue, HistoryTransactionError> {
    js_sys::JSON::parse(&value.to_string()).map_err(|_| {
        history_error(
            "transport-refusal",
            "transaction response could not be decoded",
        )
    })
}

fn history_begin_js(value: Value) -> Result<JsValue, JsValue> {
    match history_js_value(&value) {
        Ok(value) => Ok(value),
        Err(error) => {
            // Failed delivery is not acknowledgement. Restore the private
            // native checkpoint, unless interleaved state now forbids rollback.
            let token = value["token"].as_str().expect("internal token response");
            abort_history_transaction_json(token).map_err(history_js_error)?;
            Err(history_js_error(error))
        }
    }
}

#[wasm_bindgen]
pub fn begin_document_history_replay(
    expected_body: &str,
    target_body: &str,
    operations_json: &str,
) -> Result<JsValue, JsValue> {
    history_begin_js(
        begin_history_replay_json(expected_body, target_body, operations_json)
            .map_err(history_js_error)?,
    )
}

#[wasm_bindgen]
pub fn begin_trim_ghost_transaction(
    expected_body: &str,
    level: &str,
    kept_json: &str,
) -> Result<JsValue, JsValue> {
    match begin_trim_ghost_transaction_json(expected_body, level, kept_json)
        .map_err(history_js_error)?
    {
        Some(value) => history_begin_js(value),
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen]
pub fn finish_document_history_transaction(token: &str) -> Result<JsValue, JsValue> {
    history_bytes(token).map_err(history_js_error)?;
    let response = history_js_value(&json!({"ok":true,"finalized":true,"token":token}))
        .map_err(history_js_error)?;
    finish_history_transaction_json(token).map_err(history_js_error)?;
    Ok(response)
}

#[wasm_bindgen]
pub fn abort_document_history_transaction(token: &str) -> Result<JsValue, JsValue> {
    history_bytes(token).map_err(history_js_error)?;
    let response = history_js_value(&json!({"ok":true,"rolledBack":true,"token":token}))
        .map_err(history_js_error)?;
    abort_history_transaction_json(token).map_err(history_js_error)?;
    Ok(response)
}

#[wasm_bindgen]
pub fn recover_document_history_transaction(token: &str, source: &str) -> Result<JsValue, JsValue> {
    recover_history_transaction_json(token, source)
        .and_then(|v| history_js_value(&v))
        .map_err(history_js_error)
}

#[cfg(test)]
mod history_checkpoint_tests {
    use super::*;

    #[test]
    fn full_namespace_validation_rejects_unresolved_collision_without_save_renaming() {
        let mut s = DocumentSession::new();
        s.open("abc");
        let receipt = s.ghost_ranges_atomic("abc", &[(0, 1)]).unwrap().unwrap();
        s.apply_edit(0, 1, "").unwrap();
        assert_eq!(s.set_aside.ghosts[0].id, "g1");
        let before = s.clone();
        let target = json!({"id":"g1","kind":"word","anchor":{"start":0,"end":1,"text":"b"},"active":0,
            "alts":[{"text":"b","source":"original"},{"text":"bee","source":"human"}]});
        let ops = json!([{"kind":"alt","body":"bc","start":0,"deletedText":"","insertedText":"","span":"g1","target":target}]);
        assert_eq!(
            s.replay_history_group_atomic(
                "bc",
                "bc",
                &parse_history_operations(&ops.to_string()).unwrap()
            )
            .unwrap_err()
            .kind,
            "model-refusal"
        );
        assert_eq!(s, before);
        assert!(s.ghost_receipts.contains(&receipt.to_json().to_string()));
        assert_eq!(s.set_aside.ghosts[0].id, "g1");
        assert!(parse(&s.save()).is_ok());
    }

    #[test]
    fn full_schema_validation_preserves_legitimate_overlapping_set_aside_hints() {
        let mut s = DocumentSession::new();
        s.open("abc");
        s.set_aside.ghosts = vec![
            Ghost {
                id: "unresolved-one".into(),
                anchor: Anchor::new(0, 1, "x"),
            },
            Ghost {
                id: "unresolved-two".into(),
                anchor: Anchor::new(0, 1, "y"),
            },
        ];
        s.reserve_current_ids();
        let before = s.clone();
        s.replay_history_group_atomic("abc", "abc", &[]).unwrap();
        assert_eq!(
            s, before,
            "validation must not move live hints or rename IDs"
        );
        assert!(parse(&s.save()).is_ok());
        s.set_aside.ghosts[1].id = "unresolved-one".into();
        let before = s.clone();
        assert_eq!(
            s.replay_history_group_atomic("abc", "abc", &[])
                .unwrap_err()
                .kind,
            "model-refusal"
        );
        assert_eq!(s, before);
    }

    #[test]
    fn checkpoint_schema_refusal_preserves_watermark_pending_and_raw_block() {
        with_session(|s| {
            s.open("abc");
        });
        let watermark = HISTORY_BRIDGE.with(|b| b.borrow().watermark);
        with_session(|s| {
            s.doc.annotations.ghosts = vec![
                Ghost {
                    id: "duplicate".into(),
                    anchor: Anchor::new(0, 1, "a"),
                },
                Ghost {
                    id: "duplicate".into(),
                    anchor: Anchor::new(1, 2, "b"),
                },
            ];
        });
        let before = with_session(|s| s.clone());
        assert_eq!(
            begin_history_replay_json("abc", "abc", "[]")
                .unwrap_err()
                .kind,
            "model-refusal"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        HISTORY_BRIDGE.with(|b| {
            assert!(b.borrow().pending.is_none());
            assert_eq!(b.borrow().watermark, watermark);
        });
        with_session(|s| {
            s.open("abc");
        });
        let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
        let token = started["token"].as_str().unwrap();
        let before = with_session(|s| s.clone());
        assert_eq!(
            begin_history_replay_json("abc", "abc", "[]")
                .unwrap_err()
                .kind,
            "pending"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        abort_history_transaction_json(token).unwrap();
        with_session(|s| {
            s.open("abc\n\n```terraphim-alternatives\n{\n```\n");
        });
        let before = with_session(|s| s.clone());
        let body = before.body();
        assert_eq!(
            begin_history_replay_json(body, body, "[]")
                .unwrap_err()
                .kind,
            "model-refusal"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        assert_eq!(with_session(|s| s.save()), before.save());
    }

    fn edit(body: &str, start: usize, deleted: &str, inserted: &str) -> Value {
        json!({"kind":"edit","body":body,"start":start,"deletedText":deleted,"insertedText":inserted})
    }

    #[test]
    fn mixed_history_intermediate_body_and_late_failure_are_atomic() {
        let mut s = DocumentSession::new();
        s.open("abc");
        let change = s.ghost_ranges_atomic("abc", &[(0, 1)]).unwrap().unwrap();
        s.apply_edit(3, 0, "X").unwrap();
        let operations = json!([edit("abc", 3, "X", ""), {"kind":"ghost","body":"abc","change":change.to_json(),"undo":true}]);
        let parsed = parse_history_operations(&operations.to_string()).unwrap();
        let before = s.clone();
        s.replay_history_group_atomic("abcX", "abc", &parsed)
            .unwrap();
        assert!(s.doc.annotations.ghosts.is_empty());
        assert_eq!(s.reserved_ids, before.reserved_ids);
        let mut s = before;
        s.ghost(1, 2).unwrap();
        let before = s.clone();
        assert!(s
            .replay_history_group_atomic("abcX", "abc", &parsed)
            .is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn checkpoint_exact_abort_wrong_tokens_and_consumed_tokens() {
        with_session(|s| {
            s.open("abcX");
        });
        let before = with_session(|s| s.clone());
        let ops = json!([edit("abc", 3, "X", "")]).to_string();
        let started = begin_history_replay_json("abcX", "abc", &ops).unwrap();
        let token = started["token"].as_str().unwrap();
        assert!(finish_history_transaction_json("wrong").is_err());
        assert!(abort_history_transaction_json("wrong").is_err());
        assert_eq!(
            abort_history_transaction_json(token).unwrap(),
            json!({"ok":true,"rolledBack":true,"token":token})
        );
        assert_eq!(with_session(|s| s.clone()), before);
        assert!(finish_history_transaction_json(token).is_err());
        let next = begin_history_replay_json("abcX", "abc", &ops).unwrap();
        assert_ne!(next["token"], started["token"]);
        finish_history_transaction_json(next["token"].as_str().unwrap()).unwrap();
    }

    #[test]
    fn protected_checkpoint_role_invalidation_aborts_document_only() {
        const BODY: &str = "The editor is basically the only target here today.";
        crate::kg::load_thesaurus(
            r#"{"name":"public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#,
        )
        .unwrap();
        with_session(|s| {
            s.open(BODY);
        });
        crate::lab::session_trim_plan_json().unwrap();
        let before = with_session(|s| s.clone());
        let started = begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
            .unwrap()
            .unwrap();
        let token = started["token"].as_str().unwrap();
        crate::kg::clear_thesaurus();
        assert_eq!(
            finish_history_transaction_json(token).unwrap_err().kind,
            "role-conflict"
        );
        abort_history_transaction_json(token).unwrap();
        assert_eq!(with_session(|s| s.clone()), before);
        assert!(crate::kg::active_role_snapshot().is_none());
    }

    #[test]
    fn protected_unsafe_role_and_joint_document_conflict_do_not_lose_checkpoint() {
        const BODY: &str = "The editor is basically the only target here today.";
        const ROLE: &str =
            r#"{"name":"public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#;
        const UNSAFE: &str =
            r#"{"name":"unsupported","data":{"Straße":{"id":2,"nterm":"strasse"}}}"#;
        crate::kg::load_thesaurus(ROLE).unwrap();
        with_session(|s| {
            s.open(BODY);
        });
        crate::lab::session_trim_plan_json().unwrap();
        let before = with_session(|s| s.clone());
        let started = begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
            .unwrap()
            .unwrap();
        let token = started["token"].as_str().unwrap();
        crate::kg::load_thesaurus(UNSAFE).unwrap();
        let newer_role = crate::kg::role_snapshot_json();
        assert_eq!(
            finish_history_transaction_json(token).unwrap_err().kind,
            "role-conflict"
        );
        abort_history_transaction_json(token).unwrap();
        assert_eq!(
            with_session(|s| s.clone()),
            before,
            "failed record rollback releases new ghost reservations/receipt"
        );
        assert_eq!(crate::kg::role_snapshot_json(), newer_role);
        assert!(crate::lab::session_trim_status("sharper", "[]").is_err());
        crate::kg::load_thesaurus(ROLE).unwrap();
        crate::lab::session_trim_plan_json().unwrap();
        let started = begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
            .unwrap()
            .unwrap();
        let token = started["token"].as_str().unwrap();
        crate::kg::clear_thesaurus();
        with_session(|s| {
            s.set_overflow("interleaved notes").unwrap();
        });
        let interleaved = with_session(|s| s.clone());
        assert_eq!(
            finish_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(
            abort_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(with_session(|s| s.clone()), interleaved);
        assert_eq!(
            begin_history_replay_json(BODY, BODY, "[]")
                .unwrap_err()
                .kind,
            "pending"
        );
        recover_history_transaction_json(token, &interleaved.save()).unwrap();
        assert!(crate::kg::active_role_snapshot().is_none());
    }

    #[test]
    fn same_body_alternatives_and_receipt_reservations_are_full_session_conflicts() {
        for receipts in [false, true] {
            with_session(|s| {
                s.open("abc");
            });
            let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
            let token = started["token"].as_str().unwrap();
            with_session(|s| {
                if receipts {
                    let change = s.ghost_ranges_atomic("abc", &[(0, 1)]).unwrap().unwrap();
                    s.replay_ghost_change(&change, true).unwrap();
                    assert!(s.doc.annotations.ghosts.is_empty());
                } else {
                    s.alt_create_span(SpanKind::Word, 0, 1, "alias", Source::Human)
                        .unwrap();
                }
            });
            let before = with_session(|s| s.clone());
            assert_eq!(before.body(), "abc");
            assert_eq!(
                abort_history_transaction_json(token).unwrap_err().kind,
                "session-conflict"
            );
            assert_eq!(with_session(|s| s.clone()), before);
            recover_history_transaction_json(token, &before.save()).unwrap();
        }
    }

    #[test]
    fn identical_reopen_is_still_a_foreign_session() {
        with_session(|s| {
            s.open("abc");
        });
        let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
        let token = started["token"].as_str().unwrap();
        with_session(|s| {
            s.open("abc");
        });
        let before = with_session(|s| s.clone());
        assert_eq!(
            finish_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(
            abort_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        recover_history_transaction_json(token, "abc").unwrap();
    }

    #[test]
    fn all_legacy_kinds_use_model_semantics_and_check_replacement_envelopes() {
        let mut s = DocumentSession::new();
        s.open("a clip tail");
        s.alt_create_span(SpanKind::Word, 2, 6, "eraser", Source::Human)
            .unwrap();
        let original = s.live_span("s1").unwrap().clone();
        let ops = json!([
            {"kind":"swap","body":"an eraser tail","start":1,"deletedText":" clip","insertedText":"n eraser","span":"s1","from":0,"to":1},
            {"kind":"alt","body":"a clip tail","start":1,"deletedText":"n eraser","insertedText":" clip","span":"s1","target":original},
            {"kind":"move","body":"taila clip ","start":0,"deletedText":"a clip tail","insertedText":"taila clip ","move":{"start":0,"end":7,"to":11}},
            {"kind":"overflow","body":"a clip ","start":0,"deletedText":"tail","insertedText":"","before":"","after":"tail"}
        ]);
        s.replay_history_group_atomic(
            "a clip tail",
            "a clip ",
            &parse_history_operations(&ops.to_string()).unwrap(),
        )
        .unwrap();
        assert_eq!(s.overflow(), "tail");
        assert_eq!(s.live_span("s1").unwrap().anchor.start, 2);
        // Same metadata body but a contradictory replacement envelope refuses.
        let before = s.clone();
        let ops = json!([{"kind":"move","body":"a clip ","start":0,"deletedText":"","insertedText":"wrong","move":{"start":0,"end":2,"to":2}}]);
        assert!(s
            .replay_history_group_atomic(
                "a clip ",
                "a clip ",
                &parse_history_operations(&ops.to_string()).unwrap()
            )
            .is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn overflow_unapplied_and_late_body_guards_leave_complete_session_unchanged() {
        let mut s = DocumentSession::new();
        s.open("abc");
        s.set_overflow("edited").unwrap();
        let ops = json!([edit("abcX", 3, "", "X"), {"kind":"overflow","body":"abc","start":3,"deletedText":"X","insertedText":"","before":"note\n\nchunk","after":"note"}]);
        let before = s.clone();
        assert!(s
            .replay_history_group_atomic(
                "abc",
                "abc",
                &parse_history_operations(&ops.to_string()).unwrap()
            )
            .is_err());
        assert_eq!(s, before);
        // Legacy API remains permissive; only the new atomic group refuses.
        assert!(!s.replay_overflow("note\n\nchunk", "note").unwrap().applied);
        for ops in [
            json!([edit("wrong", 0, "a", "A")]),
            json!([edit("Abc", 0, "z", "A")]),
        ] {
            assert!(s
                .replay_history_group_atomic(
                    "abc",
                    "Abc",
                    &parse_history_operations(&ops.to_string()).unwrap()
                )
                .is_err());
            assert_eq!(s, before);
        }
        assert!(s
            .replay_history_group_atomic("abc", "different", &[])
            .is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn strict_parser_rejects_unknown_conflicting_float_and_bad_snapshots() {
        for input in [
            json!([{"kind":"unknown","body":"abc"}]),
            json!([{"kind":"edit","body":"abc","start":0,"deletedText":"","insertedText":"","swap":{}}]),
            json!([{"kind":"edit","body":"abc","start":1.0,"deletedText":"","insertedText":""}]),
            json!([{"kind":"edit","body":"abc","start":-1,"deletedText":"","insertedText":""}]),
            json!([{"kind":"edit","body":"abc","start":4294967296u64,"deletedText":"","insertedText":""}]),
            json!([{"kind":"ghost","body":"abc","change":{},"undo":"true"}]),
            json!([{"kind":"move","body":"abc","start":0,"deletedText":"","insertedText":"","move":{"start":0,"end":1,"to":2,"extra":true}}]),
            json!([{"kind":"alt","body":"abc","start":0,"deletedText":"","insertedText":"","span":"s1","target":{}}]),
        ] {
            assert_eq!(
                parse_history_operations(&input.to_string())
                    .unwrap_err()
                    .kind,
                "invalid-payload"
            );
        }
        let mut s = DocumentSession::new();
        s.open("a😀b");
        let mut fixture = Document::new("abc");
        fixture.add_span(SpanKind::Word, 0, 1).unwrap();
        let mut invalid = json!(fixture.annotations.spans[0]);
        invalid["active"] = json!(99);
        let invalid = json!([{"kind":"alt","body":"abc","start":0,"deletedText":"","insertedText":"","span":"s1","target":invalid}]);
        assert_eq!(
            parse_history_operations(&invalid.to_string())
                .unwrap_err()
                .kind,
            "invalid-payload",
            "typed but semantically invalid span must refuse before model use"
        );
        let before = s.clone();
        for op in [edit("bad", 2, "", "X"), edit("bad", 99, "", "X")] {
            assert_eq!(
                s.replay_history_group_atomic(
                    "a😀b",
                    "bad",
                    &parse_history_operations(&json!([op]).to_string()).unwrap()
                )
                .unwrap_err()
                .kind,
                "invalid-range"
            );
            assert_eq!(s, before);
        }
    }

    #[test]
    fn conflicted_checkpoint_preserves_interleaved_metadata_and_requires_explicit_reopen() {
        with_session(|s| {
            s.open("abcX");
        });
        let ops = json!([edit("abc", 3, "X", "")]).to_string();
        let started = begin_history_replay_json("abcX", "abc", &ops).unwrap();
        let token = started["token"].as_str().unwrap();
        with_session(|s| {
            s.set_overflow("interleaved").unwrap();
        });
        let interleaved = with_session(|s| s.clone());
        assert_eq!(
            finish_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(
            abort_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(with_session(|s| s.clone()), interleaved);
        assert_eq!(
            begin_history_replay_json("abc", "abc", "[]")
                .unwrap_err()
                .kind,
            "pending"
        );
        assert!(recover_history_transaction_json("wrong", "fresh").is_err());
        assert_eq!(with_session(|s| s.clone()), interleaved);
        let recovery = recover_history_transaction_json(token, &interleaved.save()).unwrap();
        assert_eq!(recovery["rolledBack"], false);
        assert_eq!(recovery["recovered"], true);
        assert_eq!(with_session(|s| s.overflow().to_string()), "interleaved");
        assert!(abort_history_transaction_json(token).is_err());
        let next = begin_history_replay_json("abc", "abc", "[]").unwrap();
        assert_ne!(next["token"], started["token"]);
        abort_history_transaction_json(next["token"].as_str().unwrap()).unwrap();
    }

    #[test]
    fn interleaved_open_never_discards_checkpoint_or_reuses_token() {
        with_session(|s| {
            s.open("abc");
        });
        let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
        let token = started["token"].as_str().unwrap();
        with_session(|s| {
            s.open("new body");
        });
        let before = with_session(|s| s.clone());
        assert_eq!(
            abort_history_transaction_json(token).unwrap_err().kind,
            "session-conflict"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        recover_history_transaction_json(token, "new body").unwrap();
        let next = begin_history_replay_json("new body", "new body", "[]").unwrap();
        assert_ne!(next["token"], started["token"]);
        let token = next["token"].as_str().unwrap();
        assert_eq!(
            finish_history_transaction_json(token).unwrap(),
            json!({"ok":true,"finalized":true,"token":token})
        );
        assert!(abort_history_transaction_json(token).is_err());
    }

    #[test]
    fn private_token_exhaustion_and_failed_begin_are_safe() {
        with_session(|s| {
            s.open("abc");
        });
        let before = with_session(|s| s.clone());
        HISTORY_BRIDGE.with(|b| b.borrow_mut().watermark = u64::MAX);
        assert_eq!(
            begin_history_replay_json("abc", "abc", "[]")
                .unwrap_err()
                .kind,
            "token-exhausted"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        assert!(HISTORY_BRIDGE.with(|b| b.borrow().pending.is_none()));
        HISTORY_BRIDGE.with(|b| b.borrow_mut().watermark = 0);
        let bad = json!([edit("bad", 0, "z", "X")]).to_string();
        assert!(begin_history_replay_json("abc", "bad", &bad).is_err());
        assert_eq!(with_session(|s| s.clone()), before);
        let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
        assert_eq!(started["token"], "history-1");
        abort_history_transaction_json("history-1").unwrap();
        let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
        assert_eq!(started["token"], "history-2");
        abort_history_transaction_json("history-2").unwrap();
    }

    #[test]
    fn byte_count_nesting_and_body_transport_limits_are_exact() {
        let exact = format!("[]{}", " ".repeat(HISTORY_JSON_LIMIT - 2));
        assert!(parse_history_operations(&exact).unwrap().is_empty());
        assert_eq!(
            parse_history_operations(&(exact + " ")).unwrap_err().kind,
            "limit"
        );
        let operation = edit("abc", 0, "", "");
        let exact = json!(vec![operation.clone(); HISTORY_OPERATION_LIMIT]).to_string();
        assert_eq!(
            parse_history_operations(&exact).unwrap().len(),
            HISTORY_OPERATION_LIMIT
        );
        let mut tiny = DocumentSession::new();
        tiny.open("abc");
        let before_tiny = tiny.clone();
        tiny.replay_history_group_atomic("abc", "abc", &parse_history_operations(&exact).unwrap())
            .unwrap();
        assert_eq!(tiny, before_tiny);
        assert_eq!(
            parse_history_operations(
                &json!(vec![operation; HISTORY_OPERATION_LIMIT + 1]).to_string()
            )
            .unwrap_err()
            .kind,
            "limit"
        );
        assert_eq!(
            parse_history_operations(&format!("{}0{}", "[".repeat(150), "]".repeat(150)))
                .unwrap_err()
                .kind,
            "invalid-payload"
        );
        let body = "é".repeat(HISTORY_JSON_LIMIT / 2);
        let mut s = DocumentSession::new();
        s.open(&body);
        s.replay_history_group_atomic(&body, &body, &[]).unwrap();
        let before = s.clone();
        assert_eq!(
            s.replay_history_group_atomic(&(body.clone() + "x"), &body, &[])
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(
            s.replay_history_group_atomic(&body, &(body.clone() + "x"), &[])
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(s, before);
    }

    #[test]
    fn kept_limits_and_failed_protection_preserve_pending_and_session() {
        let ids = json!(vec![0u32; HISTORY_OPERATION_LIMIT]).to_string();
        assert_eq!(
            bounded_history_array(&ids)
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            HISTORY_OPERATION_LIMIT
        );
        with_session(|s| {
            s.open("abc");
        });
        let started = begin_history_replay_json("abc", "abc", "[]").unwrap();
        let before = with_session(|s| s.clone());
        let ids = json!(vec![0u32; HISTORY_OPERATION_LIMIT + 1]).to_string();
        let oversized = " ".repeat(HISTORY_JSON_LIMIT + 1);
        assert_eq!(
            begin_history_replay_json("abc", "abc", &oversized)
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(
            begin_history_replay_json("abc", &oversized, "[]")
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(
            begin_trim_ghost_transaction_json("abc", "sharper", &ids)
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(
            begin_trim_ghost_transaction_json("abc", "sharper", "[1.0]")
                .unwrap_err()
                .kind,
            "invalid-payload"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        abort_history_transaction_json(started["token"].as_str().unwrap()).unwrap();
        crate::kg::clear_thesaurus();
        assert_eq!(
            begin_trim_ghost_transaction_json("abc", "sharper", "[]")
                .unwrap_err()
                .kind,
            "protection-refusal"
        );
        assert_eq!(with_session(|s| s.clone()), before);
    }

    #[test]
    fn protected_body_malformed_manual_overlap_and_noop_guards_precede_publication() {
        const BODY: &str = "The editor is basically the only target here today.";
        crate::kg::load_thesaurus(
            r#"{"name":"public-test","data":{"unmentioned":{"id":1,"nterm":"unmentioned"}}}"#,
        )
        .unwrap();
        let source = format!("{BODY}\n\n```terraphim-alternatives\n{{\n```\n");
        with_session(|s| {
            s.open(&source);
        });
        crate::lab::session_trim_plan_json().unwrap();
        let before = with_session(|s| s.clone());
        assert_eq!(
            begin_trim_ghost_transaction_json("wrong", "sharper", "[]")
                .unwrap_err()
                .kind,
            "body-conflict"
        );
        assert_eq!(
            begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
                .unwrap_err()
                .kind,
            "model-refusal"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        assert_eq!(with_session(|s| s.save()), source);
        with_session(|s| {
            s.open(BODY);
        });
        crate::lab::session_trim_plan_json().unwrap();
        assert_eq!(
            begin_trim_ghost_transaction_json(BODY, "original", "[]").unwrap(),
            None
        );
        let kept = json!(vec![0u32; HISTORY_OPERATION_LIMIT]).to_string();
        assert_eq!(
            begin_trim_ghost_transaction_json(BODY, "original", &kept).unwrap(),
            None
        );
        let ranges = crate::lab::protected_trim_ranges("sharper", "[]").unwrap();
        assert!(!ranges.is_empty());
        with_session(|s| {
            s.ghost(ranges[0].0, ranges[0].1).unwrap();
        });
        let before = with_session(|s| s.clone());
        assert_eq!(
            begin_trim_ghost_transaction_json(BODY, "sharper", "[]")
                .unwrap_err()
                .kind,
            "model-refusal"
        );
        assert_eq!(with_session(|s| s.clone()), before);
        assert!(HISTORY_BRIDGE.with(|b| b.borrow().pending.is_none()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terraphim_alternatives::{Source, SpanKind};

    #[test]
    fn independent_transaction_adjacent_receipt_replay_and_reservation() {
        let mut s = DocumentSession::new();
        s.open("abcdef");
        s.ghost(4, 6).unwrap();
        let manual = s.document().annotations.ghosts.clone();
        let change = s
            .ghost_ranges_atomic("abcdef", &[(0, 2), (2, 4)])
            .unwrap()
            .unwrap();
        assert_eq!(s.document().annotations.ghosts.len(), 3);
        assert_eq!(change.to_json()["body"], "abcdef");
        let saved = s.save();
        let mut reopened = DocumentSession::new();
        reopened.open(&saved);
        assert_eq!(reopened.export(), "");
        s.replay_ghost_change(&change, true).unwrap();
        assert_eq!(s.document().annotations.ghosts, manual);
        s.replay_ghost_change(&change, false).unwrap();
        s.replay_ghost_change(&change, true).unwrap();
        assert_eq!(s.ghost(0, 1).unwrap(), "g4");
        let before = s.clone();
        assert!(s.replay_ghost_change(&change, false).is_err());
        assert_eq!(s, before);
    }

    #[test]
    fn independent_transaction_refusals_restore_entire_session() {
        let mut s = DocumentSession::new();
        s.open("a😀bc");
        for ranges in [
            vec![(0, 1), (2, 3)],
            vec![(0, 1), (4, 99)],
            vec![(1, 1)],
            vec![(0, 3), (1, 4)],
        ] {
            let before = s.clone();
            assert!(s.ghost_ranges_atomic("a😀bc", &ranges).is_err());
            assert_eq!(s, before);
        }
        let before = s.clone();
        assert!(s.ghost_ranges_atomic("stale", &[(0, 1)]).is_err());
        assert_eq!(s, before);
        let source = "abc\n\n```terraphim-alternatives\n{\n```\n";
        s.open(source);
        let before = s.clone();
        assert!(s.ghost_ranges_atomic("abc", &[(0, 1)]).is_err());
        assert_eq!(s, before);
        assert_eq!(s.save(), source);
    }

    #[test]
    fn independent_transaction_preserves_spans_overflow_and_set_aside() {
        let mut s = DocumentSession::new();
        s.open("abcdef xyz");
        s.alt_create_span(SpanKind::Word, 0, 6, "alias", Source::Human)
            .unwrap();
        s.set_overflow("notes").unwrap();
        s.ghost(7, 10).unwrap();
        s.apply_edit(7, 3, "").unwrap();
        let before = s.clone();
        let body = s.body().to_string();
        let change = s
            .ghost_ranges_atomic(&body, &[(0, 3), (3, 6)])
            .unwrap()
            .unwrap();
        assert_eq!(change.after.live[0].id, "g2");
        assert_eq!(s.doc.annotations.spans, before.doc.annotations.spans);
        assert_eq!(s.set_aside, before.set_aside);
        assert_eq!(s.overflow(), "notes");
        s.alt_add("s1", "other", Source::Human).unwrap();
        s.set_overflow("later notes").unwrap();
        s.replay_ghost_change(&change, true).unwrap();
        assert_eq!(s.doc.annotations.spans[0].alts.len(), 3);
        assert_eq!(s.overflow(), "later notes");
        s.replay_ghost_change(&change, false).unwrap();
        assert_eq!(s.set_aside, before.set_aside);
        assert_eq!(s.body(), body);
        let receipt = GhostChange::from_json(&change.to_json()).unwrap();
        assert_eq!(receipt, change);
        let mut forged = receipt.clone();
        forged.before.live.clear();
        forged.after.live[0].anchor.text = "forged".into();
        let snapshot = s.clone();
        assert!(s.replay_ghost_change(&forged, true).is_err());
        assert_eq!(s, snapshot);
        s.sync_body("different");
        let snapshot = s.clone();
        assert!(s.replay_ghost_change(&receipt, true).is_err());
        assert_eq!(s, snapshot);
    }

    #[test]
    fn independent_transaction_manual_overlap_and_empty_are_no_mutation() {
        let mut s = DocumentSession::new();
        s.open("abcdef");
        s.ghost(0, 2).unwrap();
        let before = s.clone();
        assert!(s.ghost_ranges_atomic("abcdef", &[(1, 3)]).is_err());
        assert_eq!(s.ghost_ranges_atomic("abcdef", &[]).unwrap(), None);
        assert_eq!(s, before);
    }

    #[test]
    fn independent_transaction_group_preflight_refuses_late_conflict_atomically() {
        let mut s = DocumentSession::new();
        s.open("abcdef");
        let a = s.ghost_ranges_atomic("abcdef", &[(0, 1)]).unwrap().unwrap();
        let b = s.ghost_ranges_atomic("abcdef", &[(2, 3)]).unwrap().unwrap();
        let before = s.clone();
        let invalid = vec![(b.clone(), true), (a.clone(), false)];
        assert!(s.preflight_ghost_changes(&invalid).is_err());
        assert!(s.replay_ghost_changes(&invalid).is_err());
        assert_eq!(s, before);
        let undo = vec![(b.clone(), true), (a.clone(), true)];
        s.preflight_ghost_changes(&undo).unwrap();
        assert_eq!(s, before, "preflight is read-only");
        s.replay_ghost_changes(&undo).unwrap();
        assert!(s.doc.annotations.ghosts.is_empty());
        s.replay_ghost_changes(&[(a, false), (b, false)]).unwrap();
        assert_eq!(s, before);
    }

    #[test]
    fn independent_transaction_exhaustion_live_set_aside_and_partial_batch() {
        let mut s = DocumentSession::new();
        s.open("abcdef");
        // Private native fixture: no unsafe public allocator setter.
        s.reserved_ids.insert(format!("g{}", u64::MAX - 1));
        let before = s.clone();
        assert!(s.ghost_ranges_atomic("abcdef", &[(0, 1), (1, 2)]).is_err());
        assert_eq!(
            s, before,
            "second allocation failure restores first allocation too"
        );
        let change = s.ghost_ranges_atomic("abcdef", &[(0, 4)]).unwrap().unwrap();
        let before = s.clone();
        assert_eq!(s.revive(1, 2), Err(EditError::IdAllocation));
        assert_eq!(s.ghost(5, 6), Err(EditError::IdAllocation));
        assert_eq!(s, before);
        s.replay_ghost_change(&change, true).unwrap();
        let before = s.clone();
        assert_eq!(s.ghost(5, 6), Err(EditError::IdAllocation));
        assert_eq!(
            s, before,
            "undo does not release exhausted history identity"
        );
        s.replay_ghost_change(&change, false).unwrap();
        s.apply_edit(0, 4, "").unwrap();
        let before = s.clone();
        assert_eq!(s.ghost(0, 1), Err(EditError::IdAllocation));
        assert_eq!(s, before, "set-aside IDs are reserved");
        s.apply_edit(0, 0, "abcd").unwrap();
        assert_eq!(s.doc.annotations.ghosts[0].id, format!("g{}", u64::MAX));
    }

    #[test]
    fn session_allocator_reserves_shared_namespace_and_removed_split_ids() {
        let mut doc = Document::new("abcdef xyz");
        doc.add_span(SpanKind::Word, 7, 10).unwrap();
        doc.annotations.spans[0].id = "g8".into();
        let mut s = DocumentSession::new();
        s.open(&write(&doc));
        assert_eq!(s.ghost(0, 6).unwrap(), "g9");
        s.revive(2, 4).unwrap();
        assert_eq!(s.doc.annotations.ghosts[1].id, "g10");
        s.revive(0, 6).unwrap();
        s.apply_edit(8, 0, "X").unwrap();
        assert_eq!(s.set_aside.spans[0].id, "g8");
        assert_eq!(s.ghost(0, 1).unwrap(), "g11");
        assert_eq!(
            s.ghost(1, 2).unwrap(),
            "g11",
            "merge keeps earliest identity"
        );
    }

    // ----- in-place cycling (issue #9) --------------------------------------

    /// "Pass me a paperclip. Drop this." with span `s1` over "paperclip"
    /// (alternatives "eraser", "thumbtack") and a ghost over " Drop this.".
    fn cycling_source() -> String {
        let mut doc = Document::new("Pass me a paperclip. Drop this.");
        let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
        doc.add_alternative(&id, "eraser", Source::Human, None)
            .unwrap();
        doc.add_alternative(&id, "thumbtack", Source::Human, None)
            .unwrap();
        doc.ghost(20, 31).unwrap();
        write(&doc)
    }

    /// Applies a swap edit to `text`, checking the deleted text matches.
    fn apply_swap_edit(text: &str, edit: &SwapEdit) -> String {
        let start = utf16_to_byte(text, edit.start).unwrap();
        let end = start + edit.deleted_text.len();
        assert_eq!(&text[start..end], edit.deleted_text);
        format!("{}{}{}", &text[..start], edit.inserted_text, &text[end..])
    }

    #[test]
    fn swap_returns_the_exact_edit_and_fixes_the_article() {
        let mut session = DocumentSession::new();
        session.open(&cycling_source());
        let mut text = session.body().to_string();

        // a paperclip -> an eraser: one edit covering the article and span.
        let swap = session.set_active("s1", 1).unwrap();
        assert_eq!((swap.span.as_str(), swap.from, swap.to), ("s1", 0, 1));
        let edit = swap.edit.clone().unwrap();
        assert_eq!(edit.start, 9, "the edit starts after the shared 'a'");
        assert_eq!(edit.deleted_text, " paperclip");
        assert_eq!(edit.inserted_text, "n eraser");
        text = apply_swap_edit(&text, &edit);
        assert_eq!(text, "Pass me an eraser. Drop this.");
        assert_eq!(session.body(), text);
        assert_eq!(swap.outcome, EditOutcome::default());
        assert_eq!(swap.range, (11, 17));

        // an eraser -> a thumbtack, then wrap back to the original.
        let edit = session.set_active("s1", 2).unwrap().edit.unwrap();
        text = apply_swap_edit(&text, &edit);
        assert_eq!(text, "Pass me a thumbtack. Drop this.");
        let edit = session.set_active("s1", 0).unwrap().edit.unwrap();
        text = apply_swap_edit(&text, &edit);
        assert_eq!(text, "Pass me a paperclip. Drop this.");
        assert_eq!(session.body(), text);

        // Back where it started: the saved file is the original one.
        assert_eq!(session.save(), cycling_source());
    }

    #[test]
    fn swap_refreshes_context_and_moves_the_ghost() {
        let mut session = DocumentSession::new();
        session.open(&cycling_source());
        session.set_active("s1", 1).unwrap();
        let doc = session.document();
        let span = &doc.annotations.spans[0];
        assert_eq!(span.active, 1);
        assert_eq!(
            (
                span.anchor.start,
                span.anchor.end,
                span.anchor.text.as_str()
            ),
            (11, 17, "eraser")
        );
        assert_eq!(span.anchor.before.as_deref(), Some("Pass me an "));
        assert_eq!(span.anchor.after.as_deref(), Some(". Drop this."));
        let ghost = &doc.annotations.ghosts[0];
        assert_eq!(
            (ghost.anchor.start, ghost.anchor.text.as_str()),
            (18, " Drop this.")
        );
        // The saved file reopens with the chosen alternative active.
        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&session.save());
        assert_eq!(opened.unresolved, 0);
        assert_eq!(opened.body, "Pass me an eraser. Drop this.");
        assert_eq!(reopened.document().annotations.spans[0].active, 1);
        assert_eq!(reopened.export(), "Pass me an eraser.");
    }

    #[test]
    fn a_refused_swap_changes_nothing() {
        let mut session = DocumentSession::new();
        session.open(&cycling_source());
        let before = session.clone();
        assert_eq!(
            session.set_active("s1", 3).unwrap_err(),
            EditError::InvalidIndex {
                id: "s1".into(),
                index: 3
            }
        );
        assert_eq!(
            session.set_active("s9", 1).unwrap_err(),
            EditError::UnknownSpan("s9".into())
        );
        assert_eq!(session, before);
        // Making the active alternative active again is a quiet no-op.
        let swap = session.set_active("s1", 0).unwrap();
        assert_eq!(swap.edit, None);
        assert_eq!((swap.from, swap.to), (0, 0));
        assert_eq!(session, before);
        // JSON shape for JavaScript.
        let json = session.set_active("s1", 0).unwrap().to_json();
        assert_eq!(json["edit"], Value::Null);
        assert_eq!(json["span"], "s1");
        let json = session.set_active("s1", 2).unwrap().to_json();
        assert_eq!(
            json["edit"],
            json!({ "start": 10, "deletedLength": 9, "deletedText": "paperclip", "insertedText": "thumbtack" })
        );
        assert_eq!(
            (json["from"].clone(), json["to"].clone()),
            (json!(0), json!(2))
        );
        assert_eq!(json["range"], json!({ "start": 10, "end": 19 }));
    }

    #[test]
    fn swap_offsets_are_utf16_and_set_aside_hints_follow() {
        // A non-BMP character before the span: offsets are UTF-16 units.
        let mut doc = Document::new("\u{1F600} a apple and a pear.");
        let id = doc.add_span(SpanKind::Word, 5, 10).unwrap();
        doc.add_alternative(&id, "banana", Source::Human, None)
            .unwrap();
        let id2 = doc.add_span(SpanKind::Word, 17, 21).unwrap();
        doc.add_alternative(&id2, "plum", Source::Human, None)
            .unwrap();
        let mut session = DocumentSession::new();
        session.open(&write(&doc));
        // Detach s2 by typing inside it ("peXar").
        session.apply_edit(19, 0, "X").unwrap();
        assert_eq!(session.set_aside().spans.len(), 1);
        let hint = session.set_aside().spans[0].anchor.start;

        let swap = session.set_active("s1", 1).unwrap();
        let edit = swap.edit.unwrap();
        // "a apple" was ungrammatical; the fix-up follows "banana" ("a").
        assert_eq!(edit.start, 5);
        assert_eq!(edit.deleted_text, "apple");
        assert_eq!(edit.inserted_text, "banana");
        assert_eq!(session.body(), "\u{1F600} a banana and a peXar.");
        // The hint after the swap shifted by one unit.
        assert_eq!(session.set_aside().spans[0].anchor.start, hint + 1);
        assert_eq!(swap.outcome.set_aside, 1);
        assert!(swap.outcome.notice.is_some());
        // Removing the typo where the text now is re-attaches s2.
        let outcome = session.apply_edit(20, 1, "").unwrap();
        assert_eq!(outcome.reattached, vec!["s2".to_string()]);
    }

    // ----- end of in-place cycling (issue #9) -------------------------------

    // ----- moving text (issue #44) ------------------------------------------

    /// Two paragraphs: a span and a ghost in the first, plain text after.
    fn two_paragraphs() -> String {
        let mut doc = Document::new("Pass me a paperclip. Drop this.\n\nSecond block.\n\n");
        let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
        doc.add_alternative(&id, "eraser", Source::Human, None)
            .unwrap();
        doc.ghost(20, 31).unwrap();
        write(&doc)
    }

    #[test]
    fn move_carries_annotations_and_survives_save_and_reopen() {
        let mut session = DocumentSession::new();
        session.open(&two_paragraphs());
        let outcome = session.move_range(0, 33, 48).unwrap();
        assert_eq!(outcome, EditOutcome::default());
        assert_eq!(
            session.body(),
            "Second block.\n\nPass me a paperclip. Drop this.\n\n"
        );
        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&session.save());
        assert_eq!(opened.unresolved, 0);
        let doc = reopened.document();
        assert_eq!(doc.annotations.spans[0].id, "s1");
        assert_eq!(doc.annotations.spans[0].anchor.start, 25);
        assert_eq!(doc.annotations.spans[0].alts.len(), 2);
        assert_eq!(doc.annotations.ghosts[0].id, "g1");
        assert_eq!(doc.annotations.ghosts[0].anchor.text, " Drop this.");
        assert_eq!(
            reopened.export(),
            "Second block.\n\nPass me a paperclip.\n\n"
        );

        // The inverse move restores the original document exactly.
        session.move_range(15, 48, 0).unwrap();
        let mut original = DocumentSession::new();
        original.open(&two_paragraphs());
        assert_eq!(session.document(), original.document());
        assert_eq!(session.save(), two_paragraphs());
    }

    #[test]
    fn a_refused_move_changes_nothing() {
        let mut session = DocumentSession::new();
        session.open(&two_paragraphs());
        let before = session.clone();
        // 12..33 starts inside the span "paperclip".
        let error = session.move_range(12, 33, 48).unwrap_err();
        assert_eq!(error, EditError::Straddles("s1".into()));
        assert_eq!(session, before);
        assert!(session.move_range(0, 33, 20).is_err(), "inside the range");
        assert!(session.move_range(0, 99, 0).is_err(), "out of bounds");
        assert_eq!(session, before);
        // A no-op move succeeds and changes nothing.
        assert_eq!(
            session.move_range(0, 33, 33).unwrap(),
            EditOutcome::default()
        );
        assert_eq!(session, before);
    }

    #[test]
    fn set_aside_hints_follow_a_move_and_reattach() {
        let mut session = DocumentSession::new();
        session.open(&two_paragraphs());
        // Detach the span by typing inside it ("papXerclip").
        session.apply_edit(13, 0, "X").unwrap();
        assert_eq!(session.set_aside().spans.len(), 1);
        // Move the first paragraph (now 34 units) to the end: the hint moves
        // with its text.
        let outcome = session.move_range(0, 34, 49).unwrap();
        assert_eq!(outcome.set_aside, 1);
        assert!(outcome.notice.is_some());
        assert_eq!(session.set_aside().spans[0].anchor.start, 25);
        // Fixing the typo where the text now is re-attaches the span.
        let outcome = session.apply_edit(28, 1, "").unwrap();
        assert_eq!(outcome.reattached, vec!["s1".to_string()]);

        // Hints between the range and the destination shift; others stay.
        let mut hints = SetAside::default();
        for (start, end) in [(0, 2), (5, 7), (9, 10), (7, 9), (12, 13)] {
            hints.ghosts.push(Ghost {
                id: format!("g{start}"),
                anchor: Anchor::new(start, end, "x".repeat(end - start)),
            });
        }
        let starts = |hints: &SetAside| -> Vec<usize> {
            hints.ghosts.iter().map(|g| g.anchor.start).collect()
        };
        // Move 4..8 to 11: inside moves by 3, between moves back by 4, the
        // one straddling the range end and the ones outside stay.
        hints.move_hints(4, 8, 11);
        assert_eq!(starts(&hints), vec![0, 8, 5, 7, 12]);
        // Move 7..11 back to 4: the two hints now inside move back by 3, the
        // one between moves on by 4.
        hints.move_hints(7, 11, 4);
        assert_eq!(starts(&hints), vec![0, 5, 9, 4, 12]);
        hints.move_hints(0, 2, 2);
        assert_eq!(starts(&hints), vec![0, 5, 9, 4, 12], "a no-op move");
    }

    /// A saved file with one word span (active alternative 1), one ghost and
    /// overflow, built through the crate so the fixture is always valid.
    fn annotated_source() -> String {
        let mut doc = Document::new("Pass me a paperclip. Drop this sentence.");
        let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
        doc.add_alternative(&id, "eraser", Source::Human, None)
            .unwrap();
        doc.set_active(&id, 1).unwrap();
        doc.ghost(18, 38).unwrap();
        doc.annotations.overflow = "stashed".into();
        write(&doc)
    }

    #[test]
    fn plain_markdown_opens_and_saves_unchanged() {
        let mut session = DocumentSession::new();
        let opened = session.open("# Title\n\nJust text.\n");
        assert_eq!(opened.body, "# Title\n\nJust text.\n");
        assert_eq!(opened.warning, None);
        assert_eq!(session.save(), "# Title\n\nJust text.\n");
        assert_eq!(session.export(), "# Title\n\nJust text.\n");
    }

    #[test]
    fn block_is_hidden_from_body_and_round_trips() {
        let source = annotated_source();
        let mut session = DocumentSession::new();
        let opened = session.open(&source);
        assert_eq!(opened.body, "Pass me an eraser. Drop this sentence.");
        assert!(!opened.body.contains("terraphim-alternatives"));
        assert_eq!(opened.warning, None);
        assert_eq!(session.save(), source);

        let mut reopened = DocumentSession::new();
        reopened.open(&session.save());
        assert_eq!(reopened.document(), session.document());
    }

    #[test]
    fn edits_move_anchors_and_survive_save_and_reopen() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        // Type a word at the start; everything shifts by six code units.
        assert!(session
            .apply_edit(0, 0, "Hey! ")
            .unwrap()
            .detached
            .is_empty());
        assert!(session.apply_edit(0, 0, "😀").unwrap().detached.is_empty());
        let saved = session.save();

        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&saved);
        assert_eq!(opened.body, "😀Hey! Pass me an eraser. Drop this sentence.");
        let doc = reopened.document();
        let span = &doc.annotations.spans[0];
        assert_eq!(span.active, 1);
        assert_eq!(span.alts.len(), 2);
        assert_eq!((span.anchor.start, span.anchor.end), (18, 24));
        assert_eq!(
            doc.annotations.ghosts[0].anchor.text,
            " Drop this sentence."
        );
        assert_eq!(doc.annotations.overflow, "stashed");
        assert_eq!(reopened.export(), "😀Hey! Pass me an eraser.");
    }

    #[test]
    fn edit_inside_a_span_detaches_it() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        let outcome = session.apply_edit(12, 1, "X").unwrap();
        assert_eq!(outcome.detached, vec!["s1".to_string()]);
        assert_eq!(outcome.set_aside, 1);
        let warning = outcome.warning.expect("detaching warns");
        assert!(
            warning.contains("1 span with 2 alternatives") && warning.contains("preserved"),
            "{warning}"
        );
        assert!(session.document().annotations.spans.is_empty());
        let kept = &session.set_aside().spans;
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "s1");
        assert_eq!(
            kept[0].alts.len(),
            2,
            "the detached span keeps its alternatives"
        );
        assert_eq!(
            session.annotations_json()["setAside"]["spans"][0]["id"],
            "s1"
        );
    }

    #[test]
    fn counts_include_ghosted_text() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        let counts = session.counts();
        assert_eq!(counts.words, 7);
        assert_eq!(counts.chars, "Pass me an eraser. Drop this sentence.".len());
        assert_eq!(session.export(), "Pass me an eraser.");
    }

    #[test]
    fn malformed_block_warns_once_and_is_saved_verbatim() {
        let source =
            "Body text.\n\n```terraphim-alternatives\n{ \"version\": 1, \"spans\": [,] }\n```\n";
        let mut session = DocumentSession::new();
        let opened = session.open(source);
        assert_eq!(opened.body, "Body text.");
        let warning = opened.warning.expect("a malformed block warns");
        assert!(warning.contains("could not be read"), "{warning}");
        assert!(session.has_preserved_block());
        assert_eq!(session.save(), source);

        session.apply_edit(0, 4, "Some").unwrap();
        assert_eq!(
            session.save(),
            source.replacen("Body", "Some", 1),
            "edits to the body keep the raw block"
        );
        assert_eq!(session.export(), "Some text.");

        // Opening another file clears the warning and the preserved block.
        let opened = session.open("fresh");
        assert_eq!(opened.warning, None);
        assert!(!session.has_preserved_block());
    }

    #[test]
    fn stale_anchors_are_reanchored_or_reported() {
        // Hand edit outside the editor: text inserted before the span.
        let source = annotated_source().replacen("Pass me", "Please pass me", 1);
        let mut session = DocumentSession::new();
        let opened = session.open(&source);
        assert_eq!(opened.warning, None);
        assert_eq!(session.document().annotations.spans[0].anchor.start, 18);

        // The span's text vanished: reported, not guessed.
        let source = annotated_source().replacen("an eraser", "a pencil", 1);
        let opened = session.open(&source);
        assert_eq!(opened.unresolved, 1);
        assert!(opened.warning.unwrap().contains("preserved and saved"));
        assert_eq!(session.set_aside().spans[0].alts[1].text, "eraser");
    }

    #[test]
    fn sync_body_reanchors_after_drift() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        assert_eq!(
            session.sync_body("Pass me an eraser. Drop this sentence."),
            Synced::default()
        );
        let synced = session.sync_body("Well. Pass me an eraser. Drop this sentence.");
        assert!(synced.changed);
        assert_eq!(synced.unresolved, 0);
        assert_eq!(session.document().annotations.spans[0].anchor.start, 17);
    }

    /// Ids, starts and active indices of live spans, for comparisons.
    fn live(session: &DocumentSession) -> Vec<(String, usize, usize)> {
        session
            .document()
            .annotations
            .spans
            .iter()
            .map(|s| (s.id.clone(), s.anchor.start, s.active))
            .collect()
    }

    #[test]
    fn undoing_the_detaching_edit_reattaches_the_span() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        let before = session.document().clone();
        session.apply_edit(12, 0, "z").unwrap();
        assert!(session.document().annotations.spans.is_empty());
        // The inverse edit (what Ctrl+Z replays) brings the alternatives back.
        let outcome = session.apply_edit(12, 1, "").unwrap();
        assert_eq!(outcome.reattached, vec!["s1".to_string()]);
        assert_eq!(outcome.set_aside, 0);
        assert_eq!(outcome.warning, None);
        assert_eq!(session.document(), &before);
    }

    #[test]
    fn set_aside_hints_follow_edits_before_them() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        session.apply_edit(12, 0, "z").unwrap();
        // Edits before the detached span shift its hint; edits after do not.
        session.apply_edit(0, 0, "Oh, ").unwrap();
        session.apply_edit(30, 0, "!").unwrap();
        assert_eq!(session.set_aside().spans[0].anchor.start, 15);
        let outcome = session.apply_edit(16, 1, "").unwrap();
        assert_eq!(outcome.reattached, vec!["s1".to_string()]);
        assert_eq!(live(&session), vec![("s1".to_string(), 15, 1)]);
    }

    #[test]
    fn detached_spans_are_saved_and_survive_reopen() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        session.apply_edit(12, 0, "z").unwrap();
        let saved = session.save();
        // The block holds the detached span with its last known anchor, and
        // the crate reads it back (stale anchors are structurally legal).
        let parsed = parse(&saved).expect("saved block parses");
        assert_eq!(parsed.annotations.spans.len(), 1);
        assert_eq!(parsed.annotations.spans[0].anchor.start, 11);
        assert_eq!(parsed.annotations.spans[0].alts[1].text, "eraser");

        // Reopening sets it aside again (its text is gone) and keeps it.
        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&saved);
        assert_eq!(opened.unresolved, 1);
        assert!(opened.warning.is_some());
        assert_eq!(reopened.save(), saved, "a save/open cycle is lossless");

        // Fixing the text in the reopened file re-attaches it.
        let outcome = reopened.apply_edit(12, 1, "").unwrap();
        assert_eq!(outcome.reattached, vec!["s1".to_string()]);
        let mut again = DocumentSession::new();
        let opened = again.open(&reopened.save());
        assert_eq!(opened.unresolved, 0);
        assert_eq!(live(&again), vec![("s1".to_string(), 11, 1)]);
    }

    #[test]
    fn set_aside_items_that_would_overlap_or_clash_are_moved_and_renamed() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        session.apply_edit(12, 0, "X").unwrap(); // "eXraser"; s1 set aside at 11..17
                                                 // A new live span over the edited word takes the free id `s1` and
                                                 // overlaps the set-aside span's last known range.
        let id = session.doc.add_span(SpanKind::Word, 11, 18).unwrap();
        assert_eq!(id, "s1");
        session
            .doc
            .add_alternative(&id, "rubber", Source::Human, None)
            .unwrap();

        let saved = session.save();
        let parsed = parse(&saved).expect("no overlap or duplicate-id error");
        let spans = &parsed.annotations.spans;
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].id, "s1");
        assert_eq!(spans[0].anchor.text, "eXraser");
        assert_eq!(spans[1].id, "s2", "the clashing id is renamed");
        assert_eq!(spans[1].anchor.text, "eraser");
        let body_len = utf16_len(&parsed.body);
        assert!(spans[1].anchor.start >= body_len, "moved past the body");

        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&saved);
        assert_eq!(opened.unresolved, 1);
        assert_eq!(live(&reopened), vec![("s1".to_string(), 11, 0)]);
        assert_eq!(reopened.set_aside().spans[0].alts.len(), 2);
        assert_eq!(reopened.save(), saved);
    }

    #[test]
    fn unplaceable_ghosts_are_saved_too() {
        let source = annotated_source().replacen("Drop this sentence.", "Keep it.", 1);
        let mut session = DocumentSession::new();
        let opened = session.open(&source);
        assert_eq!(opened.unresolved, 1);
        assert_eq!(session.set_aside().ghosts.len(), 1);
        assert_eq!(session.export(), "Pass me an eraser. Keep it.");
        let parsed = parse(&session.save()).unwrap();
        assert_eq!(parsed.annotations.ghosts.len(), 1);
        assert_eq!(
            parsed.annotations.ghosts[0].anchor.text,
            " Drop this sentence."
        );
        assert_eq!(parsed.annotations.overflow, "stashed");
    }

    #[test]
    fn open_kind_and_notices_follow_the_set_aside_count() {
        let mut session = DocumentSession::new();
        assert_eq!(session.open(&annotated_source()).kind, None);
        assert_eq!(session.set_aside_notice(), None);
        let malformed = "x\n\n```terraphim-alternatives\n{\n```\n";
        assert_eq!(session.open(malformed).kind, Some("malformed"));

        // Same length, so the stored offsets still point at the fixed text.
        let source = annotated_source().replacen("an eraser", "an erasXr", 1);
        let opened = session.open(&source);
        assert_eq!(opened.kind, Some("set-aside"));
        let notice = session.set_aside_notice().expect("one item set aside");
        assert!(notice.starts_with("1 annotation"), "{notice}");
        let outcome = session.apply_edit(15, 1, "e").unwrap();
        assert_eq!(outcome.reattached, vec!["s1".to_string()]);
        assert_eq!(outcome.notice, None);

        let json = sync_json(&mut session, "Pass me an eraXer. Drop this sentence.");
        assert_eq!(json["changed"], true);
        assert_eq!(json["unresolved"], 1);
        assert!(json["notice"].as_str().unwrap().contains("preserved"));
        let json = sync_json(&mut session, "Pass me an eraser. Drop this sentence.");
        assert_eq!(json["unresolved"], 0);
        assert!(json["notice"].is_null());
    }

    #[test]
    fn out_of_range_edit_is_refused_without_change() {
        let mut session = DocumentSession::new();
        session.open("abc");
        assert!(session.apply_edit(2, 5, "x").is_err());
        assert!(session.apply_edit(usize::MAX, 1, "x").is_err());
        assert_eq!(session.body(), "abc");
    }

    /// `(id, start, end, text)` of every live ghost.
    fn ghosts(session: &DocumentSession) -> Vec<(String, usize, usize, String)> {
        session
            .document()
            .annotations
            .ghosts
            .iter()
            .map(|g| {
                (
                    g.id.clone(),
                    g.anchor.start,
                    g.anchor.end,
                    g.anchor.text.clone(),
                )
            })
            .collect()
    }

    fn ghost(id: &str, start: usize, end: usize, text: &str) -> (String, usize, usize, String) {
        (id.to_string(), start, end, text.to_string())
    }

    const PLAIN: &str = "One two three. Four five six. Seven eight.";

    #[test]
    fn ghost_range_returns_id_and_annotations() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        let json = ghost_json(&mut session, 15, 29);
        assert_eq!(json["ok"], true);
        assert_eq!(json["id"], "g1");
        assert_eq!(
            json["annotations"]["ghosts"][0]["anchor"]["text"],
            "Four five six."
        );
        assert_eq!(json["annotations"]["ghosts"][0]["anchor"]["start"], 15);
        assert_eq!(json["annotations"], session.annotations_json());
        // Ghosted text stays in the body and the counts (decision 3) but not
        // in the export (R-9.3).
        assert_eq!(session.body(), PLAIN);
        assert_eq!(session.counts().words, 8);
        assert_eq!(session.export(), "One two three. Seven eight.");
    }

    #[test]
    fn overlapping_and_touching_ghosts_merge() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        assert_eq!(session.ghost(4, 7).unwrap(), "g1");
        assert_eq!(session.ghost(30, 35).unwrap(), "g2");
        // Touching g1 at its end: merged, keeping the first id.
        assert_eq!(session.ghost(7, 13).unwrap(), "g1");
        assert_eq!(
            ghosts(&session),
            vec![
                ghost("g1", 4, 13, "two three"),
                ghost("g2", 30, 35, "Seven")
            ]
        );
        // Overlapping both: one ghost spanning them, the first id kept.
        assert_eq!(session.ghost(10, 32).unwrap(), "g1");
        assert_eq!(
            ghosts(&session),
            vec![ghost("g1", 4, 35, "two three. Four five six. Seven")]
        );
    }

    #[test]
    fn ghost_may_cover_and_partly_overlap_spans() {
        let mut session = DocumentSession::new();
        session.open(&annotated_source());
        assert!(session.revive(0, 38).unwrap());
        let before = session.document().annotations.spans.clone();
        // A sentence containing the word with alternatives (11..17)...
        session.ghost(0, 18).unwrap();
        assert_eq!(
            ghosts(&session),
            vec![ghost("g2", 0, 18, "Pass me an eraser.")]
        );
        // ...and a ghost cutting the span in half are both allowed.
        session.revive(0, 18).unwrap();
        session.ghost(13, 24).unwrap();
        assert_eq!(
            session.document().annotations.spans,
            before,
            "spans untouched"
        );
        assert_eq!(ghosts(&session)[0].3, "aser. Drop ");
        // Export keeps the active alternative and drops the ghosted text.
        assert_eq!(session.export(), "Pass me an erthis sentence.");
    }

    #[test]
    fn partial_revive_trims_or_splits() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        session.ghost(0, 29).unwrap();
        // Strictly inside: split, the left part keeps the id.
        let json = revive_json(&mut session, 4, 8);
        assert_eq!(json["ok"], true);
        assert_eq!(json["changed"], true);
        assert_eq!(
            ghosts(&session),
            vec![
                ghost("g1", 0, 4, "One "),
                ghost("g2", 8, 29, "three. Four five six."),
            ]
        );
        assert_eq!(json["annotations"]["ghosts"].as_array().unwrap().len(), 2);
        // Across the end of a ghost: trimmed.
        session.revive(20, 40).unwrap();
        assert_eq!(ghosts(&session)[1], ghost("g2", 8, 20, "three. Four "));
        // Covering a ghost: removed.
        assert!(session.revive(0, 4).unwrap());
        assert_eq!(ghosts(&session).len(), 1);
        // Nothing ghosted there: no change, still ok.
        let json = revive_json(&mut session, 30, 35);
        assert_eq!(json["ok"], true);
        assert_eq!(json["changed"], false);
    }

    #[test]
    fn invalid_ranges_return_an_error_object_and_change_nothing() {
        let mut session = DocumentSession::new();
        session.open("Hi 😀 there");
        session.ghost(0, 2).unwrap();
        let before = session.clone();
        for (start, end) in [(5, 3), (4, 4), (0, 99), (4, 5)] {
            for json in [
                ghost_json(&mut session, start, end),
                revive_json(&mut session, start, end),
            ] {
                assert_eq!(json["ok"], false, "{start}..{end}");
                assert_eq!(json["kind"], "invalid-range");
                assert!(json["error"].as_str().unwrap().contains("range"));
                assert!(json.get("annotations").is_none());
            }
        }
        // An empty range on a character boundary is refused too.
        assert_eq!(ghost_json(&mut session, 2, 2)["ok"], false);
        assert_eq!(session, before);
        // A whole surrogate pair is fine (UTF-16 offsets).
        assert_eq!(ghost_json(&mut session, 3, 5)["ok"], true);
        assert_eq!(ghosts(&session)[1].3, "😀");
    }

    #[test]
    fn ghosts_follow_edits_and_survive_save_and_reopen() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        session.ghost(15, 29).unwrap();
        // Typing before the ghost moves it; typing inside resizes it.
        session.apply_edit(0, 0, "Oh. ").unwrap();
        session.apply_edit(29, 0, "big ").unwrap();
        assert_eq!(
            ghosts(&session),
            vec![ghost("g1", 19, 37, "Four five big six.")]
        );
        let saved = session.save();
        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&saved);
        assert_eq!(opened.unresolved, 0);
        assert_eq!(ghosts(&reopened), ghosts(&session));
        assert_eq!(reopened.export(), "Oh. One two three. Seven eight.");
        // Reviving after reopening saves plain Markdown again.
        assert!(reopened.revive(19, 37).unwrap());
        assert_eq!(
            reopened.save(),
            "Oh. One two three. Four five big six. Seven eight."
        );
    }

    #[test]
    fn deleting_ghosted_text_sets_the_ghost_aside_and_undo_restores_it() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        session.ghost(15, 29).unwrap();
        let before = ghosts(&session);
        // Delete exactly the ghosted text: the live ghost goes (R-5.3) ...
        let outcome = session.apply_edit(15, 14, "").unwrap();
        assert_eq!(outcome.detached_ghosts, vec!["g1".to_string()]);
        assert_eq!(outcome.set_aside, 1);
        assert_eq!(outcome.to_json()["detachedGhosts"][0], "g1");
        assert!(ghosts(&session).is_empty());
        // ... but it is kept, and saved, with its pre-edit anchor.
        assert_eq!(session.set_aside().ghosts[0].anchor.text, "Four five six.");
        let saved = session.save();
        assert_eq!(parse(&saved).unwrap().annotations.ghosts.len(), 1);
        // Undo (the inverse edit) re-attaches it with the same extent and id.
        let outcome = session.apply_edit(15, 0, "Four five six.").unwrap();
        assert_eq!(outcome.reattached, vec!["g1".to_string()]);
        assert_eq!(outcome.set_aside, 0);
        assert_eq!(ghosts(&session), before);

        // Reopening the file saved in between sets it aside again (its text
        // is gone) without losing it; putting the text back restores it.
        let mut reopened = DocumentSession::new();
        assert_eq!(reopened.open(&saved).unresolved, 1);
        assert_eq!(reopened.save(), saved);
        let outcome = reopened.apply_edit(15, 0, "Four five six.").unwrap();
        assert_eq!(outcome.reattached, vec!["g1".to_string()]);
        assert_eq!(ghosts(&reopened), before);
    }

    #[test]
    fn partial_deletion_trims_and_a_wider_replacement_sets_the_ghost_aside() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        session.ghost(15, 29).unwrap();
        // Partial deletion: trimmed, nothing set aside.
        let outcome = session.apply_edit(15, 5, "").unwrap();
        assert!(outcome.detached_ghosts.is_empty());
        assert_eq!(outcome.set_aside, 0);
        assert_eq!(ghosts(&session), vec![ghost("g1", 15, 24, "five six.")]);
        // Typing over exactly the ghost's text is elastic (R-5.3): the ghost
        // now covers the new text, nothing is set aside, and undoing the
        // replacement resizes it back.
        let outcome = session.apply_edit(15, 9, "x").unwrap();
        assert!(outcome.detached_ghosts.is_empty());
        assert_eq!(ghosts(&session), vec![ghost("g1", 15, 16, "x")]);
        session.apply_edit(15, 1, "five six.").unwrap();
        assert_eq!(ghosts(&session), vec![ghost("g1", 15, 24, "five six.")]);
        // A replacement that also covers text outside the ghost removes it:
        // set aside, and undoing the replacement restores it.
        let outcome = session.apply_edit(14, 11, "x").unwrap();
        assert_eq!(outcome.detached_ghosts, vec!["g1".to_string()]);
        assert_eq!(outcome.set_aside, 1);
        session.apply_edit(14, 1, " five six.").unwrap();
        assert_eq!(ghosts(&session), vec![ghost("g1", 15, 24, "five six.")]);
        assert!(session.set_aside().is_empty());
    }

    #[test]
    fn a_restored_ghost_merges_with_touching_ghosts() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        session.ghost(15, 20).unwrap(); // "Four "
        session.apply_edit(15, 5, "").unwrap();
        assert_eq!(session.set_aside().ghosts.len(), 1);
        // Meanwhile a new ghost right where the text will come back.
        session.ghost(15, 19).unwrap(); // "five"
                                        // The text returns at 15..20, touching the live ghost: one merged
                                        // ghost, never two overlapping or touching ones.
        let outcome = session.apply_edit(15, 0, "Four ").unwrap();
        assert_eq!(outcome.set_aside, 0);
        assert_eq!(outcome.reattached.len(), 1);
        assert_eq!(ghosts(&session).len(), 1);
        let g = &ghosts(&session)[0];
        assert_eq!((g.1, g.2, g.3.as_str()), (15, 24, "Four five"));
    }

    // ----- #10 alternatives side panel --------------------------------------

    const PANEL: &str = "Pass me a paperclip. Drop this.";

    /// Applies a panel edit to `text`, checking the deleted text matches.
    fn apply_alt_edit(text: &str, edit: &AltEdit) -> String {
        let start = utf16_to_byte(text, edit.start).unwrap();
        let end = start + edit.deleted_text.len();
        assert_eq!(&text[start..end], edit.deleted_text);
        format!("{}{}{}", &text[..start], edit.inserted_text, &text[end..])
    }

    /// Undoes `change` the way the editor does: restore `before` with the
    /// inverse edit.
    fn undo_alt(session: &mut DocumentSession, change: &AltChange) {
        let (start, deleted, inserted) = match &change.edit {
            Some(e) => (
                e.start,
                utf16_len(&e.inserted_text),
                e.deleted_text.as_str(),
            ),
            None => (0, 0, ""),
        };
        session
            .alt_restore(
                &change.span,
                change.before.clone(),
                start,
                deleted,
                inserted,
            )
            .unwrap();
    }

    /// Redoes `change`: restore `after` with the forward edit.
    fn redo_alt(session: &mut DocumentSession, change: &AltChange) {
        let (start, deleted, inserted) = match &change.edit {
            Some(e) => (
                e.start,
                utf16_len(&e.deleted_text),
                e.inserted_text.as_str(),
            ),
            None => (0, 0, ""),
        };
        session
            .alt_restore(&change.span, change.after.clone(), start, deleted, inserted)
            .unwrap();
    }

    /// Checks undo then redo of `change` lands exactly on the states around it.
    fn round_trip(session: &mut DocumentSession, change: &AltChange, before: &DocumentSession) {
        let after = session.clone();
        undo_alt(session, change);
        assert_eq!(session.document(), before.document(), "undo");
        redo_alt(session, change);
        assert_eq!(session.document(), after.document(), "redo");
    }

    #[test]
    fn body_edit_is_minimal_and_char_safe() {
        assert_eq!(body_edit("abc", "abc"), None);
        let e = body_edit("Pass me a paperclip.", "Pass me an eraser.").unwrap();
        assert_eq!(
            (e.start, e.deleted_text.as_str(), e.inserted_text.as_str()),
            (9, " paperclip", "n eraser")
        );
        // Non-BMP text before the edit: the start is in UTF-16 units.
        let e = body_edit("\u{1F600} a b", "\u{1F600} a c").unwrap();
        assert_eq!(
            (e.start, e.deleted_text.as_str(), e.inserted_text.as_str()),
            (5, "b", "c")
        );
        // A shared byte inside different characters is not split.
        let e = body_edit("\u{e9}", "\u{e8}").unwrap();
        assert_eq!(
            (e.deleted_text.as_str(), e.inserted_text.as_str()),
            ("\u{e9}", "\u{e8}")
        );
    }

    #[test]
    fn create_adds_span_and_first_alternative_as_one_operation() {
        let mut session = DocumentSession::new();
        session.open(PANEL);
        let before = session.clone();
        let change = session
            .alt_create_span(SpanKind::Word, 10, 19, "eraser", Source::Human)
            .unwrap();
        assert_eq!((change.span.as_str(), change.index), ("s1", Some(1)));
        assert_eq!(change.before, None);
        assert_eq!(change.edit, None, "the new alternative is not made active");
        let after = change.after.clone().unwrap();
        assert_eq!(after.alts.len(), 2);
        assert_eq!(after.alts[1].source, Source::Human);
        assert_eq!(session.body(), PANEL);
        round_trip(&mut session, &change, &before);

        // Refusals change nothing and never leave an inert span.
        let snapshot = session.clone();
        assert!(session
            .alt_create_span(SpanKind::Word, 0, 4, "Pass", Source::Human)
            .is_err());
        assert!(session
            .alt_create_span(SpanKind::Word, 0, 4, "", Source::Human)
            .is_err());
        assert!(session
            .alt_create_span(SpanKind::Word, 12, 14, "x", Source::Human)
            .is_err());
        assert_eq!(session, snapshot);
        assert!(parse_span_kind("clause").is_err());
        assert!(parse_alt_source("original").is_err());
    }

    #[test]
    fn new_span_reserves_set_aside_and_undone_identities() {
        let mut session = DocumentSession::new();
        session.open("One two three.");
        let old = session
            .alt_create_span(SpanKind::Word, 4, 7, "pair", Source::Human)
            .unwrap();
        session.sync_body("Red green blue.");
        assert_eq!(session.set_aside.spans[0].id, old.span);
        let new = session
            .alt_create_span(SpanKind::Word, 10, 14, "pair", Source::Human)
            .unwrap();
        assert_ne!(old.span, new.span);
        let before = session.clone();
        let add = session
            .alt_add(&new.span, "quill", Source::Human)
            .unwrap()
            .unwrap();
        round_trip(&mut session, &add, &before);
        undo_alt(&mut session, &add);
        assert_eq!(session.live_span(&new.span).unwrap().alts.len(), 2);
        assert_eq!(session.set_aside.spans[0].id, old.span);
        session.validate_history_annotations().unwrap();
        undo_alt(&mut session, &new);
        let replacement = session
            .alt_create_span(SpanKind::Word, 10, 14, "pen", Source::Human)
            .unwrap();
        assert_ne!(replacement.span, new.span);
    }

    #[test]
    fn kg_span_reserves_detached_identity_and_preserves_provenance() {
        let mut session = DocumentSession::new();
        session.open("One two three.");
        let old = session
            .alt_create_span(SpanKind::Word, 4, 7, "pair", Source::Human)
            .unwrap();
        session.sync_body("Red green blue.");
        let before = session.clone();
        let new = session
            .append_alternatives(
                SpanKind::Word,
                10,
                14,
                &["quill".into()],
                Source::Ai,
                Some("local-kg"),
            )
            .unwrap()
            .unwrap();
        assert_ne!(new.span, old.span);
        let span = session.live_span(&new.span).unwrap();
        assert_eq!(span.alts[1].source, Source::Ai);
        assert_eq!(span.alts[1].model.as_deref(), Some("local-kg"));
        round_trip(&mut session, &new, &before);
        assert_eq!(session.set_aside.spans[0].id, old.span);
        session.validate_history_annotations().unwrap();
    }

    #[test]
    fn exhausted_span_identity_refuses_atomically() {
        let mut session = DocumentSession::new();
        session.open("word");
        session.reserved_ids.insert(format!("s{}", u64::MAX));
        let before = session.clone();
        assert!(session
            .alt_create_span(SpanKind::Word, 0, 4, "alias", Source::Human)
            .is_err());
        assert_eq!(session, before);
    }

    #[test]
    fn add_edit_move_and_remove_round_trip_through_restore() {
        let mut session = DocumentSession::new();
        session.open(PANEL);
        session
            .alt_create_span(SpanKind::Word, 10, 19, "eraser", Source::Human)
            .unwrap();

        // Add (AI provenance kept), and a duplicate is a no-op.
        let before = session.clone();
        let add = session
            .alt_add("s1", "thumbtack", Source::Ai)
            .unwrap()
            .unwrap();
        assert_eq!(add.index, Some(2));
        assert_eq!(add.after.as_ref().unwrap().alts[2].source, Source::Ai);
        round_trip(&mut session, &add, &before);
        assert_eq!(
            session.alt_add("s1", "eraser", Source::Human).unwrap(),
            None
        );

        // Edit an inactive line: annotations only.
        let before = session.clone();
        let edit = session.alt_edit("s1", 1, "pencil").unwrap().unwrap();
        assert_eq!(edit.edit, None);
        assert_eq!(
            session.document().annotations.spans[0].alts[1].text,
            "pencil"
        );
        round_trip(&mut session, &edit, &before);
        assert_eq!(session.alt_edit("s1", 1, "pencil").unwrap(), None);
        assert!(session.alt_edit("s1", 1, "thumbtack").is_err(), "duplicate");
        assert!(session.alt_edit("s1", 0, "x").is_err(), "original");
        assert!(session.alt_edit("s1", 1, "").is_err(), "empty");

        // Make "pencil" active, then edit the active line: the body and the
        // article follow (a pencil -> an eraser).
        // (Making a line active is issue #9's swap, `DocumentSession::set_active`.)
        session.set_active("s1", 1).unwrap();
        assert_eq!(session.body(), "Pass me a pencil. Drop this.");
        let before = session.clone();
        let text = session.body().to_string();
        let edit = session.alt_edit("s1", 1, "eraser").unwrap().unwrap();
        assert_eq!(session.body(), "Pass me an eraser. Drop this.");
        assert_eq!(
            apply_alt_edit(&text, edit.edit.as_ref().unwrap()),
            session.body()
        );
        round_trip(&mut session, &edit, &before);

        // Move: the active alternative stays active by text.
        let before = session.clone();
        let moved = session.alt_move("s1", 1, 2).unwrap().unwrap();
        let span = moved.after.as_ref().unwrap();
        assert_eq!(span.alts[2].text, "eraser");
        assert_eq!(span.active, 2);
        assert_eq!(moved.edit, None);
        round_trip(&mut session, &moved, &before);
        assert_eq!(session.alt_move("s1", 2, 2).unwrap(), None);
        assert!(session.alt_move("s1", 0, 1).is_err());
        assert!(session.alt_move("s1", 1, 9).is_err());

        // Remove the active line: the original comes back with its article.
        let before = session.clone();
        let index = session.document().annotations.spans[0].active;
        let removed = session.alt_remove("s1", index).unwrap();
        assert_eq!(session.body(), PANEL);
        assert!(removed.after.is_some());
        round_trip(&mut session, &removed, &before);
    }

    #[test]
    fn removing_the_last_alternative_removes_the_span_and_undo_brings_it_back() {
        let mut session = DocumentSession::new();
        session.open(PANEL);
        session
            .alt_create_span(SpanKind::Word, 10, 19, "eraser", Source::Ai)
            .unwrap();
        session.ghost(20, 31).unwrap();
        let before = session.clone();
        let removed = session.alt_remove("s1", 1).unwrap();
        assert_eq!(removed.after, None, "R-2.7: nothing left to keep");
        assert!(session.document().annotations.spans.is_empty());
        assert_eq!(session.document().annotations.ghosts.len(), 1);
        round_trip(&mut session, &removed, &before);
        // Saved and reopened, the restored span is intact.
        let mut reopened = DocumentSession::new();
        assert_eq!(reopened.open(&session.save()).unresolved, 0);
        assert_eq!(reopened.document(), session.document());
    }

    #[test]
    fn restore_moves_other_anchors_and_refuses_what_does_not_fit() {
        let mut session = DocumentSession::new();
        session.open("Pass me a paperclip and a pen. Drop this.");
        session
            .alt_create_span(SpanKind::Word, 10, 19, "eraser", Source::Human)
            .unwrap();
        session
            .alt_create_span(SpanKind::Word, 26, 29, "quill", Source::Human)
            .unwrap();
        session.ghost(31, 41).unwrap();
        session.set_active("s1", 1).unwrap();
        assert_eq!(session.body(), "Pass me an eraser and a pen. Drop this.");
        // Editing the active line rewrites the body (an eraser -> a pencil);
        // the later span and ghost move with it, and back on undo.
        let before = session.clone();
        let swap = session.alt_edit("s1", 1, "pencil").unwrap().unwrap();
        assert_eq!(session.body(), "Pass me a pencil and a pen. Drop this.");
        let s2 = session.document().span("s2").unwrap().anchor.clone();
        assert_eq!((s2.start, s2.end), (23, 26));
        let g1 = session.document().annotations.ghosts[0].anchor.clone();
        assert_eq!((g1.start, g1.end), (28, 38));
        round_trip(&mut session, &swap, &before);

        // Replays that do not fit change nothing.
        let snapshot = session.clone();
        assert!(
            session
                .alt_restore("s1", swap.before.clone(), 0, 0, "")
                .is_err(),
            "stale: the snapshot's text is not in the body"
        );
        assert_eq!(session, snapshot);
        assert!(
            session
                .alt_restore("s2", before.document().span("s1").cloned(), 0, 0, "")
                .is_err(),
            "snapshot of another span"
        );
        assert_eq!(session, snapshot);
        assert!(session.alt_restore("s1", None, 999, 1, "").is_err());
        assert_eq!(session, snapshot);
    }

    // ----- overflow panel (issue #12) ---------------------------------------

    #[test]
    fn redo_fallback_appends_a_separate_entry() {
        // Stash onto an overflow ending with a blank line: the recorded chunk
        // has no separator of its own.
        let from = "old\n\n";
        let to = append_overflow(from, "chunk");
        assert_eq!(to, "old\n\nchunk");
        // After undo the author rewrote the panel, so redo falls back to
        // appending: it must still be a separate entry.
        let redo = rebase_overflow("edited", from, &to);
        assert!(redo.applied);
        assert_eq!(redo.text, "edited\n\nchunk");
        let redo = rebase_overflow("edited\n", from, &to);
        assert_eq!(redo.text, "edited\n\nchunk");
        // A separator recorded with the chunk is replaced, not doubled.
        let to = append_overflow("note", "chunk");
        assert_eq!(rebase_overflow("other", "note", &to).text, "other\n\nchunk");
        assert_eq!(rebase_overflow("", "note", &to).text, "chunk");
        // A stashed entry that itself starts with a newline keeps it.
        let to = append_overflow("note", "\nchunk");
        assert_eq!(
            rebase_overflow("other", "note", &to).text,
            "other\n\n\nchunk"
        );
    }

    #[test]
    fn append_overflow_separates_entries_with_a_blank_line() {
        assert_eq!(append_overflow("", "a"), "a");
        assert_eq!(append_overflow("x", "a"), "x\n\na");
        assert_eq!(append_overflow("x\n", "a"), "x\n\na");
        assert_eq!(append_overflow("x\n\n", "a"), "x\n\na");
    }

    #[test]
    fn stash_moves_the_text_into_overflow_atomically() {
        let mut session = DocumentSession::new();
        session.open(PLAIN);
        session.set_overflow("Earlier note").unwrap();
        let stash = session.stash(15, 30).unwrap(); // "Four five six. "
        assert_eq!(stash.text, "Four five six. ");
        assert_eq!(stash.start, 15);
        assert_eq!(stash.overflow_before, "Earlier note");
        assert_eq!(stash.overflow_after, "Earlier note\n\nFour five six. ");
        assert_eq!(session.body(), "One two three. Seven eight.");
        assert_eq!(session.overflow(), "Earlier note\n\nFour five six. ");
        let json = stash.to_json();
        assert_eq!(json["edit"]["start"], 15);
        assert_eq!(json["edit"]["deletedLength"], 15);
        assert_eq!(json["edit"]["deletedText"], "Four five six. ");
        assert_eq!(json["edit"]["insertedText"], "");
        assert_eq!(json["overflow"]["before"], "Earlier note");
        assert_eq!(json["overflow"]["after"], "Earlier note\n\nFour five six. ");

        // Refusals change neither the body nor the overflow.
        let before = session.clone();
        assert_eq!(session.stash(3, 3).unwrap_err().kind(), "empty");
        assert_eq!(session.stash(5, 3).unwrap_err().kind(), "invalid-range");
        assert_eq!(session.stash(0, 999).unwrap_err().kind(), "invalid-range");
        assert_eq!(session, before);
    }

    #[test]
    fn stash_offsets_are_utf16_and_never_split_a_character() {
        let mut session = DocumentSession::new();
        session.open("a\u{1F600}b c");
        let before = session.clone();
        // Offset 2 is inside the surrogate pair.
        assert_eq!(session.stash(0, 2).unwrap_err().kind(), "invalid-range");
        assert_eq!(session, before);
        let stash = session.stash(1, 3).unwrap();
        assert_eq!(stash.text, "\u{1F600}");
        assert_eq!(session.body(), "ab c");
    }

    #[test]
    fn stash_sets_annotations_aside_and_undo_brings_them_back() {
        let mut session = DocumentSession::new();
        session.open(&cycling_source());
        // "Pass me a paperclip. Drop this." with s1 over "paperclip" (10..19)
        // and a ghost over " Drop this." (20..31). Stash "a paperclip. Drop
        // this.": the span is detached, the ghost set aside.
        let stash = session.stash(8, 31).unwrap();
        assert_eq!(stash.outcome.detached, vec!["s1".to_string()]);
        assert_eq!(stash.outcome.detached_ghosts.len(), 1);
        assert!(stash.outcome.warning.is_some());
        assert!(session.document().annotations.spans.is_empty());
        assert!(session.document().annotations.ghosts.is_empty());
        assert_eq!(session.overflow(), "a paperclip. Drop this.");
        // Saved meanwhile: the set-aside annotations are kept.
        assert!(session.save().contains("thumbtack"));

        // Undo, as the editor replays it: the text comes back through the
        // normal edit path, then the overflow change is replayed backwards.
        session.apply_edit(8, 0, &stash.text).unwrap();
        let undone = session
            .replay_overflow(&stash.overflow_after, &stash.overflow_before)
            .unwrap();
        assert_eq!((undone.text.as_str(), undone.applied), ("", true));
        assert_eq!(session.body(), "Pass me a paperclip. Drop this.");
        assert!(session.set_aside().is_empty());
        assert_eq!(session.document().annotations.spans.len(), 1);
        assert_eq!(session.document().annotations.ghosts.len(), 1);
        assert_eq!(session.save(), cycling_source());

        // Redo.
        session.apply_edit(8, 23, "").unwrap();
        session
            .replay_overflow(&stash.overflow_before, &stash.overflow_after)
            .unwrap();
        assert_eq!(session.overflow(), "a paperclip. Drop this.");
        assert_eq!(session.body(), "Pass me ");
    }

    #[test]
    fn replaying_a_stash_keeps_text_typed_in_the_panel() {
        let (before, after) = ("note", "note\n\nchunk");
        let undo = |current: &str| rebase_overflow(current, after, before);
        let redo = |current: &str| rebase_overflow(current, before, after);
        let applied = |text: &str| Rebased {
            text: text.into(),
            applied: true,
        };
        let kept = |text: &str| Rebased {
            text: text.into(),
            applied: false,
        };
        // Unchanged since the stash: exact.
        assert_eq!(undo(after), applied("note"));
        assert_eq!(redo(before), applied(after));
        // Typed after the chunk: the chunk is removed at its position.
        assert_eq!(undo("note\n\nchunk more"), applied("note more"));
        // The same chunk typed again after it: the stashed copy (at the
        // recorded position) goes, the author's copy stays.
        assert_eq!(undo("note\n\nchunk\n\nchunk"), applied("note\n\nchunk"));
        // Text typed before the chunk moves it off its recorded position:
        // nothing is removed, and the caller is told.
        assert_eq!(undo("edited note\n\nchunk"), kept("edited note\n\nchunk"));
        // The chunk edited or deleted by hand: nothing is removed.
        assert_eq!(undo("note\n\nchUnk"), kept("note\n\nchUnk"));
        assert_eq!(undo("note"), kept("note"));
        // A copy elsewhere is never taken for the stashed one.
        assert_eq!(undo("x\n\nchunk"), kept("x\n\nchunk"));
        // Redo: at the recorded position when the text before it is intact,
        // otherwise at the end.
        assert_eq!(redo("note, more"), applied("note\n\nchunk, more"));
        assert_eq!(redo("edited"), applied("edited\n\nchunk"));
        // An unrelated change is never forced over the current text.
        assert_eq!(rebase_overflow("mine", "x", "y"), kept("mine"));
    }

    // ----- pull back (R-6.4): a copy, separated as terraphim_lsp_core does --

    /// The body after copying `overflow`'s `from..to` (UTF-16) into `body` at
    /// `at`, as the editor does it: the computed text inserted through the
    /// normal edit path. The overflow never changes.
    fn pulled_with(body: &str, overflow: &str, at: usize, from: usize, to: usize) -> String {
        let mut session = DocumentSession::new();
        session.open(body);
        session.set_overflow(overflow).unwrap();
        let pull = session.pull_back_text(at, from, to).unwrap();
        assert_eq!(session.body(), body, "computing the text changes nothing");
        session.apply_edit(pull.start, 0, &pull.text).unwrap();
        assert_eq!(session.overflow(), overflow, "a copy");
        session.body().to_string()
    }

    /// The body after pulling the overflow paragraph "Para." into `body` at
    /// UTF-16 offset `at` (terraphim_lsp_core's `pulled` helper).
    fn pulled(body: &str, at: usize) -> String {
        pulled_with(body, "Para.", at, 0, 5)
    }

    // The expected bodies below are terraphim_lsp_core's own (terraphim-ai
    // #3472, tests/overflow_export.rs, the `pulled` helper and pull_back_*
    // tests), so the editor and terraphim-lsp separate a pulled paragraph
    // byte for byte the same.

    #[test]
    fn pull_back_mid_line_inserts_inline() {
        assert_eq!(pulled("Start end.", 5), "StartPara. end.");
        assert_eq!(pulled("Start end.", 6), "Start Para.end.");
    }

    #[test]
    fn pull_back_at_a_line_start_makes_its_own_paragraph() {
        assert_eq!(
            pulled("First line.\nSecond line.", 12),
            "First line.\n\nPara.\n\nSecond line."
        );
        assert_eq!(pulled("Body.", 0), "Para.\n\nBody.");
    }

    #[test]
    fn pull_back_at_a_line_end_makes_its_own_paragraph() {
        assert_eq!(pulled("Body.", 5), "Body.\n\nPara.");
        assert_eq!(pulled("A line.\nNext.", 7), "A line.\n\nPara.\n\nNext.");
        // The defect this fixes: a caret at the end of a line glued the
        // pulled text to the next line ("stuff.Pass me").
        assert_eq!(
            pulled("Some stuff.\nPass me a paperclip.", 11),
            "Some stuff.\n\nPara.\n\nPass me a paperclip."
        );
    }

    #[test]
    fn pull_back_between_paragraphs_keeps_one_blank_line_each_side() {
        let expected = "One.\n\nPara.\n\nTwo.";
        assert_eq!(
            pulled("One.\n\nTwo.", 4),
            expected,
            "end of the first paragraph"
        );
        assert_eq!(pulled("One.\n\nTwo.", 5), expected, "on the blank line");
        assert_eq!(pulled("One.\n\nTwo.", 6), expected, "start of the second");
    }

    #[test]
    fn pull_back_into_an_empty_body_is_the_paragraph() {
        assert_eq!(pulled("", 0), "Para.");
    }

    #[test]
    fn pull_back_follows_crlf_line_endings_and_never_splits_a_pair() {
        let expected = "A.\r\n\r\nPara.\r\n\r\nB.";
        assert_eq!(pulled("A.\r\nB.", 4), expected);
        assert_eq!(pulled("A.\r\nB.", 3), expected, "between the CR and the LF");
        assert_eq!(pulled("A.\r\n\r\nB.", 2), expected);
    }

    #[test]
    fn pull_back_of_a_multi_line_paragraph_keeps_its_lines() {
        assert_eq!(
            pulled_with("Head.\nTail.", "Line one\nline two", 6, 0, 17),
            "Head.\n\nLine one\nline two\n\nTail."
        );
    }

    #[test]
    fn pull_back_takes_the_selection_or_the_current_line() {
        // A selection inside a paragraph: just that text.
        assert_eq!(
            pulled_with("Start end.", "Keep this, move this.", 5, 11, 21),
            "Startmove this. end."
        );
        // Nothing selected: the line at the overflow caret.
        assert_eq!(
            pulled_with("Body.", "One.\n\nTwo.", 5, 8, 8),
            "Body.\n\nTwo."
        );
        // A selection's own line breaks at either end do not go in.
        assert_eq!(
            pulled_with("Body.", "One.\n\nTwo.", 5, 4, 10),
            "Body.\n\nTwo."
        );
    }

    #[test]
    fn pull_back_shifts_later_spans_past_the_separators() {
        // terraphim_lsp_core's pull_back_shifts_later_spans_past_the_separators.
        let mut doc = Document::new("Alpha beta gamma delta.\nSecond line here.");
        let id = doc.add_span(SpanKind::Word, 6, 10).unwrap();
        doc.add_alternative(&id, "bravo", Source::Human, None)
            .unwrap();
        let id = doc.add_span(SpanKind::Word, 31, 35).unwrap();
        doc.add_alternative(&id, "row", Source::Human, None)
            .unwrap();
        let mut session = DocumentSession::new();
        session.open(&write(&doc));
        session.set_overflow("Para.").unwrap();
        let pull = session.pull_back_text(24, 0, 5).unwrap();
        session.apply_edit(pull.start, 0, &pull.text).unwrap();
        assert_eq!(
            session.body(),
            "Alpha beta gamma delta.\n\nPara.\n\nSecond line here."
        );
        // "beta" is before the insertion; "line" moves past the eight units
        // inserted ("\nPara.\n\n").
        let spans = &session.document().annotations.spans;
        assert_eq!(
            (spans[0].anchor.start, spans[0].anchor.text.as_str()),
            (6, "beta")
        );
        assert_eq!(
            (spans[1].anchor.start, spans[1].anchor.text.as_str()),
            (39, "line")
        );
    }

    #[test]
    fn pull_back_offsets_are_utf16() {
        // "Café 🎉." is 8 UTF-16 units; the overflow holds an astral character.
        assert_eq!(
            pulled_with("Café 🎉.", "Ünïcode 🎉 note", 8, 0, 15),
            "Café 🎉.\n\nÜnïcode 🎉 note"
        );
        let mut session = DocumentSession::new();
        session.open("🎉 x");
        session.set_overflow("Para.").unwrap();
        assert_eq!(session.pull_back_text(2, 0, 5).unwrap().start, 2);
        // An offset inside a surrogate pair is refused.
        assert!(session.pull_back_text(1, 0, 5).is_err());
    }

    #[test]
    fn pull_back_refusals_change_nothing() {
        let mut session = DocumentSession::new();
        session.open("Body.");
        session.set_overflow("One.\n\n\nTwo.").unwrap();
        // Nothing to pull: a blank line, or an empty overflow.
        assert_eq!(
            session.pull_back_text(0, 5, 5).unwrap_err(),
            OverflowError::NothingToPull
        );
        // Ranges past the end of the overflow or the body.
        assert!(session.pull_back_text(0, 0, 99).is_err());
        assert!(session.pull_back_text(9, 0, 4).is_err());
        session.set_overflow("").unwrap();
        assert_eq!(
            session.pull_back_text(0, 0, 0).unwrap_err(),
            OverflowError::NothingToPull
        );
        assert_eq!(session.body(), "Body.");
    }

    #[test]
    fn overflow_round_trips_through_save_and_is_omitted_from_export() {
        let mut session = DocumentSession::new();
        session.open("Body text.\n");
        session.set_overflow("Idea one\n```\ncode\n```\n").unwrap();
        let saved = session.save();
        assert!(saved.starts_with("Body text.\n"));
        assert!(!session.export().contains("Idea one"));
        assert_eq!(session.export(), "Body text.\n");

        let mut reopened = DocumentSession::new();
        let opened = reopened.open(&saved);
        assert_eq!(opened.body, "Body text.\n");
        assert_eq!(reopened.overflow(), "Idea one\n```\ncode\n```\n");
        assert_eq!(
            reopened.annotations_json()["overflow"],
            "Idea one\n```\ncode\n```\n"
        );

        // Emptying the overflow saves plain Markdown again.
        reopened.set_overflow("").unwrap();
        assert_eq!(reopened.save(), "Body text.\n");
    }

    #[test]
    fn a_preserved_malformed_block_refuses_overflow_changes() {
        let source =
            "Body text.\n\n```terraphim-alternatives\n{ \"version\": 1, \"spans\": [,] }\n```\n";
        let mut session = DocumentSession::new();
        session.open(source);
        assert!(session.has_preserved_block());
        let before = session.clone();
        assert_eq!(
            session.stash(0, 4).unwrap_err(),
            OverflowError::PreservedBlock
        );
        assert_eq!(
            session.set_overflow("x").unwrap_err().kind(),
            "preserved-block"
        );
        assert!(session.replay_overflow("", "x").is_err());
        assert_eq!(session, before);
        assert_eq!(session.save(), source);
        let json = set_overflow_json(&mut session, "x");
        assert_eq!(json["ok"], false);
        assert_eq!(json["kind"], "preserved-block");
    }

    #[test]
    fn overflow_json_helpers_report_the_new_text() {
        let mut session = DocumentSession::new();
        session.open("Text");
        let json = set_overflow_json(&mut session, "a");
        assert_eq!(json, json!({ "ok": true, "overflow": "a" }));
        let json = replay_overflow_json(&mut session, "a", "a\n\nb");
        assert_eq!(
            json,
            json!({ "ok": true, "overflow": "a\n\nb", "applied": true })
        );
    }
}

#[cfg(test)]
mod ordinary_action_tests {
    use super::*;

    fn reset(body: &str) {
        HISTORY_BRIDGE.with(|b| *b.borrow_mut() = HistoryBridge::default());
        with_session(|s| {
            s.open(body);
        });
        crate::providers::reset_providers();
    }
    fn run(meta: Value, args: &[&str]) -> Option<Value> {
        let body = with_session(|s| s.body().to_owned());
        begin_document_action_json(
            &body,
            &meta.to_string(),
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )
        .unwrap()
    }
    fn commit(meta: Value, args: &[&str]) -> Value {
        let v = run(meta, args).unwrap();
        finish_history_transaction_json(v["token"].as_str().unwrap()).unwrap();
        v["result"].clone()
    }
    fn edit(before: &str, after: &str, start: usize, len: usize, arg: usize) -> Value {
        json!({"kind":"text_edit","start":start,"deleteUtf16":len,"insertedRef":arg,
            "beforeBodyHash":crate::host::source_hash(before),"afterBodyHash":crate::host::source_hash(after)})
    }
    #[test]
    fn actual_actions_keep_private_receipt_identity_and_support_guarded_abort() {
        reset("risk here");
        let receipt = with_session(|s| {
            s.ghost_ranges_atomic("risk here", &[(5, 9)])
                .unwrap()
                .unwrap()
        });
        let before = with_session(|s| s.clone());
        let v = run(edit("risk here", "risk here!", 9, 0, 0), &["!"]).unwrap();
        abort_history_transaction_json(v["token"].as_str().unwrap()).unwrap();
        assert_eq!(with_session(|s| s.clone()), before);
        commit(edit("risk here", "risk here!", 9, 0, 0), &["!"]);
        commit(edit("risk here!", "risk here", 9, 1, 0), &[""]);
        with_session(|s| s.replay_ghost_change(&receipt, true)).unwrap();
        assert!(with_session(|s| s.annotations_json()["ghosts"]
            .as_array()
            .unwrap()
            .is_empty()));
    }
    #[test]
    fn real_alternative_crud_swap_manual_ghost_and_overflow_actions() {
        reset("risk here");
        let created = commit(
            json!({"kind":"alt_create","spanKind":"word","start":0,"end":4,"textRef":0,"source":"human"}),
            &["concern"],
        );
        let id = created["span"].as_str().unwrap();
        commit(
            json!({"kind":"alt_add","spanId":id,"textRef":0,"source":"human"}),
            &["worry"],
        );
        commit(
            json!({"kind":"alt_edit","spanId":id,"index":2,"textRef":0}),
            &["hazard"],
        );
        commit(json!({"kind":"alt_move","spanId":id,"from":2,"to":1}), &[]);
        commit(json!({"kind":"alt_active","spanId":id,"index":1}), &[]);
        assert_eq!(with_session(|s| s.body().to_owned()), "hazard here");
        commit(json!({"kind":"alt_remove","spanId":id,"index":1}), &[]);
        assert_eq!(with_session(|s| s.body().to_owned()), "risk here");
        commit(json!({"kind":"ghost_range","start":5,"end":9}), &[]);
        commit(json!({"kind":"revive_range","start":5,"end":9}), &[]);
        commit(json!({"kind":"stash","start":5,"end":9}), &[]);
        assert_eq!(with_session(|s| s.overflow().to_owned()), "here");
        let pulled = commit(json!({"kind":"pull","at":5,"from":0,"to":4}), &[]);
        assert_eq!(pulled["edit"]["insertedText"], pulled["text"]);
        assert_eq!(pulled["overflow"]["before"], "here");
        assert_eq!(pulled["overflow"]["after"], "here");
        assert!(with_session(|s| s.body().contains("here")));
        commit(json!({"kind":"overflow_set","textRef":0}), &["notes"]);
        assert_eq!(with_session(|s| s.overflow().to_owned()), "notes");
    }
    #[test]
    fn strict_metadata_refs_and_batch_failure_leave_actual_state_and_pending_unchanged() {
        reset("abc");
        for (meta, args) in [
            (json!({"kind":"history_replay"}), vec![]),
            (json!({"kind":"trim_promote"}), vec![]),
            (
                json!({"kind":"overflow_set","textRef":0,"extra":true}),
                vec!["x".into()],
            ),
            (json!({"kind":"overflow_set","textRef":1}), vec!["x".into()]),
            (
                json!({"kind":"overflow_set","textRef":0}),
                vec!["x".into(), "unused".into()],
            ),
        ] {
            let before = with_session(|s| s.clone());
            assert!(begin_document_action_json("abc", &meta.to_string(), &args).is_err());
            assert_eq!(with_session(|s| s.clone()), before);
            assert!(HISTORY_BRIDGE.with(|b| b.borrow().pending.is_none()));
        }
        let first = edit("abc", "abcd", 3, 0, 0);
        let second = edit("wrong", "abcde", 4, 0, 1);
        let entries: Vec<Value> = [first, second]
            .into_iter()
            .map(|mut v| {
                v.as_object_mut().unwrap().remove("kind");
                v
            })
            .collect();
        assert!(begin_document_action_json(
            "abc",
            &json!({"kind":"text_edits","edits":entries}).to_string(),
            &["d".into(), "e".into()]
        )
        .is_err());
        assert_eq!(with_session(|s| s.body().to_owned()), "abc");
    }
    #[test]
    fn append_uses_real_provider_parser_and_stale_role_cannot_finish() {
        reset("risk");
        crate::providers::register_external_provider("test-provider", "real-model").unwrap();
        let generation: Value = serde_json::from_str(&crate::kg::role_snapshot_json()).unwrap();
        let meta = json!({"kind":"append_alternatives","start":0,"end":4,"suppliedJsonRef":0,
            "roleGeneration":generation["generation"],"providerGeneration":"1"});
        let prepared = run(meta, &[r#"{"test-provider":["concern"]}"#]).unwrap();
        assert_eq!(
            prepared["result"]["after"]["alts"][1]["model"],
            "real-model"
        );
        crate::kg::clear_thesaurus();
        assert_eq!(
            finish_history_transaction_json(prepared["token"].as_str().unwrap())
                .unwrap_err()
                .kind,
            "role-conflict"
        );
        abort_history_transaction_json(prepared["token"].as_str().unwrap()).unwrap();
        assert!(with_session(|s| s.document().annotations.spans.is_empty()));
    }
    #[test]
    fn kg_swap_binds_displayed_role_at_begin_and_finish_without_resetting_native_state() {
        reset("risk");
        let role = r#"{"name":"one","data":{"risk":{"id":1,"nterm":"risk"},"concern":{"id":1,"nterm":"risk"}}}"#;
        crate::kg::load_thesaurus(role).unwrap();
        let snapshot: Value = serde_json::from_str(&crate::kg::role_snapshot_json()).unwrap();
        let spans = with_session(|s| crate::kg::kg_spans_json(s));
        let row = &spans[0];
        let index = row["alts"]
            .as_array()
            .unwrap()
            .iter()
            .position(|v| v["text"] == "concern")
            .unwrap();
        let action = json!({"kind":"kg_swap","spanId":row["id"],"index":index,"roleGeneration":snapshot["generation"]});
        let before = with_session(|s| s.clone());
        let v = run(action.clone(), &[]).unwrap();
        assert_eq!(with_session(|s| s.body().to_owned()), "concern");
        crate::kg::load_thesaurus(role).unwrap();
        assert_eq!(
            finish_history_transaction_json(v["token"].as_str().unwrap())
                .unwrap_err()
                .kind,
            "role-conflict"
        );
        abort_history_transaction_json(v["token"].as_str().unwrap()).unwrap();
        assert_eq!(with_session(|s| s.clone()), before);
        assert_eq!(
            begin_document_action_json("risk", &action.to_string(), &[])
                .unwrap_err()
                .kind,
            "role-conflict"
        );
    }
    #[test]
    fn valid_batch_and_nochange_preserve_order_and_do_not_issue_phantom_tokens() {
        reset("a");
        let mut first = edit("a", "ab", 1, 0, 0);
        let mut second = edit("ab", "abc", 2, 0, 1);
        first.as_object_mut().unwrap().remove("kind");
        second.as_object_mut().unwrap().remove("kind");
        let v = run(
            json!({"kind":"text_edits","edits":[first,second]}),
            &["b", "c"],
        )
        .unwrap();
        assert_eq!(v["result"]["edits"][1]["edit"]["insertedText"], "c");
        assert_eq!(
            begin_document_action_json(
                "abc",
                &json!({"kind":"overflow_set","textRef":0}).to_string(),
                &["x".into()]
            )
            .unwrap_err()
            .kind,
            "pending"
        );
        finish_history_transaction_json(v["token"].as_str().unwrap()).unwrap();
        let watermark = HISTORY_BRIDGE.with(|b| b.borrow().watermark);
        assert!(run(edit("abc", "abc", 3, 0, 0), &[""]).is_none());
        assert_eq!(HISTORY_BRIDGE.with(|b| b.borrow().watermark), watermark);
    }
    #[test]
    fn action_limits_and_utf16_refusals_preserve_actual_engine() {
        reset("😀a");
        for field in ["start", "deleteUtf16", "insertedRef"] {
            let mut v = edit("😀a", "😀b", 2, 1, 0);
            v[field] = json!(9_007_199_254_740_991u64);
            assert!(begin_document_action_json("😀a", &v.to_string(), &["b".into()]).is_err());
        }
        assert!(begin_document_action_json(
            "😀a",
            &edit("😀a", "bad", 1, 1, 0).to_string(),
            &["b".into()]
        )
        .is_err());
        assert_eq!(with_session(|s| s.body().to_owned()), "😀a");
        assert_eq!(
            begin_document_action_json("😀a", &" ".repeat(HISTORY_JSON_LIMIT + 1), &[])
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(
            begin_document_action_json(
                "😀a",
                r#"{"kind":"overflow_set","textRef":0}"#,
                &["a".repeat(HISTORY_JSON_LIMIT), "b".into()]
            )
            .unwrap_err()
            .kind,
            "limit"
        );
        let repeated = json!({"start":0,"deleteUtf16":0,"insertedRef":0,"beforeBodyHash":crate::host::source_hash("😀a"),"afterBodyHash":crate::host::source_hash("😀a")});
        assert!(begin_document_action_json(
            "😀a",
            &json!({"kind":"text_edits","edits":[repeated.clone(),repeated]}).to_string(),
            &["".into()]
        )
        .is_err());
    }
    #[test]
    fn exact_argument_and_operation_budgets_admit_boundary_and_refuse_next_item() {
        reset("x");
        let mut edits = Vec::new();
        for index in 0..HISTORY_OPERATION_LIMIT {
            let mut value = edit("x", "x", 1, 0, index);
            value.as_object_mut().unwrap().remove("kind");
            edits.push(value);
        }
        let args = vec![String::new(); HISTORY_OPERATION_LIMIT];
        let meta = json!({"kind":"text_edits","edits":edits});
        assert!(begin_document_action_json("x", &meta.to_string(), &args)
            .unwrap()
            .is_none());
        let mut over = meta;
        let repeated = over["edits"][0].clone();
        over["edits"].as_array_mut().unwrap().push(repeated);
        assert_eq!(
            begin_document_action_json("x", &over.to_string(), &args)
                .unwrap_err()
                .kind,
            "limit"
        );
        let exact = vec!["x".repeat(HISTORY_JSON_LIMIT)];
        assert!(parse_action_metadata(r#"{"kind":"overflow_set","textRef":0}"#, &exact).is_ok());
        let excessive = vec!["x".repeat(HISTORY_JSON_LIMIT + 1)];
        assert_eq!(
            parse_action_metadata(r#"{"kind":"overflow_set","textRef":0}"#, &excessive)
                .unwrap_err()
                .kind,
            "limit"
        );
        assert_eq!(with_session(|s| s.body().to_owned()), "x");
    }

    #[test]
    fn admitted_near_limit_full_replacement_does_not_duplicate_deleted_transport_budget() {
        let before = "a".repeat(HISTORY_JSON_LIMIT - 1024);
        let after = "b".repeat(HISTORY_JSON_LIMIT - 1024);
        reset(&before);
        let v = begin_document_action_json(
            &before,
            &edit(&before, &after, 0, before.len(), 0).to_string(),
            std::slice::from_ref(&after),
        )
        .unwrap()
        .unwrap();
        assert_eq!(v["source"], after);
        abort_history_transaction_json(v["token"].as_str().unwrap()).unwrap();
        assert_eq!(with_session(|s| s.body().to_owned()), before);
    }
    #[test]
    fn pure_persisted_eol_materialization_retains_native_receipts_and_roundtrips_annotations() {
        reset("😀 risk\nnext line\n");
        let receipt = with_session(|s| {
            s.ghost_ranges_atomic("😀 risk\nnext line\n", &[(8, 17)])
                .unwrap()
                .unwrap()
        });
        let native = with_session(|s| s.clone());
        let source = native.save();
        let crlf = crate::host::host_persistence_source(&source, true).unwrap();
        assert!(crlf.contains("\r\n"));
        assert_eq!(
            crate::host::host_presentation_source(&crlf).unwrap(),
            source
        );
        assert_eq!(with_session(|s| s.clone()), native);
        with_session(|s| s.replay_ghost_change(&receipt, true)).unwrap();
        let malformed = "body\n\n```terraphim-alternatives\n{ broken }\n```\n";
        assert!(crate::host::host_persistence_source(malformed, true).is_err());
    }
    #[test]
    fn dispatcher_saved_synonyms_query_real_graph_and_original_authority_undo_redo() {
        use terraphim_rolegraph_browser::GraphSession;
        fn query(group: &Value, text: &str) -> Value {
            let mut graph = GraphSession::default();
            let mut call = |generation: &str, op: &str, payload: Value| -> Value {
                let response: Value = serde_json::from_str(&graph.dispatch(&json!({"version":1,"requestId":"native-authority-synonyms","generation":generation,"expectedRevision":null,"op":op,"payload":payload}).to_string()).unwrap()).unwrap();
                assert_eq!(response["ok"], true, "{response}");
                response["result"].clone()
            };
            let prepared = call(
                "0",
                "prepare_role",
                json!({"thesaurusText":group["thesaurus"].to_string()}),
            );
            call(
                "0",
                "activate_role",
                json!({"token":prepared["token"],"nextGeneration":"1"}),
            );
            call("1", "match", json!({"text":text}))
        }
        reset("risk here");
        let ghost = with_session(|s| {
            s.ghost_ranges_atomic("risk here", &[(5, 9)])
                .unwrap()
                .unwrap()
        });
        let added = commit(
            json!({"kind":"alt_create","spanKind":"word","start":0,"end":4,"textRef":0,"source":"human"}),
            &["concern"],
        );
        let id = added["span"].as_str().unwrap();
        assert_eq!(
            with_session(|s| s.synonyms_for(id, "risk")),
            vec!["risk", "concern"]
        );
        let groups = with_session(|s| s.synonym_groups_json());
        assert!(groups[0]["markdown"]
            .as_str()
            .unwrap()
            .contains("synonyms:: risk, concern"));
        assert_eq!(query(&groups[0], "concern"), query(&groups[0], "risk"));
        assert_eq!(query(&groups[0], "concern"), json!(["1"]));
        let saved = with_session(|s| s.save());
        let path = std::env::temp_dir().join(format!(
            "terraphim-authority-synonyms-{}-{}.md",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, saved.as_bytes()).unwrap();
        let actual = std::fs::read_to_string(&path).unwrap();
        assert_eq!(actual, saved);
        let mut reopened = DocumentSession::new();
        reopened.open(&actual);
        assert_eq!(reopened.synonym_groups_json(), groups);
        assert_eq!(
            query(&reopened.synonym_groups_json()[0], "concern"),
            json!(["1"])
        );
        let op = |target: Value| {
            json!([{"kind":"alt","body":"risk here","start":0,"deletedText":"","insertedText":"","span":id,"target":target}]).to_string()
        };
        let undo = begin_history_replay_json("risk here", "risk here", &op(Value::Null)).unwrap();
        finish_history_transaction_json(undo["token"].as_str().unwrap()).unwrap();
        assert!(with_session(|s| s.synonyms_for(id, "risk").is_empty()));
        assert_eq!(with_session(|s| s.synonym_groups_json()), json!([]));
        let redo = begin_history_replay_json("risk here", "risk here", &op(added["after"].clone()))
            .unwrap();
        finish_history_transaction_json(redo["token"].as_str().unwrap()).unwrap();
        assert_eq!(with_session(|s| s.synonym_groups_json()), groups);
        assert_eq!(with_session(|s| s.save()), saved);
        with_session(|s| s.replay_ghost_change(&ghost, true)).unwrap();
        std::fs::remove_file(path).unwrap();
    }
}
