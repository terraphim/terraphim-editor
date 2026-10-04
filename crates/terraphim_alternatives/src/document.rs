//! [`Document`]: the body plus its annotations, and the operations on them.
//!
//! All offsets taken and returned are UTF-16 code units into the body. Spans
//! never overlap; operations that would create an overlap are refused.
//! Operations that need a span's location first check that the body still
//! holds the anchor text at the stored offsets and return
//! [`EditError::StaleAnchor`] if it does not (call
//! [`Document::reanchor`] first in that case).

use thiserror::Error;

use crate::article::{article_for, preceding_article, respell};
use crate::model::{Alternative, Anchor, Annotations, Source, Span, SpanKind};
use crate::offset::{utf16_len, utf16_to_byte};

/// A Markdown body with its annotations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    /// Markdown text without the annotation block. Holds the active
    /// alternative of every span.
    pub body: String,
    /// Spans and overflow.
    pub annotations: Annotations,
}

/// Errors from document operations. The document is unchanged when one is
/// returned.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EditError {
    /// No span has this id.
    #[error("no span with id {0:?}")]
    UnknownSpan(String),
    /// The range is reversed, empty where text is required, out of bounds, or
    /// splits a character.
    #[error("invalid UTF-16 range {start}..{end}")]
    InvalidRange {
        /// Start offset.
        start: usize,
        /// End offset.
        end: usize,
    },
    /// The range overlaps an existing span.
    #[error("range overlaps span {0:?}")]
    Overlap(String),
    /// The body no longer holds the span's anchor text at its offsets.
    #[error("span {0:?} is stale; re-anchor the document first")]
    StaleAnchor(String),
    /// The alternative index is out of range.
    #[error("span {id:?} has no alternative {index}")]
    InvalidIndex {
        /// Span id.
        id: String,
        /// Requested index.
        index: usize,
    },
    /// The original alternative cannot be removed or added.
    #[error("the original alternative cannot be added or removed")]
    OriginalImmutable,
    /// Alternative text must not be empty.
    #[error("alternative text is empty")]
    EmptyText,
}

/// Whether an operation left the span in place or removed it (R-2.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanFate {
    /// The span still exists.
    Kept,
    /// The span had nothing left worth keeping and was removed; its text stays
    /// in the body as plain text.
    Removed,
}

/// Word and character counts of the body (ghosted text included).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Whitespace-separated words.
    pub words: usize,
    /// Unicode scalar values.
    pub chars: usize,
}

impl Document {
    /// A document with the given body and no annotations.
    pub fn new(body: impl Into<String>) -> Self {
        Self {
            body: body.into(),
            annotations: Annotations::default(),
        }
    }

    /// Looks up a span by id.
    pub fn span(&self, id: &str) -> Option<&Span> {
        self.annotations.spans.iter().find(|s| s.id == id)
    }

    /// Creates a span over `start..end` whose only alternative is the current
    /// text, marked original. Returns the new id (`s1`, `s2`, ...).
    ///
    /// A fresh span is inert until it gets an alternative or a ghost flag.
    pub fn add_span(
        &mut self,
        kind: SpanKind,
        start: usize,
        end: usize,
    ) -> Result<String, EditError> {
        let (byte_start, byte_end) = self.byte_range(start, end)?;
        if byte_start == byte_end {
            return Err(EditError::InvalidRange { start, end });
        }
        if let Some(other) = self
            .annotations
            .spans
            .iter()
            .find(|s| start < s.anchor.end && end > s.anchor.start)
        {
            return Err(EditError::Overlap(other.id.clone()));
        }
        let text = self.body[byte_start..byte_end].to_string();
        let id = self.next_id();
        self.annotations.spans.push(Span {
            id: id.clone(),
            kind,
            anchor: Anchor {
                start,
                end,
                text: text.clone(),
            },
            active: 0,
            alts: vec![Alternative::new(text, Source::Original)],
            ghost: false,
        });
        Ok(id)
    }

    /// Appends an alternative and returns its index. If an alternative with
    /// the same text already exists, its index is returned and nothing
    /// changes. The body is not modified; use [`Document::set_active`] to show
    /// it.
    pub fn add_alternative(
        &mut self,
        id: &str,
        text: impl Into<String>,
        source: Source,
        model: Option<String>,
    ) -> Result<usize, EditError> {
        let text = text.into();
        if text.is_empty() {
            return Err(EditError::EmptyText);
        }
        if source == Source::Original {
            return Err(EditError::OriginalImmutable);
        }
        let span = self.span_mut(id)?;
        if let Some(index) = span.alts.iter().position(|a| a.text == text) {
            return Ok(index);
        }
        span.alts.push(Alternative {
            text,
            source,
            model,
        });
        Ok(span.alts.len() - 1)
    }

