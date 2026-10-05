//! Trim levels (spec R-8.3 to R-8.5, issue #15): which spans each level
//! fades.
//!
//! The algorithm is the research spike's (`docs/research/lab-heuristics.md`
//! §5, prototype in `research/lab-heuristics/src/lib.rs`), moved onto the
//! crate's original-offset [`Doc`] and its rolegraph-primary weakness score:
//!
//! 1. **Candidates** at three granularities, each a byte range that can be
//!    deleted verbatim. Word: a filler or hedge from the role's KG lists, with
//!    the whitespace before it (or, sentence-initially, the comma and
//!    whitespace after it). Clause: a parenthetical, an em-dash aside or tail,
//!    or a comma aside opened by a connective. Sentence: the gap before it
//!    plus the sentence (a paragraph opener takes the gap after it instead; a
//!    one-sentence paragraph takes its whole block, bullet included).
//! 2. **Guards**: nothing that overlaps protected structure (headings, code,
//!    tables, quotes, HTML, footnotes, link destinations), so a sentence or
//!    parenthetical holding inline code is never a candidate; a clause is at
//!    most 60% of its sentence; a resumptive dash tail and a comma aside
//!    followed by *but* or *yet* are skipped.
//! 3. **Rank**: rank tier (word, clause, sentence, then paragraph openers and
//!    short sentences), then the host sentence's primary weakness, then its
//!    text-only tie-break, then position.
//! 4. **Fit**, per level and nested: pass 1 walks the ranked list and takes
//!    a candidate only if it does not overshoot the target; pass 2 then takes
//!    the candidate that brings the cut closest to the target while that
//!    strictly helps. Paragraph openers and short sentences are only eligible
//!    at [`TrimLevel::Half`].

use serde::{Deserialize, Serialize};

use crate::LabConfig;
use crate::actions::Analysis;
use crate::lists::{Connective, StyleCategory, find_connectives};
use crate::offset::bytes_to_utf16;
use crate::text::{Doc, Sentence};
use crate::weak::{self, Weakness};

/// A trim level (R-8.3). Levels are nested: everything a level fades, every
/// higher level fades too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrimLevel {
    /// "Original": nothing faded.
    Original,
    /// "Slight trim ~10%".
    Slight,
    /// "Tighten more ~20%".
    Tighten,
    /// "Even sharper ~30%".
    Sharper,
    /// "Cut in half ~50%".
    Half,
}

impl TrimLevel {
    /// Every level, in button order.
    pub const ALL: [TrimLevel; 5] = [
        TrimLevel::Original,
        TrimLevel::Slight,
        TrimLevel::Tighten,
        TrimLevel::Sharper,
        TrimLevel::Half,
    ];

    /// The share of the document's words this level aims to cut.
    pub fn target_fraction(self) -> f64 {
        match self {
            TrimLevel::Original => 0.0,
            TrimLevel::Slight => 0.10,
            TrimLevel::Tighten => 0.20,
            TrimLevel::Sharper => 0.30,
            TrimLevel::Half => 0.50,
        }
    }

    /// Words to cut from a document of `total_words`, rounded to nearest.
    pub fn target_words(self, total_words: usize) -> usize {
        (total_words as f64 * self.target_fraction()).round() as usize
    }

    /// The button label from the spec.
    pub fn label(self) -> &'static str {
        match self {
            TrimLevel::Original => "Original",
            TrimLevel::Slight => "Slight trim",
            TrimLevel::Tighten => "Tighten more",
            TrimLevel::Sharper => "Even sharper",
            TrimLevel::Half => "Cut in half",
        }
    }
}

/// The granularity of a cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// A filler or hedge word or phrase.
    Word,
    /// A parenthetical, dash aside or comma aside.
    Clause,
    /// A whole sentence.
    Sentence,
}

/// Identifies a cut within one [`TrimPlan`]: its index in [`TrimPlan::cuts`].
/// Ids are only meaningful for the plan that issued them; a plan is computed
/// for one version of the body and discarded after an edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CutId(pub u32);

