//! Re-anchoring spans and ghosts after the body was edited outside the span
//! model (R-9.2), for example when a file is reopened after a hand edit.
//!
//! Each span or ghost is found again by searching the body for its anchor
//! text, with the stored `start` as a hint and the stored context
//! ([`Anchor::before`], [`Anchor::after`]) as evidence. Spans and ghosts are
//! placed independently (a ghost may legitimately overlap spans), but within
//! each collection no two items may claim overlapping text. The rules are
//! applied in order and never guess:
//!
//! 1. An occurrence starting exactly at the hint is claimed, whatever its
//!    context.
//! 2. **Anchors with context** (decision 2026-10-05: context re-anchoring).
//!    Every occurrence not overlapping a rule-1 claim is a candidate and is
//!    scored against the stored context. For each side, `m` is how many
//!    UTF-16 units of the stored context agree with the body next to the
//!    candidate (common suffix of `before` and the text preceding it, common
//!    prefix of `after` and the text following it), and `L` is the stored
//!    length. A side **agrees** when `L > 0` and
//!    `m >= max(ceil(L / 2), CONTEXT_UNITS / 4)` (at least half of the stored
//!    context, and at least 8 units), or when `m == L` and the body on that
//!    side is exactly the stored context (the candidate is as far from the
//!    same document edge as the anchor was; with `L == 0`, it touches that
//!    edge). A candidate **qualifies** when at least one side agrees. Its
//!    score is the number of agreeing sides, then the total `m` of both
//!    sides. The single best qualifying candidate is claimed. If no candidate
//!    qualifies the item is [`UnresolvedReason::Missing`]; if several tie for
//!    best it is [`UnresolvedReason::Ambiguous`] with the tied candidates;
//!    and if two items' best candidates overlap, neither is placed and each
//!    is [`UnresolvedReason::Ambiguous`] with its best candidate.
//! 3. **Anchors without context** (files written before context existed):
//!    if exactly one unclaimed, non-overlapping occurrence remains and no
//!    other unresolved item of the same collection has the same text, it is
//!    claimed. This is the rule that applied to every anchor before context
//!    existed, so such files behave exactly as before.
//! 4. Otherwise the item is unresolved: [`UnresolvedReason::Missing`] when no
//!    usable occurrence exists, [`UnresolvedReason::Ambiguous`] when several
//!    could match.
//!
//! The threshold makes the stored context, not the uniqueness of the text,
//! decide. An edit inside a span or ghost changes its text; if the old text
//! occurs once elsewhere, its surroundings there almost never agree with the
//! stored context, so the item is reported instead of attached to unrelated
//! text. An edit elsewhere in the document leaves the context intact, and an
//! edit right next to the anchor changes only one side, so the other side
//! still agrees. Stored context is at least `CONTEXT_UNITS / 2` units long
//! unless a document edge cut it short (see [`Anchor`]), so the minimum of 8
//! units only bites near an edge, where the candidate must then sit at the
//! same distance from it.
//!
//! Every placed item's context is then stored afresh from the new body.
//!
//! Every occurrence of every distinct anchor text is found in one pass over
//! the body by a [`terraphim_automata::CompiledMatcher`] built for that pass
//! (overlapping, case-sensitive, matching inside words, blank texts allowed),
//! so the cost of searching no longer grows with the number of distinct
//! texts. There is no second, hand-rolled search: in the (size-limit only)
//! case that the matcher cannot be built, every item is returned unresolved
//! with [`UnresolvedReason::SearchFailed`].
//!
//! Known limitation: an anchor without context (rule 3) whose text was edited
//! is still attached to the old text if it happens to occur exactly once
//! elsewhere; saving the file once stores context and closes that gap. Live
//! edits should go through [`Document::apply_edit`], which tracks positions
//! exactly (and keeps ghosts elastic).

use std::collections::HashMap;

use terraphim_automata::{
    MatchOverlap, MatcherBuilder, MatcherOptions, PositionedMatch, TerraphimAutomataError,
};
use terraphim_types::{NormalizedTerm, NormalizedTermValue};

use crate::context::{self, Probe, Score};
use crate::document::Document;
use crate::model::{Anchor, Ghost, Span};
use crate::offset::{byte_to_utf16, utf16_len};

