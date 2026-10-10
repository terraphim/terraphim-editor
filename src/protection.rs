//! Complete, original-offset KG occurrence protection, independent of UI ranking.
//!
//! Normalization is deliberately bounded: ASCII case-insensitivity, existing
//! whitespace folding, exact CJK ideographs, and simple lowercase Latin-1,
//! Greek and Cyrillic letters. Other Unicode (including combining marks,
//! case expansion and non-ASCII uppercase) is refused, not silently unmatched.

use terraphim_alternatives::{text::ByteRange, words::word_spans};
use terraphim_lab::{
    automata::{CompiledMatcher, MatchOverlap, MatcherBuilder, MatcherOptions},
    types::Thesaurus,
    Cut, Folded, LabConfig, MadeCuts, TrimPlan,
};

/// One original occurrence. Canonical IDs never cross a lossy JS number seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConceptOccurrence {
    pub concept_id: u64,
    pub range: ByteRange,
}

/// Immutable complete matcher for one local role.
#[derive(Debug, Clone)]
pub struct ConceptProtection {
    matcher: CompiledMatcher,
}

/// Reject unsupported normalization before matching or publishing a plan.
/// This is a capability whitelist, not a Unicode normalizer/casefold algorithm.
/// Diagnostics intentionally contain no role aliases or document excerpts.
pub fn validate_supported(text: &str) -> Result<(), String> {
    for c in text.chars() {
        if c.is_ascii() || c.is_whitespace() {
            continue;
        }
        let supported_letter = matches!(c, '\u{e0}'..='\u{f6}' | '\u{f8}'..='\u{ff}'
            | '\u{3b1}'..='\u{3c9}' | '\u{430}'..='\u{44f}' | '\u{4e00}'..='\u{9fff}');
        let punctuation = matches!(
            c,
            '—' | '–' | '‘' | '’' | '“' | '”' | '…' | '。' | '！' | '？' | '、' | '·'
        );
        // Final sigma is not round-trip simple case; no expansion is allowed.
        if punctuation
            || (supported_letter
                && c.to_uppercase().count() == 1
                && c.to_uppercase()
                    .flat_map(char::to_lowercase)
                    .eq(std::iter::once(c)))
        {
            continue;
        }
        return Err("unsupported_normalization: KG-protected trim supports ASCII case, whitespace folding, exact CJK ideographs and simple lowercase Latin-1/Greek/Cyrillic; combining marks, case expansion and other Unicode require normalization support".into());
    }
    Ok(())
}

impl ConceptProtection {
    pub fn from_thesaurus(thesaurus: &Thesaurus) -> Result<Self, String> {
        let mut patterns = Vec::new();
        for (key, term) in thesaurus {
            validate_supported(key.as_str())?;
            let pattern = Folded::new(key.as_str()).text.trim().to_owned();
            if pattern.is_empty() {
                return Err("invalid_protection_role: blank pattern".into());
            }
            patterns.push((pattern, term.clone()));
        }
        if patterns.is_empty() {
            return Err(
                "role_required: load a nonempty local role/thesaurus to trim with KG protection"
                    .into(),
            );
        }
        patterns.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
        let mut builder = MatcherBuilder::new(
            MatcherOptions::default()
                .with_overlap(MatchOverlap::All)
                .with_min_pattern_length(1)
                .with_word_boundaries(false),
        );
        // Fold collisions are explicitly refused by insert, never overwritten.
        for (pattern, term) in patterns {
            builder
                .insert(pattern, term)
                .map_err(|_| "invalid_protection_role: matcher rejected pattern".to_owned())?;
        }
        Ok(Self {
            matcher: builder
                .build()
                .map_err(|_| "invalid_protection_role: matcher compilation failed".to_owned())?,
        })
    }

    pub fn occurrences(&self, body: &str) -> Result<Vec<ConceptOccurrence>, String> {
        validate_supported(body)?;
        let folded = Folded::new(body);
        let words: Vec<_> = word_spans(body).collect();
        let inside_word = |pos| {
            let i = words.partition_point(|w| w.0 < pos);
            i > 0 && pos < words[i - 1].1
        };
        let mut positions = Vec::new();
        self.matcher.push_positions(&folded.text, &mut positions);
        let mut result = Vec::new();
        for m in positions {
            let (start, end) = folded.to_original(m.start, m.end);
            if body.get(start..end).is_none() {
                return Err("invalid_protection_mapping: non-scalar matcher range".into());
            }
            // Unicode/contextual-joiner boundary postfilter, not ASCII automata boundaries.
            if inside_word(start) || inside_word(end) {
                continue;
            }
            let term = self
                .matcher
                .term(m.pattern_index)
                .ok_or("invalid_protection_mapping: missing identity")?;
            result.push(ConceptOccurrence {
                concept_id: term.id,
                range: ByteRange { start, end },
            });
        }
        Ok(result)
    }

