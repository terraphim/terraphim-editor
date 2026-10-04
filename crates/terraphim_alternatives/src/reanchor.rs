//! Re-anchoring spans and ghosts after the body was edited outside the span
//! model (R-9.2), for example when a file is reopened after a hand edit.
//!
//! Each span or ghost is found again by searching the body for its anchor
//! text, with the stored `start` as a hint. Spans and ghosts are placed
//! independently (a ghost may legitimately overlap spans), but within each
//! collection no two items may claim overlapping text. The rules are applied
//! in order and never guess:
//!
//! 1. An occurrence starting exactly at the hint is claimed.
//! 2. Otherwise, if exactly one unclaimed, non-overlapping occurrence remains
//!    and no other unresolved item of the same collection has the same text,
//!    it is claimed.
//! 3. Otherwise the item is unresolved: [`UnresolvedReason::Missing`] when no
//!    usable occurrence exists, [`UnresolvedReason::Ambiguous`] when several
//!    could match.
//!
//! Known limitation: an edit inside a span or ghost changes its text, so it is
//! reported missing, unless the old text happens to occur exactly once
//! elsewhere, in which case rule 2 attaches it there. Live edits should go
//! through [`Document::apply_edit`], which tracks positions exactly (and keeps
//! ghosts elastic).

use std::collections::HashMap;
use std::rc::Rc;

use crate::document::Document;
use crate::model::{Anchor, Ghost, Span};
use crate::offset::{byte_to_utf16, utf16_len};

/// Why a span or ghost could not be re-anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// The anchor text no longer occurs in the body (or every occurrence is
    /// claimed by another item of the same collection).
    Missing,
    /// Several occurrences could match. Candidate start offsets (UTF-16) are
    /// listed in body order.
    Ambiguous {
        /// UTF-16 start offsets of the possible matches.
        candidates: Vec<usize>,
    },
}

/// A span taken out of the document because it could not be re-anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    /// The span exactly as it was, with its stale anchor.
    pub span: Span,
    /// Why it could not be placed.
    pub reason: UnresolvedReason,
}

/// A ghost taken out of the document because it could not be re-anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedGhost {
    /// The ghost exactly as it was, with its stale anchor.
    pub ghost: Ghost,
    /// Why it could not be placed.
    pub reason: UnresolvedReason,
}

/// Outcome of [`Document::reanchor`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReanchorReport {
    /// Ids of spans and ghosts that were found at a different offset and
    /// updated (spans first, then ghosts).
    pub moved: Vec<String>,
    /// Spans that could not be placed. They are removed from the document so
    /// its invariants hold, and returned here so nothing is lost.
    pub unresolved: Vec<Unresolved>,
    /// Ghosts that could not be placed, removed and returned likewise.
    pub unresolved_ghosts: Vec<UnresolvedGhost>,
}

impl ReanchorReport {
    /// True when every span and ghost was placed.
    pub fn is_clean(&self) -> bool {
        self.unresolved.is_empty() && self.unresolved_ghosts.is_empty()
    }
}

impl Document {
    /// Re-finds every span and ghost in the current body. See the [module
    /// docs](crate::reanchor) for the rules.
    pub fn reanchor(&mut self) -> ReanchorReport {
        let mut report = ReanchorReport::default();

        let spans = std::mem::take(&mut self.annotations.spans);
        let anchors: Vec<&Anchor> = spans.iter().map(|s| &s.anchor).collect();
        let placements = place(&self.body, &anchors);
        for (mut span, placement) in spans.into_iter().zip(placements) {
            match placement {
                Ok((start, end)) => {
                    if span.anchor.start != start {
                        report.moved.push(span.id.clone());
                    }
                    (span.anchor.start, span.anchor.end) = (start, end);
                    self.annotations.spans.push(span);
                }
                Err(reason) => report.unresolved.push(Unresolved { span, reason }),
            }
        }

        let ghosts = std::mem::take(&mut self.annotations.ghosts);
        let anchors: Vec<&Anchor> = ghosts.iter().map(|g| &g.anchor).collect();
        let placements = place(&self.body, &anchors);
        for (mut ghost, placement) in ghosts.into_iter().zip(placements) {
            match placement {
                Ok((start, end)) => {
                    if ghost.anchor.start != start {
                        report.moved.push(ghost.id.clone());
                    }
                    (ghost.anchor.start, ghost.anchor.end) = (start, end);
                    self.annotations.ghosts.push(ghost);
                }
                Err(reason) => report
                    .unresolved_ghosts
                    .push(UnresolvedGhost { ghost, reason }),
            }
        }
        self.annotations.ghosts.sort_by_key(|g| g.anchor.start);
        report
    }
}

