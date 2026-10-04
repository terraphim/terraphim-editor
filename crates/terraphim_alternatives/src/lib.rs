//! Span model for terraphim-editor's alternative control.
//!
//! This crate holds only the state the knowledge graph cannot derive:
//! human-written (and AI) alternatives, ghost flags, the overflow stash, and
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
//!   [`Annotations`]); [`write`] appends the block back deterministically. A
//!   malformed block yields a [`BlockError`] that keeps the body and the raw
//!   block.
//! * [`Document::reanchor`] re-finds spans after outside edits and reports
//!   missing or ambiguous anchors instead of guessing.
//! * [`Document::set_active`] / [`Document::cycle_active`] swap the visible
//!   alternative with the `a`/`an` fix-up.
//! * [`Document::remove_alternative`], [`Document::clear_alternatives`] and
//!   [`Document::set_ghost`] remove spans that have nothing left to keep.
//! * [`Document::export`] produces clean Markdown; [`Document::counts`]
//!   counts words and characters, ghosted text included.
//!
//! Spans never overlap in v1, so a sentence containing a word span cannot
//! itself become a span.
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
pub use model::{Alternative, Anchor, Annotations, SCHEMA_VERSION, Source, Span, SpanKind};
pub use offset::{byte_to_utf16, utf16_len, utf16_to_byte};
pub use reanchor::{ReanchorReport, Unresolved, UnresolvedReason};
