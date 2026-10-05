//! The six mark actions, on byte offsets into the original body.

use crate::lists::{Hit, StyleCategory};
use crate::rolegraph::RoleKnowledge;
use crate::text::{Doc, Folded, overlaps};
use crate::{LabConfig, MarkKind};

/// A mark before UTF-16 conversion.
#[derive(Debug, Clone)]
pub(crate) struct RawMark {
    pub kind: MarkKind,
    pub start: usize,
    pub end: usize,
    pub score: f64,
    pub reason: String,
    pub proposal: Option<String>,
}

/// One parsed body plus the matcher passes several actions share.
pub(crate) struct Analysis<'a> {
    pub doc: Doc<'a>,
    /// Whitespace-folded view used for every KG match.
    pub folded: Folded,
    /// Style-list matches in prose outside protected text, in body order,
    /// on original byte offsets.
    pub style_hits: Vec<(usize, usize, StyleCategory)>,
}

impl<'a> Analysis<'a> {
    pub(crate) fn new(body: &'a str, config: &LabConfig) -> Analysis<'a> {
        let doc = Doc::parse(body);
        let folded = Folded::new(body);
        let style_hits = config
            .style
            .find(&folded.text)
            .into_iter()
            .map(|(s, e, cat)| {
                let (s, e) = folded.to_original(s, e);
                (s, e, cat)
            })
            .filter(|&(s, e, _)| usable(&doc, s, e))
            .collect();
        Analysis {
            doc,
            folded,
            style_hits,
        }
    }

    /// Typo matches, on original offsets, in usable prose.
    fn typo_hits(&self, config: &LabConfig) -> Vec<Hit> {
        config
            .typos
            .find(&self.folded.text)
            .into_iter()
            .filter_map(|mut h| {
                (h.start, h.end) = self.folded.to_original(h.start, h.end);
                usable(&self.doc, h.start, h.end).then_some(h)
            })
            .collect()
    }

    /// Role concept matches, on original offsets, in usable prose.
    pub(crate) fn concept_hits(&self, role: &RoleKnowledge) -> Vec<Hit> {
        role.concepts
            .find(&self.folded.text)
            .into_iter()
            .filter_map(|mut h| {
                (h.start, h.end) = self.folded.to_original(h.start, h.end);
                usable(&self.doc, h.start, h.end).then_some(h)
            })
            .collect()
    }
}

/// A match is usable when it lies in one prose paragraph and touches no
/// protected text.
fn usable(doc: &Doc<'_>, start: usize, end: usize) -> bool {
    !doc.is_protected(start, end) && in_prose(doc, start, end)
}

/// True when `start..end` lies inside one prose paragraph.
fn in_prose(doc: &Doc<'_>, start: usize, end: usize) -> bool {
    let i = doc.paragraphs.partition_point(|p| p.end <= start);
    doc.paragraphs
        .get(i)
        .is_some_and(|p| p.start <= start && end <= p.end)
}

// ------------------------------------------------------------ typos

/// Repeated words that are often correct ("had had", "that that").
const LEGITIMATE_REPEATS: [&str; 3] = ["had", "that", "is"];

/// Match the case of `original` on `proposal`: all-caps stays all-caps, an
/// initial capital stays an initial capital.
fn match_case(original: &str, proposal: &str) -> String {
    let letters: Vec<char> = original.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return proposal.to_uppercase();
    }
    if original.chars().next().is_some_and(char::is_uppercase) {
        let mut chars = proposal.chars();
        if let Some(first) = chars.next() {
            return first.to_uppercase().chain(chars).collect();
        }
    }
    proposal.to_string()
}

pub(crate) fn typos_and_punctuation(a: &Analysis<'_>, config: &LabConfig, out: &mut Vec<RawMark>) {
    let doc = &a.doc;
    let text = doc.text;
    // Typos: positioned matches against the typo thesaurus (word-boundary
    // filtered, so "teh" never matches inside "tehran").
    for hit in a.typo_hits(config) {
        let original = &text[hit.start..hit.end];
        let proposal = match_case(original, hit.term.display());
        if proposal == original {
            continue;
        }
        out.push(RawMark {
            kind: MarkKind::Typo,
            start: hit.start,
            end: hit.end,
            score: 1.0,
            reason: format!("typo: \"{original}\" -> \"{proposal}\""),
            proposal: Some(proposal),
        });
    }
    for p in &doc.paragraphs {
        punctuation(doc, p.start, p.end, out);
    }
    repeated_words(doc, out);
}

fn punct_mark(start: usize, end: usize, reason: &str, proposal: &str) -> RawMark {
    RawMark {
        kind: MarkKind::Punctuation,
        start,
        end,
        score: 1.0,
        reason: reason.to_string(),
        proposal: Some(proposal.to_string()),
    }
}