/// Places each anchor in `body` by the module rules. Items placed by one call
/// never overlap each other.
fn place(body: &str, anchors: &[&Anchor]) -> Vec<Result<(usize, usize), UnresolvedReason>> {
    let mut by_text: HashMap<&str, Rc<[usize]>> = HashMap::new();
    let occurrences: Vec<Rc<[usize]>> = anchors
        .iter()
        .map(|a| {
            by_text
                .entry(a.text.as_str())
                .or_insert_with(|| find_all(body, &a.text).into())
                .clone()
        })
        .collect();
    let lengths: Vec<usize> = anchors.iter().map(|a| utf16_len(&a.text)).collect();

    let mut placed: Vec<Option<usize>> = vec![None; anchors.len()];
    let mut claimed: Vec<(usize, usize)> = Vec::new();
    let overlaps_claim = |claimed: &[(usize, usize)], start: usize, len: usize| {
        claimed.iter().any(|&(s, e)| start < e && start + len > s)
    };

    // Rule 1: exact hint.
    for (i, anchor) in anchors.iter().enumerate() {
        let hint = anchor.start;
        if occurrences[i].contains(&hint) && !overlaps_claim(&claimed, hint, lengths[i]) {
            placed[i] = Some(hint);
            claimed.push((hint, hint + lengths[i]));
        }
    }

    // Rule 2: the single remaining occurrence, if uncontested. An item placed
    // here was the only unresolved one with its text, so counting once after
    // rule 1 is exact.
    let mut unresolved_with_text: HashMap<&str, usize> = HashMap::new();
    for (i, anchor) in anchors.iter().enumerate() {
        if placed[i].is_none() {
            *unresolved_with_text
                .entry(anchor.text.as_str())
                .or_default() += 1;
        }
    }
    let mut reasons: Vec<Option<UnresolvedReason>> = vec![None; anchors.len()];
    for (i, anchor) in anchors.iter().enumerate() {
        if placed[i].is_some() {
            continue;
        }
        let remaining: Vec<usize> = occurrences[i]
            .iter()
            .copied()
            .filter(|&start| !overlaps_claim(&claimed, start, lengths[i]))
            .collect();
        let contested = unresolved_with_text[anchor.text.as_str()] > 1;
        match remaining.as_slice() {
            [] => reasons[i] = Some(UnresolvedReason::Missing),
            [only] if !contested => {
                placed[i] = Some(*only);
                claimed.push((*only, *only + lengths[i]));
            }
            _ => {
                reasons[i] = Some(UnresolvedReason::Ambiguous {
                    candidates: remaining,
                })
            }
        }
    }

    placed
        .into_iter()
        .zip(reasons)
        .zip(lengths)
        .map(|((start, reason), len)| match start {
            Some(start) => Ok((start, start + len)),
            None => Err(reason.unwrap_or(UnresolvedReason::Missing)),
        })
        .collect()
}

/// UTF-16 start offsets of every (possibly overlapping) occurrence of
/// `needle` in `haystack`.
fn find_all(haystack: &str, needle: &str) -> Vec<usize> {
    if needle.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    let mut from = 0;
    let mut units_before = 0;
    let mut last_byte = 0;
    while let Some(pos) = haystack[from..].find(needle) {
        let byte = from + pos;
        units_before += utf16_len(&haystack[last_byte..byte]);
        last_byte = byte;
        debug_assert_eq!(Some(units_before), byte_to_utf16(haystack, byte));
        found.push(units_before);
        let step = haystack[byte..].chars().next().map_or(1, char::len_utf8);
        from = byte + step;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_all_reports_utf16_offsets_and_overlaps() {
        assert_eq!(find_all("aaa", "aa"), vec![0, 1]);
        assert_eq!(find_all("𝄞 cat é cat", "cat"), vec![3, 9]);
        assert_eq!(find_all("abc", ""), Vec::<usize>::new());
        assert_eq!(find_all("abc", "x"), Vec::<usize>::new());
    }
}
