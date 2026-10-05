//! "Mark the weakest sentences": rolegraph-primary weakness ranking.
//!
//! The ranking key is lexicographic:
//!
//! 1. **primary** (role data only): how little the sentence carries the
//!    selected role's knowledge, plus how hedged and padded it is according to
//!    the role's KG lists;
//! 2. **tie-break** (text only): lexical centrality, neighbour redundancy and
//!    function-word ratio, computed from the document itself;
//! 3. document position.
//!
//! Text-only signals never outweigh a difference in the primary score; they
//! only order sentences whose primary scores are equal (to 1e-9).

use std::collections::{BTreeMap, BTreeSet};

use crate::lists::{Hit, StyleCategory};
use crate::rolegraph::{GraphScore, RoleKnowledge, graph_scores};
use crate::text::{Doc, word_spans};

/// Weight of the rolegraph term `1 - strength` in the primary score.
pub const W_GRAPH: f64 = 0.75;
/// Weight of the role-KG hedge and filler density in the primary score.
pub const W_STYLE: f64 = 0.25;
/// Tie-break weight of low lexical centrality.
pub const T_LOW_CENTRALITY: f64 = 0.5;
/// Tie-break weight of neighbour redundancy.
pub const T_REDUNDANCY: f64 = 0.3;
/// Tie-break weight of the function-word ratio.
pub const T_FUNCTION: f64 = 0.2;

/// Quantum used when comparing scores, so float noise never reorders ties.
const QUANTUM: f64 = 1e-9;

const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "above",
    "after",
    "again",
    "all",
    "also",
    "although",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "being",
    "both",
    "but",
    "by",
    "came",
    "can",
    "come",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "down",
    "each",
    "even",
    "ever",
    "every",
    "few",
    "for",
    "from",
    "further",
    "get",
    "go",
    "got",
    "had",
    "has",
    "have",
    "having",
    "he",
    "her",
    "here",
    "hers",
    "him",
    "himself",
    "his",
    "how",
    "however",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "made",
    "make",
    "may",
    "me",
    "might",
    "more",
    "most",
    "much",
    "must",
    "my",
    "myself",
    "no",
    "nor",
    "not",
    "now",
    "of",
    "off",
    "on",
    "once",
    "one",
    "only",
    "or",
    "other",
    "our",
    "ours",
    "out",
    "over",
    "own",
    "said",
    "same",
    "say",
    "shall",
    "she",
    "should",
    "so",
    "some",
    "still",
    "such",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "therefore",
    "these",
    "they",
    "thing",
    "things",
    "this",
    "those",
    "though",
    "through",
    "thus",
    "to",
    "too",
    "two",
    "under",
    "until",
    "up",
    "upon",
    "us",
    "very",
    "was",
    "way",
    "we",
    "well",
    "went",
    "were",
    "what",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "who",
    "whom",
    "whose",
    "why",
    "will",
    "with",
    "would",
    "yet",
    "you",
    "your",
    "yours",
];

fn is_stopword(w: &str) -> bool {
    STOPWORDS.binary_search(&w).is_ok()
}

/// Crude, deterministic lemma: drop a possessive and a plural `s`.
fn lemma(lower: &str) -> String {
    let mut l = lower.replace('\u{2019}', "'");
    if let Some(stripped) = l.strip_suffix("'s") {
        l = stripped.to_string();
    }
    if l.len() > 3 && l.ends_with('s') && !l.ends_with("ss") {
        l.pop();
    }
    l
}

/// One sentence's weakness.
#[derive(Debug, Clone)]
pub(crate) struct Weakness {
    pub words: usize,
    /// Role-derived score in `[0, 1]`; higher is weaker.
    pub primary: f64,
    /// Text-only score in `[0, 1]`; orders equal primaries.
    pub tiebreak: f64,
    pub graph: GraphScore,
    /// Hedge and filler matches in the sentence.
    pub hedges: usize,
}

