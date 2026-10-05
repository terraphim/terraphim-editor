//! Deterministic Lab mark engine for terraphim-editor (spec R-8.2, issue #14).
//!
//! The Lab's six mark actions, with no LLM and no DOM. Each action takes the
//! Markdown body and returns [`LabMark`]s: spans to highlight, with a score, a
//! human-readable reason and, for typos and punctuation, a proposed
//! replacement. **Marks never change text**: the body is borrowed immutably
//! and proposals are alternatives the user may accept, never applied here.
//!
//! | Action | Rule |
//! |---|---|
//! | [`LabAction::TyposAndPunctuation`] | typo thesaurus matches (`terraphim_automata`) and conservative punctuation rules, each with a proposal |
//! | [`LabAction::WeakestSentences`] | rolegraph-primary weakness ranking; weakest 15% (at least one), paragraph openers never marked |
//! | [`LabAction::LongSentences`] | sentences of [`LabOptions::long_sentence_words`] words or more |
//! | [`LabAction::ConvolutedSentences`] | clause load (commas, breaks, brackets, subordinate clauses, nesting) at or above [`LabOptions::convoluted_load`] |
//! | [`LabAction::OffTone`] | the role's `lab-tone` KG list |
//! | [`LabAction::HedgesAndFiller`] | the role's `lab-filler`, `lab-hedge` and `lab-hedge-phrase` KG lists |
//!
//! # Offsets
//!
//! `start` and `end` are **UTF-16 code-unit offsets into the original body**
//! (the unit of JavaScript strings and DOM ranges), not into a normalised
//! copy: hard-wrapped lines, `\r\n` endings and multi-byte characters are all
//! accounted for.
//!
//! # Protected text
//!
//! Headings, fenced and indented code, tables, block quotes, HTML blocks,
//! link reference definitions, inline code, link destinations and bare URLs
//! are never marked. A sentence-level mark on a sentence that holds inline
//! code is split into one mark per piece of prose around the code.
//!
//! # Weakest sentences
//!
//! The ranking is lexicographic. The **primary** score uses role data only:
//!
//! ```text
//! primary = 0.75 * (1 - strength) + 0.25 * hedge_density
//! strength = 1 - prod over the sentence's distinct role concepts c of (1 - s(c))
//! s(c) = 0.25 + 0.75 * (0.5 * rank(c) + 0.5 * connectivity(c))
//! ```
//!
//! where `rank(c)` is `ln(1 + node rank) / ln(1 + max node rank)` in the
//! role's rolegraph and `connectivity(c)` is the share of the document's
//! other role concepts that `c` has an edge to. `hedge_density` is the
//! sentence's hedge and filler matches per word (role KG lists), normalised
//! to the document maximum. Only sentences with equal primary scores are then
//! ordered by a text-only **tie-break** (`0.5 * (1 - lexical centrality) +
//! 0.3 * neighbour redundancy + 0.2 * function-word ratio`), and finally by
//! position.
//!
//! # Example
//!
//! ```
//! use terraphim_lab::{LabAction, LabConfig, MarkKind, mark};
//!
//! let config = LabConfig::with_defaults().unwrap();
//! let body = "We recieve it.  It is basically fine.";
//! let marks = mark(body, &config, LabAction::TyposAndPunctuation);
//! assert_eq!(marks[0].kind, MarkKind::Typo);
//! assert_eq!(&body[marks[0].start..marks[0].end], "recieve");
//! assert_eq!(marks[0].proposal.as_deref(), Some("receive"));
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod actions;
mod lists;
mod offset;
mod rolegraph;
mod text;
mod weak;

use serde::Serialize;

pub use lists::{StyleCategory, StyleLists, TypoList};
pub use rolegraph::{
    CONCEPT_CONNECTIVITY_WEIGHT, CONCEPT_PRESENCE, CONCEPT_RANK_WEIGHT, RoleGraphData,
    RoleKnowledge, edge_id,
};
pub use weak::{T_FUNCTION, T_LOW_CENTRALITY, T_REDUNDANCY, W_GRAPH, W_STYLE};

