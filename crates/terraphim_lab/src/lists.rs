//! KG lists compiled into `terraphim_automata` matchers.
//!
//! Every matcher is built with [`MatcherBuilder`] from patterns **sorted by
//! key**, never by iterating a `Thesaurus` (an `AHashMap` whose order varies
//! from process to process). That keeps pattern indices, and so the outcome
//! of any leftmost-longest tie, identical on every run and every target.

use std::collections::BTreeMap;

use terraphim_automata::{
    CompiledMatcher, MatcherBuilder, MatcherOptions, load_thesaurus_from_json,
    parse_markdown_directives_str,
};
use terraphim_types::{NormalizedTerm, NormalizedTermValue, Thesaurus};

use crate::LabError;

/// `terraphim_automata` rejects patterns shorter than this (in bytes).
const MIN_PATTERN_LENGTH: usize = terraphim_automata::compiled::DEFAULT_MIN_PATTERN_LENGTH;

/// Which style list a term belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StyleCategory {
    /// Padding that adds emphasis but no meaning (`lab-filler`).
    Filler,
    /// Hedging adverbs and parenthetical hedges (`lab-hedge`).
    Hedge,
    /// Hedges that are part of the verb phrase (`lab-hedge-phrase`).
    HedgePhrase,
    /// Words that do not fit the role's register (`lab-tone`).
    Tone,
}

impl StyleCategory {
    /// The KG concept name (file stem) for this category.
    pub fn concept(self) -> &'static str {
        match self {
            StyleCategory::Filler => "lab-filler",
            StyleCategory::Hedge => "lab-hedge",
            StyleCategory::HedgePhrase => "lab-hedge-phrase",
            StyleCategory::Tone => "lab-tone",
        }
    }

    /// Parse a KG concept name; `None` for concepts the Lab does not use.
    pub fn from_concept(concept: &str) -> Option<StyleCategory> {
        match concept.trim().to_lowercase().as_str() {
            "lab-filler" => Some(StyleCategory::Filler),
            "lab-hedge" => Some(StyleCategory::Hedge),
            "lab-hedge-phrase" => Some(StyleCategory::HedgePhrase),
            "lab-tone" => Some(StyleCategory::Tone),
            _ => None,
        }
    }

    fn id(self) -> u64 {
        match self {
            StyleCategory::Filler => 1,
            StyleCategory::Hedge => 2,
            StyleCategory::HedgePhrase => 3,
            StyleCategory::Tone => 4,
        }
    }

    fn from_id(id: u64) -> Option<StyleCategory> {
        match id {
            1 => Some(StyleCategory::Filler),
            2 => Some(StyleCategory::Hedge),
            3 => Some(StyleCategory::HedgePhrase),
            4 => Some(StyleCategory::Tone),
            _ => None,
        }
    }
}

/// The default style lists, compiled from `kg/lab-*.md` into thesaurus JSON
/// (a test checks the two agree). Embedding JSON rather than the markdown
/// keeps `terraphim_markdown_parser` (and the `markdown` crate behind it)
/// out of a wasm build that only uses the defaults: about 280 KB.
/// Regenerate with `python3 scripts/kg_to_json.py style kg kg/lab-style.json`.
const DEFAULT_STYLE: &str = include_str!("../kg/lab-style.json");
const DEFAULT_TYPOS: &str = include_str!("../kg/typos.json");

/// The `synonyms::` of one KG markdown file, lower-cased, parsed with
/// `terraphim_automata::parse_markdown_directives_str`.
fn kg_synonyms(concept: &str, markdown: &str) -> Vec<String> {
    let parsed = parse_markdown_directives_str(concept, markdown);
    parsed
        .directives
        .get(concept)
        .map(|d| d.synonyms.iter().map(|s| s.trim().to_lowercase()).collect())
        .unwrap_or_default()
}

/// Add the curly-apostrophe spelling of every term that has an ASCII one, so
/// smart-quoted prose matches ASCII lists. This is data, not matcher logic.
fn with_apostrophe_variants(term: &str) -> impl Iterator<Item = String> + '_ {
    let curly = term.contains('\'').then(|| term.replace('\'', "\u{2019}"));
    std::iter::once(term.to_string()).chain(curly)
}

/// Compile `patterns` (already unique and sorted) into a matcher.
fn compile(patterns: BTreeMap<String, NormalizedTerm>) -> Result<CompiledMatcher, LabError> {
    let mut builder = MatcherBuilder::new(MatcherOptions::default());
    for (pattern, term) in patterns {
        builder
            .insert(pattern, term)
            .map_err(|e| LabError::Matcher(e.to_string()))?;
    }
    builder
        .build()
        .map_err(|e| LabError::Matcher(e.to_string()))
}