/// Score every sentence of `doc`.
pub(crate) fn score(
    doc: &Doc<'_>,
    style_hits: &[(usize, usize, StyleCategory)],
    role: Option<(&RoleKnowledge, &[Hit])>,
) -> Vec<Weakness> {
    let text = doc.text;
    let n = doc.sentences.len();
    let spans: Vec<(usize, usize)> = doc.sentences.iter().map(|s| (s.start, s.end)).collect();
    let graph = match role {
        Some((r, hits)) => graph_scores(r, hits, &spans),
        None => vec![GraphScore::default(); n],
    };

    // Hedge and filler counts per sentence (hits are sorted by start).
    let mut hedges = vec![0usize; n];
    for &(hs, _, cat) in style_hits {
        if cat == StyleCategory::Tone {
            continue;
        }
        let i = spans.partition_point(|s| s.1 <= hs);
        if spans.get(i).is_some_and(|s| s.0 <= hs) {
            hedges[i] += 1;
        }
    }

    // Text-only features.
    let mut sets: Vec<BTreeSet<String>> = Vec::with_capacity(n);
    let mut function_ratio = Vec::with_capacity(n);
    let mut words = Vec::with_capacity(n);
    for &(s, e) in &spans {
        let sentence = &text[s..e];
        let mut set = BTreeSet::new();
        let mut total = 0usize;
        let mut function = 0usize;
        for (ws, we) in word_spans(sentence) {
            total += 1;
            let lower = sentence[ws..we].to_lowercase();
            if is_stopword(&lower) {
                function += 1;
            } else if lower.chars().count() > 1 && !lower.chars().all(|c| c.is_ascii_digit()) {
                set.insert(lemma(&lower));
            }
        }
        words.push(total);
        function_ratio.push(function as f64 / total.max(1) as f64);
        sets.push(set);
    }
    let mut df: BTreeMap<&str, usize> = BTreeMap::new();
    for set in &sets {
        for l in set {
            *df.entry(l.as_str()).or_default() += 1;
        }
    }
    let raw_centrality: Vec<f64> = sets
        .iter()
        .map(|set| {
            if set.is_empty() {
                0.0
            } else {
                set.iter()
                    .map(|l| ((df[l.as_str()] - 1) as f64).ln_1p())
                    .sum::<f64>()
                    / set.len() as f64
            }
        })
        .collect();
    let max_centrality = raw_centrality.iter().copied().fold(0.0, f64::max);
    let raw_hedge: Vec<f64> = (0..n)
        .map(|i| hedges[i] as f64 / words[i].max(1) as f64)
        .collect();
    let max_hedge = raw_hedge.iter().copied().fold(0.0, f64::max);

    graph
        .into_iter()
        .enumerate()
        .map(|(i, graph)| {
            let mut redundancy: f64 = 0.0;
            for j in i.saturating_sub(2)..(i + 3).min(n) {
                if j == i || sets[i].is_empty() || sets[j].is_empty() {
                    continue;
                }
                let inter = sets[i].intersection(&sets[j]).count() as f64;
                let union = (sets[i].len() + sets[j].len()) as f64 - inter;
                redundancy = redundancy.max(inter / union);
            }
            let centrality = if max_centrality > 0.0 {
                raw_centrality[i] / max_centrality
            } else {
                0.0
            };
            let hedge = if max_hedge > 0.0 {
                raw_hedge[i] / max_hedge
            } else {
                0.0
            };
            Weakness {
                words: words[i],
                primary: W_GRAPH * (1.0 - graph.strength) + W_STYLE * hedge,
                tiebreak: T_LOW_CENTRALITY * (1.0 - centrality)
                    + T_REDUNDANCY * redundancy
                    + T_FUNCTION * function_ratio[i],
                graph,
                hedges: hedges[i],
            }
        })
        .collect()
}

fn quantise(x: f64) -> i64 {
    (x / QUANTUM).round() as i64
}

/// The sentences to mark, weakest first: paragraph openers and sentences
/// under `min_words` are never eligible; `share` of all prose sentences
/// (rounded up, at least one) are marked, capped at the eligible count.
pub(crate) fn select(
    scores: &[Weakness],
    doc: &Doc<'_>,
    share: f64,
    min_words: usize,
) -> Vec<usize> {
    let n = scores.len();
    if n == 0 {
        return Vec::new();
    }
    let mut eligible: Vec<usize> = (0..n)
        .filter(|&i| !doc.sentences[i].first_in_para && scores[i].words >= min_words)
        .collect();
    eligible.sort_by(|&a, &b| {
        quantise(scores[b].primary)
            .cmp(&quantise(scores[a].primary))
            .then(quantise(scores[b].tiebreak).cmp(&quantise(scores[a].tiebreak)))
            .then(a.cmp(&b))
    });
    let k = ((n as f64 * share).ceil() as usize)
        .max(1)
        .min(eligible.len());
    eligible.truncate(k);
    eligible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwords_are_sorted_for_binary_search() {
        let mut sorted = STOPWORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, STOPWORDS);
    }

    #[test]
    fn tie_break_never_outweighs_primary() {
        // Weights are documented as lexicographic, not additive.
        let doc = Doc::parse(
            "Opener sentence here today. Alpha beta gamma delta epsilon. Zeta eta theta iota kappa.",
        );
        let mut s = score(&doc, &[], None);
        s[1].primary = 0.5;
        s[1].tiebreak = 1.0;
        s[2].primary = 0.5 + 1e-6;
        s[2].tiebreak = 0.0;
        assert_eq!(select(&s, &doc, 1.0, 5), vec![2, 1]);
    }
}
