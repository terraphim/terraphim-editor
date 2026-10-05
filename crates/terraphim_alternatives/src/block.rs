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
//!   `.md` file stays plain, unless the body would then be misread (see
//!   below), in which case an empty *guard block* is written.
//! * The JSON fields are written in the order `version`, `spans`, `ghosts`,
//!   `overflow`. `spans` and `ghosts` are always written, even when empty;
//!   `overflow` is omitted when empty. A block without `ghosts` reads as no
//!   ghosts.
//!
//! # Empty blocks and fenced examples
//!
//! Markdown may legitimately contain a ```` ```terraphim-alternatives ````
//! fence of its own, for instance documentation showing the format. The
//! reader always looks at the *last* opening fence, so such an example at the
//! end of a document would otherwise be taken for the annotations and lost
//! on the next save. Two rules, which keep schema v1 unchanged, prevent that:
//!
//! * **Reader.** A block that decodes to empty annotations (no spans, no
//!   ghosts, empty overflow) is the annotation block only if it is a guard
//!   block the writer produced: the text before it contains an opening-fence
//!   line, and its JSON is, after `\r\n` is normalised to `\n` and outer
//!   whitespace trimmed, exactly the writer's canonical empty JSON (or the
//!   pre-ghost form without `ghosts`). Any other empty block is body text,
//!   wherever it sits: the whole source is returned as the body. An empty
//!   block carries no data, so this can never discard annotations. Non-empty
//!   and malformed blocks are handled as before.
//! * **Writer.** For empty annotations the writer emits the body alone
//!   whenever the reader would return it unchanged, and a guard block
//!   otherwise (the body ends in something that reads as an annotation block,
//!   or as a malformed one). Since a guard block always follows a body with an
//!   opening fence, the reader recognises it, so `parse(write(doc))` is the
//!   identity, and a hand-written document ending in an empty example is
//!   byte-stable through `write(parse(source))`.
//!
//! The rule is an approximation of intent with one inherent limit: a source
//! whose last block is byte-identical to a guard block, after an earlier
//! opening fence, is read as a guard block. Without a format change no reader
//! can tell such a source apart from the writer's own output. A sentinel line
//! before the fence (the alternative considered in issue #26) would remove
//! the ambiguity but change the format; it was not needed because the
//! remaining case only arises from text written to look exactly like the
//! writer's output.
//!
//! # Validation
//!
//! Ids are one namespace across spans and ghosts, so a repeated id in either
//! collection, or one shared between them, is
//! [`BlockErrorKind::DuplicateId`]. Spans must not overlap spans
//! ([`BlockErrorKind::OverlappingSpans`]) and ghosts must not overlap ghosts
//! ([`BlockErrorKind::OverlappingGhosts`]; touching is allowed); a ghost may
//! overlap spans freely. Whether anchors still match the body is not checked
//! here: that is [`Document::reanchor`]'s job, for spans and ghosts alike.
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
use crate::model::{Anchor, Annotations, SCHEMA_VERSION, WireV1};

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
    /// Two items share an id. Spans and ghosts share one id namespace.
    #[error("annotation block has duplicate id {id:?}")]
    DuplicateId {
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
    /// Two ghosts cover overlapping ranges; ghosts never overlap each other
    /// (decision 2026-10-04: ghost layer).
    #[error("annotation block has overlapping ghosts {first:?} and {second:?}")]
    OverlappingGhosts {
        /// The ghost that starts first.
        first: String,
        /// The ghost that starts inside it.
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
    /// A ghost breaks a structural rule (empty id or text, or offsets that do
    /// not match its text's UTF-16 length).
    #[error("ghost {id:?} is invalid: {reason}")]
    InvalidGhost {
        /// The ghost's id.
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
/// annotations, as does one whose last block is empty and not the writer's
/// guard block (a fenced example of the format; see the module docs). Anchors are returned as stored; call
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
    let json = &source[content_start..close_start];
    let decoded = decode(json);
    // An empty block the writer did not produce is ordinary body text: a
    // fenced example of the format, say. Nothing is persisted in it, so
    // keeping it in the body can never lose annotations.
    if matches!(&decoded, Ok(annotations) if annotations.is_empty())
        && !is_writer_guard_block(&source[..body_end], json)
    {
        return Ok(Document::new(source));
    }
    if !source[close_end..].trim().is_empty() {
        return Err(fail(BlockErrorKind::NotTrailing));
    }
    let annotations = decoded.map_err(fail)?;
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
    // An empty annotation set normally writes no block. If the reader would
    // not return the body unchanged on its own (it ends in something that
    // reads as an annotation block, or as a malformed one), a guard block is
    // written so the reader stops there instead.
    if doc.annotations.is_empty() && !needs_guard_block(body) {
        return body.to_string();
    }
    let json = encode(&doc.annotations);

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

/// The block's JSON exactly as the writer emits it, backticks escaped.
fn encode(annotations: &Annotations) -> String {
    let wire = WireV1 {
        version: SCHEMA_VERSION,
        spans: annotations.spans.clone(),
        ghosts: annotations.ghosts.clone(),
        overflow: annotations.overflow.clone(),
    };
    serde_json::to_string_pretty(&wire)
        .expect("annotation types always serialise")
        .replace('`', "\\u0060")
}

/// The guard block's JSON as written before the ghost layer existed
/// (no `ghosts` field). Still recognised so files saved by that writer keep
/// their guard block out of the body.
const LEGACY_GUARD_JSON: &str = "{\n  \"version\": 1,\n  \"spans\": []\n}";

/// Whether the writer must add a guard block after `body` for an empty
/// annotation set: true unless parsing `body` on its own returns it intact.
fn needs_guard_block(body: &str) -> bool {
    find_opening_fence(body).is_some() && !matches!(parse(body), Ok(ref doc) if doc.body == body)
}

/// Whether an empty block with content `json`, preceded by `body`, is a guard
/// block the writer produced. The writer only writes one after a body that
/// contains an opening-fence line, and always in its canonical layout, so
/// both must hold. Line endings are normalised so a file converted to CRLF
/// as a whole (by Git or another editor) is still recognised.
fn is_writer_guard_block(body: &str, json: &str) -> bool {
    if find_opening_fence(body).is_none() {
        return false;
    }
    let normalised = json.replace("\r\n", "\n");
    let normalised = normalised.trim();
    normalised == encode(&Annotations::default()) || normalised == LEGACY_GUARD_JSON
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

    let mut seen = HashSet::with_capacity(wire.spans.len() + wire.ghosts.len());
    let span_ids = wire.spans.iter().map(|s| &s.id);
    if let Some(id) = span_ids
        .chain(wire.ghosts.iter().map(|g| &g.id))
        .find(|id| !seen.insert(id.as_str()))
    {
        return Err(BlockErrorKind::DuplicateId { id: id.clone() });
    }
    for span in &wire.spans {
        span.validate()
            .map_err(|reason| BlockErrorKind::InvalidSpan {
                id: span.id.clone(),
                reason,
            })?;
    }
    for ghost in &wire.ghosts {
        ghost
            .validate()
            .map_err(|reason| BlockErrorKind::InvalidGhost {
                id: ghost.id.clone(),
                reason,
            })?;
    }
    let spans = wire.spans.iter().map(|s| (&s.anchor, &s.id));
    if let Some((first, second)) = first_overlap(spans) {
        return Err(BlockErrorKind::OverlappingSpans { first, second });
    }
    let ghosts = wire.ghosts.iter().map(|g| (&g.anchor, &g.id));
    if let Some((first, second)) = first_overlap(ghosts) {
        return Err(BlockErrorKind::OverlappingGhosts { first, second });
    }
    Ok(Annotations {
        spans: wire.spans,
        ghosts: wire.ghosts,
        overflow: wire.overflow,
    })
}

/// Ids of the first pair of overlapping ranges in start order, if any.
/// Touching ranges do not overlap.
fn first_overlap<'a>(
    items: impl Iterator<Item = (&'a Anchor, &'a String)>,
) -> Option<(String, String)> {
    let mut ranges: Vec<_> = items
        .map(|(anchor, id)| (anchor.start, anchor.end, id.as_str()))
        .collect();
    ranges.sort_unstable();
    ranges
        .windows(2)
        .find(|pair| pair[1].0 < pair[0].1)
        .map(|pair| (pair[0].2.to_string(), pair[1].2.to_string()))
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
    use crate::model::{Alternative, Ghost, Source, Span, SpanKind};

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
            "Hi\n\n```terraphim-alternatives\n{\n  \"version\": 1,\n  \"spans\": [],\n  \"ghosts\": [],\n  \"overflow\": \"x\"\n}\n```\n"
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
        let src = "Body\r\n```terraphim-alternatives  \r\n{\"version\":1,\"spans\":[],\"overflow\":\"o\"}\r\n```\r\n\r\n";
        let d = parse(src).unwrap();
        assert_eq!(d.body, "Body");
        assert_eq!(d.annotations.overflow, "o");
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
        let src = "B\n\n```terraphim-alternatives\n{\"version\":1,\"spans\":[],\"overflow\":\"o\"}\n```\nMore text\n";
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

    #[test]
    fn block_without_ghosts_field_reads_as_no_ghosts() {
        let src = "B\n\n```terraphim-alternatives\n{\"version\":1,\"spans\":[],\"overflow\":\"o\"}\n```\n";
        let d = parse(src).unwrap();
        assert!(d.annotations.ghosts.is_empty());
        assert_eq!(d.annotations.overflow, "o");
    }

    #[test]
    fn ghosts_are_written_after_spans_and_round_trip() {
        let mut d = doc("The tension rises.");
        d.annotations.ghosts.push(Ghost {
            id: "g1".into(),
            anchor: Anchor {
                start: 0,
                end: 18,
                text: "The tension rises.".into(),
            },
        });
        let written = write(&d);
        let spans_at = written.find("\"spans\"").unwrap();
        let ghosts_at = written.find("\"ghosts\"").unwrap();
        assert!(spans_at < ghosts_at);
        assert_eq!(parse(&written).unwrap(), d);
    }

    #[test]
    fn touching_ghosts_are_accepted_and_invalid_ghosts_rejected() {
        let ghost = |id: &str, start: usize, text: &str| Ghost {
            id: id.into(),
            anchor: Anchor {
                start,
                end: start + text.len(),
                text: text.into(),
            },
        };
        let mut d = Document::new("abcdef");
        d.annotations.ghosts = vec![ghost("g1", 0, "abc"), ghost("g2", 3, "def")];
        assert_eq!(parse(&write(&d)).unwrap(), d);

        d.annotations.ghosts = vec![ghost("g1", 0, "")];
        let err = parse(&write(&d)).unwrap_err();
        assert!(matches!(err.kind, BlockErrorKind::InvalidGhost { ref id, .. } if id == "g1"));
    }

    #[test]
    fn a_ghost_sharing_a_span_id_is_a_duplicate() {
        let mut d = doc("The tension rises.");
        d.annotations.ghosts.push(Ghost {
            id: "s1".into(),
            anchor: Anchor {
                start: 0,
                end: 3,
                text: "The".into(),
            },
        });
        let err = parse(&write(&d)).unwrap_err();
        assert_eq!(err.kind, BlockErrorKind::DuplicateId { id: "s1".into() });
    }

    // Issue #26: fenced examples of the format must survive parse and write.

    /// The canonical empty block, as the writer emits it.
    const GUARD: &str = "```terraphim-alternatives\n{\n  \"version\": 1,\n  \"spans\": [],\n  \"ghosts\": []\n}\n```\n";

    /// Asserts `source` is read entirely as body and saved back unchanged.
    fn assert_kept_as_body(source: &str) {
        let parsed = parse(source).unwrap();
        assert_eq!(parsed.body, source, "whole source is body");
        assert!(parsed.annotations.is_empty());
        assert_eq!(write(&parsed), source, "saved unchanged");
    }

    #[test]
    fn trailing_empty_example_without_earlier_fence_is_body() {
        assert_kept_as_body(&format!("Write this to start:\n\n{GUARD}"));
        assert_kept_as_body(
            "Compact:\n```terraphim-alternatives\n{\"version\":1,\"spans\":[]}\n```",
        );
        assert_kept_as_body(&format!("Only the example:\n\n{GUARD}\n\n  \n"));
        assert_kept_as_body(GUARD);
    }

    #[test]
    fn trailing_empty_example_with_crlf_is_body() {
        assert_kept_as_body(&format!("Example:\r\n\r\n{}", GUARD.replace('\n', "\r\n")));
        assert_kept_as_body(
            "Example:\r\n```terraphim-alternatives\r\n{\"version\":1,\"spans\":[],\"ghosts\":[]}\r\n```\r\n",
        );
    }

    #[test]
    fn two_empty_examples_are_both_body() {
        let source = "First:\n\n```terraphim-alternatives\n{\"version\":1,\"spans\":[]}\n```\n\n\
                      Second:\n\n```terraphim-alternatives\n{ \"version\": 1, \"spans\": [], \"ghosts\": [] }\n```\n";
        assert_kept_as_body(source);
    }

    #[test]
    fn two_canonical_examples_round_trip_once_saved() {
        // A body whose last block is byte-identical to a guard block after an
        // earlier fence is the documented limit: read raw, that block is taken
        // for a guard. Once the editor has saved such a body, its own guard
        // follows and the body survives every later round trip.
        let body = format!("First:\n\n{GUARD}\nSecond:\n\n{GUARD}");
        assert_eq!(
            parse(&body).unwrap().body,
            format!("First:\n\n{GUARD}\nSecond:")
        );
        let d = Document::new(body.as_str());
        let written = write(&d);
        assert_eq!(written, format!("{body}\n\n{GUARD}"));
        assert_eq!(parse(&written).unwrap(), d);
        assert_eq!(write(&parse(&written).unwrap()), written);
    }

    #[test]
    fn empty_example_not_at_the_end_is_body() {
        assert_kept_as_body(&format!(
            "Intro.\n\n{GUARD}\nMore prose after the example.\n"
        ));
        // With real annotations after it, the example stays in the body.
        let d = doc(&format!("The tension rises.\n\n{GUARD}\nAfter.\n"));
        assert_eq!(parse(&write(&d)).unwrap(), d);
    }

    #[test]
    fn writer_adds_no_guard_when_the_body_reads_back_intact() {
        let body = format!("Example:\n\n{GUARD}");
        assert_eq!(write(&Document::new(body.as_str())), body);
    }

    #[test]
    fn guard_follows_a_body_ending_in_a_non_empty_block_and_is_stripped() {
        // The body itself ends in a valid, non-empty annotation block (an
        // example document pasted in). The reader would take it, so the
        // writer adds a guard block, and the reader strips only that.
        let body = write(&doc("The tension rises."));
        let d = Document::new(body.as_str());
        let written = write(&d);
        assert_eq!(written, format!("{body}\n\n{GUARD}"));
        assert_eq!(parse(&written).unwrap(), d);
    }

    #[test]
    fn guard_survives_whole_file_crlf_conversion() {
        let body = "Example:\n\n```terraphim-alternatives\n{}\n```";
        let written = write(&Document::new(body));
        assert!(written.ends_with(GUARD));
        let converted = written.replace('\n', "\r\n");
        let parsed = parse(&converted).unwrap();
        assert_eq!(parsed.body, body.replace('\n', "\r\n"));
        assert!(parsed.annotations.is_empty());
    }

    #[test]
    fn legacy_guard_without_ghosts_is_still_stripped() {
        let body = "Example:\n\n```terraphim-alternatives\n{}\n```";
        let legacy = format!("{body}\n\n```terraphim-alternatives\n{LEGACY_GUARD_JSON}\n```\n");
        let parsed = parse(&legacy).unwrap();
        assert_eq!(parsed.body, body);
        assert!(parsed.annotations.is_empty());
        // Saving rewrites the guard in the current canonical form.
        assert_eq!(write(&parsed), format!("{body}\n\n{GUARD}"));
    }

    #[test]
    fn guard_followed_by_text_is_not_trailing() {
        let body = "Example:\n\n```terraphim-alternatives\n{}\n```";
        let src = format!("{}Appended elsewhere.\n", write(&Document::new(body)));
        let err = parse(&src).unwrap_err();
        assert_eq!(err.kind, BlockErrorKind::NotTrailing);
        assert_eq!(err.body, body);
        assert_eq!(format!("{}{}", err.body, err.raw_block), src);
    }

    #[test]
    fn guard_constant_matches_the_writer() {
        let expected = format!(
            "{FENCE}{FENCE_INFO}\n{}\n{FENCE}\n",
            encode(&Annotations::default())
        );
        assert_eq!(GUARD, expected);
    }
}
