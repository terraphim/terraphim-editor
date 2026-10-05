//! Research prototype for terraphim-editor issue #3.
//!
//! Deterministic (no LLM) heuristics for two Lab actions:
//! * "Mark the weakest sentences" -> [`score_sentences`]
//! * trim levels ~10/20/30/50% -> [`trim`]
//!
//! Every cut is a byte span into the (normalised) document body. Nothing is
//! ever rewritten: the UI fades the spans (ghost treatment, R-5.1) and
//! "Make the cuts" deletes them verbatim.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use terraphim_automata::{
    load_thesaurus_from_json, parse_markdown_directives_str, CompiledMatcher, MatcherOptions,
};

/// Trim levels from R-8.3: (label, target fraction of words to cut).
pub const LEVELS: [(&str, f64); 4] = [
    ("Slight trim", 0.10),
    ("Tighten more", 0.20),
    ("Even sharper", 0.30),
    ("Cut in half", 0.50),
];

/// Share of sentences flagged by "Mark the weakest sentences".
pub const WEAKEST_SHARE: f64 = 0.15;

/// Acceptance tolerance from issue #3, in percentage points.
pub const TOLERANCE_PP: f64 = 3.0;

// ---------------------------------------------------------------- words

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_joiner(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-' | '.' | '/')
}

/// The one word definition used for totals, cut counts and the status card.
///
/// A word is a maximal run of alphanumerics (and `_`), where an apostrophe,
/// hyphen, full stop or slash joins two runs only when it sits between
/// word characters. So `us\u{2014}George` is two words, `hadn\u{2019}t`,
/// `liver-pill`, `e.g`, `R-8.7` and `terraphim_lsp` are one word each, and
/// Markdown syntax (`**`, backticks, `#`) is never a word.
pub fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if !is_word_char(chars[i].1) {
            i += 1;
            continue;
        }
        let start = chars[i].0;
        let mut j = i + 1;
        while j < chars.len() {
            let c = chars[j].1;
            if is_word_char(c) {
                j += 1;
            } else if is_joiner(c) && j + 1 < chars.len() && is_word_char(chars[j + 1].1) {
                j += 2;
            } else {
                break;
            }
        }
        let end = if j < chars.len() {
            chars[j].0
        } else {
            text.len()
        };
        out.push((start, end));
        i = j;
    }
    out
}

pub fn word_count(text: &str) -> usize {
    word_spans(text).len()
}

/// Percentage of words cut, as shown on the status card (`535 -> 480 words`).
pub fn cut_percentage(total_words: usize, cut_words: usize) -> f64 {
    if total_words == 0 {
        return 0.0;
    }
    100.0 * cut_words as f64 / total_words as f64
}

/// Number of words to cut for a target fraction, rounded to nearest.
pub fn target_words(total_words: usize, fraction: f64) -> usize {
    (total_words as f64 * fraction).round() as usize
}

// ---------------------------------------------------------------- KG
//
// Matching is delegated to `terraphim_automata`: the KG markdown is parsed
// with its portable `parse_markdown_directives_str`, the thesaurus is loaded
// with `load_thesaurus_from_json`, and a `CompiledMatcher` (Aho-Corasick,
// LeftmostLongest, ASCII case-insensitive, word-boundary filtered) is built
// once per list and reused for every query. Nothing here re-implements the
// automaton, overlap resolution or the boundary rule.

/// `terraphim_automata`'s minimum pattern length (`MatcherBuilder` rejects
/// shorter patterns, so they are dropped while loading the lists).
const MIN_PATTERN_LENGTH: usize = terraphim_automata::compiled::DEFAULT_MIN_PATTERN_LENGTH;

/// One thesaurus entry: a surface term and the concept (file stem) it maps to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Term {
    pub term: String,
    pub concept: String,
}

/// Parse a Terraphim KG markdown file (`synonyms:: a, b, c`) with
/// `terraphim_automata::parse_markdown_directives_str`.
/// `include_name` mirrors the terraphim_automata builder, which also adds the
/// concept name itself as a pattern.
pub fn parse_kg_markdown(concept: &str, md: &str, include_name: bool) -> Vec<Term> {
    let parsed = parse_markdown_directives_str(concept, md);
    let synonyms = parsed
        .directives
        .get(concept)
        .map(|d| d.synonyms.clone())
        .unwrap_or_default();
    let name = include_name.then(|| concept.to_lowercase());
    name.into_iter()
        .chain(synonyms.into_iter().map(|s| s.to_lowercase()))
        .filter(|s| s.len() >= MIN_PATTERN_LENGTH)
        .map(|term| Term {
            term,
            concept: concept.to_string(),
        })
        .collect()
}

/// Load every `*.md` in `dir` (non-recursive), sorted by file name.
pub fn load_kg_dir(dir: &Path, include_name: bool) -> Vec<Term> {
    let mut paths: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.path()).collect(),
        Err(_) => return Vec::new(),
    };
    paths.retain(|p| p.extension().map(|x| x == "md").unwrap_or(false));
    paths.sort();
    let mut out = Vec::new();
    for p in paths {
        let stem = p.file_stem().unwrap().to_string_lossy().to_string();
        let md = fs::read_to_string(&p).unwrap_or_default();
        out.extend(parse_kg_markdown(&stem, &md, include_name));
    }
    out
}