/// One span a trim level fades, and "Make the cuts" deletes verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cut {
    /// This cut's id in its plan.
    pub id: CutId,
    /// UTF-16 code-unit offset of the first unit, into the original body.
    pub start: usize,
    /// UTF-16 code-unit offset one past the last unit.
    pub end: usize,
    /// Word, clause or sentence.
    pub tier: Tier,
    /// The first level that fades this cut. Never [`TrimLevel::Original`].
    pub first_level: TrimLevel,
    /// The host sentence's primary weakness in `[0, 1]` (higher is weaker).
    pub score: f64,
    /// Why the span would go, for the UI (`filler "quite"`, `aside "which"`,
    /// `weak sentence`, ...).
    pub reason: String,
    /// Words this cut removes on its own (the crate's word definition).
    pub words: usize,
}

/// The status card numbers for a level (R-8.4): `535 -> 480 words · -10%`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrimStatus {
    /// The level.
    pub level: TrimLevel,
    /// Words in the body now.
    pub words_before: usize,
    /// Words left after "Make the cuts" at this level, honouring kept cuts.
    pub words_after: usize,
    /// Achieved share of words cut, in percent (`100 * cut / before`). It is
    /// the honest number: on code-dense prose "Cut in half" can fall short
    /// of 50%, and this says by how much.
    pub percent: f64,
    /// The level's target share, in percent.
    pub target_percent: f64,
}

impl TrimStatus {
    /// Words cut at this level.
    pub fn words_cut(&self) -> usize {
        self.words_before - self.words_after
    }

    /// The card's count line, `535 → 480 words · −10%` (percent rounded).
    pub fn card_text(&self) -> String {
        format!(
            "{} \u{2192} {} words \u{b7} \u{2212}{}%",
            self.words_before,
            self.words_after,
            self.percent.round() as i64
        )
    }
}

/// Every span any trim level fades, computed once per version of the body.
/// Switching level is a filter ([`TrimPlan::faded`]); keeping a span is a
/// filter on top ([`TrimPlan::active`]); neither re-runs the analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrimPlan {
    total_words: usize,
    cuts: Vec<Cut>,
    /// UTF-16 offset of every word's first unit, in order.
    word_starts: Vec<usize>,
    /// The body in UTF-16, to tell whitespace-only pieces of split cuts.
    units: Vec<u16>,
}

impl TrimPlan {
    /// Words in the body (the status card's "before").
    pub fn total_words(&self) -> usize {
        self.total_words
    }

    /// Every cut some level fades, in document order (by `start`, then
    /// `end`). Candidates that no level selects are not included. Cuts can
    /// nest (a filler inside a faded sentence); deleting both is the same as
    /// deleting the sentence.
    pub fn cuts(&self) -> &[Cut] {
        &self.cuts
    }

    /// The cut with this id.
    pub fn get(&self, id: CutId) -> Option<&Cut> {
        self.cuts.get(id.0 as usize)
    }

    /// Cuts faded at `level`, in document order (which is also the "Walk
    /// through" order). A superset of every lower level's.
    pub fn faded(&self, level: TrimLevel) -> impl Iterator<Item = &Cut> {
        self.cuts
            .iter()
            .filter(move |c| level != TrimLevel::Original && c.first_level <= level)
    }

