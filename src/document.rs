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
//! # Recovery
//!
//! A malformed block never loses data. The body opens for editing, one warning
//! is reported, and the raw block is kept verbatim: [`DocumentSession::save`]
//! writes `body + raw_block`, so the block survives untouched until it is
//! repaired by hand (the crate's `body + raw_block == source` contract).

use std::cell::RefCell;

use serde_json::{json, Value};
use terraphim_alternatives::{parse, write, Counts, Document, EditError};
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
    /// Spans and ghosts that could not be re-anchored in the new body.
    pub unresolved: usize,
}

/// One open document: the span model plus, after a malformed open, the raw
/// annotation block kept for saving.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentSession {
    doc: Document,
    /// The unreadable block exactly as found, written back verbatim on save.
    raw_block: Option<String>,
    warning: Option<String>,
    /// Ids of spans taken out of the model because an edit changed their text
    /// or they could not be re-anchored, since the last open. Reported, not
    /// saved.
    set_aside: Vec<String>,
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
    ///   their text. Anything that cannot be placed is reported in `warning`
    ///   and `unresolved` (and is not written back on save).
    /// * A malformed block: the body opens, `warning` explains the problem
    ///   once, and the raw block is preserved for [`save`](Self::save).
    pub fn open(&mut self, source: &str) -> Opened {
        *self = Self::default();
        let mut unresolved = 0;
        match parse(source) {
            Ok(mut doc) => {
                let report = doc.reanchor();
                unresolved = report.unresolved.len() + report.unresolved_ghosts.len();
                self.set_aside
                    .extend(report.unresolved.iter().map(|u| u.span.id.clone()));
                self.set_aside
                    .extend(report.unresolved_ghosts.iter().map(|u| u.ghost.id.clone()));
                if unresolved > 0 {
                    self.warning = Some(format!(
                        "{unresolved} saved alternative or ghost annotation(s) no longer match \
                         the text and were set aside; they will not be saved."
                    ));
                }
                self.doc = doc;
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
            unresolved,
        }
    }

    /// The `.md` text to write to disk: the body followed by the annotation
    /// block (or, after a malformed open, by the preserved raw block). A
    /// document with nothing to persist saves as plain Markdown.
    pub fn save(&self) -> String {
        match &self.raw_block {
            Some(raw) => format!("{}{raw}", self.doc.body),
            None => write(&self.doc),
        }
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

    /// Read-only access to the span model.
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Mirrors one editing-surface edit: replaces `deleted_len` UTF-16 code
    /// units at `start` with `inserted`, moving every anchor exactly.
    ///
    /// Returns the ids of spans the edit detached (it changed their text, see
    /// [`Document::apply_edit`]); they are set aside, not saved. Ghosts resize
    /// with the text. On error the model is unchanged; the caller should
    /// resynchronise with [`sync_body`](Self::sync_body).
    pub fn apply_edit(
        &mut self,
        start: usize,
        deleted_len: usize,
        inserted: &str,
    ) -> Result<Vec<String>, EditError> {
        let end = start
            .checked_add(deleted_len)
            .ok_or(EditError::InvalidRange {
                start,
                end: usize::MAX,
            })?;
        let detached = self.doc.apply_edit(start, end, inserted)?;
        let ids: Vec<String> = detached.into_iter().map(|span| span.id).collect();
        self.set_aside.extend(ids.iter().cloned());
        Ok(ids)
    }

    /// Makes the model body equal to `text` when the two have drifted apart
    /// (for example after line-ending normalisation on the surface, or a
    /// failed [`apply_edit`](Self::apply_edit)). Spans and ghosts are then
    /// re-anchored by their text. A no-op when the bodies already match.
    pub fn sync_body(&mut self, text: &str) -> Synced {
        if self.doc.body == text {
            return Synced::default();
        }
        self.doc.body = text.to_string();
        let report = self.doc.reanchor();
        self.set_aside
            .extend(report.unresolved.iter().map(|u| u.span.id.clone()));
        self.set_aside
            .extend(report.unresolved_ghosts.iter().map(|u| u.ghost.id.clone()));
        Synced {
            changed: true,
            unresolved: report.unresolved.len() + report.unresolved_ghosts.len(),
        }
    }

    /// Spans, ghosts and overflow as JSON, for decorations and inspection:
    /// `{ spans, ghosts, overflow, setAside, preservedBlock }`. Spans and
    /// ghosts use the block's own schema (UTF-16 anchors).
    pub fn annotations_json(&self) -> Value {
        let annotations = &self.doc.annotations;
        json!({
            "spans": annotations.spans,
            "ghosts": annotations.ghosts,
            "overflow": annotations.overflow,
            "setAside": self.set_aside,
            "preservedBlock": self.raw_block.is_some(),
        })
    }
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

/// The current document as `.md` text: body plus annotation block.
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
/// in the model. Returns the ids of detached spans as an array; throws if the
/// edit does not fit the model, in which case call [`sync_document_body`].
#[wasm_bindgen]
pub fn apply_edit(start: u32, deleted_len: u32, inserted: &str) -> Result<JsValue, JsValue> {
    with_session(|s| s.apply_edit(start as usize, deleted_len as usize, inserted))
        .map(|ids| to_js(&json!(ids)))
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

/// `{ spans, ghosts, overflow, setAside, preservedBlock }` of the current
/// document, in UTF-16 offsets.
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
        assert!(session.apply_edit(0, 0, "Hey! ").unwrap().is_empty());
        assert!(session.apply_edit(0, 0, "😀").unwrap().is_empty());
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
        let detached = session.apply_edit(12, 1, "X").unwrap();
        assert_eq!(detached, vec!["s1".to_string()]);
        assert!(session.document().annotations.spans.is_empty());
        assert_eq!(session.annotations_json()["setAside"], json!(["s1"]));
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
        assert!(opened.warning.unwrap().contains("set aside"));
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

    #[test]
    fn out_of_range_edit_is_refused_without_change() {
        let mut session = DocumentSession::new();
        session.open("abc");
        assert!(session.apply_edit(2, 5, "x").is_err());
        assert!(session.apply_edit(usize::MAX, 1, "x").is_err());
        assert_eq!(session.body(), "abc");
    }
}
