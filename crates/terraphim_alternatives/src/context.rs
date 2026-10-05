//! Context stored around each anchor, and how a candidate is scored against
//! it (decision 2026-10-05: context re-anchoring).
//!
//! # Capture
//!
//! [`capture`] takes up to [`CONTEXT_UNITS`] UTF-16 code units of body text on
//! each side of an anchor, walking whole characters so a surrogate pair is
//! never split. When the window ends inside a word (the window does not reach
//! the document edge, and neither the character just outside the window nor
//! the window's outer character is whitespace), the partial word is dropped
//! and the context starts (or ends) at the whitespace before (or after) it,
//! so the stored context does not depend on where the window happened to cut.
//! The partial word is kept when dropping it would leave fewer than
//! `CONTEXT_UNITS / 2` units (one long word, or text without spaces such as
//! CJK). Context therefore shorter than `CONTEXT_UNITS / 2` units was cut
//! short by a document edge.
//!
//! # Scoring
//!
//! See [`crate::reanchor`] for how the score is used. For one side with
//! stored context of `L` units, `m` is the number of units that agree with
//! the body next to the candidate: the common suffix of `before` and the text
//! preceding the candidate, or the common prefix of `after` and the text
//! following it, compared character by character (plain string comparison at
//! known offsets, not a search). The side **agrees** when
//!
//! * `L > 0` and `m >= max(ceil(L / 2), CONTEXT_UNITS / 4)`: at least half of
//!   the stored context, and at least 8 units, matches; or
//! * `m == L` and the body on that side of the candidate is exactly the stored
//!   context: the candidate sits at the same distance from the same document
//!   edge (with `L == 0`, it touches that edge too).
//!
//! A side without stored context (`None`) never agrees.

use crate::model::{Anchor, Annotations, CONTEXT_UNITS};
use crate::offset::{utf16_len, utf16_to_byte_from, utf16_to_bytes_sorted};

/// Context before and after the byte range `byte_start..byte_end` of `body`.
/// Both offsets must lie on character boundaries.
pub(crate) fn capture(body: &str, byte_start: usize, byte_end: usize) -> (String, String) {
    (
        before_window(&body[..byte_start]).to_string(),
        after_window(&body[byte_end..]).to_string(),
    )
}

/// The context to store from `head`, the body text before an anchor.
fn before_window(head: &str) -> &str {
    let mut units = 0;
    let mut cut = head.len();
    for (index, ch) in head.char_indices().rev() {
        units += ch.len_utf16();
        if units > CONTEXT_UNITS {
            break;
        }
        cut = index;
    }
    let window = &head[cut..];
    let Some(outside) = head[..cut].chars().next_back() else {
        return window;
    };
    let mid_word = !outside.is_whitespace() && window.chars().next().is_some_and(is_word_char);
    if !mid_word {
        return window;
    }
    match window.find(char::is_whitespace) {
        Some(space) if utf16_len(&window[space..]) >= CONTEXT_UNITS / 2 => &window[space..],
        _ => window,
    }
}

/// The context to store from `tail`, the body text after an anchor.
fn after_window(tail: &str) -> &str {
    let mut units = 0;
    let mut cut = 0;
    for (index, ch) in tail.char_indices() {
        units += ch.len_utf16();
        if units > CONTEXT_UNITS {
            break;
        }
        cut = index + ch.len_utf8();
    }
    let window = &tail[..cut];
    let Some(outside) = tail[cut..].chars().next() else {
        return window;
    };
    let mid_word = !outside.is_whitespace() && window.chars().next_back().is_some_and(is_word_char);
    if !mid_word {
        return window;
    }
    match window.char_indices().rev().find(|(_, c)| c.is_whitespace()) {
        Some((space, ch)) if utf16_len(&window[..space + ch.len_utf8()]) >= CONTEXT_UNITS / 2 => {
            &window[..space + ch.len_utf8()]
        }
        _ => window,
    }
}

fn is_word_char(ch: char) -> bool {
    !ch.is_whitespace()
}