/// Errors from building a [`LabConfig`]. Marking itself never fails.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LabError {
    /// A thesaurus JSON could not be loaded.
    #[error("invalid thesaurus: {0}")]
    Thesaurus(String),
    /// A KG list could not be compiled into a matcher.
    #[error("cannot compile matcher: {0}")]
    Matcher(String),
    /// Rolegraph JSON could not be parsed.
    #[error("invalid rolegraph data: {0}")]
    RoleGraph(String),
}

/// The six Lab mark actions (spec R-8.2), in popover order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LabAction {
    /// "Fix punctuation and typos".
    TyposAndPunctuation,
    /// "Mark the weakest sentences".
    WeakestSentences,
    /// "Mark sentences that run long".
    LongSentences,
    /// "Mark convoluted sentences".
    ConvolutedSentences,
    /// "Mark words that don't fit the tone".
    OffTone,
    /// "Mark hedges and filler".
    HedgesAndFiller,
}

impl LabAction {
    /// Every action, in popover order.
    pub const ALL: [LabAction; 6] = [
        LabAction::TyposAndPunctuation,
        LabAction::WeakestSentences,
        LabAction::LongSentences,
        LabAction::ConvolutedSentences,
        LabAction::OffTone,
        LabAction::HedgesAndFiller,
    ];

    /// The popover label from the spec.
    pub fn label(self) -> &'static str {
        match self {
            LabAction::TyposAndPunctuation => "Fix punctuation and typos",
            LabAction::WeakestSentences => "Mark the weakest sentences",
            LabAction::LongSentences => "Mark sentences that run long",
            LabAction::ConvolutedSentences => "Mark convoluted sentences",
            LabAction::OffTone => "Mark words that don't fit the tone",
            LabAction::HedgesAndFiller => "Mark hedges and filler",
        }
    }
}

/// What a mark flags. Several kinds can come from one [`LabAction`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    /// A misspelling from the typo thesaurus (has a proposal).
    Typo,
    /// A punctuation or spacing slip (has a proposal).
    Punctuation,
    /// One of the weakest sentences.
    WeakSentence,
    /// A sentence that runs long.
    LongSentence,
    /// A sentence with a heavy clause structure.
    ConvolutedSentence,
    /// A word or phrase outside the role's register.
    OffTone,
    /// A hedge.
    Hedge,
    /// Filler.
    Filler,
}

impl MarkKind {
    /// The action that produces this kind.
    pub fn action(self) -> LabAction {
        match self {
            MarkKind::Typo | MarkKind::Punctuation => LabAction::TyposAndPunctuation,
            MarkKind::WeakSentence => LabAction::WeakestSentences,
            MarkKind::LongSentence => LabAction::LongSentences,
            MarkKind::ConvolutedSentence => LabAction::ConvolutedSentences,
            MarkKind::OffTone => LabAction::OffTone,
            MarkKind::Hedge | MarkKind::Filler => LabAction::HedgesAndFiller,
        }
    }
}

/// One mark: a span of the original body to highlight. Never an edit.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LabMark {
    /// What the mark flags.
    pub kind: MarkKind,
    /// UTF-16 code-unit offset of the first unit, into the original body.
    pub start: usize,
    /// UTF-16 code-unit offset one past the last unit.
    pub end: usize,
    /// Strength of the mark. Weak sentences: the primary weakness in
    /// `[0, 1]`. Long and convoluted sentences: the measure over its limit
    /// (at least 1). Word-level marks: 1.
    pub score: f64,
    /// Why the span was marked, for the UI.
    pub reason: String,
    /// A proposed replacement for exactly `start..end` (typos and
    /// punctuation only). It is an alternative to offer, never applied.
    pub proposal: Option<String>,
}

