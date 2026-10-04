//! The trailing annotation block: splitting it from the body and writing it.
//!
//! # Format
//!
//! ~~~text
//! <body>
//! <blank line>
//! ```terraphim-alternatives
//! { pretty-printed JSON, schema version 1 }
//! ```
//! ~~~
//!
//! * The writer emits `body`, then `"\n\n"` (omitted when the body is empty),
//!   then the opening fence line, the JSON, and a closing fence line ending in
//!   `"\n"`. Backticks inside JSON strings are written as ``` so the JSON
//!   can never contain a fence, whatever the overflow or alternatives hold.
//! * When there is nothing to persist, no block is written at all and a plain
//!   `.md` file stays plain.
//! * The reader is lenient: it strips up to two newlines before the opening
//!   fence, accepts trailing whitespace and `\r` on fence lines, and ignores
//!   trailing whitespace after the closing fence.
//!
//! # Recovery
//!
//! A malformed block never discards data. [`parse`] returns a [`BlockError`]
//! that carries the body and the raw block text, with the invariant
//! `error.body + error.raw_block == source`, so the caller can keep editing the
//! body and write the untouched block back.

use std::collections::HashSet;

use thiserror::Error;

use crate::document::Document;
use crate::model::{Annotations, SCHEMA_VERSION, WireV1};

/// Info string identifying the annotation block's opening fence.
pub const FENCE_INFO: &str = "terraphim-alternatives";

const FENCE: &str = "```";

/// Why a block could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BlockErrorKind {
    /// The opening fence has no closing fence after it.
    #[error("annotation block is truncated: no closing fence")]
    Truncated,
    /// Non-whitespace text follows the closing fence.
    #[error("annotation block is not at the end of the document")]
    NotTrailing,
    /// The block content is not valid JSON.
    #[error("annotation block is not valid JSON (line {line}, column {column}): {message}")]
    InvalidJson {
        /// 1-based line within the JSON.
        line: usize,
        /// 1-based column within the JSON.
        column: usize,
        /// Parser message.
        message: String,
    },
    /// The JSON has no integer `version` field.
    #[error("annotation block has no integer \"version\" field")]
    MissingVersion,
    /// The schema version is not one this crate understands.
    #[error("annotation block has unsupported version {found}")]
    UnknownVersion {
        /// The version found in the block.
        found: u64,
    },
    /// The JSON does not match schema version 1 (wrong types, unknown fields).
    #[error("annotation block does not match schema v1: {message}")]
    InvalidSchema {
        /// Deserialiser message.
        message: String,
    },
    /// Two spans share an id.
    #[error("annotation block has duplicate span id {id:?}")]
    DuplicateSpanId {
        /// The repeated id.
        id: String,
    },
    /// Two spans cover overlapping ranges; spans never overlap in v1.
    #[error("annotation block has overlapping spans {first:?} and {second:?}")]
    OverlappingSpans {
        /// The span that starts first.
        first: String,
        /// The span that starts inside it.
        second: String,
    },
    /// A span breaks a structural rule.
    #[error("span {id:?} is invalid: {reason}")]
    InvalidSpan {
        /// The span's id.
        id: String,
        /// What is wrong.
        reason: String,
    },
}

/// A block that could not be read, with everything needed to recover.
///
/// Invariant: `body + raw_block` is exactly the parsed source.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{kind}")]
pub struct BlockError {
    /// What went wrong.
    pub kind: BlockErrorKind,
    /// The document body (text before the block), safe to keep editing.
    pub body: String,
    /// The block exactly as found, including the separator and fences.
    pub raw_block: String,
}