/// Stores current context on `anchor` if the body still holds its text at
/// its offsets; a stale anchor keeps the context it has. Offsets are
/// converted by walking from `reference`, a known `(UTF-16, byte)` character
/// boundary near the anchor, so the cost does not grow with the body.
pub(crate) fn refresh_anchor_near(body: &str, anchor: &mut Anchor, reference: (usize, usize)) {
    let located = (anchor.start <= anchor.end)
        .then(|| {
            let start = utf16_to_byte_from(body, reference, anchor.start)?;
            let end = start + anchor.text.len();
            let at_end = utf16_to_byte_from(body, (anchor.start, start), anchor.end)?;
            (end == at_end && body.get(start..end) == Some(anchor.text.as_str()))
                .then_some((start, end))
        })
        .flatten();
    if let Some((byte_start, byte_end)) = located {
        set(body, anchor, byte_start, byte_end);
    }
}

/// Stores context for an anchor known to sit at `byte_start..byte_end`.
/// Context that is already current is left as it is, so the common case of
/// an anchor whose surroundings did not change allocates nothing.
pub(crate) fn set(body: &str, anchor: &mut Anchor, byte_start: usize, byte_end: usize) {
    store(&mut anchor.before, before_window(&body[..byte_start]));
    store(&mut anchor.after, after_window(&body[byte_end..]));
}

fn store(slot: &mut Option<String>, current: &str) {
    if slot.as_deref() != Some(current) {
        *slot = Some(current.to_string());
    }
}

/// Byte range of an anchor that still matches the body.
pub(crate) fn locate(body: &str, anchor: &Anchor) -> Option<(usize, usize)> {
    if anchor.start > anchor.end {
        return None;
    }
    let start = crate::offset::utf16_to_byte(body, anchor.start)?;
    let end = crate::offset::utf16_to_byte(body, anchor.end)?;
    (body[start..end] == anchor.text).then_some((start, end))
}

/// Refreshes the context of every span and ghost that still matches the
/// body, converting all offsets in one walk of the body.
pub(crate) fn refresh_all(body: &str, annotations: &mut Annotations) {
    let anchors: Vec<&mut Anchor> = annotations
        .spans
        .iter_mut()
        .map(|s| &mut s.anchor)
        .chain(annotations.ghosts.iter_mut().map(|g| &mut g.anchor))
        .collect();
    if anchors.is_empty() {
        return;
    }
    // Each anchor contributes its start (slot 2i) and end (slot 2i + 1).
    let mut requests: Vec<(usize, usize)> = anchors
        .iter()
        .enumerate()
        .flat_map(|(i, a)| [(a.start, 2 * i), (a.end, 2 * i + 1)])
        .collect();
    requests.sort_unstable();
    let offsets: Vec<usize> = requests.iter().map(|&(offset, _)| offset).collect();
    let mut bytes = vec![None; requests.len()];
    for (&(_, slot), byte) in requests.iter().zip(utf16_to_bytes_sorted(body, &offsets)) {
        bytes[slot] = byte;
    }
    for (i, anchor) in anchors.into_iter().enumerate() {
        if let (Some(start), Some(end)) = (bytes[2 * i], bytes[2 * i + 1])
            && start <= end
            && body[start..end] == anchor.text
        {
            set(body, anchor, start, end);
        }
    }
}

/// How well a candidate's surroundings agree with an anchor's context.
/// Ordered: more agreeing sides first, then more matching units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Score {
    /// Sides (0 to 2) that agree by the module rule.
    pub sides: usize,
    /// UTF-16 units matched on both sides together.
    pub units: usize,
}

/// Scores the candidate occurrence at `byte_start..byte_end` of `body`
/// against `anchor`'s stored context. `None` when no side agrees, so the
/// candidate does not qualify.
#[cfg(test)]
pub(crate) fn score(
    anchor: &Anchor,
    body: &str,
    byte_start: usize,
    byte_end: usize,
) -> Option<Score> {
    Probe::new(anchor).score(body, byte_start, byte_end)
}