/// One positioned match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hit {
    pub start: usize,
    pub end: usize,
    pub term: NormalizedTerm,
}

fn find(matcher: &CompiledMatcher, text: &str) -> Vec<Hit> {
    let mut buf = Vec::new();
    matcher.push_positions(text, &mut buf);
    buf.into_iter()
        .filter_map(|m| {
            Some(Hit {
                start: m.start,
                end: m.end,
                term: matcher.term(m.pattern_index)?.clone(),
            })
        })
        .collect()
}

/// The hedge, filler and tone lists for a role, compiled once.
///
/// Built from the role's KG markdown files (`lab-filler.md`, `lab-hedge.md`,
/// `lab-hedge-phrase.md`, `lab-tone.md`), or from the small defaults embedded
/// in the crate ([`StyleLists::defaults`]).
#[derive(Debug, Clone)]
pub struct StyleLists {
    matcher: CompiledMatcher,
}

impl StyleLists {
    /// The embedded default lists (the research spike's lists, issue #3;
    /// sources in `kg/lab-*.md`).
    pub fn defaults() -> Result<StyleLists, LabError> {
        let thesaurus = load_thesaurus_from_json(DEFAULT_STYLE)
            .map_err(|e| LabError::Thesaurus(e.to_string()))?;
        StyleLists::from_thesaurus(&thesaurus)
    }

    /// Build from a compiled role thesaurus: every key whose concept
    /// (`nterm`) is one of the four `lab-*` concepts becomes a term of that
    /// category; other keys are ignored, so the role's whole thesaurus can be
    /// passed in.
    pub fn from_thesaurus(thesaurus: &Thesaurus) -> Result<StyleLists, LabError> {
        let terms: Vec<(StyleCategory, String)> = thesaurus
            .into_iter()
            .filter_map(|(key, term)| {
                Some((
                    StyleCategory::from_concept(term.value.as_str())?,
                    key.as_str().to_string(),
                ))
            })
            .collect();
        StyleLists::from_terms(terms)
    }

    /// Build from `(concept, markdown)` pairs in the Terraphim KG format (one
    /// concept per file, the concept is the file stem, terms on the
    /// `synonyms::` line). Concepts other than the four `lab-*` ones are
    /// ignored, so a whole role KG directory can be passed in. A term listed
    /// under two concepts keeps the first category in [`StyleCategory`] order.
    pub fn from_kg_markdown<'a, I>(files: I) -> Result<StyleLists, LabError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut terms: Vec<(StyleCategory, String)> = Vec::new();
        for (concept, markdown) in files {
            let Some(cat) = StyleCategory::from_concept(concept) else {
                continue;
            };
            for syn in kg_synonyms(concept, markdown) {
                terms.push((cat, syn));
            }
        }
        StyleLists::from_terms(terms)
    }

    /// Build from explicit `(category, term)` pairs.
    pub fn from_terms<I, S>(terms: I) -> Result<StyleLists, LabError>
    where
        I: IntoIterator<Item = (StyleCategory, S)>,
        S: AsRef<str>,
    {
        let mut sorted: Vec<(StyleCategory, String)> = terms
            .into_iter()
            .map(|(c, s)| (c, s.as_ref().trim().to_lowercase()))
            .filter(|(_, s)| s.len() >= MIN_PATTERN_LENGTH)
            .collect();
        sorted.sort();
        let mut patterns = BTreeMap::new();
        for (cat, term) in &sorted {
            for variant in with_apostrophe_variants(term) {
                patterns.entry(variant).or_insert_with(|| {
                    NormalizedTerm::new(cat.id(), NormalizedTermValue::from(cat.concept()))
                });
            }
        }
        Ok(StyleLists {
            matcher: compile(patterns)?,
        })
    }

    /// Number of compiled patterns (including apostrophe variants).
    pub fn len(&self) -> usize {
        self.matcher.len()
    }

    /// True when no pattern was compiled.
    pub fn is_empty(&self) -> bool {
        self.matcher.is_empty()
    }

    pub(crate) fn find(&self, text: &str) -> Vec<(usize, usize, StyleCategory)> {
        find(&self.matcher, text)
            .into_iter()
            .filter_map(|h| Some((h.start, h.end, StyleCategory::from_id(h.term.id)?)))
            .collect()
    }
}