    /// Makes alternative `index` active: replaces the span's body text with
    /// it, then fixes an immediately preceding `a`/`an` (R-2.6). Later
    /// anchors are shifted to stay correct.
    pub fn set_active(&mut self, id: &str, index: usize) -> Result<(), EditError> {
        let span_index = self.span_index(id)?;
        if index >= self.annotations.spans[span_index].alts.len() {
            return Err(EditError::InvalidIndex {
                id: id.to_string(),
                index,
            });
        }
        let (byte_start, byte_end) = self.locate(span_index)?;
        let new_text = self.annotations.spans[span_index].alts[index].text.clone();

        self.splice(byte_start, byte_end, &new_text, Some(span_index));
        let span = &mut self.annotations.spans[span_index];
        span.active = index;
        span.anchor.end = span.anchor.start + utf16_len(&new_text);
        span.anchor.text = new_text.clone();

        self.fix_article(span_index, byte_start, &new_text);
        Ok(())
    }

    /// Moves the active alternative by `step`, wrapping at either end (the
    /// up/down cycling of R-2.4). Returns the new active index.
    pub fn cycle_active(&mut self, id: &str, step: isize) -> Result<usize, EditError> {
        let span = &self.annotations.spans[self.span_index(id)?];
        let len = span.alts.len() as isize;
        let next = (span.active as isize + step).rem_euclid(len) as usize;
        self.set_active(id, next)?;
        Ok(next)
    }

    /// Removes a non-original alternative. Removing the active one first
    /// switches the body back to the original. When only the original remains
    /// and the span is not ghosted, the span is removed (R-2.7).
    pub fn remove_alternative(&mut self, id: &str, index: usize) -> Result<SpanFate, EditError> {
        let span_index = self.span_index(id)?;
        let span = &self.annotations.spans[span_index];
        if index == 0 {
            return Err(EditError::OriginalImmutable);
        }
        if index >= span.alts.len() {
            return Err(EditError::InvalidIndex {
                id: id.to_string(),
                index,
            });
        }
        if span.active == index {
            self.set_active(id, 0)?;
        } else {
            self.locate(span_index)?;
        }
        let span = &mut self.annotations.spans[span_index];
        span.alts.remove(index);
        if span.active > index {
            span.active -= 1;
        }
        Ok(self.prune(span_index))
    }

    /// Removes every non-original alternative while keeping the text the
    /// author currently sees: the active text becomes the span's sole,
    /// original alternative. Unless ghosted, the span is then removed and
    /// its text stays as plain text (R-2.7).
    pub fn clear_alternatives(&mut self, id: &str) -> Result<SpanFate, EditError> {
        let span_index = self.span_index(id)?;
        self.locate(span_index)?;
        let span = &mut self.annotations.spans[span_index];
        let shown = span.active_alternative().text.clone();
        span.alts = vec![Alternative::new(shown, Source::Original)];
        span.active = 0;
        Ok(self.prune(span_index))
    }

    /// Ghosts or revives a span (R-5). Reviving a span that has no
    /// alternatives other than its original removes it.
    pub fn set_ghost(&mut self, id: &str, ghost: bool) -> Result<SpanFate, EditError> {
        let span_index = self.span_index(id)?;
        self.annotations.spans[span_index].ghost = ghost;
        Ok(self.prune(span_index))
    }

    /// Applies a text edit made in the editor: replaces `start..end` with
    /// `replacement` and shifts anchors exactly.
    ///
    /// Spans after the edit move by the length change; spans before it are
    /// untouched. An insertion exactly at a span's start counts as before it,
    /// one at its end as after it. A span whose text the edit changes is
    /// detached: it is removed from the document and returned, so the caller
    /// can decide what to do with it. Nothing is dropped silently.
    pub fn apply_edit(
        &mut self,
        start: usize,
        end: usize,
        replacement: &str,
    ) -> Result<Vec<Span>, EditError> {
        let (byte_start, byte_end) = self.byte_range(start, end)?;
        let touches = |s: &Span| {
            if start == end {
                s.anchor.start < start && start < s.anchor.end
            } else {
                start < s.anchor.end && end > s.anchor.start
            }
        };
        let (detached, kept): (Vec<Span>, Vec<Span>) = std::mem::take(&mut self.annotations.spans)
            .into_iter()
            .partition(touches);
        self.annotations.spans = kept;
        self.splice(byte_start, byte_end, replacement, None);
        Ok(detached)
    }