/// An anchor's stored context, prepared for scoring many candidates.
pub(crate) struct Probe<'a> {
    before: Option<(&'a str, usize)>,
    after: Option<(&'a str, usize)>,
}

impl<'a> Probe<'a> {
    /// Prepares `anchor`'s context (its UTF-16 lengths are computed once).
    pub(crate) fn new(anchor: &'a Anchor) -> Self {
        let prepare = |side: &'a Option<String>| side.as_deref().map(|s| (s, utf16_len(s)));
        Self {
            before: prepare(&anchor.before),
            after: prepare(&anchor.after),
        }
    }

    /// Scores the candidate occurrence at `byte_start..byte_end` of `body`.
    /// `None` when no side agrees, so the candidate does not qualify.
    pub(crate) fn score(&self, body: &str, byte_start: usize, byte_end: usize) -> Option<Score> {
        let mut result = Score { sides: 0, units: 0 };
        if let Some((before, stored_units)) = self.before {
            let preceding = &body[..byte_start];
            // Equal bytes are equal characters once the match is cut back to
            // a character boundary of the stored text.
            let same = common_len(before.bytes().rev(), preceding.bytes().rev());
            let mut from = before.len() - same;
            while !before.is_char_boundary(from) {
                from += 1;
            }
            let matched = utf16_len(&before[from..]);
            let exact = preceding.len() == before.len();
            result.add(side(stored_units, matched, exact));
        }
        if let Some((after, stored_units)) = self.after {
            let following = &body[byte_end..];
            let mut to = common_len(after.bytes(), following.bytes());
            while !after.is_char_boundary(to) {
                to -= 1;
            }
            let matched = utf16_len(&after[..to]);
            let exact = following.len() == after.len();
            result.add(side(stored_units, matched, exact));
        }
        (result.sides > 0).then_some(result)
    }
}

impl Score {
    fn add(&mut self, (agrees, units): (bool, usize)) {
        self.sides += usize::from(agrees);
        self.units += units;
    }
}

/// Whether one side agrees, and its matched units. `exact` says the body on
/// that side is as long as the stored context (only meaningful when every
/// stored unit matched).
fn side(stored_units: usize, matched: usize, exact: bool) -> (bool, usize) {
    let enough = stored_units > 0 && matched >= stored_units.div_ceil(2).max(CONTEXT_UNITS / 4);
    let at_same_edge = matched == stored_units && exact;
    (enough || at_same_edge, matched)
}

/// Length of the common prefix of two byte sequences.
fn common_len(a: impl Iterator<Item = u8>, b: impl Iterator<Item = u8>) -> usize {
    a.zip(b).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn around(body: &str, text: &str) -> (String, String) {
        let start = body.find(text).expect("text in body");
        capture(body, start, start + text.len())
    }

    #[test]
    fn short_documents_keep_everything_up_to_the_edges() {
        assert_eq!(
            around("The tension rises.", "tension"),
            ("The ".into(), " rises.".into())
        );
        assert_eq!(around("cat", "cat"), (String::new(), String::new()));
    }

    #[test]
    fn long_context_is_cut_to_whole_words() {
        let body =
            "alpha bravo charlie delta echo foxtrot TARGET golf hotel india juliet kilo lima";
        let (before, after) = around(body, "TARGET");
        // 32 units back is "ravo charlie ..."; the partial word is dropped.
        assert_eq!(before, " charlie delta echo foxtrot ");
        assert!(utf16_len(&before) <= CONTEXT_UNITS);
        assert_eq!(after, " golf hotel india juliet kilo ");
        assert!(utf16_len(&after) <= CONTEXT_UNITS);
    }

    #[test]
    fn window_on_a_word_boundary_is_not_trimmed() {
        // Exactly 32 units before the anchor, starting after a space.
        let before = "abcdefg abcdefg abcdefg abcdefg ";
        assert_eq!(utf16_len(before), 32);
        let body = format!("zz {before}X");
        assert_eq!(around(&body, "X").0, before);
    }

    #[test]
    fn text_without_spaces_keeps_the_full_window() {
        let body = format!("{}X{}", "中".repeat(40), "文".repeat(40));
        let (before, after) = around(&body, "X");
        assert_eq!(before, "中".repeat(32));
        assert_eq!(after, "文".repeat(32));
    }

    #[test]
    fn astral_characters_are_never_split() {
        // Each clef is two units: 16 fit exactly, a 17th would exceed.
        let body = format!("{}X{}", "𝄞".repeat(20), "𝄞".repeat(20));
        let (before, after) = around(&body, "X");
        assert_eq!(before, "𝄞".repeat(16));
        assert_eq!(after, "𝄞".repeat(16));
        // An odd budget: one unit of "a" leaves 31, so only 15 clefs fit.
        let body = format!("{}aX", "𝄞".repeat(20));
        assert_eq!(around(&body, "X").0, format!("{}a", "𝄞".repeat(15)));
    }

    #[test]
    fn scoring_follows_the_documented_threshold() {
        let mut anchor = Anchor::new(0, 3, "cat");
        anchor.before = Some("the quick brown ".into()); // 16 units
        anchor.after = Some(" sat on the mat".into()); // 15 units
        // Both sides intact.
        let body = "the quick brown cat sat on the mat";
        let s = score(&anchor, body, 16, 19).unwrap();
        assert_eq!(
            s,
            Score {
                sides: 2,
                units: 31
            }
        );
        // Before rewritten, after intact: one side agrees.
        let body = "a slow red cat sat on the mat";
        assert_eq!(score(&anchor, body, 11, 14).unwrap().sides, 1);
        // Only a space and a word on each side: below half and below 8.
        let body = "x brown cat sat x";
        assert_eq!(score(&anchor, body, 8, 11), None);
    }

    #[test]
    fn short_edge_context_needs_the_same_edge() {
        let mut anchor = Anchor::new(4, 7, "cat");
        anchor.before = Some("The ".into());
        anchor.after = Some(String::new());
        // Same distance from the start, at the end: both sides agree.
        assert_eq!(score(&anchor, "The cat", 4, 7).unwrap().sides, 2);
        // "The " matches fully but the text before is longer: not the edge,
        // and four units are below the minimum of eight.
        assert_eq!(score(&anchor, "So The cat!", 7, 10), None);
        // At the end of the document, though not after "The ".
        assert_eq!(score(&anchor, "A cat", 2, 5).unwrap().sides, 1);
    }

    #[test]
    fn a_shared_leading_byte_is_not_a_matching_character() {
        // "é" is C3 A9 and "è" is C3 A8: the first byte agrees, the
        // character does not, so no unit matches on the after side.
        let mut anchor = Anchor::new(0, 1, "x");
        anchor.after = Some("é and more text here".into());
        anchor.before = Some("some text before the 😀 ".into());
        let body = "some text before the 😀 xè and more text here";
        let x = body.find("xè").unwrap();
        let s = score(&anchor, body, x, x + 1).unwrap();
        assert_eq!(s.sides, 1);
        assert_eq!(s.units, utf16_len("some text before the 😀 "));
    }

    #[test]
    fn a_missing_side_never_agrees() {
        let mut anchor = Anchor::new(0, 1, "x");
        assert_eq!(score(&anchor, "x", 0, 1), None, "no context at all");
        anchor.after = Some(String::new());
        assert_eq!(score(&anchor, "x", 0, 1).unwrap().sides, 1);
    }

    #[test]
    fn refresh_skips_stale_anchors_and_updates_located_ones() {
        let mut annotations = Annotations::default();
        annotations.ghosts.push(crate::model::Ghost {
            id: "g1".into(),
            anchor: Anchor::new(4, 7, "cat"),
        });
        annotations.ghosts.push(crate::model::Ghost {
            id: "g2".into(),
            anchor: Anchor {
                before: Some("kept".into()),
                ..Anchor::new(40, 43, "dog")
            },
        });
        refresh_all("the cat sat", &mut annotations);
        let fresh = &annotations.ghosts[0].anchor;
        assert_eq!(fresh.before.as_deref(), Some("the "));
        assert_eq!(fresh.after.as_deref(), Some(" sat"));
        let stale = &annotations.ghosts[1].anchor;
        assert_eq!(stale.before.as_deref(), Some("kept"));
        assert_eq!(stale.after, None);
    }
}