/// A typo thesaurus: misspelling (key) to correction (`nterm`, or its
/// `display_value` when the correction needs capitals).
///
/// This is the ordinary Terraphim thesaurus shape, so a KG concept file
/// `receive.md` with `synonyms:: recieve, receeve` is a typo list entry.
#[derive(Debug, Clone)]
pub struct TypoList {
    matcher: CompiledMatcher,
}

impl TypoList {
    /// The small typo list embedded in the crate (`kg/typos.json`).
    pub fn defaults() -> Result<TypoList, LabError> {
        TypoList::from_json(DEFAULT_TYPOS)
    }

    /// An empty list (typo marks off; punctuation rules still run).
    pub fn empty() -> TypoList {
        TypoList::from_thesaurus(&Thesaurus::new("empty".into()))
            .expect("an empty matcher always compiles")
    }

    /// Load a thesaurus JSON (`{"name", "data": {key: {"id", "nterm"}}}`) with
    /// `terraphim_automata::load_thesaurus_from_json`.
    pub fn from_json(json: &str) -> Result<TypoList, LabError> {
        let thesaurus =
            load_thesaurus_from_json(json).map_err(|e| LabError::Thesaurus(e.to_string()))?;
        TypoList::from_thesaurus(&thesaurus)
    }

    /// Build from a thesaurus: every key is a misspelling, its term the fix.
    pub fn from_thesaurus(thesaurus: &Thesaurus) -> Result<TypoList, LabError> {
        let mut patterns = BTreeMap::new();
        for (key, term) in thesaurus {
            let key = key.as_str().trim().to_lowercase();
            if key.len() < MIN_PATTERN_LENGTH {
                continue;
            }
            // A key equal to its own correction would mark correct text.
            if key == term.display().to_lowercase() {
                continue;
            }
            patterns.insert(key, term.clone());
        }
        Ok(TypoList {
            matcher: compile(patterns)?,
        })
    }

    /// Build from KG markdown: the concept (file stem) is the correction and
    /// its `synonyms::` are the misspellings.
    pub fn from_kg_markdown<'a, I>(files: I) -> Result<TypoList, LabError>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut thesaurus = Thesaurus::new("typos".into());
        let mut files: Vec<(&str, &str)> = files.into_iter().collect();
        files.sort();
        for (id, (concept, markdown)) in files.into_iter().enumerate() {
            let term = NormalizedTerm::new(id as u64 + 1, NormalizedTermValue::from(concept))
                .with_display_value(concept.trim().to_string());
            for syn in kg_synonyms(concept, markdown) {
                thesaurus.insert(NormalizedTermValue::from(syn), term.clone());
            }
        }
        TypoList::from_thesaurus(&thesaurus)
    }

    /// Number of compiled misspellings.
    pub fn len(&self) -> usize {
        self.matcher.len()
    }

    /// True when the list is empty.
    pub fn is_empty(&self) -> bool {
        self.matcher.is_empty()
    }

    pub(crate) fn find(&self, text: &str) -> Vec<Hit> {
        find(&self.matcher, text)
    }
}

/// The role's concept matcher: thesaurus keys to concept (node) ids.
#[derive(Debug, Clone)]
pub(crate) struct ConceptMatcher {
    matcher: CompiledMatcher,
}

impl ConceptMatcher {
    pub(crate) fn from_thesaurus(thesaurus: &Thesaurus) -> Result<ConceptMatcher, LabError> {
        let mut patterns = BTreeMap::new();
        for (key, term) in thesaurus {
            let key = key.as_str().trim().to_lowercase();
            if key.len() >= MIN_PATTERN_LENGTH {
                patterns.insert(key, term.clone());
            }
        }
        Ok(ConceptMatcher {
            matcher: compile(patterns)?,
        })
    }

    /// Every concept match, with its concept term.
    pub(crate) fn find(&self, text: &str) -> Vec<Hit> {
        find(&self.matcher, text)
    }
}

/// The trim engine's closed word classes (not role data): what opens a comma
/// aside, what makes a dash tail resumptive, and the coordinating
/// conjunctions the tidy-up looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Connective {
    /// Opens a removable comma aside (", which ...", ", of course,").
    Aside,
    /// Opens the main clause of a periodic sentence after a dash
    /// ("—even these forms of penance are ..."), so the tail is not an aside.
    Resumptive,
    /// "and", "or", "nor", "so".
    Conjunction,
    /// "but", "yet": a clause that depends on the aside before it.
    Contrast,
}

impl Connective {
    fn id(self) -> u64 {
        match self {
            Connective::Aside => 1,
            Connective::Resumptive => 2,
            Connective::Conjunction => 3,
            Connective::Contrast => 4,
        }
    }