/// Conservative punctuation rules inside one paragraph. Each proposes the
/// corrected text for its span; nothing is rewritten.
fn punctuation(doc: &Doc<'_>, start: usize, end: usize, out: &mut Vec<RawMark>) {
    let text = doc.text;
    let bytes = text.as_bytes();
    let mut i = start;
    while i < end {
        let c = bytes[i];
        if doc.is_protected(i, i + 1) {
            i += 1;
            continue;
        }
        match c {
            // Spaces before a closing punctuation mark: "word ,".
            b' ' => {
                let run = bytes[i..end].iter().take_while(|&&b| b == b' ').count();
                let after = i + run;
                let before = text[..i].chars().next_back();
                let word_before = before.is_some_and(|ch| ch.is_alphanumeric());
                if after < end && word_before && !doc.is_protected(after, after + 1) {
                    let p = bytes[after];
                    let ellipsis = p == b'.' && bytes.get(after + 1) == Some(&b'.');
                    if matches!(p, b',' | b';' | b':' | b'.' | b'!' | b'?') && !ellipsis {
                        let punct = &text[after..after + 1];
                        out.push(punct_mark(
                            i,
                            after + 1,
                            &format!("space before \"{punct}\""),
                            punct,
                        ));
                    } else if run >= 2
                        && text[after..]
                            .chars()
                            .next()
                            .is_some_and(|ch| !ch.is_whitespace())
                    {
                        // Two or more spaces between words on one line.
                        out.push(punct_mark(i, after, "repeated space", " "));
                    }
                }
                i = after;
            }
            // Doubled comma or semicolon: ",," ";;".
            b',' | b';' if bytes.get(i + 1) == Some(&c) && i + 1 < end => {
                let run = bytes[i..end].iter().take_while(|&&b| b == c).count();
                let punct = &text[i..i + 1];
                out.push(punct_mark(
                    i,
                    i + run,
                    &format!("repeated \"{punct}\""),
                    punct,
                ));
                i += run;
            }
            // Missing space after a comma or semicolon between letters:
            // "word,next" (digits are left alone: "1,000").
            b',' | b';' => {
                let before = text[..i].chars().next_back();
                let after = text[i + 1..end].chars().next();
                if before.is_some_and(char::is_alphabetic)
                    && after.is_some_and(char::is_alphabetic)
                    && !doc.is_protected(i + 1, i + 2)
                {
                    let punct = &text[i..i + 1];
                    out.push(punct_mark(
                        i,
                        i + 1,
                        &format!("missing space after \"{punct}\""),
                        &format!("{punct} "),
                    ));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}

/// "the the": the second occurrence, with the whitespace before it.
fn repeated_words(doc: &Doc<'_>, out: &mut Vec<RawMark>) {
    let text = doc.text;
    for p in &doc.paragraphs {
        let words = doc.words_in(p.start, p.end);
        for pair in words.windows(2) {
            let ((s1, e1), (s2, e2)) = (pair[0], pair[1]);
            let gap = &text[e1..s2];
            if gap.is_empty() || !gap.chars().all(|c| c == ' ' || c == '\t') {
                continue;
            }
            let (w1, w2) = (&text[s1..e1], &text[s2..e2]);
            if !w1.chars().any(char::is_alphabetic)
                || !w1.eq_ignore_ascii_case(w2)
                || LEGITIMATE_REPEATS.contains(&w1.to_lowercase().as_str())
                || overlaps(&doc.protected, s1, e2)
            {
                continue;
            }
            out.push(RawMark {
                kind: MarkKind::Punctuation,
                start: s1,
                end: e2,
                score: 1.0,
                reason: format!("repeated word \"{w2}\""),
                proposal: Some(w1.to_string()),
            });
        }
    }
}

// ------------------------------------------------------------ word lists

pub(crate) fn hedges_and_filler(a: &Analysis<'_>, out: &mut Vec<RawMark>) {
    let text = a.doc.text;
    for &(s, e, cat) in &a.style_hits {
        let (kind, label) = match cat {
            StyleCategory::Filler => (MarkKind::Filler, "filler"),
            StyleCategory::Hedge | StyleCategory::HedgePhrase => (MarkKind::Hedge, "hedge"),
            StyleCategory::Tone => continue,
        };
        out.push(RawMark {
            kind,
            start: s,
            end: e,
            score: 1.0,
            reason: format!("{label} \"{}\"", text[s..e].to_lowercase()),
            proposal: None,
        });
    }
}

pub(crate) fn off_tone(a: &Analysis<'_>, out: &mut Vec<RawMark>) {
    let text = a.doc.text;
    for &(s, e, cat) in &a.style_hits {
        if cat == StyleCategory::Tone {
            out.push(RawMark {
                kind: MarkKind::OffTone,
                start: s,
                end: e,
                score: 1.0,
                reason: format!("off-tone \"{}\"", text[s..e].to_lowercase()),
                proposal: None,
            });
        }
    }
}

// ------------------------------------------------------------ sentences

/// Push one mark per unprotected segment of sentence `i`.
fn sentence_marks(
    doc: &Doc<'_>,
    i: usize,
    kind: MarkKind,
    score: f64,
    reason: &str,
    out: &mut Vec<RawMark>,
) {
    let s = doc.sentences[i];
    for (start, end) in doc.unprotected_segments(s.start, s.end) {
        out.push(RawMark {
            kind,
            start,
            end,
            score,
            reason: reason.to_string(),
            proposal: None,
        });
    }
}

pub(crate) fn long_sentences(a: &Analysis<'_>, config: &LabConfig, out: &mut Vec<RawMark>) {
    let doc = &a.doc;
    let limit = config.options.long_sentence_words.max(1);
    for (i, s) in doc.sentences.iter().enumerate() {
        let words = doc.word_count(s.start, s.end);
        if words >= limit {
            sentence_marks(
                doc,
                i,
                MarkKind::LongSentence,
                words as f64 / limit as f64,
                &format!("{words} words (limit {limit})"),
                out,
            );
        }
    }
}

/// Words that open a subordinate or relative clause.
const SUBORDINATORS: &[&str] = &[
    "although", "because", "though", "unless", "whereas", "whereby", "wherein", "whether", "which",
    "while", "whilst", "who", "whom", "whose",
];

/// Clause-structure counts for one sentence (protected text excluded).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct Structure {
    pub commas: usize,
    /// Semicolons, colons and dashes used as clause breaks.
    pub breaks: usize,
    pub parentheticals: usize,
    pub subordinators: usize,
    /// Deepest bracket nesting.
    pub depth: usize,
}

impl Structure {
    /// Weighted clause load: commas count half, so a plain enumeration
    /// ("a, b, c and d") does not read as convoluted on its own; nested
    /// brackets add two per extra level.
    pub fn load(&self) -> f64 {
        self.commas as f64 * 0.5
            + self.breaks as f64
            + self.parentheticals as f64
            + self.subordinators as f64
            + 2.0 * self.depth.saturating_sub(1) as f64
    }
}

pub(crate) fn structure(doc: &Doc<'_>, start: usize, end: usize) -> Structure {
    let text = doc.text;
    let mut st = Structure::default();
    let mut depth = 0usize;
    for (off, c) in text[start..end].char_indices() {
        let p = start + off;
        if doc.is_protected(p, p + c.len_utf8()) {
            continue;
        }
        match c {
            '(' | '[' => {
                if depth == 0 {
                    st.parentheticals += 1;
                }
                depth += 1;
                st.depth = st.depth.max(depth);
            }
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' => st.commas += 1,
            ';' | ':' | '\u{2014}' => st.breaks += 1,
            // An en dash or a double hyphen spaced as a dash.
            '\u{2013}' if text[..p].ends_with(' ') => st.breaks += 1,
            '-' if text[p..].starts_with("--") && text[..p].ends_with(' ') => st.breaks += 1,
            _ => {}
        }
    }
    for &(ws, we) in doc.words_in(start, end) {
        if !doc.is_protected(ws, we) {
            let w = text[ws..we].to_lowercase();
            if SUBORDINATORS.contains(&w.as_str()) {
                st.subordinators += 1;
            }
        }
    }
    st
}

pub(crate) fn convoluted_sentences(a: &Analysis<'_>, config: &LabConfig, out: &mut Vec<RawMark>) {
    let doc = &a.doc;
    let threshold = config.options.convoluted_load;
    for (i, s) in doc.sentences.iter().enumerate() {
        let st = structure(doc, s.start, s.end);
        let load = st.load();
        if load >= threshold {
            let reason = format!(
                "clause load {load:.1} (limit {threshold:.1}): {} commas, {} breaks, {} brackets, {} subordinate clauses{}",
                st.commas,
                st.breaks,
                st.parentheticals,
                st.subordinators,
                if st.depth > 1 {
                    ", nested brackets"
                } else {
                    ""
                }
            );
            sentence_marks(
                doc,
                i,
                MarkKind::ConvolutedSentence,
                load / threshold,
                &reason,
                out,
            );
        }
    }
}

pub(crate) fn weakest_sentences(a: &Analysis<'_>, config: &LabConfig, out: &mut Vec<RawMark>) {
    let doc = &a.doc;
    let role = config.role.as_ref().map(|r| (r, a.concept_hits(r)));
    let scores = crate::weak::score(
        doc,
        &a.style_hits,
        role.as_ref().map(|(r, hits)| (*r, hits.as_slice())),
    );
    let picked = crate::weak::select(
        &scores,
        doc,
        config.options.weakest_share,
        config.options.min_sentence_words,
    );
    for i in picked {
        let w = &scores[i];
        let mut reason = if w.graph.concepts.is_empty() {
            if config.role.is_some() {
                "touches no role concepts".to_string()
            } else {
                "no role selected".to_string()
            }
        } else {
            format!(
                "weak role concepts ({}; strength {:.2})",
                w.graph.concepts.join(", "),
                w.graph.strength
            )
        };
        if w.hedges > 0 {
            reason.push_str(&format!(
                "; {} hedge{}/filler",
                w.hedges,
                if w.hedges == 1 { "" } else { "s" }
            ));
        }
        sentence_marks(doc, i, MarkKind::WeakSentence, w.primary, &reason, out);
    }
}