    /// What "Make the cuts" deletes at `level` when the user has kept the
    /// cuts in `kept` ("Click one to keep it"), in document order. **A keep
    /// always wins** (R-8.5): no kept text is ever deleted.
    ///
    /// * A faded cut inside a kept cut is dropped, so keeping a sentence also
    ///   keeps the filler faded within it.
    /// * A faded cut that encloses or overlaps a kept cut is split around it:
    ///   each remaining piece is returned as its own [`Cut`] (same id, tier,
    ///   level and reason; `start`, `end` and `words` of the piece). Pieces
    ///   that hold only whitespace are dropped. So keeping a word inside a
    ///   faded sentence deletes the rest of the sentence and keeps the word;
    ///   [`make_cuts`](crate::make_cuts) tidies the joins around it.
    ///
    /// Unknown ids in `kept` are ignored.
    pub fn active(&self, level: TrimLevel, kept: &[CutId]) -> Vec<Cut> {
        let mut kept_ranges: Vec<(usize, usize)> = kept
            .iter()
            .filter_map(|&id| self.get(id))
            .map(|c| (c.start, c.end))
            .collect();
        kept_ranges.sort_unstable();
        let mut out = Vec::new();
        for c in self.faded(level) {
            if kept_ranges.iter().any(|&(s, e)| s <= c.start && c.end <= e) {
                continue;
            }
            // Subtract every kept range from `c`.
            let mut pieces = vec![(c.start, c.end)];
            for &(ks, ke) in &kept_ranges {
                if ke <= c.start || ks >= c.end {
                    continue;
                }
                pieces = pieces
                    .into_iter()
                    .flat_map(|(ps, pe)| {
                        if ke <= ps || ks >= pe {
                            vec![(ps, pe)]
                        } else {
                            [(ps, ks.max(ps)), (ke.min(pe), pe)]
                                .into_iter()
                                .filter(|(a, b)| b > a)
                                .collect()
                        }
                    })
                    .collect();
            }
            let whole = pieces.len() == 1 && pieces[0] == (c.start, c.end);
            for (ps, pe) in pieces {
                if !whole && self.is_blank(ps, pe) {
                    continue;
                }
                out.push(Cut {
                    start: ps,
                    end: pe,
                    words: self.words_in(ps, pe),
                    ..c.clone()
                });
            }
        }
        out.sort_by_key(|c| (c.start, c.end));
        out
    }

    /// Words whose first unit lies in UTF-16 range `start..end`.
    fn words_in(&self, start: usize, end: usize) -> usize {
        let lo = self.word_starts.partition_point(|&w| w < start);
        let hi = self.word_starts.partition_point(|&w| w < end);
        hi - lo
    }

    /// True when UTF-16 range `start..end` of the body is all whitespace.
    fn is_blank(&self, start: usize, end: usize) -> bool {
        char::decode_utf16(self.units[start..end].iter().copied())
            .all(|c| c.is_ok_and(char::is_whitespace))
    }

    /// The status card for `level` with `kept` cuts excluded. Words are
    /// counted as a union over the active cuts, with the same word definition
    /// as the editor's word count, so `words_after` is exactly the word count
    /// of the text "Make the cuts" produces.
    pub fn status(&self, level: TrimLevel, kept: &[CutId]) -> TrimStatus {
        let mut ranges: Vec<(usize, usize)> = self
            .active(level, kept)
            .iter()
            .map(|c| (c.start, c.end))
            .collect();
        ranges.sort_unstable();
        let mut cut = 0usize;
        let mut covered_to = 0usize;
        for (s, e) in ranges {
            let s = s.max(covered_to);
            if e > s {
                cut += self.words_in(s, e);
                covered_to = e;
            }
        }
        let percent = if self.total_words == 0 {
            0.0
        } else {
            100.0 * cut as f64 / self.total_words as f64
        };
        TrimStatus {
            level,
            words_before: self.total_words,
            words_after: self.total_words - cut,
            percent,
            target_percent: 100.0 * level.target_fraction(),
        }
    }
}

/// Rank tiers: word, clause, sentence, then paragraph openers and short
/// sentences (eligible only at [`TrimLevel::Half`]).
const RANK_OPENER: u8 = 3;

/// A candidate on byte offsets.
#[derive(Debug, Clone)]
struct Candidate {
    start: usize,
    end: usize,
    tier: Tier,
    rank: u8,
    primary: f64,
    tiebreak: f64,
    reason: String,
}

