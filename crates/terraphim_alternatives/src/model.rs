//! The span model: spans, alternatives, anchors and document-level annotations.
//!
//! Only state the knowledge graph cannot derive lives here: human-written and
//! AI alternatives, ghost flags and the overflow stash. The text on the page is
//! always the active alternative, so `anchor.text == alts[active].text` is an
//! invariant checked when a block is parsed.

use serde::{Deserialize, Serialize};

use crate::offset::utf16_len;

/// Version of the annotation schema written by this crate.
pub const SCHEMA_VERSION: u64 = 1;

/// Granularity of a span (R-2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanKind {
    /// A single word or short phrase.
    Word,
    /// A sentence or headline.
    Sentence,
    /// A whole paragraph.
    Paragraph,
}

/// Provenance of an alternative (R-4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// The text as first written. Always, and only, at index 0.
    Original,
    /// Written by the author.
    Human,
    /// Proposed by an AI provider.
    Ai,
}

/// One candidate text for a span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alternative {
    /// The candidate text exactly as it would appear in the body.
    pub text: String,
    /// Who produced it.
    pub source: Source,
    /// Model identifier for AI alternatives, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl Alternative {
    /// Creates an alternative with no model attribution.
    pub fn new(text: impl Into<String>, source: Source) -> Self {
        Self {
            text: text.into(),
            source,
            model: None,
        }
    }
}

/// Where a span sits in the body.
///
/// `start` and `end` are UTF-16 code-unit offsets into the body (see
/// [`crate::offset`]). They are a hint: after the body is edited elsewhere,
/// [`Document::reanchor`](crate::Document::reanchor) finds the span again by
/// searching for `text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    /// Start offset, UTF-16 code units, inclusive.
    pub start: usize,
    /// End offset, UTF-16 code units, exclusive.
    pub end: usize,
    /// The body text covered by the span (the active alternative).
    pub text: String,
}

/// A contiguous piece of body text carrying alternatives and/or a ghost flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Span {
    /// Identifier, unique within the document.
    pub id: String,
    /// Granularity.
    pub kind: SpanKind,
    /// Location hint plus the text to search for.
    pub anchor: Anchor,
    /// Index into `alts` of the alternative shown in the body.
    pub active: usize,
    /// Ordered alternatives; index 0 is the original.
    pub alts: Vec<Alternative>,
    /// Whether the span is ghosted (dimmed, counted, not exported).
    #[serde(default)]
    pub ghost: bool,
}

impl Span {
    /// The alternative currently shown in the body.
    pub fn active_alternative(&self) -> &Alternative {
        &self.alts[self.active]
    }

    /// True when the span carries no state worth keeping: only its original
    /// alternative and no ghost flag. Such spans are removed by the operations
    /// that can produce them (R-2.7).
    pub fn is_inert(&self) -> bool {
        self.alts.len() <= 1 && !self.ghost
    }

    /// Structural validation used when parsing a block. Returns a
    /// human-readable reason on failure.
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() {
            return Err("span id is empty".into());
        }
        let Some(first) = self.alts.first() else {
            return Err("span has no alternatives".into());
        };
        if first.source != Source::Original {
            return Err("alternative 0 must have source \"original\"".into());
        }
        if let Some(index) = self.alts[1..]
            .iter()
            .position(|alt| alt.source == Source::Original)
        {
            return Err(format!(
                "alternative {} has source \"original\"; only index 0 may",
                index + 1
            ));
        }
        if self.alts.iter().any(|alt| alt.text.is_empty()) {
            return Err("an alternative has empty text".into());
        }
        if self.active >= self.alts.len() {
            return Err(format!(
                "active index {} out of range for {} alternatives",
                self.active,
                self.alts.len()
            ));
        }
        if self.anchor.text != self.alts[self.active].text {
            return Err("anchor text does not match the active alternative".into());
        }
        if self.anchor.end < self.anchor.start
            || self.anchor.end - self.anchor.start != utf16_len(&self.anchor.text)
        {
            return Err("anchor start/end do not match the UTF-16 length of its text".into());
        }
        Ok(())
    }
}

/// Document-level annotation state: the spans and the overflow stash.
///
/// Serialised as schema version [`SCHEMA_VERSION`] in the trailing block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Annotations {
    /// Spans in insertion order. Non-overlapping.
    pub spans: Vec<Span>,
    /// Free text stashed out of the body (R-6). Never exported.
    pub overflow: String,
}

impl Annotations {
    /// True when there is nothing to persist.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty() && self.overflow.is_empty()
    }
}

/// On-disk shape of schema version 1. Field order is the serialisation order,
/// which keeps the output byte-stable.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WireV1 {
    pub version: u64,
    pub spans: Vec<Span>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub overflow: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(alts: Vec<Alternative>, active: usize) -> Span {
        let text = alts[active].text.clone();
        Span {
            id: "s1".into(),
            kind: SpanKind::Word,
            anchor: Anchor {
                start: 0,
                end: utf16_len(&text),
                text,
            },
            active,
            alts,
            ghost: false,
        }
    }

    #[test]
    fn valid_span_passes() {
        let s = span(
            vec![
                Alternative::new("tension", Source::Original),
                Alternative::new("pressure", Source::Human),
            ],
            1,
        );
        assert_eq!(s.validate(), Ok(()));
        assert_eq!(s.active_alternative().text, "pressure");
        assert!(!s.is_inert());
    }

    #[test]
    fn first_alternative_must_be_original() {
        let s = span(vec![Alternative::new("x", Source::Human)], 0);
        assert!(s.validate().unwrap_err().contains("original"));
    }

    #[test]
    fn second_original_is_rejected() {
        let s = span(
            vec![
                Alternative::new("x", Source::Original),
                Alternative::new("y", Source::Original),
            ],
            0,
        );
        assert!(s.validate().unwrap_err().contains("alternative 1"));
    }

    #[test]
    fn active_out_of_range_is_rejected() {
        let mut s = span(vec![Alternative::new("x", Source::Original)], 0);
        s.active = 3;
        assert!(s.validate().unwrap_err().contains("out of range"));
    }

    #[test]
    fn anchor_must_match_active_text_and_length() {
        let mut s = span(vec![Alternative::new("x", Source::Original)], 0);
        s.anchor.text = "y".into();
        assert!(s.validate().unwrap_err().contains("does not match"));

        let mut s = span(vec![Alternative::new("𝄞", Source::Original)], 0);
        assert_eq!(s.anchor.end, 2, "astral char is two UTF-16 units");
        s.anchor.end = 1;
        assert!(s.validate().unwrap_err().contains("UTF-16 length"));
    }

    #[test]
    fn inert_means_only_original_and_not_ghosted() {
        let mut s = span(vec![Alternative::new("x", Source::Original)], 0);
        assert!(s.is_inert());
        s.ghost = true;
        assert!(!s.is_inert());
    }

    #[test]
    fn kinds_and_sources_serialise_lowercase() {
        assert_eq!(
            serde_json::to_string(&SpanKind::Paragraph).unwrap(),
            "\"paragraph\""
        );
        assert_eq!(serde_json::to_string(&Source::Ai).unwrap(), "\"ai\"");
    }
}