    fn from_id(id: u64) -> Option<Connective> {
        match id {
            1 => Some(Connective::Aside),
            2 => Some(Connective::Resumptive),
            3 => Some(Connective::Conjunction),
            4 => Some(Connective::Contrast),
            _ => None,
        }
    }
}

/// Comma-aside openers: the general connectives from the research spike
/// (`docs/research/lab-heuristics.md` §5.1), without the six fixture-shaped
/// entries its §6.6 ablation names ("i fancy", "as i expected", "so far as",
/// "in respect to", "in its most", "if any").
const ASIDE_OPENERS: [&str; 18] = [
    "after all",
    "although",
    "considering",
    "especially",
    "for example",
    "for instance",
    "however",
    "i know",
    "i mean",
    "it would appear",
    "moreover",
    "of course",
    "such as",
    "though",
    "which",
    "who",
    "whom",
    "whose",
];

/// Words that open a resumptive dash tail.
const RESUMPTIVE: [&str; 9] = [
    "all", "even", "it", "such", "that", "these", "they", "this", "those",
];

const CONJUNCTIONS: [&str; 4] = ["and", "nor", "or", "so"];
const CONTRASTS: [&str; 2] = ["but", "yet"];

/// The connective matcher, compiled once per process. `None` only if the
/// constant lists above failed to compile (a unit test rules that out); trim
/// then simply finds no asides.
fn connective_matcher() -> Option<&'static CompiledMatcher> {
    static MATCHER: std::sync::OnceLock<Option<CompiledMatcher>> = std::sync::OnceLock::new();
    MATCHER
        .get_or_init(|| {
            let mut patterns = BTreeMap::new();
            let lists: [(&[&str], Connective); 4] = [
                (&ASIDE_OPENERS, Connective::Aside),
                (&RESUMPTIVE, Connective::Resumptive),
                (&CONJUNCTIONS, Connective::Conjunction),
                (&CONTRASTS, Connective::Contrast),
            ];
            for (words, kind) in lists {
                for w in words {
                    for variant in with_apostrophe_variants(w) {
                        patterns.entry(variant).or_insert_with(|| {
                            NormalizedTerm::new(kind.id(), NormalizedTermValue::from(*w))
                        });
                    }
                }
            }
            compile(patterns).ok()
        })
        .as_ref()
}

/// Every connective in `text` (word-boundary filtered, ASCII
/// case-insensitive, leftmost-longest), as `(start, end, kind)`.
pub(crate) fn find_connectives(text: &str) -> Vec<(usize, usize, Connective)> {
    let Some(matcher) = connective_matcher() else {
        return Vec::new();
    };
    find(matcher, text)
        .into_iter()
        .filter_map(|h| Some((h.start, h.end, Connective::from_id(h.term.id)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connectives_compile_and_match_whole_phrases() {
        assert!(connective_matcher().is_some());
        let text = "x, for example, which; but SUCH as theory and evenly";
        let hits: Vec<(&str, Connective)> = find_connectives(text)
            .into_iter()
            .map(|(s, e, k)| (&text[s..e], k))
            .collect();
        assert_eq!(
            hits,
            vec![
                ("for example", Connective::Aside),
                ("which", Connective::Aside),
                ("but", Connective::Contrast),
                ("SUCH as", Connective::Aside),
                ("and", Connective::Conjunction),
            ]
        );
    }

    #[test]
    fn embedded_json_matches_the_kg_markdown() {
        let from_md = StyleLists::from_kg_markdown([
            ("lab-filler", include_str!("../kg/lab-filler.md")),
            ("lab-hedge", include_str!("../kg/lab-hedge.md")),
            (
                "lab-hedge-phrase",
                include_str!("../kg/lab-hedge-phrase.md"),
            ),
            ("lab-tone", include_str!("../kg/lab-tone.md")),
        ])
        .unwrap();
        let from_json = StyleLists::defaults().unwrap();
        assert_eq!(from_md.matcher.patterns(), from_json.matcher.patterns());
        assert!(from_json.len() > 90);
    }

    #[test]
    fn whole_role_thesaurus_keeps_only_lab_concepts() {
        let json = r#"{"name":"role","data":{
            "quite":{"id":7,"nterm":"lab-filler"},
            "zed":{"id":8,"nterm":"zed"}}}"#;
        let lists = StyleLists::from_thesaurus(&load_thesaurus_from_json(json).unwrap()).unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists.find("quite zed"), vec![(0, 5, StyleCategory::Filler)]);
    }
}
