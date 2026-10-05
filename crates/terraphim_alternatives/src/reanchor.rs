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
//! Every occurrence of every distinct anchor text is found in one pass over
//! the body by a [`terraphim_automata::CompiledMatcher`] built for that pass
//! (overlapping, case-sensitive, matching inside words), so the cost of
//! searching no longer grows with the number of distinct texts.
//!
//! Known limitation: an edit inside a span or ghost changes its text, so it is
//! reported missing, unless the old text happens to occur exactly once
//! elsewhere, in which case rule 2 attaches it there. Live edits should go
//! through [`Document::apply_edit`], which tracks positions exactly (and keeps
//! ghosts elastic).

use std::collections::HashMap;

use terraphim_automata::{MatchOverlap, MatcherBuilder, MatcherOptions};
use terraphim_types::{NormalizedTerm, NormalizedTermValue};

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
        let ghosts = std::mem::take(&mut self.annotations.ghosts);
        let texts = spans
            .iter()
            .map(|s| s.anchor.text.as_str())
            .chain(ghosts.iter().map(|g| g.anchor.text.as_str()));
        let occurrences = Occurrences::find(&self.body, texts);
        let span_anchors: Vec<&Anchor> = spans.iter().map(|s| &s.anchor).collect();
        let span_placements = place(&span_anchors, &occurrences);
        let ghost_anchors: Vec<&Anchor> = ghosts.iter().map(|g| &g.anchor).collect();
        let ghost_placements = place(&ghost_anchors, &occurrences);

        for (mut span, placement) in spans.into_iter().zip(span_placements) {
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

        for (mut ghost, placement) in ghosts.into_iter().zip(ghost_placements) {
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

/// Every occurrence of each distinct anchor text in the body, found in one
/// pass for the whole re-anchor run.
struct Occurrences<'a> {
    /// Anchor text to its slot in `starts`.
    slot: HashMap<&'a str, usize>,
    /// UTF-16 start offsets of every (possibly overlapping) occurrence of the
    /// text in each slot, in body order.
    starts: Vec<Vec<usize>>,
}

impl<'a> Occurrences<'a> {
    /// Finds every occurrence of every distinct text in `texts`.
    ///
    /// The texts are compiled into one [`terraphim_automata::CompiledMatcher`]
    /// and the body is scanned once. A text the matcher builder rejects (it
    /// refuses blank patterns, but a span or ghost may cover only whitespace)
    /// is searched for by [`occurrences_without_matcher`] instead, so every
    /// text the span model accepts can still be re-anchored.
    fn find(body: &str, texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut slot: HashMap<&'a str, usize> = HashMap::new();
        let mut distinct: Vec<&'a str> = Vec::new();
        for text in texts {
            slot.entry(text).or_insert_with(|| {
                distinct.push(text);
                distinct.len() - 1
            });
        }
        let mut starts = vec![Vec::new(); distinct.len()];

        let options = MatcherOptions::default()
            .with_overlap(MatchOverlap::All)
            .with_word_boundaries(false)
            .with_case_insensitive(false)
            .with_min_pattern_length(1);
        let mut builder = MatcherBuilder::new(options);
        let mut rejected: Vec<usize> = Vec::new();
        for (i, text) in distinct.iter().enumerate() {
            // The term id carries the slot back out of each match. Texts are
            // distinct, so the builder's duplicate check never fires.
            let term = NormalizedTerm::new(i as u64, NormalizedTermValue::default());
            if builder.insert((*text).to_owned(), term).is_err() {
                rejected.push(i);
            }
        }
        let matches = if builder.is_empty() {
            Ok(Vec::new())
        } else {
            builder.build().and_then(|m| m.find_matches(body, true))
        };
        match matches {
            Ok(matches) => {
                // Matches arrive sorted by start byte, so one running count
                // converts every start to UTF-16 in a single walk of the body.
                let (mut byte, mut units) = (0, 0);
                for matched in matches {
                    let Some((start, _)) = matched.pos else {
                        continue;
                    };
                    units += utf16_len(&body[byte..start]);
                    byte = start;
                    debug_assert_eq!(Some(units), byte_to_utf16(body, start));
                    starts[matched.normalized_term.id as usize].push(units);
                }
            }
            // Only automaton size limits can fail a build over validated
            // patterns. Degrade to the per-text search rather than lose spans.
            Err(_) => rejected = (0..distinct.len()).collect(),
        }
        for i in rejected {
            starts[i] = occurrences_without_matcher(body, distinct[i]);
        }
        Self { slot, starts }
    }

    /// Occurrences of `text`, which must be one of the texts passed to
    /// [`Occurrences::find`].
    fn of(&self, text: &str) -> &[usize] {
        &self.starts[self.slot[text]]
    }
}

/// UTF-16 start offsets of every (possibly overlapping) occurrence of
/// `needle` in `haystack`, for the texts the matcher cannot take: blank
/// anchors, which [`MatcherBuilder::insert`] rejects, and every text in the
/// (not expected) case that building the matcher fails.
fn occurrences_without_matcher(haystack: &str, needle: &str) -> Vec<usize> {
    let mut found = Vec::new();
    if needle.is_empty() {
        return found;
    }
    let (mut from, mut byte, mut units) = (0, 0, 0);
    while let Some(pos) = haystack[from..].find(needle) {
        let start = from + pos;
        units += utf16_len(&haystack[byte..start]);
        byte = start;
        found.push(units);
        from = start + haystack[start..].chars().next().map_or(1, char::len_utf8);
    }
    found
}

/// Places each anchor by the module rules, given every occurrence of its
/// text. Items placed by one call never overlap each other.
fn place(
    anchors: &[&Anchor],
    occurrences: &Occurrences<'_>,
) -> Vec<Result<(usize, usize), UnresolvedReason>> {
    let occurrences: Vec<&[usize]> = anchors.iter().map(|a| occurrences.of(&a.text)).collect();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(body: &str, texts: &[&str], text: &str) -> Vec<usize> {
        Occurrences::find(body, texts.iter().copied())
            .of(text)
            .to_vec()
    }

    #[test]
    fn reports_utf16_offsets_and_overlaps() {
        assert_eq!(starts("aaa", &["aa"], "aa"), vec![0, 1]);
        assert_eq!(starts("aaaa", &["aa"], "aa"), vec![0, 1, 2]);
        assert_eq!(starts("𝄞 cat é cat", &["cat"], "cat"), vec![3, 9]);
        assert_eq!(starts("abc", &["x"], "x"), Vec::<usize>::new());
    }

    #[test]
    fn one_pass_serves_every_distinct_text() {
        let texts = ["concatenate", "cat", "cat", "nope"];
        let occ = Occurrences::find("concatenate the cat", texts);
        assert_eq!(occ.starts.len(), 3, "duplicate texts share a slot");
        assert_eq!(occ.of("concatenate"), &[0]);
        assert_eq!(occ.of("cat"), &[3, 16], "inside another text's match too");
        assert_eq!(occ.of("nope"), &[] as &[usize]);
    }

    #[test]
    fn is_case_sensitive() {
        let texts = ["The", "the"];
        assert_eq!(starts("The the THE", &texts, "The"), vec![0]);
        assert_eq!(starts("The the THE", &texts, "the"), vec![4]);
    }

    #[test]
    fn single_character_and_cjk_texts() {
        assert_eq!(starts("a𝄞a", &["a"], "a"), vec![0, 3]);
        assert_eq!(starts("中文中文", &["中文"], "中文"), vec![0, 2]);
        assert_eq!(starts("𝄞中文", &["文"], "文"), vec![3]);
    }

    #[test]
    fn blank_texts_bypass_the_matcher() {
        let texts = ["\n\n", " ", "b"];
        let body = "a\n\n\nb  c";
        assert_eq!(starts(body, &texts, "\n\n"), vec![1, 2]);
        assert_eq!(starts(body, &texts, " "), vec![5, 6]);
        assert_eq!(starts(body, &texts, "b"), vec![4]);
    }

    #[test]
    fn no_texts_and_empty_text_find_nothing() {
        let occ = Occurrences::find("abc", std::iter::empty());
        assert!(occ.starts.is_empty());
        // Empty anchor text cannot be parsed, but `annotations` is public.
        assert_eq!(starts("abc", &[""], ""), Vec::<usize>::new());
        assert_eq!(occurrences_without_matcher("abc", ""), Vec::<usize>::new());
    }

    #[test]
    fn fallback_agrees_with_the_matcher() {
        let body = "𝄞 aaa cat é cat 中文中文 \n\n";
        for text in ["aa", "cat", "中文", "𝄞", "a", "x"] {
            assert_eq!(
                starts(body, &[text], text),
                occurrences_without_matcher(body, text),
                "{text:?}"
            );
        }
    }
}
