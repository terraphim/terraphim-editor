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
//!   `annotations`.
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
    /// Ids of set-aside spans and ghosts this edit re-attached.
    pub reattached: Vec<String>,
    /// Spans and ghosts set aside after the edit.
    pub set_aside: usize,
    /// A non-blocking notice when the edit detached alternatives.
    pub warning: Option<String>,
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
    /// `inserted` code units: an anchor wholly after the edit (an insertion
    /// exactly at its start included) shifts; any other anchor keeps its
    /// position, which is where its text would come back on undo.
    fn shift_hints(&mut self, start: usize, end: usize, inserted: usize) {
        let anchors = self
            .spans
            .iter_mut()
            .map(|s| &mut s.anchor)
            .chain(self.ghosts.iter_mut().map(|g| &mut g.anchor));
        for anchor in anchors {
            if end <= anchor.start {
                let len = anchor.end - anchor.start;
                anchor.start = anchor.start - (end - start) + inserted;
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
        Opened {
            body: self.doc.body.clone(),
            warning: self.warning.clone(),
            unresolved: self.set_aside.len(),
        }
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
        let ghosts_before = self.doc.annotations.ghosts.len();
        let detached = self.doc.apply_edit(start, end, inserted)?;
        let inserted_len = utf16_len(inserted);
        self.set_aside.shift_hints(start, end, inserted_len);
        let detached_ids: Vec<String> = detached.iter().map(|s| s.id.clone()).collect();
        let warning = detach_warning(&detached);
        // Freshly detached spans keep their pre-edit anchors: that is exactly
        // where their text returns if the edit is undone.
        // Only an item touching the changed text can have its text come back,
        // unless the edit removed a live item that was blocking it.
        let freed = !detached.is_empty() || self.doc.annotations.ghosts.len() < ghosts_before;
        self.set_aside.spans.extend(detached);
        let window = (!freed).then_some((start, start + inserted_len));
        let reattached = self.reattach(window);
        Ok(EditOutcome {
            detached: detached_ids,
            reattached,
            set_aside: self.set_aside.len(),
            warning,
        })
    }

    /// Re-attaches every set-aside span and ghost whose text is back exactly
    /// at its anchor and that does not overlap a live item of its kind.
    /// Returns the re-attached ids (a re-attached item whose id was taken
    /// meanwhile gets a fresh one).
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
            let a = &ghost.anchor;
            let fits = near(a, window)
                && text_at(body, a)
                && !annotations
                    .ghosts
                    .iter()
                    .any(|g| overlaps(&g.anchor, a.start, a.end));
            if fits {
                if used.contains(&ghost.id) {
                    ghost.id = fresh_id('g', &used);
                }
                used.insert(ghost.id.clone());
                reattached.push(ghost.id.clone());
                annotations.ghosts.push(ghost);
            } else {
                waiting.push(ghost);
            }
        }
        self.set_aside.ghosts = waiting;
        annotations.ghosts.sort_by_key(|g| g.anchor.start);
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
        })
    }
}

impl EditOutcome {
    /// `{ detached, reattached, setAside, warning }` for JavaScript.
    pub fn to_json(&self) -> Value {
        json!({
            "detached": self.detached,
            "reattached": self.reattached,
            "setAside": self.set_aside,
            "warning": self.warning,
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
/// in the model. Returns `{ detached, reattached, setAside, warning }`; throws
/// if the edit does not fit the model, in which case call
/// [`sync_document_body`].
#[wasm_bindgen]
pub fn apply_edit(start: u32, deleted_len: u32, inserted: &str) -> Result<JsValue, JsValue> {
    with_session(|s| s.apply_edit(start as usize, deleted_len as usize, inserted))
        .map(|outcome| to_js(&outcome.to_json()))
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Replaces the model body with `text` if it differs and re-anchors; returns
/// `{ changed, unresolved }`.
#[wasm_bindgen]
pub fn sync_document_body(text: &str) -> JsValue {
    let synced = with_session(|s| s.sync_body(text));
    to_js(&json!({ "changed": synced.changed, "unresolved": synced.unresolved }))
}

/// The body as the model holds it (should always equal the surface text).
#[wasm_bindgen]
pub fn document_body() -> String {
    with_session(|s| s.body().to_string())
}

/// `{ spans, ghosts, overflow, setAside: { spans, ghosts }, preservedBlock }`
/// of the current document, in UTF-16 offsets.
#[wasm_bindgen]
pub fn document_annotations() -> JsValue {
    to_js(&with_session(|s| s.annotations_json()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use terraphim_alternatives::{Source, SpanKind};

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
    fn out_of_range_edit_is_refused_without_change() {
        let mut session = DocumentSession::new();
        session.open("abc");
        assert!(session.apply_edit(2, 5, "x").is_err());
        assert!(session.apply_edit(usize::MAX, 1, "x").is_err());
        assert_eq!(session.body(), "abc");
    }
}