/// Tunable thresholds. The defaults are the product decisions for v1.
#[derive(Debug, Clone, PartialEq)]
pub struct LabOptions {
    /// Share of prose sentences "Mark the weakest sentences" marks (rounded
    /// up, at least one). Default 0.15.
    pub weakest_share: f64,
    /// Sentences with fewer words are never marked weakest. Default 5.
    pub min_sentence_words: usize,
    /// Sentences with at least this many words run long. Default 30.
    pub long_sentence_words: usize,
    /// Clause load at which a sentence is convoluted. Default 5.0.
    pub convoluted_load: f64,
}

impl Default for LabOptions {
    fn default() -> Self {
        LabOptions {
            weakest_share: 0.15,
            min_sentence_words: 5,
            long_sentence_words: 30,
            convoluted_load: 5.0,
        }
    }
}

/// Everything an action needs: the role's style lists, the typo list, the
/// role's knowledge (thesaurus and rolegraph) and the options. Build it once
/// per role and reuse it for every call.
#[derive(Debug, Clone)]
pub struct LabConfig {
    /// Hedge, filler and tone lists (the role's KG, or the defaults).
    pub style: StyleLists,
    /// Typo thesaurus.
    pub typos: TypoList,
    /// The selected role's thesaurus and rolegraph. Without it, weakness
    /// falls back to hedge density with text-only tie-breaks.
    pub role: Option<RoleKnowledge>,
    /// Thresholds.
    pub options: LabOptions,
}

impl LabConfig {
    /// The embedded default style and typo lists, no role, default options.
    pub fn with_defaults() -> Result<LabConfig, LabError> {
        Ok(LabConfig {
            style: StyleLists::defaults()?,
            typos: TypoList::defaults()?,
            role: None,
            options: LabOptions::default(),
        })
    }

    /// Set the selected role.
    pub fn with_role(mut self, role: RoleKnowledge) -> LabConfig {
        self.role = Some(role);
        self
    }
}

/// Run one action over `body`.
///
/// Marks are sorted by `start`, then `end`, then kind. The result depends
/// only on `body` and `config`: the same inputs give the same marks on every
/// run and every target.
pub fn mark(body: &str, config: &LabConfig, action: LabAction) -> Vec<LabMark> {
    mark_many(body, config, &[action])
}

/// Run every action over `body`, parsing it once.
pub fn mark_all(body: &str, config: &LabConfig) -> Vec<LabMark> {
    mark_many(body, config, &LabAction::ALL)
}

/// Run the given actions over `body`, parsing it once.
pub fn mark_many(body: &str, config: &LabConfig, actions: &[LabAction]) -> Vec<LabMark> {
    let analysis = actions::Analysis::new(body, config);
    let mut raw = Vec::new();
    let mut seen = [false; 6];
    for &action in actions {
        let slot = action as usize;
        if std::mem::replace(&mut seen[slot], true) {
            continue;
        }
        match action {
            LabAction::TyposAndPunctuation => {
                actions::typos_and_punctuation(&analysis, config, &mut raw)
            }
            LabAction::WeakestSentences => actions::weakest_sentences(&analysis, config, &mut raw),
            LabAction::LongSentences => actions::long_sentences(&analysis, config, &mut raw),
            LabAction::ConvolutedSentences => {
                actions::convoluted_sentences(&analysis, config, &mut raw)
            }
            LabAction::OffTone => actions::off_tone(&analysis, &mut raw),
            LabAction::HedgesAndFiller => actions::hedges_and_filler(&analysis, &mut raw),
        }
    }
    raw.sort_by(|a, b| {
        (a.start, a.end, a.kind)
            .cmp(&(b.start, b.end, b.kind))
            .then(a.reason.cmp(&b.reason))
    });
    raw.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.kind == b.kind);
    let mut marks: Vec<LabMark> = raw
        .into_iter()
        .map(|m| LabMark {
            kind: m.kind,
            start: m.start,
            end: m.end,
            score: m.score,
            reason: m.reason,
            proposal: m.proposal,
        })
        .collect();
    let mut offsets: Vec<&mut usize> = marks
        .iter_mut()
        .flat_map(|m| [&mut m.start, &mut m.end])
        .collect();
    offset::bytes_to_utf16(body, &mut offsets);
    marks
}
