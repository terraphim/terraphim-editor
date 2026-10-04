//! [`Document`]: the body plus its annotations, and the operations on them.
//!
//! All offsets taken and returned are UTF-16 code units into the body. Spans
//! never overlap; operations that would create an overlap are refused.
//! Operations that need a span's or ghost's location first check that the body
//! still holds the anchor text at the stored offsets and return
//! [`EditError::StaleAnchor`] if it does not (call
//! [`Document::reanchor`] first in that case).

use thiserror::Error;

use crate::article::{article_for, preceding_article, respell};
use crate::model::{Alternative, Anchor, Annotations, Ghost, Source, Span, SpanKind};
use crate::offset::{utf16_len, utf16_to_byte};

/// A Markdown body with its annotations.
///
/// # Ghosts
///
/// Ghosts are a layer independent of spans (decision 2026-10-04). A ghost may
/// cover or partially overlap any spans; ghosts never overlap each other.
///
/// * **Merge.** [`Document::ghost`] over a range that overlaps *or touches* an
///   existing ghost merges them into one ghost covering the union. The merged
///   ghost keeps the id of the involved ghost that starts first in the body.
/// * **Revive.** [`Document::revive`] removes a range from the ghost layer. A
///   ghost strictly containing the range is split in two: the left part keeps
///   the id, the right part gets a fresh one.
/// * **Elastic.** Every body change ([`Document::apply_edit`],
///   [`Document::set_active`] / [`Document::cycle_active`] swaps, and the
///   `a`/`an` fix-up) moves ghosts with the text. For a ghost `gs..ge` and an
///   edit replacing `s..e`:
///   * an edit wholly before the ghost (`e <= gs`) shifts it;
///   * an edit wholly after it (`s >= ge`) leaves it alone;
///   * an **insertion exactly at either boundary is outside** the ghost (at
///     `gs` it shifts the ghost, at `ge` it is not ghosted), the same rule
///     spans use; an insertion strictly inside grows the ghost;
///   * a replacement lying within the ghost (`gs <= s && e <= ge`, including
///     an exact match of the ghost's range) resizes it and the replacement is
///     ghosted; if the ghost becomes empty it is removed;
///   * an edit that covers the whole ghost and more deletes all of its text,
///     so the ghost is removed;
///   * an edit that partially overlaps the ghost trims it to the surviving
///     ghosted text; the replacement text is not ghosted.
///
///   A resized ghost's `anchor.text` is refreshed from the body. Edits never
///   merge ghosts that come to touch; touching ghosts are valid state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    /// Markdown text without the annotation block. Holds the active
    /// alternative of every span.
    pub body: String,
    /// Spans, ghosts and overflow.
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
    /// The body no longer holds the span's or ghost's anchor text at its
    /// offsets.
    #[error("{0:?} is stale; re-anchor the document first")]
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
    /// A fresh span is inert until it gets an alternative.
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
            .find(|s| s.anchor.overlaps(start, end))
        {
            return Err(EditError::Overlap(other.id.clone()));
        }
        let text = self.body[byte_start..byte_end].to_string();
        let id = self.next_id('s');
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
    /// anchors are shifted to stay correct and ghosts follow the edit (see
    /// [`Document`]), so a ghost over a sentence resizes when a
    /// word inside it is swapped. Atomic: if the span, or a ghost that the
    /// swap or the article fix-up would touch, is stale, it returns
    /// [`EditError::StaleAnchor`] and changes nothing.
    pub fn set_active(&mut self, id: &str, index: usize) -> Result<(), EditError> {
        let span_index = self.span_index(id)?;
        if index >= self.annotations.spans[span_index].alts.len() {
            return Err(EditError::InvalidIndex {
                id: id.to_string(),
                index,
            });
        }
        let (byte_start, byte_end) = self.locate(span_index)?;
        let anchor = &self.annotations.spans[span_index].anchor;
        self.check_ghosts_for_edit(anchor.start, anchor.end)?;
        let new_text = self.annotations.spans[span_index].alts[index].text.clone();
        // Preflight the a/an fix-up so a stale ghost over the article fails
        // the whole call before anything changes, instead of leaving the
        // swap done and the article wrong.
        self.check_article_edit(span_index, byte_start, &new_text)?;

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
    /// the span is removed (R-2.7); any ghost over its text is unaffected.
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
    /// author currently sees. The span is then removed and its text stays as
    /// plain text (R-2.7). Ghosting is independent: a ghost over the text
    /// stays, so the old rule that a ghosted span survives losing its
    /// alternatives no longer exists (decision 2026-10-04).
    pub fn clear_alternatives(&mut self, id: &str) -> Result<SpanFate, EditError> {
        let span_index = self.span_index(id)?;
        self.locate(span_index)?;
        let span = &mut self.annotations.spans[span_index];
        let shown = span.active_alternative().text.clone();
        span.alts = vec![Alternative::new(shown, Source::Original)];
        span.active = 0;
        Ok(self.prune(span_index))
    }

    /// The ghost covering the UTF-16 offset (`start <= offset < end`), if any.
    pub fn ghost_at(&self, offset: usize) -> Option<&Ghost> {
        self.annotations
            .ghosts
            .iter()
            .find(|g| g.anchor.start <= offset && offset < g.anchor.end)
    }

    /// Ghosts `start..end` (R-5.1) and returns the id of the ghost now
    /// covering it.
    ///
    /// The range may cover or partially overlap any spans. A ghost that
    /// overlaps or touches the range is merged into it; the result keeps the
    /// id of the merged ghost that starts first, or a fresh `g<n>` id when
    /// nothing was merged.
    pub fn ghost(&mut self, start: usize, end: usize) -> Result<String, EditError> {
        let (byte_start, byte_end) = self.byte_range(start, end)?;
        if byte_start == byte_end {
            return Err(EditError::InvalidRange { start, end });
        }
        let (merged, kept): (Vec<Ghost>, Vec<Ghost>) = self
            .annotations
            .ghosts
            .iter()
            .cloned()
            .partition(|g| g.anchor.start <= end && g.anchor.end >= start);
        if let Some(stale) = merged.iter().find(|g| self.located(&g.anchor).is_none()) {
            return Err(EditError::StaleAnchor(stale.id.clone()));
        }
        let new_start = merged
            .iter()
            .map(|g| g.anchor.start)
            .fold(start, usize::min);
        let new_end = merged.iter().map(|g| g.anchor.end).fold(end, usize::max);
        let id = match merged.iter().min_by_key(|g| g.anchor.start) {
            Some(first) => first.id.clone(),
            None => self.next_id('g'),
        };
        self.annotations.ghosts = kept;
        let ghost = self.ghost_over(id.clone(), new_start, new_end);
        self.annotations.ghosts.push(ghost);
        self.sort_ghosts();
        Ok(id)
    }

    /// Ghosts exactly the text of span `id` (convenience for [`Document::ghost`]).
    pub fn ghost_span(&mut self, id: &str) -> Result<String, EditError> {
        let span_index = self.span_index(id)?;
        self.locate(span_index)?;
        let anchor = &self.annotations.spans[span_index].anchor;
        self.ghost(anchor.start, anchor.end)
    }

    /// Revives `start..end` (R-5.2): removes it from the ghost layer. A ghost
    /// lying inside the range is removed, one partly inside is trimmed, and
    /// one strictly containing the range is split in two (the left part keeps
    /// the id, the right part gets a fresh one). Returns whether any ghost
    /// changed.
    pub fn revive(&mut self, start: usize, end: usize) -> Result<bool, EditError> {
        let (byte_start, byte_end) = self.byte_range(start, end)?;
        if byte_start == byte_end {
            return Err(EditError::InvalidRange { start, end });
        }
        let affected: Vec<&Ghost> = self
            .annotations
            .ghosts
            .iter()
            .filter(|g| g.anchor.overlaps(start, end))
            .collect();
        if let Some(stale) = affected.iter().find(|g| self.located(&g.anchor).is_none()) {
            return Err(EditError::StaleAnchor(stale.id.clone()));
        }
        if affected.is_empty() {
            return Ok(false);
        }
        let mut pieces: Vec<(Option<String>, usize, usize)> = Vec::new();
        let mut kept = Vec::new();
        for ghost in std::mem::take(&mut self.annotations.ghosts) {
            let (gs, ge) = (ghost.anchor.start, ghost.anchor.end);
            if !ghost.anchor.overlaps(start, end) {
                kept.push(ghost);
                continue;
            }
            let mut id = Some(ghost.id);
            if gs < start {
                pieces.push((id.take(), gs, start));
            }
            if end < ge {
                pieces.push((id.take(), end, ge));
            }
        }
        self.annotations.ghosts = kept;
        for (id, piece_start, piece_end) in pieces {
            let id = id.unwrap_or_else(|| self.next_id('g'));
            let ghost = self.ghost_over(id, piece_start, piece_end);
            self.annotations.ghosts.push(ghost);
        }
        self.sort_ghosts();
        Ok(true)
    }

    /// Applies a text edit made in the editor: replaces `start..end` with
    /// `replacement` and shifts anchors exactly.
    ///
    /// Spans after the edit move by the length change; spans before it are
    /// untouched. An insertion exactly at a span's start counts as before it,
    /// one at its end as after it. A span whose text the edit changes is
    /// detached: it is removed from the document and returned, so the caller
    /// can decide what to do with it. Nothing is dropped silently.
    ///
    /// Ghosts are elastic rather than detached: they resize, shift or vanish
    /// with the text as described on [`Document`]. A ghost the
    /// edit touches must still match the body, or
    /// [`EditError::StaleAnchor`] is returned and nothing changes.
    pub fn apply_edit(
        &mut self,
        start: usize,
        end: usize,
        replacement: &str,
    ) -> Result<Vec<Span>, EditError> {
        let (byte_start, byte_end) = self.byte_range(start, end)?;
        self.check_ghosts_for_edit(start, end)?;
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

    /// Plain Markdown for export (R-9.3): the text of every ghost is dropped
    /// (including any spans inside it), overflow and the annotation block are
    /// omitted.
    ///
    /// Active alternatives need no resolving: the body already holds them.
    /// Whitespace around a dropped range is tidied minimally: a space left
    /// doubled or before punctuation is removed, a space or blank lines left
    /// at the start of a line or paragraph are removed. A stale ghost (one the
    /// body no longer matches) is skipped rather than cutting the wrong text.
    pub fn export(&self) -> String {
        let mut ranges: Vec<(usize, usize)> = self
            .annotations
            .ghosts
            .iter()
            .filter_map(|g| self.located(&g.anchor))
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

    /// All ids in use; spans and ghosts share one namespace.
    fn ids(&self) -> impl Iterator<Item = &str> {
        let spans = self.annotations.spans.iter().map(|s| s.id.as_str());
        let ghosts = self.annotations.ghosts.iter().map(|g| g.id.as_str());
        spans.chain(ghosts)
    }

    /// A fresh id `<prefix><n>`, one past the highest number used with that
    /// prefix across spans and ghosts, so it is unique in both.
    fn next_id(&self, prefix: char) -> String {
        let max = self
            .ids()
            .filter_map(|id| id.strip_prefix(prefix)?.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("{prefix}{}", max + 1)
    }

    /// A ghost over `start..end` whose text is read from the body. Callers
    /// pass a range on character boundaries.
    fn ghost_over(&self, id: String, start: usize, end: usize) -> Ghost {
        let (byte_start, byte_end) = self
            .byte_range(start, end)
            .expect("ghost range lies on character boundaries");
        Ghost {
            id,
            anchor: Anchor {
                start,
                end,
                text: self.body[byte_start..byte_end].to_string(),
            },
        }
    }

    fn sort_ghosts(&mut self) {
        self.annotations.ghosts.sort_by_key(|g| g.anchor.start);
    }

    /// Refuses an edit of `start..end` that would resize or remove a ghost
    /// whose anchor no longer matches the body, so a stale ghost is never
    /// silently re-pointed at different text.
    fn check_ghosts_for_edit(&self, start: usize, end: usize) -> Result<(), EditError> {
        match self.annotations.ghosts.iter().find(|g| {
            edit_touches_ghost(g.anchor.start, g.anchor.end, start, end)
                && self.located(&g.anchor).is_none()
        }) {
            Some(stale) => Err(EditError::StaleAnchor(stale.id.clone())),
            None => Ok(()),
        }
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

    /// Byte range of an anchor that still matches the body.
    fn located(&self, anchor: &Anchor) -> Option<(usize, usize)> {
        let (start, end) = self.byte_range(anchor.start, anchor.end).ok()?;
        (self.body[start..end] == anchor.text).then_some((start, end))
    }

    fn locate(&self, span_index: usize) -> Result<(usize, usize), EditError> {
        let span = &self.annotations.spans[span_index];
        self.located(&span.anchor)
            .ok_or_else(|| EditError::StaleAnchor(span.id.clone()))
    }

    /// Replaces a byte range of the body, shifts every span (other than
    /// `skip`) that starts at or after the end of the range, and moves ghosts
    /// with the edit ([`Document`] docs, "Elastic"). Callers ensure no other span
    /// overlaps the range and that touched ghosts are located.
    fn splice(
        &mut self,
        byte_start: usize,
        byte_end: usize,
        replacement: &str,
        skip: Option<usize>,
    ) {
        let edit_start = utf16_len(&self.body[..byte_start]);
        let removed_units = utf16_len(&self.body[byte_start..byte_end]);
        let edit_end = edit_start + removed_units;
        let added_units = utf16_len(replacement);
        self.body.replace_range(byte_start..byte_end, replacement);
        for (i, span) in self.annotations.spans.iter_mut().enumerate() {
            if Some(i) == skip || span.anchor.start < edit_end {
                continue;
            }
            span.anchor.start = span.anchor.start + added_units - removed_units;
            span.anchor.end = span.anchor.end + added_units - removed_units;
        }
        for mut ghost in std::mem::take(&mut self.annotations.ghosts) {
            let (gs, ge) = (ghost.anchor.start, ghost.anchor.end);
            match ghost_after_edit(gs, ge, edit_start, edit_end, added_units) {
                GhostAfterEdit::Removed => continue,
                GhostAfterEdit::Moved(start, end) => {
                    ghost.anchor.start = start;
                    ghost.anchor.end = end;
                }
                GhostAfterEdit::Resized(start, end) => {
                    ghost = self.ghost_over(ghost.id, start, end);
                }
            }
            self.annotations.ghosts.push(ghost);
        }
    }

    /// Errors with [`EditError::StaleAnchor`] when the a/an fix-up that
    /// swapping in `new_text` would make touches a stale ghost. The article
    /// precedes the span, so the pre-swap body locates it exactly.
    fn check_article_edit(
        &self,
        span_index: usize,
        span_byte_start: usize,
        new_text: &str,
    ) -> Result<(), EditError> {
        let Some(wanted) = article_for(new_text) else {
            return Ok(());
        };
        let Some((art_start, art_end)) = preceding_article(&self.body, span_byte_start) else {
            return Ok(());
        };
        let existing = &self.body[art_start..art_end];
        if respell(existing, wanted) == existing {
            return Ok(());
        }
        let art_u16_start = utf16_len(&self.body[..art_start]);
        let art_u16_end = art_u16_start + utf16_len(existing);
        let blocked_by_span = self
            .annotations
            .spans
            .iter()
            .enumerate()
            .any(|(i, s)| i != span_index && s.anchor.overlaps(art_u16_start, art_u16_end));
        if blocked_by_span {
            return Ok(());
        }
        self.check_ghosts_for_edit(art_u16_start, art_u16_end)
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
        // The article must be plain text, not part of another span. Ghosts do
        // not block it (they follow the respelling), unless one it touches is
        // stale.
        if self
            .annotations
            .spans
            .iter()
            .enumerate()
            .any(|(i, s)| i != span_index && s.anchor.overlaps(art_u16_start, art_u16_end))
            || self
                .check_ghosts_for_edit(art_u16_start, art_u16_end)
                .is_err()
        {
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

/// Where a ghost ends up after an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GhostAfterEdit {
    /// Same text, possibly shifted.
    Moved(usize, usize),
    /// Text changed; re-read it from the body.
    Resized(usize, usize),
    /// All of its text is gone.
    Removed,
}

/// True when replacing `s..e` changes the text of ghost `gs..ge` (it lies
/// neither wholly before nor wholly after it). Insertions at a boundary are
/// outside.
fn edit_touches_ghost(gs: usize, ge: usize, s: usize, e: usize) -> bool {
    if s == e {
        gs < s && s < ge
    } else {
        s < ge && e > gs
    }
}

/// The elastic ghost rule ([`Document`] docs): ghost `gs..ge`, edit replacing
/// `s..e` with `added` UTF-16 units.
fn ghost_after_edit(gs: usize, ge: usize, s: usize, e: usize, added: usize) -> GhostAfterEdit {
    let removed = e - s;
    if !edit_touches_ghost(gs, ge, s, e) {
        return if e <= gs {
            GhostAfterEdit::Moved(gs + added - removed, ge + added - removed)
        } else {
            GhostAfterEdit::Moved(gs, ge)
        };
    }
    if gs <= s && e <= ge {
        let new_end = ge + added - removed;
        return if new_end == gs {
            GhostAfterEdit::Removed
        } else {
            GhostAfterEdit::Resized(gs, new_end)
        };
    }
    if s <= gs && e >= ge {
        return GhostAfterEdit::Removed;
    }
    if s < gs {
        // Head cut: the surviving text e..ge now follows the replacement.
        let start = s + added;
        GhostAfterEdit::Resized(start, start + (ge - e))
    } else {
        // Tail cut: gs..s survives.
        GhostAfterEdit::Resized(gs, s)
    }
}

#[cfg(test)]
mod tests {
    use super::GhostAfterEdit::{Moved, Removed, Resized};
    use super::*;

    #[test]
    fn insertions_at_boundaries_are_outside() {
        assert_eq!(ghost_after_edit(5, 10, 5, 5, 2), Moved(7, 12));
        assert_eq!(ghost_after_edit(5, 10, 10, 10, 2), Moved(5, 10));
        assert_eq!(ghost_after_edit(5, 10, 7, 7, 2), Resized(5, 12));
        assert_eq!(ghost_after_edit(5, 10, 0, 0, 3), Moved(8, 13));
    }

    #[test]
    fn replacements_before_after_and_inside() {
        assert_eq!(
            ghost_after_edit(5, 10, 0, 5, 1),
            Moved(1, 6),
            "touching before"
        );
        assert_eq!(
            ghost_after_edit(5, 10, 10, 12, 0),
            Moved(5, 10),
            "touching after"
        );
        assert_eq!(
            ghost_after_edit(5, 10, 5, 10, 3),
            Resized(5, 8),
            "exact range"
        );
        assert_eq!(ghost_after_edit(5, 10, 6, 8, 5), Resized(5, 13), "inside");
        assert_eq!(ghost_after_edit(5, 10, 5, 10, 0), Removed, "emptied");
        assert_eq!(
            ghost_after_edit(5, 10, 4, 10, 9),
            Removed,
            "covered and more"
        );
        assert_eq!(
            ghost_after_edit(5, 10, 5, 11, 9),
            Removed,
            "covered and more"
        );
    }

    #[test]
    fn partial_overlaps_trim_to_the_surviving_text() {
        // Head cut: 3..7 becomes 4 units; surviving 7..10 now starts at 3 + 4.
        assert_eq!(ghost_after_edit(5, 10, 3, 7, 4), Resized(7, 10));
        // Tail cut: 8..12 replaced; 5..8 survives.
        assert_eq!(ghost_after_edit(5, 10, 8, 12, 1), Resized(5, 8));
    }
}
