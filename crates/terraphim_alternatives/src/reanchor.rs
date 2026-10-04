//! Re-anchoring spans after the body was edited outside the span model
//! (R-9.2), for example when a file is reopened after a hand edit.
//!
//! Each span is found again by searching the body for its anchor text, with
//! the stored `start` as a hint. The rules are applied in order and never
//! guess:
//!
//! 1. An occurrence starting exactly at the hint is claimed.
//! 2. Otherwise, if exactly one unclaimed, non-overlapping occurrence remains
//!    and no other unresolved span has the same text, it is claimed.
//! 3. Otherwise the span is unresolved: [`UnresolvedReason::Missing`] when no
//!    usable occurrence exists, [`UnresolvedReason::Ambiguous`] when several
//!    could match.
//!
//! Known limitation: an edit inside a span changes its text, so the span is
//! reported missing, unless the old text happens to occur exactly once
//! elsewhere, in which case rule 2 attaches it there. Live edits should go
//! through [`Document::apply_edit`], which tracks positions exactly.

use std::collections::HashMap;
use std::rc::Rc;

use crate::document::Document;
use crate::model::Span;
use crate::offset::{byte_to_utf16, utf16_len};

/// Why a span could not be re-anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// The anchor text no longer occurs in the body (or every occurrence is
    /// claimed by another span).
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

/// Outcome of [`Document::reanchor`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReanchorReport {
    /// Ids of spans that were found at a different offset and updated.
    pub moved: Vec<String>,
    /// Spans that could not be placed. They are removed from the document so
    /// its invariants hold, and returned here so nothing is lost.
    pub unresolved: Vec<Unresolved>,
}

impl ReanchorReport {
    /// True when every span was placed.
    pub fn is_clean(&self) -> bool {
        self.unresolved.is_empty()
    }
}

impl Document {
    /// Re-finds every span in the current body. See the [module
    /// docs](crate::reanchor) for the rules.
    pub fn reanchor(&mut self) -> ReanchorReport {
        let spans = std::mem::take(&mut self.annotations.spans);
        let mut by_text: HashMap<&str, Rc<[usize]>> = HashMap::new();
        let occurrences: Vec<Rc<[usize]>> = spans
            .iter()
            .map(|s| {
                by_text
                    .entry(s.anchor.text.as_str())
                    .or_insert_with(|| find_all(&self.body, &s.anchor.text).into())
                    .clone()
            })
            .collect();
        let lengths: Vec<usize> = spans.iter().map(|s| utf16_len(&s.anchor.text)).collect();

        let mut placed: Vec<Option<usize>> = vec![None; spans.len()];
        let mut claimed: Vec<(usize, usize)> = Vec::new();
        let overlaps_claim = |claimed: &[(usize, usize)], start: usize, len: usize| {
            claimed.iter().any(|&(s, e)| start < e && start + len > s)
        };

        // Rule 1: exact hint.
        for (i, span) in spans.iter().enumerate() {
            let hint = span.anchor.start;
            if occurrences[i].contains(&hint) && !overlaps_claim(&claimed, hint, lengths[i]) {
                placed[i] = Some(hint);
                claimed.push((hint, hint + lengths[i]));
            }
        }

        // Rule 2: the single remaining occurrence, if uncontested. A span placed
        // here was the only unresolved one with its text, so counting once
        // after rule 1 is exact.
        let mut unresolved_with_text: HashMap<&str, usize> = HashMap::new();
        for (i, span) in spans.iter().enumerate() {
            if placed[i].is_none() {
                *unresolved_with_text
                    .entry(span.anchor.text.as_str())
                    .or_default() += 1;
            }
        }
        let mut reasons: Vec<Option<UnresolvedReason>> = vec![None; spans.len()];
        for (i, span) in spans.iter().enumerate() {
            if placed[i].is_some() {
                continue;
            }
            let remaining: Vec<usize> = occurrences[i]
                .iter()
                .copied()
                .filter(|&start| !overlaps_claim(&claimed, start, lengths[i]))
                .collect();
            let contested = unresolved_with_text[span.anchor.text.as_str()] > 1;
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

        let mut report = ReanchorReport::default();
        for (i, mut span) in spans.into_iter().enumerate() {
            match placed[i] {
                Some(start) => {
                    if span.anchor.start != start {
                        report.moved.push(span.id.clone());
                    }
                    span.anchor.start = start;
                    span.anchor.end = start + lengths[i];
                    self.annotations.spans.push(span);
                }
                None => report.unresolved.push(Unresolved {
                    span,
                    reason: reasons[i].clone().unwrap_or(UnresolvedReason::Missing),
                }),
            }
        }
        report
    }
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