/// Why a span or ghost could not be re-anchored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedReason {
    /// The anchor text no longer occurs in the body, every occurrence is
    /// claimed by another item of the same collection, or (for an anchor with
    /// stored context) no occurrence's surroundings agree with that context.
    Missing,
    /// Several occurrences could match. Candidate start offsets (UTF-16) are
    /// listed in body order.
    Ambiguous {
        /// UTF-16 start offsets of the possible matches.
        candidates: Vec<usize>,
    },
    /// The body could not be searched at all, so nothing was placed: building
    /// the matcher over the run's anchor texts failed. Over valid anchors this
    /// only happens when the automaton would exceed its size limits, which
    /// needs gigabytes of anchor text. Every span and ghost of the run is
    /// returned with this reason; none is guessed at.
    SearchFailed {
        /// The matcher builder's error, for diagnostics.
        message: String,
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
        let spans = std::mem::take(&mut self.annotations.spans);
        let ghosts = std::mem::take(&mut self.annotations.ghosts);
        let texts = spans
            .iter()
            .map(|s| s.anchor.text.as_str())
            .chain(ghosts.iter().map(|g| g.anchor.text.as_str()));
        let (span_placements, ghost_placements) = match Occurrences::find(&self.body, texts) {
            Ok(occurrences) => {
                let span_anchors: Vec<&Anchor> = spans.iter().map(|s| &s.anchor).collect();
                let ghost_anchors: Vec<&Anchor> = ghosts.iter().map(|g| &g.anchor).collect();
                (
                    place(&span_anchors, &occurrences, &self.body),
                    place(&ghost_anchors, &occurrences, &self.body),
                )
            }
            Err(error) => {
                let reason = UnresolvedReason::SearchFailed {
                    message: error.to_string(),
                };
                (
                    vec![Err(reason.clone()); spans.len()],
                    vec![Err(reason); ghosts.len()],
                )
            }
        };
        self.settle(spans, ghosts, span_placements, ghost_placements)
    }

    /// Puts each span and ghost back at its placement, or into the report
    /// with the reason it could not be placed.
    fn settle(
        &mut self,
        spans: Vec<Span>,
        ghosts: Vec<Ghost>,
        span_placements: Vec<Placement>,
        ghost_placements: Vec<Placement>,
    ) -> ReanchorReport {
        let mut report = ReanchorReport::default();
        for (mut span, placement) in spans.into_iter().zip(span_placements) {
            match placement {
                Ok(at) => {
                    if span.anchor.start != at.start {
                        report.moved.push(span.id.clone());
                    }
                    at.apply(&self.body, &mut span.anchor);
                    self.annotations.spans.push(span);
                }
                Err(reason) => report.unresolved.push(Unresolved { span, reason }),
            }
        }

        for (mut ghost, placement) in ghosts.into_iter().zip(ghost_placements) {
            match placement {
                Ok(at) => {
                    if ghost.anchor.start != at.start {
                        report.moved.push(ghost.id.clone());
                    }
                    at.apply(&self.body, &mut ghost.anchor);
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

/// Where one span or ghost lands, or why it cannot.
type Placement = Result<Landing, UnresolvedReason>;

/// The occurrence a span or ghost was placed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Landing {
    /// UTF-16 start offset.
    start: usize,
    /// UTF-16 end offset.
    end: usize,
    /// Byte start offset, so context is captured without converting again.
    byte_start: usize,
}

impl Landing {
    /// Moves `anchor` here and stores its context from the new body.
    fn apply(self, body: &str, anchor: &mut Anchor) {
        (anchor.start, anchor.end) = (self.start, self.end);
        let byte_end = self.byte_start + anchor.text.len();
        context::set(body, anchor, self.byte_start, byte_end);
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
    /// Byte start offsets of the same occurrences, parallel to `starts`, so
    /// context can be compared without converting offsets again.
    byte_starts: Vec<Vec<usize>>,
}

impl<'a> Occurrences<'a> {
    /// Finds every occurrence of every distinct text in `texts`.
    ///
    /// The texts are compiled into one [`terraphim_automata::CompiledMatcher`]
    /// and the body is scanned once. Blank (whitespace-only) texts are valid
    /// span and ghost anchors, so the matcher is told to accept them; only the
    /// empty text is refused, and it occurs nowhere.
    ///
    /// Fails only when building the automaton fails, which over validated
    /// patterns means an automaton size limit was exceeded. There is no
    /// second search to fall back to: the caller reports every item as
    /// [`UnresolvedReason::SearchFailed`] instead.
    fn find(
        body: &str,
        texts: impl IntoIterator<Item = &'a str>,
    ) -> Result<Self, TerraphimAutomataError> {
        let mut slot: HashMap<&'a str, usize> = HashMap::new();
        let mut distinct: Vec<&'a str> = Vec::new();
        for text in texts {
            slot.entry(text).or_insert_with(|| {
                distinct.push(text);
                distinct.len() - 1
            });
        }
        let mut starts = vec![Vec::new(); distinct.len()];
        let mut byte_starts = vec![Vec::new(); distinct.len()];

        let options = MatcherOptions::default()
            .with_overlap(MatchOverlap::All)
            .with_word_boundaries(false)
            .with_case_insensitive(false)
            .with_min_pattern_length(1)
            .with_allow_blank_patterns(true);
        let mut builder = MatcherBuilder::new(options);
        // `PositionedMatch::pattern_index` is the insertion index among the
        // patterns the builder accepted, so record the slot each one fills.
        let mut slot_of_pattern: Vec<usize> = Vec::with_capacity(distinct.len());
        for (i, text) in distinct.iter().enumerate() {
            // Texts are distinct and every non-empty text passes the checks,
            // so the only refusal is the empty text, which occurs nowhere.
            // The term is unused: matches are mapped by `pattern_index`.
            let term = NormalizedTerm::new(i as u64, NormalizedTermValue::default());
            if builder.insert((*text).to_owned(), term).is_ok() {
                slot_of_pattern.push(i);
            } else {
                debug_assert!(text.is_empty(), "matcher refused {text:?}");
            }
        }
        if builder.is_empty() {
            return Ok(Self {
                slot,
                starts,
                byte_starts,
            });
        }
        let matcher = builder.build()?;
        let mut positions: Vec<PositionedMatch> = Vec::new();
        matcher.push_positions(body, &mut positions);

        // Positions arrive sorted by start byte, so one running count
        // converts every start to UTF-16 in a single walk of the body.
        let (mut byte, mut units) = (0, 0);
        for found in &positions {
            units += utf16_len(&body[byte..found.start]);
            byte = found.start;
            debug_assert_eq!(Some(units), byte_to_utf16(body, found.start));
            let slot = slot_of_pattern[found.pattern_index];
            starts[slot].push(units);
            byte_starts[slot].push(found.start);
        }
        Ok(Self {
            slot,
            starts,
            byte_starts,
        })
    }

    /// The slot of `text`, which must be one of the texts passed to
    /// [`Occurrences::find`].
    fn slot_of(&self, text: &str) -> usize {
        self.slot[text]
    }

    /// UTF-16 starts of the occurrences of `text`.
    #[cfg(test)]
    fn of(&self, text: &str) -> &[usize] {
        &self.starts[self.slot_of(text)]
    }
}

/// An anchor's text slot and stored context: items with equal keys score
/// equally.
type ContextKey<'a> = (usize, Option<&'a str>, Option<&'a str>);

/// UTF-16 starts of the qualifying candidates that share the best context
/// score (rule 2), in body order. `starts` and `byte_starts` are the
/// occurrences of the anchor's text; those for which `excluded` is true are
/// skipped.
fn best_candidates(
    anchor: &Anchor,
    starts: &[usize],
    byte_starts: &[usize],
    body: &str,
    excluded: impl Fn(usize) -> bool,
) -> Vec<usize> {
    let probe = Probe::new(anchor);
    let mut best: Option<Score> = None;
    let mut tied: Vec<usize> = Vec::new();
    for (&start, &byte_start) in starts.iter().zip(byte_starts) {
        if excluded(start) {
            continue;
        }
        let Some(score) = probe.score(body, byte_start, byte_start + anchor.text.len()) else {
            continue;
        };
        if best.is_none_or(|b| score > b) {
            best = Some(score);
            tied.clear();
        }
        if best == Some(score) {
            tied.push(start);
        }
    }
    tied
}

/// Places each anchor by the module rules, given every occurrence of its
/// text. Items placed by one call never overlap each other.
fn place(anchors: &[&Anchor], found: &Occurrences<'_>, body: &str) -> Vec<Placement> {
    let slots: Vec<usize> = anchors.iter().map(|a| found.slot_of(&a.text)).collect();
    let occurrences: Vec<&[usize]> = slots.iter().map(|&slot| &found.starts[slot][..]).collect();
    let byte_starts: Vec<&[usize]> = slots
        .iter()
        .map(|&slot| &found.byte_starts[slot][..])
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
        // Starts are in body order, so the hint can be looked up by halving.
        let at_hint = occurrences[i].binary_search(&hint).is_ok();
        if at_hint && !overlaps_claim(&claimed, hint, lengths[i]) {
            placed[i] = Some(hint);
            claimed.push((hint, hint + lengths[i]));
        }
    }

    // Rule 2: anchors with context take their single best-scoring candidate.
    // Items with the same text and context see the same candidates (only
    // rule-1 claims are excluded so far), so where several items share a
    // text each distinct context is scored once.
    let mut reasons: Vec<Option<UnresolvedReason>> = vec![None; anchors.len()];
    let mut tentative: Vec<(usize, usize)> = Vec::new();
    let needs_context = |i: usize| placed[i].is_none() && anchors[i].has_context();
    let mut sharing = vec![0usize; found.starts.len()];
    for i in (0..anchors.len()).filter(|&i| needs_context(i)) {
        sharing[slots[i]] += 1;
    }
    let mut best_by_key: HashMap<ContextKey<'_>, Vec<usize>> = HashMap::new();
    for (i, anchor) in anchors.iter().enumerate() {
        if !needs_context(i) {
            continue;
        }
        let score = || {
            best_candidates(anchor, occurrences[i], byte_starts[i], body, |start| {
                overlaps_claim(&claimed, start, lengths[i])
            })
        };
        let computed;
        let tied: &Vec<usize> = if sharing[slots[i]] > 1 {
            let key = (slots[i], anchor.before.as_deref(), anchor.after.as_deref());
            best_by_key.entry(key).or_insert_with(score)
        } else {
            computed = score();
            &computed
        };
        match tied.as_slice() {
            [] => reasons[i] = Some(UnresolvedReason::Missing),
            [only] => tentative.push((i, *only)),
            _ => {
                reasons[i] = Some(UnresolvedReason::Ambiguous {
                    candidates: tied.clone(),
                })
            }
        }
    }
    // Two items whose best candidates overlap are both left unplaced: the
    // context cannot say which one the text belongs to. One sweep in start
    // order finds every overlapping pair.
    tentative.sort_unstable_by_key(|&(i, start)| (start, i));
    let mut conflicted = vec![false; tentative.len()];
    let mut widest: Option<(usize, usize)> = None; // (end, index in tentative)
    for (k, &(i, start)) in tentative.iter().enumerate() {
        let end = start + lengths[i];
        if let Some((widest_end, widest_k)) = widest
            && start < widest_end
        {
            conflicted[k] = true;
            conflicted[widest_k] = true;
        }
        if widest.is_none_or(|(widest_end, _)| end > widest_end) {
            widest = Some((end, k));
        }
    }
    for (k, &(i, start)) in tentative.iter().enumerate() {
        if conflicted[k] {
            reasons[i] = Some(UnresolvedReason::Ambiguous {
                candidates: vec![start],
            });
        } else {
            placed[i] = Some(start);
            claimed.push((start, start + lengths[i]));
        }
    }

    // Rule 3: an anchor without context takes the single remaining
    // occurrence, if uncontested. An item placed here was the only unresolved
    // one with its text, so counting once after rule 2 is exact.
    let mut unresolved_with_text: HashMap<&str, usize> = HashMap::new();
    for (i, anchor) in anchors.iter().enumerate() {
        if placed[i].is_none() {
            *unresolved_with_text
                .entry(anchor.text.as_str())
                .or_default() += 1;
        }
    }
    for (i, anchor) in anchors.iter().enumerate() {
        if placed[i].is_some() || anchor.has_context() {
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
        .enumerate()
        .map(|(i, (start, reason))| match start {
            Some(start) => {
                // Every placement is one of the item's occurrences.
                let index = occurrences[i]
                    .binary_search(&start)
                    .expect("placed on an occurrence");
                Ok(Landing {
                    start,
                    end: start + lengths[i],
                    byte_start: byte_starts[i][index],
                })
            }
            None => Err(reason.unwrap_or(UnresolvedReason::Missing)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(body: &str, texts: &[&str], text: &str) -> Vec<usize> {
        Occurrences::find(body, texts.iter().copied())
            .unwrap()
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
        let occ = Occurrences::find("concatenate the cat", texts).unwrap();
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
    fn blank_texts_go_through_the_matcher() {
        let texts = ["\n\n", " ", "b"];
        let body = "a\n\n\nb  c";
        assert_eq!(starts(body, &texts, "\n\n"), vec![1, 2]);
        assert_eq!(starts(body, &texts, " "), vec![5, 6]);
        assert_eq!(starts(body, &texts, "b"), vec![4]);
    }

    #[test]
    fn no_texts_and_empty_text_find_nothing() {
        let occ = Occurrences::find("abc", std::iter::empty()).unwrap();
        assert!(occ.starts.is_empty());
        // Empty anchor text cannot be parsed, but `annotations` is public.
        assert_eq!(starts("abc", &[""], ""), Vec::<usize>::new());
    }

    #[test]
    fn refused_empty_text_does_not_shift_other_slots() {
        // The matcher refuses "", so "cat" is its pattern 0 but slot 1, and
        // "a" its pattern 1 but slot 2: matches must still land in the slot.
        let texts = ["", "cat", "a"];
        let occ = Occurrences::find("a cat", texts).unwrap();
        assert_eq!(occ.of(""), &[] as &[usize]);
        assert_eq!(occ.of("cat"), &[2]);
        assert_eq!(occ.of("a"), &[0, 3]);
    }

    #[test]
    fn mixed_texts_in_one_pass_report_every_occurrence() {
        // UTF-16: clef 0-1, aaa 3-5, cat 7-9, e-acute 11, cat 13-15,
        // CJK 17-20, then " \n\n" at 21-23. These are the cases the removed
        // per-text search used to cover, now asserted through the matcher.
        let body = "𝄞 aaa cat é cat 中文中文 \n\n";
        let texts = ["aa", "cat", "中文", "𝄞", "a", "x", "\n", " ", "\n\n"];
        let occ = Occurrences::find(body, texts).unwrap();
        assert_eq!(occ.of("aa"), &[3, 4]);
        assert_eq!(occ.of("cat"), &[7, 13]);
        assert_eq!(occ.of("中文"), &[17, 19]);
        assert_eq!(occ.of("𝄞"), &[0]);
        assert_eq!(occ.of("a"), &[3, 4, 5, 8, 14]);
        assert_eq!(occ.of("x"), &[] as &[usize]);
        assert_eq!(occ.of("\n"), &[22, 23]);
        assert_eq!(occ.of(" "), &[2, 6, 10, 12, 16, 21]);
        assert_eq!(occ.of("\n\n"), &[22]);
    }

    #[test]
    fn search_failure_returns_every_item_unresolved() {
        // A real build failure needs an automaton past aho-corasick's state
        // limits (gigabytes of anchor text), so the failure branch's
        // placements are fed to `settle` directly.
        let mut doc = Document::new("one cat two dogs");
        let span = doc
            .add_span(crate::model::SpanKind::Word, 4, 7)
            .expect("span");
        doc.ghost(12, 16).expect("ghost");
        let spans = std::mem::take(&mut doc.annotations.spans);
        let ghosts = std::mem::take(&mut doc.annotations.ghosts);
        let before = (spans.clone(), ghosts.clone());
        let reason = UnresolvedReason::SearchFailed {
            message: "automaton too large".to_string(),
        };
        let report = doc.settle(
            spans,
            ghosts,
            vec![Err(reason.clone())],
            vec![Err(reason.clone())],
        );
        assert!(!report.is_clean());
        assert!(report.moved.is_empty());
        assert!(doc.annotations.spans.is_empty());
        assert!(doc.annotations.ghosts.is_empty());
        assert_eq!(report.unresolved.len(), 1);
        assert_eq!(report.unresolved[0].span.id, span);
        assert_eq!(report.unresolved[0].span, before.0[0], "returned as it was");
        assert_eq!(report.unresolved[0].reason, reason);
        assert_eq!(report.unresolved_ghosts[0].ghost, before.1[0]);
        assert_eq!(report.unresolved_ghosts[0].reason, reason);
    }
}
