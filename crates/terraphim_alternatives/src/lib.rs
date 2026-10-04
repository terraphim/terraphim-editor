//! Span model for terraphim-editor's alternative control.
//!
//! This crate holds only the state the knowledge graph cannot derive:
//! human-written (and AI) alternatives, the ghost layer, the overflow stash, and
//! the annotation block that persists them inside the `.md` file. Alternatives
//! that come from the KG are not stored; the text on the page is always the
//! active alternative.
//!
//! It has no DOM or `web-sys` dependency and builds for native targets and
//! `wasm32-unknown-unknown` alike.
//!
//! # Offsets
//!
//! All offsets in the public API and in the serialised anchors are **UTF-16
//! code units into the body** (the Markdown text with the annotation block
//! removed), matching JavaScript strings and DOM selections. Use
//! [`utf16_to_byte`] and [`byte_to_utf16`] to convert. Offsets that split a
//! character are rejected, never rounded.
//!
//! # Overview
//!
//! * [`parse`] splits a `.md` source into a [`Document`] (body plus
//!   [`Annotations`]); [`write()`] appends the block back deterministically. A
//!   malformed block yields a [`BlockError`] that keeps the body and the raw
//!   block.
//! * [`Document::reanchor`] re-finds spans and ghosts after outside edits and
//!   reports missing or ambiguous anchors instead of guessing.
//! * [`Document::set_active`] / [`Document::cycle_active`] swap the visible
//!   alternative with the `a`/`an` fix-up.
//! * [`Document::remove_alternative`] and [`Document::clear_alternatives`]
//!   remove spans that have nothing left to keep (R-2.7).
//! * [`Document::ghost`], [`Document::ghost_span`] and [`Document::revive`]
//!   edit the ghost layer (R-5); [`Document::apply_edit`] tracks live edits.
//! * [`Document::export`] produces clean Markdown; [`Document::counts`]
//!   counts words and characters, ghosted text included.
//!
//! # Spans and ghosts
//!
//! Alternative spans never overlap each other. Ghosting is an independent
//! layer of [`Ghost`] ranges (decision 2026-10-04): a ghost may cover or
//! partially overlap any spans, so a sentence containing a word with
//! alternatives can be ghosted, and swapping that word resizes the ghost.
//! Ghosts never overlap each other: ghosting a range that overlaps or touches
//! an existing ghost merges them. See the [`Document`] docs for the exact
//! merge, revive and edit rules.
//!
//! # Example
//!
//! ```
//! use terraphim_alternatives::{parse, write, Document, Source, SpanKind};
//!
//! let mut doc = Document::new("Pass me a paperclip.");
//! let id = doc.add_span(SpanKind::Word, 10, 19).unwrap();
//! doc.add_alternative(&id, "eraser", Source::Human, None).unwrap();
//! doc.set_active(&id, 1).unwrap();
//! assert_eq!(doc.body, "Pass me an eraser.");
//!
//! let saved = write(&doc);
//! assert_eq!(parse(&saved).unwrap(), doc);
//! assert_eq!(doc.export(), "Pass me an eraser.");
//!
//! // Ghost the whole sentence, word span included; swapping the word resizes
//! // the ghost, and export drops all of it.
//! let ghost = doc.ghost(0, 18).unwrap();
//! doc.set_active(&id, 0).unwrap();
//! assert_eq!(doc.body, "Pass me a paperclip.");
//! assert_eq!(doc.ghost_at(0).unwrap().anchor.text, "Pass me a paperclip.");
//! assert_eq!(doc.ghost_at(0).unwrap().id, ghost);
//! assert_eq!(doc.export(), "");
//! assert_eq!(parse(&write(&doc)).unwrap(), doc);
//! ```

mod article;
mod block;
mod document;
mod model;
pub mod offset;
pub mod reanchor;

pub use article::{Article, article_for};
pub use block::{BlockError, BlockErrorKind, FENCE_INFO, parse, write};
pub use document::{Counts, Document, EditError, SpanFate};
pub use model::{Alternative, Anchor, Annotations, Ghost, SCHEMA_VERSION, Source, Span, SpanKind};
pub use offset::{byte_to_utf16, utf16_len, utf16_to_byte};
pub use reanchor::{ReanchorReport, Unresolved, UnresolvedGhost, UnresolvedReason};