    /// Plain Markdown for export (R-9.3): ghosted spans are dropped, overflow
    /// and the annotation block are omitted.
    ///
    /// Active alternatives need no resolving: the body already holds them.
    /// Whitespace around a dropped span is tidied minimally: a space left
    /// doubled or before punctuation is removed, a space or blank lines left
    /// at the start of a line or paragraph are removed.
    pub fn export(&self) -> String {
        let mut ranges: Vec<(usize, usize)> = self
            .annotations
            .spans
            .iter()
            .filter(|s| s.ghost)
            .filter_map(|s| self.located_bytes(s))
            .collect();
        ranges.sort_unstable();

        let mut out = String::with_capacity(self.body.len());
        let mut cursor = 0;
        for (start, end) in ranges {
            if start < cursor {
                continue;
            }
            out.push_str(&self.body[cursor..start]);
            cursor = end;
            let rest = &self.body[cursor..];
            let next = rest.chars().next();
            if out.ends_with(' ')
                && next.is_none_or(|c| c == ' ' || c == '\n' || ".,;:!?)".contains(c))
            {
                out.pop();
            }
            if out.is_empty() || out.ends_with('\n') {
                cursor += rest.len() - rest.trim_start_matches(' ').len();
            }
            if out.is_empty() || out.ends_with("\n\n") {
                let rest = &self.body[cursor..];
                cursor += rest.len() - rest.trim_start_matches('\n').len();
            }
        }
        out.push_str(&self.body[cursor..]);
        out
    }

    /// Word and character counts of the body, ghosted text included
    /// (decision 3). The annotation block is never part of the body.
    pub fn counts(&self) -> Counts {
        Counts {
            words: self.body.split_whitespace().count(),
            chars: self.body.chars().count(),
        }
    }

    // ----- internals -------------------------------------------------------

    fn next_id(&self) -> String {
        let max = self
            .annotations
            .spans
            .iter()
            .filter_map(|s| s.id.strip_prefix('s')?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("s{}", max + 1)
    }

    fn span_index(&self, id: &str) -> Result<usize, EditError> {
        self.annotations
            .spans
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| EditError::UnknownSpan(id.to_string()))
    }

    fn span_mut(&mut self, id: &str) -> Result<&mut Span, EditError> {
        let index = self.span_index(id)?;
        Ok(&mut self.annotations.spans[index])
    }

    fn byte_range(&self, start: usize, end: usize) -> Result<(usize, usize), EditError> {
        let invalid = EditError::InvalidRange { start, end };
        if start > end {
            return Err(invalid);
        }
        let byte_start = utf16_to_byte(&self.body, start).ok_or(invalid.clone())?;
        let byte_end = utf16_to_byte(&self.body, end).ok_or(invalid)?;
        Ok((byte_start, byte_end))
    }

    /// Byte range of a span whose anchor still matches the body.
    fn located_bytes(&self, span: &Span) -> Option<(usize, usize)> {
        let (start, end) = self.byte_range(span.anchor.start, span.anchor.end).ok()?;
        (self.body[start..end] == span.anchor.text).then_some((start, end))
    }

    fn locate(&self, span_index: usize) -> Result<(usize, usize), EditError> {
        let span = &self.annotations.spans[span_index];
        self.located_bytes(span)
            .ok_or_else(|| EditError::StaleAnchor(span.id.clone()))
    }

    /// Replaces a byte range of the body and shifts every span (other than
    /// `skip`) that starts at or after the end of the range. Callers ensure no
    /// other span overlaps the range.
    fn splice(
        &mut self,
        byte_start: usize,
        byte_end: usize,
        replacement: &str,
        skip: Option<usize>,
    ) {
        let removed_units = utf16_len(&self.body[byte_start..byte_end]);
        let edit_end = utf16_len(&self.body[..byte_end]);
        let added_units = utf16_len(replacement);
        self.body.replace_range(byte_start..byte_end, replacement);
        for (i, span) in self.annotations.spans.iter_mut().enumerate() {
            if Some(i) == skip || span.anchor.start < edit_end {
                continue;
            }
            span.anchor.start = span.anchor.start + added_units - removed_units;
            span.anchor.end = span.anchor.end + added_units - removed_units;
        }
    }

    fn fix_article(&mut self, span_index: usize, span_byte_start: usize, new_text: &str) {
        let Some(wanted) = article_for(new_text) else {
            return;
        };
        let Some((art_start, art_end)) = preceding_article(&self.body, span_byte_start) else {
            return;
        };
        let art_u16_start = utf16_len(&self.body[..art_start]);
        let art_u16_end = art_u16_start + utf16_len(&self.body[art_start..art_end]);
        // The article must be plain text, not part of another span.
        if self.annotations.spans.iter().enumerate().any(|(i, s)| {
            i != span_index && art_u16_start < s.anchor.end && art_u16_end > s.anchor.start
        }) {
            return;
        }
        let respelt = respell(&self.body[art_start..art_end], wanted);
        if respelt != &self.body[art_start..art_end] {
            self.splice(art_start, art_end, respelt, None);
        }
    }

    /// Removes the span if it has become inert.
    fn prune(&mut self, span_index: usize) -> SpanFate {
        if self.annotations.spans[span_index].is_inert() {
            self.annotations.spans.remove(span_index);
            SpanFate::Removed
        } else {
            SpanFate::Kept
        }
    }
}
