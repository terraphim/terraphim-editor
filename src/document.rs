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

use serde_json::{json, Value};
use terraphim_alternatives::{
    parse, utf16_len, utf16_to_byte, write, Anchor, Counts, Document, EditError, Ghost,
    ReanchorReport, Source, Span, SpanKind,
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
    doc: Document,
    /// The unreadable block exactly as found, written back verbatim on save.
    raw_block: Option<String>,
    warning: Option<String>,
    /// Spans and ghosts out of the model, kept whole and saved (see the
    /// module docs).
    set_aside: SetAside,
}

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
    format!("{prefix}{}", max + 1)
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
                match self.doc.ghost(start, end) {
                    Ok(id) => {
                        used.insert(id.clone());
                        reattached.push(id);
                    }
                    Err(_) => waiting.push(ghost),
                }
            } else {
                if used.contains(&ghost.id) {
                    ghost.id = fresh_id('g', &used);
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
        })
    }

    /// Ghosts `start..end` (R-5.1, R-5.3) and returns the id of the ghost now
    /// covering it. Ghosts are an independent layer: the range may cover or
    /// partly overlap spans, and a ghost overlapping or touching it is merged
    /// in (see [`Document::ghost`]). The text is unchanged, so it stays
    /// counted and editable; export drops it. On error nothing changes.
    pub fn ghost(&mut self, start: usize, end: usize) -> Result<String, EditError> {
        self.doc.ghost(start, end)
    }

    /// Revives `start..end` (R-5.2): ghosts inside it are removed, ghosts
    /// partly inside are trimmed and a ghost strictly containing it is split
    /// (see [`Document::revive`]). Returns whether any ghost changed. On
    /// error nothing changes.
    pub fn revive(&mut self, start: usize, end: usize) -> Result<bool, EditError> {
        self.doc.revive(start, end)
    }
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
// ---------------------------------------------------------------------------

impl DocumentSession {
    /// Appends `texts` as alternatives (with `source` and `model`) to the
    /// live span exactly over `start..end` (UTF-16), creating a `kind` span
    /// there when there is none. Texts the span already offers (its original
    /// included) are skipped, so repeating the call adds nothing. Returns the
    /// span id and the number of alternatives added.
    ///
    /// Atomic: a range that partly overlaps another span, an invalid range,
    /// or nothing to add to a new span is refused and changes nothing.
    pub fn append_alternatives(
        &mut self,
        kind: SpanKind,
        start: usize,
        end: usize,
        texts: &[String],
        source: Source,
        model: Option<&str>,
    ) -> Result<(String, usize), EditError> {
        let existing = self
            .doc
            .annotations
            .spans
            .iter()
            .find(|s| s.anchor.start == start && s.anchor.end == end);
        let mut offered: HashSet<String> = existing
            .map(|s| s.alts.iter().map(|a| a.text.clone()).collect())
            .unwrap_or_default();
        if existing.is_none() {
            let body_text = utf16_to_byte(&self.doc.body, start)
                .zip(utf16_to_byte(&self.doc.body, end))
                .map(|(a, b)| self.doc.body[a..b].to_string());
            offered.extend(body_text);
        }
        let fresh: Vec<&String> = texts
            .iter()
            .filter(|t| !t.is_empty() && offered.insert((*t).clone()))
            .collect();
        let before = self.doc.clone();
        let id = match existing {
            Some(span) => span.id.clone(),
            None if fresh.is_empty() => {
                return Err(EditError::InvalidRange { start, end });
            }
            None => self.doc.add_span(kind, start, end)?,
        };
        for text in &fresh {
            if let Err(error) =
                self.doc
                    .add_alternative(&id, text.as_str(), source, model.map(str::to_owned))
            {
                self.doc = before;
                return Err(error);
            }
        }
        Ok((id, fresh.len()))
    }
}

// ---------------------------------------------------------------------------
// End of appending alternatives (issue #13).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use terraphim_alternatives::{Source, SpanKind};

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
            vec![ghost("g1", 0, 18, "Pass me an eraser.")]
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
}