/// Thesaurus content for prose: curly apostrophes (Gutenberg, smart quotes)
/// must match the ASCII apostrophes in the lists, so both spellings become
/// thesaurus keys for the same concept. This is data, not matcher logic.
fn with_apostrophe_variants(terms: &[Term]) -> Vec<Term> {
    let mut out = Vec::with_capacity(terms.len());
    for t in terms {
        out.push(t.clone());
        if t.term.contains('\'') {
            out.push(Term {
                term: t.term.replace('\'', "\u{2019}"),
                concept: t.concept.clone(),
            });
        }
    }
    out
}

/// Emit the terms in the terraphim thesaurus JSON shape
/// `{"name": .., "data": {term: {"id": n, "nterm": concept}}}`, with sorted
/// keys so the artefact is deterministic. Concept ids start at 100 in order
/// of first appearance; a term listed under two concepts keeps the first.
pub fn thesaurus_json(name: &str, terms: &[Term]) -> String {
    let mut ids: BTreeMap<&str, u64> = BTreeMap::new();
    for t in terms {
        let next = 100 + ids.len() as u64;
        ids.entry(t.concept.as_str()).or_insert(next);
    }
    let mut data: BTreeMap<&str, serde_json::Value> = BTreeMap::new();
    for t in terms {
        data.entry(t.term.as_str()).or_insert_with(
            || serde_json::json!({"id": ids[t.concept.as_str()], "nterm": t.concept}),
        );
    }
    let doc = serde_json::json!({"name": name, "data": data});
    let mut s = serde_json::to_string_pretty(&doc).expect("thesaurus JSON serialises");
    s.push('\n');
    s
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
    pub term: String,
    pub concept: String,
}

/// A compiled `terraphim_automata` matcher over one KG list.
pub struct Matcher {
    compiled: CompiledMatcher,
}

impl Matcher {
    /// Build the thesaurus JSON, load it with `load_thesaurus_from_json` and
    /// compile it once. The JSON is exactly what `out/thesaurus.json` ships.
    pub fn new(terms: &[Term]) -> Matcher {
        let json = thesaurus_json("Lab heuristics", &with_apostrophe_variants(terms));
        let thesaurus = load_thesaurus_from_json(&json).expect("valid thesaurus JSON");
        let compiled = CompiledMatcher::from_thesaurus(&thesaurus, MatcherOptions::default())
            .expect("KG terms compile");
        Matcher { compiled }
    }

    /// Positioned, non-overlapping (leftmost-longest), word-boundary-filtered
    /// matches, in text order.
    pub fn find(&self, text: &str) -> Vec<Hit> {
        self.compiled
            .find_matches(text, true)
            .expect("CompiledMatcher::find_matches does not fail")
            .into_iter()
            .filter_map(|m| {
                let (start, end) = m.pos?;
                Some(Hit {
                    start,
                    end,
                    term: m.term,
                    concept: m.normalized_term.value.as_str().to_string(),
                })
            })
            .collect()
    }
}

/// The KG lists the Lab needs, loaded from `kg/` and `kg/domain/`.
pub struct Lists {
    pub style: Matcher,
    pub domain: Matcher,
    pub style_terms: Vec<Term>,
    pub domain_terms: Vec<Term>,
}

pub const CUTTABLE: [&str; 2] = ["lab-filler", "lab-hedge"];
pub const HEDGE_OR_FILLER: [&str; 3] = ["lab-filler", "lab-hedge", "lab-hedge-phrase"];
pub const TONE: &str = "lab-tone";

impl Lists {
    pub fn load(kg_root: &Path) -> Lists {
        let style_terms = load_kg_dir(kg_root, false);
        let domain_terms = load_kg_dir(&kg_root.join("domain"), true);
        Lists {
            style: Matcher::new(&style_terms),
            domain: Matcher::new(&domain_terms),
            style_terms,
            domain_terms,
        }
    }

    /// Every thesaurus key the matchers were compiled from (style and domain,
    /// including apostrophe variants), for `out/thesaurus.json`.
    pub fn thesaurus_terms(&self) -> Vec<Term> {
        let mut all = with_apostrophe_variants(&self.style_terms);
        all.extend(with_apostrophe_variants(&self.domain_terms));
        all
    }
}

// ---------------------------------------------------------------- document