/// Splits `source` into body and annotations.
///
/// A source with no block yields the whole text as body and empty
/// annotations. Anchors are returned as stored; call
/// [`Document::reanchor`] if the body may have been edited outside the editor.
pub fn parse(source: &str) -> Result<Document, BlockError> {
    let Some(open_start) = find_opening_fence(source) else {
        return Ok(Document::new(source));
    };
    let body_end = strip_separator(&source[..open_start]);
    let fail = |kind| BlockError {
        kind,
        body: source[..body_end].to_string(),
        raw_block: source[body_end..].to_string(),
    };

    let after_open = &source[open_start..];
    let Some(newline) = after_open.find('\n') else {
        return Err(fail(BlockErrorKind::Truncated));
    };
    let content_start = open_start + newline + 1;

    let mut line_start = content_start;
    let mut close = None;
    for line in source[content_start..].split_inclusive('\n') {
        if is_fence_line(line, FENCE) {
            close = Some((line_start, line_start + line.len()));
            break;
        }
        line_start += line.len();
    }
    let Some((close_start, close_end)) = close else {
        return Err(fail(BlockErrorKind::Truncated));
    };
    if !source[close_end..].trim().is_empty() {
        return Err(fail(BlockErrorKind::NotTrailing));
    }

    let json = &source[content_start..close_start];
    let annotations = decode(json).map_err(fail)?;
    Ok(Document {
        body: source[..body_end].to_string(),
        annotations,
    })
}

/// Serialises a document: the body followed by the annotation block.
///
/// Deterministic: equal documents produce identical bytes, and
/// `parse(&write(doc)) == Ok(doc.clone())` for any document whose spans pass
/// structural validation.
pub fn write(doc: &Document) -> String {
    let body = doc.body.as_str();
    // An empty annotation set normally writes no block. If the body itself
    // contains an opening-fence line, an empty block is still written so the
    // reader does not mistake that line for the annotations.
    if doc.annotations.is_empty() && find_opening_fence(body).is_none() {
        return body.to_string();
    }
    let wire = WireV1 {
        version: SCHEMA_VERSION,
        spans: doc.annotations.spans.clone(),
        overflow: doc.annotations.overflow.clone(),
    };
    let json = serde_json::to_string_pretty(&wire)
        .expect("annotation types always serialise")
        .replace('`', "\\u0060");

    let mut out = String::with_capacity(body.len() + json.len() + 64);
    out.push_str(body);
    if !body.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(FENCE);
    out.push_str(FENCE_INFO);
    out.push('\n');
    out.push_str(&json);
    out.push('\n');
    out.push_str(FENCE);
    out.push('\n');
    out
}

fn decode(json: &str) -> Result<Annotations, BlockErrorKind> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| BlockErrorKind::InvalidJson {
            line: e.line(),
            column: e.column(),
            message: e.to_string(),
        })?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(BlockErrorKind::MissingVersion)?;
    if version != SCHEMA_VERSION {
        return Err(BlockErrorKind::UnknownVersion { found: version });
    }
    let wire: WireV1 =
        serde_json::from_value(value).map_err(|e| BlockErrorKind::InvalidSchema {
            message: e.to_string(),
        })?;

    let mut seen = HashSet::with_capacity(wire.spans.len());
    for span in &wire.spans {
        if !seen.insert(span.id.as_str()) {
            return Err(BlockErrorKind::DuplicateSpanId {
                id: span.id.clone(),
            });
        }
        span.validate()
            .map_err(|reason| BlockErrorKind::InvalidSpan {
                id: span.id.clone(),
                reason,
            })?;
    }
    let mut ranges: Vec<_> = wire
        .spans
        .iter()
        .map(|span| (span.anchor.start, span.anchor.end, span.id.as_str()))
        .collect();
    ranges.sort_unstable();
    if let Some(pair) = ranges.windows(2).find(|pair| pair[1].0 < pair[0].1) {
        return Err(BlockErrorKind::OverlappingSpans {
            first: pair[0].2.to_string(),
            second: pair[1].2.to_string(),
        });
    }
    Ok(Annotations {
        spans: wire.spans,
        overflow: wire.overflow,
    })
}

/// Byte offset of the start of the last opening-fence line, if any.
fn find_opening_fence(source: &str) -> Option<usize> {
    let opening = format!("{FENCE}{FENCE_INFO}");
    let mut found = None;
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        if is_fence_line(line, &opening) {
            found = Some(line_start);
        }
        line_start += line.len();
    }
    found
}

fn is_fence_line(line: &str, fence: &str) -> bool {
    line.trim_end() == fence
}