/// Plan every trim level for `body`.
///
/// The result depends only on `body` and `config` (the role's style lists,
/// thesaurus and rolegraph, and [`crate::LabOptions::min_sentence_words`] for
/// the short-sentence rule).
pub fn trim_plan(body: &str, config: &LabConfig) -> TrimPlan {
    let analysis = Analysis::new(body, config);
    let doc = &analysis.doc;
    let role = config.role.as_ref().map(|r| (r, analysis.concept_hits(r)));
    let scores = weak::score(
        doc,
        &analysis.style_hits,
        role.as_ref().map(|(r, hits)| (*r, hits.as_slice())),
    );
    let connectives: Vec<(usize, usize, Connective)> = find_connectives(&analysis.folded.text)
        .into_iter()
        .map(|(s, e, k)| {
            let (s, e) = analysis.folded.to_original(s, e);
            (s, e, k)
        })
        .filter(|&(s, e, _)| !doc.is_protected(s, e))
        .collect();

    let mut candidates = Vec::new();
    word_candidates(doc, &analysis.style_hits, &scores, &mut candidates);
    clause_candidates(doc, &connectives, &scores, &mut candidates);
    sentence_candidates(
        doc,
        &scores,
        config.options.min_sentence_words,
        &mut candidates,
    );
    candidates.sort_by(|x, y| {
        x.rank
            .cmp(&y.rank)
            .then(quantise(y.primary).cmp(&quantise(x.primary)))
            .then(quantise(y.tiebreak).cmp(&quantise(x.tiebreak)))
            .then(x.start.cmp(&y.start))
            .then(x.end.cmp(&y.end))
    });
    // The same range twice (a filler that is a whole clause): keep the
    // better-ranked one.
    let mut seen = std::collections::BTreeSet::new();
    candidates.retain(|c| seen.insert((c.start, c.end)));

    let levels = fit(doc, &candidates);

    let mut picked: Vec<(Candidate, TrimLevel)> = candidates
        .into_iter()
        .zip(levels)
        .filter_map(|(c, l)| Some((c, l?)))
        .collect();
    picked.sort_by_key(|a| (a.0.start, a.0.end));

    let mut cuts: Vec<Cut> = picked
        .into_iter()
        .enumerate()
        .map(|(i, (c, level))| Cut {
            id: CutId(i as u32),
            start: c.start,
            end: c.end,
            tier: c.tier,
            first_level: level,
            score: c.primary,
            reason: c.reason,
            words: doc.word_count(c.start, c.end),
        })
        .collect();
    let mut word_starts: Vec<usize> = doc.words.iter().map(|w| w.0).collect();
    {
        let mut offsets: Vec<&mut usize> = cuts
            .iter_mut()
            .flat_map(|c| [&mut c.start, &mut c.end])
            .chain(word_starts.iter_mut())
            .collect();
        bytes_to_utf16(body, &mut offsets);
    }
    TrimPlan {
        total_words: doc.words.len(),
        cuts,
        word_starts,
        units: body.encode_utf16().collect(),
    }
}

fn quantise(x: f64) -> i64 {
    (x / 1e-9).round() as i64
}