    pub fn plan(&self, body: &str, config: &LabConfig) -> Result<TrimPlan, String> {
        let mut protected: Vec<ByteRange> = self
            .occurrences(body)?
            .into_iter()
            .map(|o| o.range)
            .collect();
        protected.sort_by_key(|p| (p.start, p.end));
        let mut merged: Vec<ByteRange> = Vec::new();
        for p in protected {
            if let Some(last) = merged.last_mut().filter(|last| p.start <= last.end) {
                last.end = last.end.max(p.end);
            } else {
                merged.push(p);
            }
        }
        let plan = terraphim_lab::trim_plan_excluding(body, config, &merged);
        // An unsafe tidy effect at any level prevents plan publication entirely.
        for level in terraphim_lab::TrimLevel::ALL {
            let active = plan.active(level, &[]);
            self.validate_ranges(body, &active)?;
            self.validate_retention(body, &terraphim_lab::make_cuts(body, &active))?;
        }
        Ok(plan)
    }

    /// Guard every preview/ghost range, including kept-cut fragments.
    pub fn validate_ranges(&self, body: &str, cuts: &[Cut]) -> Result<(), String> {
        let occurrences = self.occurrences(body)?;
        let offsets = Offsets::new(body);
        for cut in cuts {
            let start = offsets.byte(cut.start)?;
            let end = offsets.byte(cut.end)?;
            if start >= end
                || occurrences
                    .iter()
                    .any(|o| o.range.start < end && start < o.range.end)
            {
                return Err(
                    "unsafe_trim: range overlaps a protected occurrence or is invalid".into(),
                );
            }
        }
        Ok(())
    }

    /// Map each original occurrence through actual cut AND tidy edits. Exact
    /// original spelling and canonical recognition at its new range must survive.
    pub fn validate_retention(&self, before: &str, made: &MadeCuts) -> Result<(), String> {
        let occurrences = self.occurrences(before)?;
        let after = self.occurrences(&made.text)?;
        let offsets = Offsets::new(before);
        let mut edits = Vec::new();
        let mut previous_end = 0;
        let mut delta = 0isize;
        for edit in &made.edits {
            let s = offsets.byte(edit.start)?;
            let e = offsets.byte(edit.end)?;
            if s > e || s < previous_end {
                return Err("unsafe_trim: invalid edit sequence".into());
            }
            edits.push((s, e, delta, &edit.insert));
            delta = delta
                .checked_add(
                    isize::try_from(edit.insert.len()).map_err(|_| "unsafe_trim: size overflow")?,
                )
                .and_then(|d| d.checked_sub(isize::try_from(e - s).ok()?))
                .ok_or("unsafe_trim: size overflow")?;
            previous_end = e;
        }
        let mut reconstructed = before.to_owned();
        for &(s, e, _, insert) in edits.iter().rev() {
            reconstructed.replace_range(s..e, insert);
        }
        if reconstructed != made.text {
            return Err("unsafe_trim: edits disagree with result".into());
        }
        for o in occurrences {
            let i = edits.partition_point(|&(s, e, _, _)| e <= o.range.start && s < o.range.start);
            if edits
                .get(i)
                .is_some_and(|&(s, e, _, _)| s < o.range.end && (e > o.range.start || s == e))
            {
                return Err("unsafe_trim: cut/tidy edit changes a protected occurrence".into());
            }
            let shift = edits.get(i).map_or(delta, |e| e.2);
            let s = o
                .range
                .start
                .checked_add_signed(shift)
                .ok_or("unsafe_trim: mapping overflow")?;
            let e = o
                .range
                .end
                .checked_add_signed(shift)
                .ok_or("unsafe_trim: mapping overflow")?;
            if made.text.get(s..e) != before.get(o.range.start..o.range.end)
                || !after.iter().any(|a| {
                    a.concept_id == o.concept_id && a.range == (ByteRange { start: s, end: e })
                })
            {
                return Err("unsafe_trim: protected occurrence lost at cut join".into());
            }
        }
        Ok(())
    }
}

/// One original scalar-boundary table; no half-surrogate flooring for edits.
struct Offsets(Vec<(usize, usize)>);
impl Offsets {
    fn new(body: &str) -> Self {
        let mut units = 0;
        let mut points = Vec::new();
        for (byte, c) in body.char_indices() {
            points.push((units, byte));
            units += c.len_utf16();
        }
        points.push((units, body.len()));
        Self(points)
    }
    fn byte(&self, units: usize) -> Result<usize, String> {
        self.0
            .binary_search_by_key(&units, |p| p.0)
            .map(|i| self.0[i].1)
            .map_err(|_| "unsafe_trim: offset is not an original scalar boundary".into())
    }
}