/// Length of `before` once the separator is removed. The writer always emits
/// exactly `"\n\n"`, which is stripped as-is so bodies ending in `\r\n` survive;
/// otherwise up to two hand-written newlines (`\n` or `\r\n`) are removed.
fn strip_separator(before: &str) -> usize {
    if let Some(stripped) = before.strip_suffix("\n\n") {
        return stripped.len();
    }
    let mut end = before.len();
    for _ in 0..2 {
        let rest = &before[..end];
        if let Some(stripped) = rest.strip_suffix('\n') {
            end = stripped.strip_suffix('\r').unwrap_or(stripped).len();
        } else {
            break;
        }
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Alternative, Anchor, Source, Span, SpanKind};

    fn sample_span(id: &str) -> Span {
        Span {
            id: id.into(),
            kind: SpanKind::Word,
            anchor: Anchor {
                start: 4,
                end: 11,
                text: "tension".into(),
            },
            active: 0,
            alts: vec![
                Alternative::new("tension", Source::Original),
                Alternative::new("pressure", Source::Human),
            ],
            ghost: false,
        }
    }

    fn doc(body: &str) -> Document {
        let mut d = Document::new(body);
        d.annotations.spans.push(sample_span("s1"));
        d
    }

    #[test]
    fn plain_markdown_has_no_block() {
        let parsed = parse("# Title\n\nJust text.\n").unwrap();
        assert_eq!(parsed.body, "# Title\n\nJust text.\n");
        assert!(parsed.annotations.is_empty());
        assert_eq!(write(&parsed), "# Title\n\nJust text.\n");
    }

    #[test]
    fn separator_round_trips_for_awkward_bodies() {
        for body in [
            "", "abc", "abc\n", "abc\n\n", "\n", "\n\n\n", "abc\r\n", "x\r",
        ] {
            let d = doc(body);
            let written = write(&d);
            assert_eq!(parse(&written).unwrap(), d, "body {body:?}");
        }
    }

    #[test]
    fn writer_layout_is_exact() {
        let mut d = Document::new("Hi");
        d.annotations.overflow = "x".into();
        assert_eq!(
            write(&d),
            "Hi\n\n```terraphim-alternatives\n{\n  \"version\": 1,\n  \"spans\": [],\n  \"overflow\": \"x\"\n}\n```\n"
        );
    }

    #[test]
    fn backticks_in_overflow_cannot_break_the_fence() {
        let mut d = doc("The tension rises.");
        d.annotations.overflow = "notes\n```\ncode\n```\n".into();
        let written = write(&d);
        assert!(!written.contains("```\ncode"));
        assert!(written.contains("\\u0060\\u0060\\u0060"));
        assert_eq!(parse(&written).unwrap(), d);
    }

    #[test]
    fn body_containing_an_opening_fence_line_still_round_trips() {
        let body = "Example:\n\n```terraphim-alternatives\n{}\n```";
        let d = Document::new(body);
        assert_eq!(parse(&write(&d)).unwrap(), d);
    }

    #[test]
    fn lenient_reader_accepts_single_newline_and_crlf() {
        let src =
            "Body\r\n```terraphim-alternatives  \r\n{\"version\":1,\"spans\":[]}\r\n```\r\n\r\n";
        let d = parse(src).unwrap();
        assert_eq!(d.body, "Body");
        assert!(d.annotations.is_empty());
    }

    #[test]
    fn missing_version_is_reported() {
        let src = "B\n\n```terraphim-alternatives\n{\"spans\":[]}\n```\n";
        assert_eq!(parse(src).unwrap_err().kind, BlockErrorKind::MissingVersion);
    }

    #[test]
    fn unknown_fields_are_rejected_not_dropped() {
        let src =
            "B\n\n```terraphim-alternatives\n{\"version\":1,\"spans\":[],\"extra\":true}\n```\n";
        let err = parse(src).unwrap_err();
        assert!(matches!(err.kind, BlockErrorKind::InvalidSchema { .. }));
        assert_eq!(format!("{}{}", err.body, err.raw_block), src);
    }

    #[test]
    fn text_after_the_block_is_not_trailing() {
        let src = "B\n\n```terraphim-alternatives\n{\"version\":1,\"spans\":[]}\n```\nMore text\n";
        let err = parse(src).unwrap_err();
        assert_eq!(err.kind, BlockErrorKind::NotTrailing);
        assert_eq!(err.body, "B");
        assert_eq!(format!("{}{}", err.body, err.raw_block), src);
    }

    #[test]
    fn opening_fence_on_last_line_without_newline_is_truncated() {
        let src = "B\n\n```terraphim-alternatives";
        assert_eq!(parse(src).unwrap_err().kind, BlockErrorKind::Truncated);
    }
}