/// The selection for every cutting level: `Some(level)` for the first level
/// that fades each candidate (candidates are in rank order).
fn fit(doc: &Doc<'_>, candidates: &[Candidate]) -> Vec<Option<TrimLevel>> {
    let total = doc.words.len();
    // Each candidate covers a contiguous run of word indices.
    let ranges: Vec<(usize, usize)> = candidates
        .iter()
        .map(|c| {
            (
                doc.words.partition_point(|w| w.0 < c.start),
                doc.words.partition_point(|w| w.0 < c.end),
            )
        })
        .collect();
    let mut mask = vec![false; total];
    let mut cut = 0usize;
    let mut level_of: Vec<Option<TrimLevel>> = vec![None; candidates.len()];
    let new_words = |mask: &[bool], i: usize| {
        let (lo, hi) = ranges[i];
        mask[lo..hi].iter().filter(|&&m| !m).count()
    };
    let take = |mask: &mut [bool], i: usize| {
        let (lo, hi) = ranges[i];
        mask[lo..hi].iter_mut().for_each(|m| *m = true);
    };
    // Pass 2 prefers the best-ranked candidate that lands within 1% of the
    // document (at least one word) of the target.
    let band = ((total as f64 * 0.01).round() as i64).max(1);
    for level in &TrimLevel::ALL[1..] {
        let level = *level;
        let target = level.target_words(total);
        let max_rank = if level == TrimLevel::Half {
            RANK_OPENER
        } else {
            RANK_OPENER - 1
        };
        let eligible = |i: usize, level_of: &[Option<TrimLevel>]| {
            level_of[i].is_none() && candidates[i].rank <= max_rank
        };
        // Pass 1: rank order, never overshoot.
        for i in 0..candidates.len() {
            if cut >= target {
                break;
            }
            if !eligible(i, &level_of) {
                continue;
            }
            let n = new_words(&mask, i);
            if n > 0 && cut + n <= target {
                take(&mut mask, i);
                cut += n;
                level_of[i] = Some(level);
            }
        }
        // Pass 2: while it strictly helps, take the best-ranked candidate
        // within `band` of the target, else the one that lands closest.
        loop {
            let dist = (target as i64 - cut as i64).abs();
            if dist == 0 {
                break;
            }
            let mut in_band: Option<usize> = None;
            let mut closest: Option<(i64, usize, usize)> = None;
            for i in 0..candidates.len() {
                if !eligible(i, &level_of) {
                    continue;
                }
                let n = new_words(&mask, i);
                if n == 0 {
                    continue;
                }
                let d = (target as i64 - (cut + n) as i64).abs();
                if d >= dist {
                    continue;
                }
                if d <= band && in_band.is_none() {
                    in_band = Some(i);
                }
                if closest.is_none_or(|(cd, _, _)| d < cd) {
                    closest = Some((d, i, n));
                }
            }
            let Some(i) = in_band.or(closest.map(|(_, i, _)| i)) else {
                break;
            };
            cut += new_words(&mask, i);
            take(&mut mask, i);
            level_of[i] = Some(level);
        }
    }
    level_of
}

// ------------------------------------------------------------ candidates

fn prev_char(text: &str, pos: usize) -> Option<char> {
    text[..pos].chars().next_back()
}

fn next_char(text: &str, pos: usize) -> Option<char> {
    text[pos..].chars().next()
}

/// Length in bytes of the whitespace run ending at `pos`, not before `floor`.
fn ws_before(text: &str, floor: usize, pos: usize) -> usize {
    let s = &text[floor..pos];
    s.len() - s.trim_end().len()
}

/// Length in bytes of the whitespace run starting at `pos`, not past `ceil`.
fn ws_after(text: &str, pos: usize, ceil: usize) -> usize {
    let s = &text[pos..ceil];
    s.len() - s.trim_start().len()
}

/// Index of the sentence holding `start..end`, if one does.
fn sentence_of(doc: &Doc<'_>, start: usize, end: usize) -> Option<usize> {
    let i = doc.sentences.partition_point(|s| s.end <= start);
    doc.sentences
        .get(i)
        .filter(|s| s.start <= start && end <= s.end)
        .map(|_| i)
}

/// Characters a mid-sentence word cut may be followed by.
fn ends_clause(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '\u{2014}'),
    }
}