#[derive(Debug, Clone)]
pub struct Sentence {
    pub start: usize,
    pub end: usize,
    pub para: usize,
    pub first_in_para: bool,
    /// For the opening sentence of a list item: where its marker starts, so
    /// cutting the sentence also removes the marker (no empty bullets).
    pub marker_start: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Doc {
    /// Normalised body: prose paragraphs unwrapped to one line, blocks
    /// separated by a blank line.
    pub text: String,
    pub sentences: Vec<Sentence>,
    /// Byte mask: true for headings, fences, lists, tables and inline code.
    pub protected: Vec<bool>,
    pub words: Vec<(usize, usize)>,
}

fn block_is_protected(first_line: &str) -> bool {
    let t = first_line.trim_start();
    t.starts_with('#') || t.starts_with("```") || t.starts_with('|') || t.starts_with('>')
}

/// Byte length of a list marker (`- `, `* `, `12. `) including indentation.
fn list_marker_len(line: &str) -> Option<usize> {
    let indent = line.len() - line.trim_start().len();
    let t = &line[indent..];
    if t.starts_with("- ") || t.starts_with("* ") {
        return Some(indent + 2);
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && t[digits..].starts_with(". ") {
        return Some(indent + digits + 2);
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Prose,
    Protected,
    /// A list item; the value is the marker length. Items are prose, and
    /// the marker goes with the item's opening sentence when that is cut.
    Item(usize),
}

const ABBREVIATIONS: [&str; 10] = [
    "e.g.", "i.e.", "st.", "mr.", "mrs.", "dr.", "vs.", "cf.", "no.", "fig.",
];

fn split_sentences(text: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let slice = &text[start..end];
    let chars: Vec<(usize, char)> = slice.char_indices().collect();
    let mut out = Vec::new();
    let mut s_start = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if matches!(c, '.' | '!' | '?') {
            // abbreviation check on the token ending here
            let tok_start = slice[..pos]
                .rfind(char::is_whitespace)
                .map(|p| p + 1)
                .unwrap_or(0);
            let tok: String = slice[tok_start..=pos]
                .trim_start_matches(|ch: char| !ch.is_alphanumeric())
                .to_lowercase();
            // "**No.**" is a sentence, not the abbreviation "no."
            let bold_start = slice[tok_start..].starts_with("**");
            if c == '.' && ABBREVIATIONS.contains(&tok.as_str()) && !bold_start {
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < chars.len()
                && matches!(
                    chars[j].1,
                    '.' | '!' | '?' | '"' | '\u{201D}' | '\u{2019}' | ')' | ']' | '*' | '`' | '_'
                )
            {
                j += 1;
            }
            if j < chars.len() && chars[j].1.is_whitespace() {
                let mut k = j;
                while k < chars.len() && chars[k].1.is_whitespace() {
                    k += 1;
                }
                if k < chars.len() {
                    let n = chars[k].1;
                    if n.is_uppercase()
                        || n.is_ascii_digit()
                        || matches!(
                            n,
                            '\u{201C}' | '"' | '(' | '*' | '`' | '_' | '[' | '\u{2018}'
                        )
                    {
                        out.push((start + s_start, start + chars[j].0));
                        s_start = chars[k].0;
                        i = k;
                        continue;
                    }
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    if s_start < slice.len() {
        let tail = slice[s_start..].trim_end();
        if !tail.is_empty() {
            out.push((start + s_start, start + s_start + tail.len()));
        }
    }
    out
}

impl Doc {
    pub fn parse(raw: &str) -> Doc {
        // 1. group lines into blocks
        // (kind, body, separator before the block)
        let mut blocks: Vec<(BlockKind, String, &'static str)> = Vec::new();
        let mut cur: Vec<&str> = Vec::new();
        let mut in_fence = false;
        let flush = |cur: &mut Vec<&str>, blocks: &mut Vec<(BlockKind, String, &'static str)>| {
            if cur.is_empty() {
                return;
            }
            if block_is_protected(cur[0]) || cur[0].trim_start().starts_with("```") {
                blocks.push((BlockKind::Protected, cur.join("\n"), "\n\n"));
            } else if list_marker_len(cur[0]).is_some() {
                // one block per item; continuation lines join their item
                let mut items: Vec<(usize, String)> = Vec::new();
                for l in cur.iter() {
                    match list_marker_len(l) {
                        Some(m) => items.push((m, format!("{}{}", &l[..m], l[m..].trim()))),
                        None => {
                            let last = items.last_mut().unwrap();
                            last.1.push(' ');
                            last.1.push_str(l.trim());
                        }
                    }
                }
                for (k, (m, body)) in items.into_iter().enumerate() {
                    blocks.push((BlockKind::Item(m), body, if k == 0 { "\n\n" } else { "\n" }));
                }
            } else {
                let body = cur.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
                blocks.push((BlockKind::Prose, body, "\n\n"));
            }
            cur.clear();
        };
        for line in raw.lines() {
            if line.trim_start().starts_with("```") {
                if !in_fence {
                    flush(&mut cur, &mut blocks);
                }
                in_fence = !in_fence;
                cur.push(line);
                if !in_fence {
                    flush(&mut cur, &mut blocks);
                }
                continue;
            }
            if in_fence {
                cur.push(line);
                continue;
            }
            if line.trim().is_empty() {
                flush(&mut cur, &mut blocks);
            } else {
                cur.push(line);
            }
        }
        flush(&mut cur, &mut blocks);

        // 2. build text, protection mask and sentences
        let mut text = String::new();
        let mut protected_ranges = Vec::new();
        let mut sentences = Vec::new();
        for (para, (kind, body, sep)) in blocks.iter().enumerate() {
            if !text.is_empty() {
                text.push_str(sep);
            }
            let start = text.len();
            text.push_str(body);
            let end = text.len();
            match kind {
                BlockKind::Protected => protected_ranges.push((start, end)),
                BlockKind::Prose | BlockKind::Item(_) => {
                    let (prose_start, marker_start) = match kind {
                        BlockKind::Item(m) => (start + m, Some(start)),
                        _ => (start, None),
                    };
                    for (k, (s, e)) in split_sentences(&text, prose_start, end)
                        .into_iter()
                        .enumerate()
                    {
                        sentences.push(Sentence {
                            start: s,
                            end: e,
                            para,
                            first_in_para: k == 0,
                            marker_start: if k == 0 { marker_start } else { None },
                        });
                    }
                }
            }
        }
        let mut protected = vec![false; text.len()];
        for (s, e) in protected_ranges {
            protected[s..e].iter_mut().for_each(|b| *b = true);
        }
        // inline code spans
        let mut open: Option<usize> = None;
        for (i, c) in text.char_indices() {
            if c == '`' {
                match open {
                    None => open = Some(i),
                    Some(o) => {
                        protected[o..=i].iter_mut().for_each(|b| *b = true);
                        open = None;
                    }
                }
            } else if c == '\n' {
                open = None;
            }
        }
        let words = word_spans(&text);
        Doc {
            text,
            sentences,
            protected,
            words,
        }
    }

    pub fn total_words(&self) -> usize {
        self.words.len()
    }

    /// Indices of words whose first byte lies in `[start, end)`.
    pub fn words_in(&self, start: usize, end: usize) -> Vec<usize> {
        let lo = self.words.partition_point(|w| w.0 < start);
        let hi = self.words.partition_point(|w| w.0 < end);
        (lo..hi).collect()
    }

    pub fn sentence_text(&self, i: usize) -> &str {
        &self.text[self.sentences[i].start..self.sentences[i].end]
    }
}

// ---------------------------------------------------------------- weakness

const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "above",
    "after",
    "again",
    "all",
    "also",
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
    "can",
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
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "itself",
    "just",
    "me",
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
    "only",
    "or",
    "other",
    "our",
    "ours",
    "out",
    "over",
    "own",
    "same",
    "she",
    "should",
    "so",
    "some",
    "such",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "through",
    "to",
    "too",
    "under",
    "until",
    "up",
    "upon",
    "us",
    "very",
    "was",
    "we",
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
    "you",
    "your",
    "yours",
    "shall",
    "may",
    "might",
    "one",
    "two",
    "said",
    "say",
    "made",
    "make",
    "get",
    "got",
    "went",
    "go",
    "come",
    "came",
    "thing",
    "things",
    "way",
    "well",
    "still",
    "yet",
    "though",
    "although",
    "however",
    "therefore",
    "thus",
];

fn is_stopword(w: &str) -> bool {
    STOPWORDS.contains(&w)
}

/// Crude, deterministic lemma: lowercase, drop possessive, drop plural `s`.
fn lemma(w: &str) -> String {
    let mut l = w.to_lowercase().replace('\u{2019}', "'");
    if let Some(stripped) = l.strip_suffix("'s") {
        l = stripped.to_string();
    }
    if l.len() > 3 && l.ends_with('s') && !l.ends_with("ss") {
        l.pop();
    }
    l
}

fn content_lemmas(text: &str) -> Vec<String> {
    word_spans(text)
        .into_iter()
        .map(|(s, e)| text[s..e].to_lowercase())
        .filter(|w| {
            !is_stopword(w) && !w.chars().all(|c| c.is_ascii_digit()) && w.chars().count() > 1
        })
        .map(|w| lemma(&w))
        .collect()
}

#[derive(Debug, Clone)]
pub struct SentenceScore {
    pub sentence: usize,
    pub words: usize,
    /// (hedge + filler matches) / words, normalised to [0,1] by doc maximum.
    pub hedge_filler: f64,
    /// Lexical centrality in [0,1]: how much of the sentence's vocabulary
    /// recurs in the rest of the document (1 = most central).
    pub centrality: f64,
    /// Max Jaccard overlap of content lemmas with sentences within +/-2.
    pub redundancy: f64,
    /// Share of words that are function words.
    pub function_ratio: f64,
    /// Domain KG concept coverage in [0,1] (distinct concepts / 2, capped).
    pub kg_coverage: f64,
    pub weakness: f64,
}

/// Feature weights for the weakness score. Length is deliberately absent:
/// "Mark sentences that run long" is its own R-8.2 action.
pub const W_HEDGE_FILLER: f64 = 0.35;
pub const W_LOW_CENTRALITY: f64 = 0.30;
pub const W_REDUNDANCY: f64 = 0.15;
pub const W_FUNCTION: f64 = 0.10;
pub const W_NO_KG: f64 = 0.10;

pub fn score_sentences(doc: &Doc, lists: &Lists) -> Vec<SentenceScore> {
    let n = doc.sentences.len();
    let lemmas: Vec<Vec<String>> = (0..n)
        .map(|i| content_lemmas(doc.sentence_text(i)))
        .collect();
    let sets: Vec<BTreeSet<String>> = lemmas.iter().map(|l| l.iter().cloned().collect()).collect();
    // document frequency: number of sentences containing each lemma
    let mut df: BTreeMap<&str, usize> = BTreeMap::new();
    for s in &sets {
        for l in s {
            *df.entry(l.as_str()).or_default() += 1;
        }
    }
    let mut raw_hf = Vec::with_capacity(n);
    let mut raw_cent = Vec::with_capacity(n);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let text = doc.sentence_text(i);
        let words = word_count(text).max(1);
        let hf = lists
            .style
            .find(text)
            .iter()
            .filter(|h| HEDGE_OR_FILLER.contains(&h.concept.as_str()))
            .count();
        raw_hf.push(hf as f64 / words as f64);
        let cent = if sets[i].is_empty() {
            0.0
        } else {
            sets[i]
                .iter()
                .map(|l| ((df[l.as_str()] - 1) as f64).ln_1p())
                .sum::<f64>()
                / sets[i].len() as f64
        };
        raw_cent.push(cent);
        let mut red: f64 = 0.0;
        for j in i.saturating_sub(2)..(i + 3).min(n) {
            if j == i || sets[i].is_empty() || sets[j].is_empty() {
                continue;
            }
            let inter = sets[i].intersection(&sets[j]).count() as f64;
            let uni = sets[i].union(&sets[j]).count() as f64;
            red = red.max(inter / uni);
        }
        let fw = word_spans(text)
            .iter()
            .filter(|(s, e)| is_stopword(&text[*s..*e].to_lowercase()))
            .count();
        let concepts: BTreeSet<String> = lists
            .domain
            .find(text)
            .into_iter()
            .map(|h| h.concept)
            .collect();
        out.push(SentenceScore {
            sentence: i,
            words,
            hedge_filler: 0.0,
            centrality: 0.0,
            redundancy: red,
            function_ratio: fw as f64 / words as f64,
            kg_coverage: (concepts.len() as f64 / 2.0).min(1.0),
            weakness: 0.0,
        });
    }
    let max_hf = raw_hf.iter().cloned().fold(0.0, f64::max);
    let max_cent = raw_cent.iter().cloned().fold(0.0, f64::max);
    for (i, s) in out.iter_mut().enumerate() {
        s.hedge_filler = if max_hf > 0.0 {
            raw_hf[i] / max_hf
        } else {
            0.0
        };
        s.centrality = if max_cent > 0.0 {
            raw_cent[i] / max_cent
        } else {
            0.0
        };
        s.weakness = W_HEDGE_FILLER * s.hedge_filler
            + W_LOW_CENTRALITY * (1.0 - s.centrality)
            + W_REDUNDANCY * s.redundancy
            + W_FUNCTION * s.function_ratio
            + W_NO_KG * (1.0 - s.kg_coverage);
    }
    out
}

/// Sentences shorter than this are treated as deliberate emphasis ("No.",
/// "I had them all."): never marked weak, and protected like paragraph
/// openers in trim. Their features are degenerate (few or no content words)
/// and cutting them saves almost nothing.
pub const SHORT_SENTENCE_WORDS: usize = 5;

/// Sentence indices ordered weakest first (ties: document order), excluding
/// short sentences.
pub fn rank_weakest(scores: &[SentenceScore]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..scores.len())
        .filter(|&i| scores[i].words >= SHORT_SENTENCE_WORDS)
        .collect();
    idx.sort_by(|&a, &b| {
        scores[b]
            .weakness
            .partial_cmp(&scores[a].weakness)
            .unwrap()
            .then(a.cmp(&b))
    });
    idx
}

/// Number of sentences "Mark the weakest sentences" flags.
pub fn weakest_count(n: usize) -> usize {
    if n == 0 {
        0
    } else {
        ((n as f64 * WEAKEST_SHARE).ceil() as usize).max(1)
    }
}

// ---------------------------------------------------------------- candidates

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Granularity {
    Word,
    Clause,
    Sentence,
}

#[derive(Debug, Clone)]
pub struct Candidate {
    /// Byte span into `Doc::text`, including the leading space (or bracketing
    /// comma) so that deleting it verbatim leaves clean text.
    pub start: usize,
    pub end: usize,
    pub granularity: Granularity,
    pub reason: String,
    /// 0 word filler/hedge, 1 clause, 2 sentence, 3 paragraph-opening sentence.
    pub tier: u8,
    /// Weakness of the host sentence; higher is cut first within a tier.
    pub score: f64,
    pub sentence: usize,
}

const ASIDE_OPENERS: &[&str] = &[
    "which",
    "who",
    "whom",
    "whose",
    "though",
    "although",
    "considering",
    "especially",
    "for example",
    "for instance",
    "such as",
    "after all",
    "of course",
    "i know",
    "i fancy",
    "i mean",
    "as i expected",
    "so far as",
    "it would appear",
    "however",
    "moreover",
    "in respect to",
    "in its most",
    "if any",
];

const RESUMPTIVE: &[&str] = &[
    "even", "these", "this", "those", "such", "all", "that", "it", "they",
];

fn is_protected(doc: &Doc, start: usize, end: usize) -> bool {
    doc.protected[start..end.min(doc.protected.len())]
        .iter()
        .any(|&b| b)
}

fn prev_char(text: &str, pos: usize) -> Option<char> {
    text[..pos].chars().next_back()
}

fn next_char(text: &str, pos: usize) -> Option<char> {
    text[pos..].chars().next()
}

/// Position of the sentence's final terminator run (so tail clauses keep it).
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

fn word_candidates(doc: &Doc, lists: &Lists, scores: &[SentenceScore], out: &mut Vec<Candidate>) {
    for (si, s) in doc.sentences.iter().enumerate() {
        let text = &doc.text[s.start..s.end];
        let first_word = word_spans(text).first().map(|w| s.start + w.0);
        for h in lists.style.find(text) {
            if !CUTTABLE.contains(&h.concept.as_str()) {
                continue;
            }
            let (hs, he) = (s.start + h.start, s.start + h.end);
            if is_protected(doc, hs, he) {
                continue;
            }
            let (mut a, mut b) = (hs, he);
            if Some(hs) == first_word {
                // sentence-initial: take a following comma and the space after
                if next_char(&doc.text, b) == Some(',') {
                    b += 1;
                }
                while b < s.end && next_char(&doc.text, b).map(|c| c == ' ').unwrap_or(false) {
                    b += 1;
                }
            } else {
                while a > s.start && prev_char(&doc.text, a) == Some(' ') {
                    a -= 1;
                }
                let after = next_char(&doc.text, he);
                if prev_char(&doc.text, a) == Some(',')
                    && matches!(
                        after,
                        Some(',' | '.' | ';' | ':' | '!' | '?' | '\u{2014}' | ')')
                    )
                {
                    a -= 1;
                }
            }
            out.push(Candidate {
                start: a,
                end: b,
                granularity: Granularity::Word,
                reason: format!("{} \"{}\"", h.concept.trim_start_matches("lab-"), h.term),
                tier: 0,
                score: scores[si].weakness,
                sentence: si,
            });
        }
    }
}

fn clause_candidates(doc: &Doc, scores: &[SentenceScore], out: &mut Vec<Candidate>) {
    let text = &doc.text;
    for (si, s) in doc.sentences.iter().enumerate() {
        let sent_words = word_count(&text[s.start..s.end]);
        let term = terminator_pos(text, s);
        let ok_size = |a: usize, b: usize| {
            let w = word_count(&text[a..b]);
            w >= 1 && (w as f64) <= 0.6 * sent_words as f64
        };
        // structural positions outside inline code and parentheses
        let mut commas = Vec::new();
        let mut dashes = Vec::new();
        let mut hard = Vec::new();
        let mut depth = 0i32;
        let mut paren_open: Option<usize> = None;
        for (off, c) in text[s.start..s.end].char_indices() {
            let p = s.start + off;
            if doc.protected[p] {
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
                    if depth == 0 {
                        if let Some(o) = paren_open.take() {
                            let inner = &text[o + 1..p];
                            let single_char = inner.chars().count() <= 1;
                            let mut a = o;
                            while a > s.start && prev_char(text, a) == Some(' ') {
                                a -= 1;
                            }
                            if !single_char
                                && a > s.start
                                && ok_size(a, p + 1)
                                && !is_protected(doc, a, p + 1)
                            {
                                out.push(Candidate {
                                    start: a,
                                    end: p + 1,
                                    granularity: Granularity::Clause,
                                    reason: "parenthetical".into(),
                                    tier: 1,
                                    score: scores[si].weakness,
                                    sentence: si,
                                });
                            }
                        }
                    }
                }
                ',' if depth == 0 => commas.push(p),
                '\u{2014}' if depth == 0 => dashes.push(p),
                ';' | ':' if depth == 0 => hard.push(p),
                _ => {}
            }
        }
        // em-dash asides: pairs cut "--X" and keep the closing dash;
        // an unpaired final dash cuts to the terminator.
        let mut k = 0;
        while k < dashes.len() {
            let d = dashes[k];
            let (end, reason) = if k + 1 < dashes.len() {
                (dashes[k + 1], "dash aside")
            } else {
                (term, "dash tail")
            };
            // A tail that opens with a resumptive word ("--even these forms
            // of penance are ...") is the main clause of a periodic sentence,
            // not an aside: cutting it would leave a fragment.
            let resumptive = reason == "dash tail"
                && word_spans(&text[d..end])
                    .first()
                    .map(|w| RESUMPTIVE.contains(&text[d + w.0..d + w.1].to_lowercase().as_str()))
                    .unwrap_or(false);
            if end > d && !resumptive && ok_size(d, end) && !is_protected(doc, d, end) {
                out.push(Candidate {
                    start: d,
                    end,
                    granularity: Granularity::Clause,
                    reason: reason.into(),
                    tier: 1,
                    score: scores[si].weakness,
                    sentence: si,
                });
            }
            k += 2;
        }
        // comma asides opened by a known connective
        for &c in &commas {
            let seg_start = c + 1;
            let rest = text[seg_start..s.end].trim_start().to_lowercase();
            let opener = ASIDE_OPENERS.iter().find(|o| {
                rest.starts_with(*o)
                    && rest[o.len()..]
                        .chars()
                        .next()
                        .map(|ch| !is_word_char(ch))
                        .unwrap_or(true)
            });
            if opener.is_none() {
                continue;
            }
            let next = commas
                .iter()
                .chain(hard.iter())
                .chain(dashes.iter())
                .filter(|&&p| p > c)
                .min()
                .copied()
                .unwrap_or(term)
                .min(term);
            // ", though X, but Y": the "but" clause depends on the aside.
            let dependent = text[next..s.end]
                .trim_start_matches(|ch: char| ch == ',' || ch.is_whitespace())
                .split(|ch: char| !is_word_char(ch))
                .next()
                .map(|w| matches!(w.to_lowercase().as_str(), "but" | "yet"))
                .unwrap_or(false);
            if next > seg_start && !dependent && ok_size(c, next) && !is_protected(doc, c, next) {
                out.push(Candidate {
                    start: c,
                    end: next,
                    granularity: Granularity::Clause,
                    reason: format!("aside \"{}\"", opener.unwrap()),
                    tier: 1,
                    score: scores[si].weakness,
                    sentence: si,
                });
            }
        }
    }
}

fn sentence_candidates(doc: &Doc, scores: &[SentenceScore], out: &mut Vec<Candidate>) {
    for (si, s) in doc.sentences.iter().enumerate() {
        let next_in_para = doc.sentences.get(si + 1).filter(|n| n.para == s.para);
        let (a, b) = if s.first_in_para {
            // take the trailing gap so the next sentence moves up cleanly;
            // a list item's opener also takes its marker, and a single-
            // sentence item takes the newline before it (no empty bullet)
            let mut a = s.marker_start.unwrap_or(s.start);
            if s.marker_start.is_some() && next_in_para.is_none() {
                let bytes = doc.text.as_bytes();
                if a >= 2 && bytes[a - 1] == b'\n' && bytes[a - 2] != b'\n' {
                    a -= 1;
                }
            }
            (a, next_in_para.map(|n| n.start).unwrap_or(s.end))
        } else {
            let prev_end = doc.sentences[si - 1].end;
            (prev_end, s.end)
        };
        // A sentence holding inline code is kept whole: cutting it would
        // cut the code span with it.
        if is_protected(doc, a, b) {
            continue;
        }
        out.push(Candidate {
            start: a,
            end: b,
            granularity: Granularity::Sentence,
            reason: if s.first_in_para {
                "paragraph-opening sentence".into()
            } else if scores[si].words < SHORT_SENTENCE_WORDS {
                "short sentence".into()
            } else {
                "weak sentence".into()
            },
            tier: if s.first_in_para || scores[si].words < SHORT_SENTENCE_WORDS {
                3
            } else {
                2
            },
            score: scores[si].weakness,
            sentence: si,
        });
    }
}

/// All trim candidates in rank order: tier ascending, then host-sentence
/// weakness descending, then document position. Fully deterministic.
pub fn ranked_candidates(doc: &Doc, lists: &Lists, scores: &[SentenceScore]) -> Vec<Candidate> {
    let mut c = Vec::new();
    word_candidates(doc, lists, scores, &mut c);
    clause_candidates(doc, scores, &mut c);
    sentence_candidates(doc, scores, &mut c);
    c.sort_by(|x, y| {
        x.tier
            .cmp(&y.tier)
            .then(y.score.partial_cmp(&x.score).unwrap())
            .then(x.start.cmp(&y.start))
            .then(x.end.cmp(&y.end))
    });
    c
}

// ---------------------------------------------------------------- trim

#[derive(Debug, Clone)]
pub struct LevelResult {
    pub label: &'static str,
    pub target_pct: f64,
    pub target_words: usize,
    pub cut_words: usize,
    pub achieved_pct: f64,
    /// Candidate indices selected at this level (includes all lower levels).
    pub selected: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct TrimResult {
    pub total_words: usize,
    pub candidates: Vec<Candidate>,
    /// First level (0..4) at which each candidate is faded, if any.
    pub level_of: Vec<Option<usize>>,
    pub levels: Vec<LevelResult>,
}

/// Select spans for every trim level.
///
/// Levels are nested: level k starts from level k-1's selection. Within a
/// level, pass 1 walks the ranked list and accepts a candidate only if it
/// does not overshoot the target; pass 2 then repeatedly accepts the single
/// candidate that brings the count closest to the target, while that
/// strictly reduces the distance.
pub fn trim(doc: &Doc, lists: &Lists) -> TrimResult {
    trim_filtered(doc, lists, |_| true)
}

/// [`trim`] restricted to candidates accepted by `keep` (used for ablations).
pub fn trim_filtered(doc: &Doc, lists: &Lists, keep: impl Fn(&Candidate) -> bool) -> TrimResult {
    let scores = score_sentences(doc, lists);
    let mut candidates = ranked_candidates(doc, lists, &scores);
    candidates.retain(|c| keep(c));
    let cand_words: Vec<Vec<usize>> = candidates
        .iter()
        .map(|c| doc.words_in(c.start, c.end))
        .collect();
    let total = doc.total_words();
    let mut mask = vec![false; total];
    let mut cut = 0usize;
    let mut level_of: Vec<Option<usize>> = vec![None; candidates.len()];
    let mut levels = Vec::new();
    let new_words = |mask: &[bool], i: usize| cand_words[i].iter().filter(|&&w| !mask[w]).count();
    // "close enough" band for pass 2: 1% of the document, at least one word
    let band = ((total as f64 * 0.01).round() as i64).max(1);
    let tolerance = (total as f64 * TOLERANCE_PP / 100.0).floor() as i64;
    for (li, (label, frac)) in LEVELS.iter().enumerate() {
        let target = target_words(total, *frac);
        // Paragraph-opening sentences (tier 3) are protected below the
        // "Cut in half" level; they are only a last resort there.
        let max_tier = if li == LEVELS.len() - 1 { 3 } else { 2 };
        let run = |max_tier: u8,
                   mask: &mut Vec<bool>,
                   cut: &mut usize,
                   level_of: &mut Vec<Option<usize>>| {
            // pass 1: rank order, never overshoot
            for i in 0..candidates.len() {
                if level_of[i].is_some() || candidates[i].tier > max_tier {
                    continue;
                }
                let n = new_words(mask, i);
                if n > 0 && *cut + n <= target {
                    cand_words[i].iter().for_each(|&w| mask[w] = true);
                    *cut += n;
                    level_of[i] = Some(li);
                }
                if *cut == target {
                    break;
                }
            }
            // pass 2: while it strictly helps, take the best-ranked
            // candidate that lands within `band` of the target, else the
            // closest one.
            loop {
                let dist = (target as i64 - *cut as i64).abs();
                let options: Vec<(usize, usize, i64)> = (0..candidates.len())
                    .filter(|&i| level_of[i].is_none() && candidates[i].tier <= max_tier)
                    .map(|i| (i, new_words(mask, i)))
                    .filter(|&(_, n)| n > 0)
                    .map(|(i, n)| (i, n, (target as i64 - (*cut + n) as i64).abs()))
                    .filter(|&(_, _, d)| d < dist)
                    .collect();
                let best = options
                    .iter()
                    .filter(|o| o.2 <= band)
                    .min_by_key(|o| o.0)
                    .or_else(|| options.iter().min_by_key(|o| (o.2, o.0)))
                    .copied();
                match best {
                    Some((i, n, _)) => {
                        cand_words[i].iter().for_each(|&w| mask[w] = true);
                        *cut += n;
                        level_of[i] = Some(li);
                    }
                    None => break,
                }
            }
        };
        run(max_tier, &mut mask, &mut cut, &mut level_of);
        // fallback: if still outside tolerance, allow every tier
        if (target as i64 - cut as i64).abs() > tolerance && max_tier < 3 {
            run(3, &mut mask, &mut cut, &mut level_of);
        }
        let selected = (0..candidates.len())
            .filter(|&i| level_of[i].map(|l| l <= li).unwrap_or(false))
            .collect();
        levels.push(LevelResult {
            label,
            target_pct: frac * 100.0,
            target_words: target,
            cut_words: cut,
            achieved_pct: cut_percentage(total, cut),
            selected,
        });
    }
    TrimResult {
        total_words: total,
        candidates,
        level_of,
        levels,
    }
}

/// Ablation baseline: walk the ranked list and take every candidate until
/// the target is reached or passed (no skipping, no fit step). Returns the
/// achieved percentage per level.
pub fn naive_prefix(doc: &Doc, lists: &Lists) -> Vec<f64> {
    let scores = score_sentences(doc, lists);
    let candidates = ranked_candidates(doc, lists, &scores);
    let total = doc.total_words();
    LEVELS
        .iter()
        .map(|(_, frac)| {
            let target = target_words(total, *frac);
            let mut mask = vec![false; total];
            let mut cut = 0;
            for c in &candidates {
                if cut >= target {
                    break;
                }
                for w in doc.words_in(c.start, c.end) {
                    if !mask[w] {
                        mask[w] = true;
                        cut += 1;
                    }
                }
            }
            cut_percentage(total, cut)
        })
        .collect()
}

/// Words removed by a set of candidates, counted as a union (a sentence cut
/// subsumes fillers already faded inside it).
pub fn union_cut_words(doc: &Doc, candidates: &[Candidate], selected: &[usize]) -> usize {
    let mut set = BTreeSet::new();
    for &i in selected {
        set.extend(doc.words_in(candidates[i].start, candidates[i].end));
    }
    set.len()
}

/// Merge selected spans into sorted, non-overlapping byte ranges.
pub fn merged_ranges(candidates: &[Candidate], selected: &[usize]) -> Vec<(usize, usize)> {
    let mut r: Vec<(usize, usize)> = selected
        .iter()
        .map(|&i| (candidates[i].start, candidates[i].end))
        .collect();
    r.sort();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in r {
        if let Some(last) = out.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        out.push((s, e));
    }
    out
}

/// Render with faded spans as ~~strikethrough~~ (whitespace kept outside the
/// markers so GFM renders it).
pub fn render_strike(text: &str, ranges: &[(usize, usize)]) -> String {
    let mut out = String::new();
    let mut pos = 0;
    for &(s, e) in ranges {
        out.push_str(&text[pos..s]);
        let seg = &text[s..e];
        let lead = seg.len() - seg.trim_start().len();
        let trail = seg.len() - seg.trim_end().len();
        let core = seg.trim();
        out.push_str(&seg[..lead]);
        if !core.is_empty() {
            out.push_str("~~");
            out.push_str(core);
            out.push_str("~~");
        }
        out.push_str(&seg[seg.len() - trail..]);
        pos = e;
    }
    out.push_str(&text[pos..]);
    out
}

/// Text after "Make the cuts": selected spans deleted verbatim.
pub fn apply_cuts(text: &str, ranges: &[(usize, usize)]) -> String {
    let mut out = String::new();
    let mut pos = 0;
    for &(s, e) in ranges {
        out.push_str(&text[pos..s]);
        pos = e;
    }
    out.push_str(&text[pos..]);
    out
}