/// Fillers and hedges (not verb-phrase hedges, not tone words).
fn word_candidates(
    doc: &Doc<'_>,
    style_hits: &[(usize, usize, StyleCategory)],
    scores: &[Weakness],
    out: &mut Vec<Candidate>,
) {
    let text = doc.text;
    for &(hs, he, cat) in style_hits {
        let label = match cat {
            StyleCategory::Filler => "filler",
            StyleCategory::Hedge => "hedge",
            StyleCategory::HedgePhrase | StyleCategory::Tone => continue,
        };
        let Some(si) = sentence_of(doc, hs, he) else {
            continue;
        };
        let s = doc.sentences[si];
        // Whole words under the crate's word definition only: a hit on
        // "quite" in "quite-good" would count a word it does not remove.
        let words = doc.words_in(s.start, s.end);
        let first = words.iter().position(|w| w.0 == hs);
        let last = words.iter().position(|w| w.1 == he);
        let (Some(first), Some(last)) = (first, last) else {
            continue;
        };
        if last < first || last - first + 1 == words.len() {
            continue;
        }
        let (start, end) = if hs == s.start {
            // Sentence-initial: the term, a following comma, the gap after.
            let mut b = he;
            if text[b..].starts_with(',') {
                b += 1;
            }
            b += ws_after(text, b, s.end);
            if b == he {
                continue;
            }
            (hs, b)
        } else {
            // Mid-sentence: the gap before it; the comma before it too when
            // the term is bracketed by punctuation (", generally.").
            let gap = ws_before(text, s.start, hs);
            let after = next_char(text, he);
            if gap == 0 || !ends_clause(after) {
                continue;
            }
            let mut a = hs - gap;
            if prev_char(text, a) == Some(',') && after.is_some_and(|c| !c.is_whitespace()) {
                a -= 1;
            }
            (a, he)
        };
        if doc.is_protected(start, end) {
            continue;
        }
        out.push(Candidate {
            start,
            end,
            tier: Tier::Word,
            rank: 0,
            primary: scores[si].primary,
            tiebreak: scores[si].tiebreak,
            reason: format!("{label} \"{}\"", text[hs..he].to_lowercase()),
        });
    }
}

/// Position of the sentence's final terminator run (tail clauses keep it).
fn terminator_pos(text: &str, s: &Sentence) -> usize {
    let body = &text[s.start..s.end];
    let trimmed = body.trim_end_matches(|c: char| {
        matches!(
            c,
            '.' | '!' | '?' | '"' | '\u{201D}' | ')' | '*' | '`' | '_' | ':' | ';'
        )
    });
    s.start + trimmed.len()
}

/// The connective starting exactly at `pos`, if any.
fn connective_at(
    connectives: &[(usize, usize, Connective)],
    pos: usize,
) -> Option<(usize, usize, Connective)> {
    let i = connectives.partition_point(|c| c.0 < pos);
    connectives.get(i).filter(|c| c.0 == pos).copied()
}

fn clause_candidates(
    doc: &Doc<'_>,
    connectives: &[(usize, usize, Connective)],
    scores: &[Weakness],
    out: &mut Vec<Candidate>,
) {
    let text = doc.text;
    for (si, s) in doc.sentences.iter().enumerate() {
        let sent_words = doc.word_count(s.start, s.end);
        let term = terminator_pos(text, s);
        let ok_size = |a: usize, b: usize| {
            let w = doc.word_count(a, b);
            w >= 1 && (w as f64) <= 0.6 * sent_words as f64
        };
        let mut push = |start: usize, end: usize, reason: String| {
            if end > start && ok_size(start, end) && !doc.is_protected(start, end) {
                out.push(Candidate {
                    start,
                    end,
                    tier: Tier::Clause,
                    rank: 1,
                    primary: scores[si].primary,
                    tiebreak: scores[si].tiebreak,
                    reason,
                });
            }
        };
        // Structural positions outside protected text and brackets.
        let mut commas = Vec::new();
        let mut dashes = Vec::new();
        let mut hard = Vec::new();
        let mut depth = 0i32;
        let mut paren_open: Option<usize> = None;
        for (off, c) in text[s.start..s.end].char_indices() {
            let p = s.start + off;
            if doc.is_protected(p, p + c.len_utf8()) {
                continue;
            }
            match c {
                '(' => {
                    if depth == 0 {
                        paren_open = Some(p);
                    }
                    depth += 1;
                }
                ')' => {
                    depth -= 1;
                    if depth == 0
                        && let Some(o) = paren_open.take()
                    {
                        // "(a)" enumerations are not asides.
                        let single_char = text[o + 1..p].chars().count() <= 1;
                        let a = o - ws_before(text, s.start, o);
                        if !single_char && a > s.start {
                            push(a, p + 1, "parenthetical".into());
                        }
                    }
                    depth = depth.max(0);
                }
                ',' if depth == 0 => commas.push(p),
                '\u{2014}' if depth == 0 => dashes.push(p),
                ';' | ':' if depth == 0 => hard.push(p),
                _ => {}
            }
        }
        // Em-dash asides: a pair cuts the first dash and the aside and keeps
        // the closing dash; an unpaired final dash cuts to the terminator,
        // unless the tail opens with a resumptive word (it is the main
        // clause of a periodic sentence, not an aside).
        let mut k = 0;
        while k < dashes.len() {
            let d = dashes[k];
            let (end, reason) = match dashes.get(k + 1) {
                Some(&next) => (next, "dash aside"),
                None => (term, "dash tail"),
            };
            let resumptive = reason == "dash tail"
                && doc
                    .words_in(d, end)
                    .first()
                    .and_then(|w| connective_at(connectives, w.0).filter(|c| c.1 == w.1))
                    .is_some_and(|c| c.2 == Connective::Resumptive);
            if !resumptive {
                push(d, end, reason.into());
            }
            k += 2;
        }
        // Comma asides opened by a connective, up to the next comma, break,
        // dash or the terminator; skipped when the next clause opens with
        // "but" or "yet" (it depends on the aside).
        for &c in &commas {
            let q = c + 1 + ws_after(text, c + 1, s.end);
            let Some((_, oe, Connective::Aside)) = connective_at(connectives, q) else {
                continue;
            };
            let next = commas
                .iter()
                .chain(&hard)
                .chain(&dashes)
                .copied()
                .filter(|&p| p > c)
                .min()
                .unwrap_or(term)
                .min(term);
            if next < oe {
                continue;
            }
            let dependent = doc
                .words_in(next, s.end)
                .first()
                .and_then(|w| connective_at(connectives, w.0).filter(|k| k.1 == w.1))
                .is_some_and(|k| k.2 == Connective::Contrast);
            if dependent {
                continue;
            }
            let opener = text[q..oe].to_lowercase();
            // A relative clause between commas ("X, which Y, is") takes its
            // closing comma too, unless a connective follows it ("X, which
            // Y, though Z" keeps it): otherwise the cut would leave a comma
            // between subject and verb.
            let relative = matches!(opener.as_str(), "which" | "who" | "whom" | "whose");
            let mut end = next;
            if relative && text[next..].starts_with(',') {
                let continues = doc
                    .words_in(next + 1, s.end)
                    .first()
                    .and_then(|w| connective_at(connectives, w.0).filter(|k| k.1 == w.1))
                    .is_some();
                if !continues {
                    end = next + 1;
                }
            }
            push(c, end, format!("aside \"{opener}\""));
        }
    }
}

fn sentence_candidates(
    doc: &Doc<'_>,
    scores: &[Weakness],
    min_words: usize,
    out: &mut Vec<Candidate>,
) {
    for (si, s) in doc.sentences.iter().enumerate() {
        let next_in_para = doc.sentences.get(si + 1).filter(|n| n.para == s.para);
        let (start, end) = if s.first_in_para {
            match next_in_para {
                // The opener takes the gap after it, so the next sentence
                // moves up cleanly.
                Some(n) => (s.start, n.start),
                // A one-sentence paragraph goes with its whole block; one
                // whose block cannot go cleanly is not a candidate.
                None => match doc.block_extent(s.para) {
                    Some(extent) => extent,
                    None => continue,
                },
            }
        } else {
            (doc.sentences[si - 1].end, s.end)
        };
        // A sentence holding inline code (or any protected text) is kept
        // whole: cutting it would cut the code with it.
        if doc.is_protected(start, end) {
            continue;
        }
        let short = scores[si].words < min_words;
        let (rank, reason) = if s.first_in_para {
            (RANK_OPENER, "paragraph-opening sentence")
        } else if short {
            (RANK_OPENER, "short sentence")
        } else {
            (2, "weak sentence")
        };
        out.push(Candidate {
            start,
            end,
            tier: Tier::Sentence,
            rank,
            primary: scores[si].primary,
            tiebreak: scores[si].tiebreak,
            reason: reason.into(),
        });
    }
}
