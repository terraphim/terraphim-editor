//! Lightweight boundaries into the exact original UTF-8 text. Markdown
//! structure belongs to callers; this module never normalises or searches
//! for copies of returned text to recover offsets.
use unicode_segmentation::UnicodeSegmentation;

/// Half-open original UTF-8 byte range (not UTF-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// Automatic targeting granularity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextUnit {
    Word,
    Sentence,
    Paragraph,
}

const ABBREVIATIONS: [&str; 12] = [
    "e.g.", "i.e.", "st.", "mr.", "mrs.", "ms.", "dr.", "vs.", "cf.", "no.", "fig.", "etc.",
];

/// Existing Lab abbreviation policy, shared with sentence extraction and tidy
/// edits. `**No.**` is deliberately a sentence, not the abbreviation `no.`.
pub fn is_abbreviation(text: &str, floor: usize, dot: usize) -> bool {
    let Some(prefix) = text.get(floor..dot) else {
        return false;
    };
    if text.as_bytes().get(dot) != Some(&b'.') {
        return false;
    }
    let tok_start = prefix.rfind(char::is_whitespace).map_or(floor, |p| {
        floor + p + text[floor + p..].chars().next().map_or(0, char::len_utf8)
    });
    let tok = text[tok_start..=dot]
        .trim_start_matches(|ch: char| !ch.is_alphanumeric())
        .to_lowercase();
    ABBREVIATIONS.contains(&tok.as_str()) && !text[tok_start..].starts_with("**")
}

/// Existing tidy-edit full-stop policy: excludes abbreviations, decimal points
/// and ellipses. Ellipses can still end a UAX sentence; tidy-up is stricter.
pub fn full_stop_ends_sentence(text: &str, dot: usize) -> bool {
    if !text.is_char_boundary(dot) || text.as_bytes().get(dot) != Some(&b'.') {
        return false;
    }
    let before = text[..dot].chars().next_back();
    let after = text[dot + 1..].chars().next();
    let ellipsis = before == Some('.') || after == Some('.');
    let decimal =
        before.is_some_and(|c| c.is_ascii_digit()) && after.is_some_and(|c| c.is_ascii_digit());
    !ellipsis && !decimal && !is_abbreviation(text, 0, dot)
}

fn closer(c: char) -> bool {
    matches!(c, '"' | '\'' | '”' | '’' | ')' | ']' | '*' | '`' | '_')
}

// Lab's existing Markdown openers can begin a sentence even when the visible
// text is lowercase. UAX alone treats a lowercase inline-code start as a
// full-stop continuation. This is a boundary policy, not Markdown parsing;
// callers supply mdast-protected syntax ranges.
fn markdown_opener(c: char) -> bool {
    matches!(c, '“' | '"' | '(' | '*' | '`' | '_' | '[' | '‘')
}

fn sentence_terminator(c: char) -> bool {
    if c.is_ascii() {
        return matches!(c, '.' | '!' | '?');
    }
    if c.is_alphanumeric() || c.is_whitespace() {
        return false;
    }
    // Use the pinned UAX engine, not a second Unicode punctuation table. A
    // scalar followed by space + uppercase starter has two UAX sentences only
    // if it terminates a sentence. The probe is stack-backed, never a modified
    // document or a source-offset recovery mechanism.
    let mut bytes = [0u8; 6];
    let len = c.encode_utf8(&mut bytes).len();
    bytes[len] = b' ';
    bytes[len + 1] = b'A';
    std::str::from_utf8(&bytes[..len + 2])
        .ok()
        .is_some_and(|probe| probe.split_sentence_bounds().nth(1).is_some())
}

fn is_protected(protected: &[ByteRange], pos: usize) -> bool {
    protected.iter().any(|r| r.start <= pos && pos < r.end)
}

fn trim(text: &str, start: usize, end: usize) -> Option<ByteRange> {
    let slice = text.get(start..end)?;
    let left = slice.trim_start();
    let start = start + slice.len() - left.len();
    let end = start + left.trim_end().len();
    (end > start).then_some(ByteRange { start, end })
}

/// Blank-line paragraphs, retaining hardwraps and CRLF bytes. Structural
/// Markdown boundaries (headings, code, lists) must be supplied by mdast.
pub fn paragraph_ranges(text: &str) -> Vec<ByteRange> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim().is_empty() {
            if let Some(range) = trim(text, start, offset) {
                ranges.push(range);
            }
            start = offset + line.len();
        }
        offset += line.len();
    }
    if let Some(range) = trim(text, start, text.len()) {
        ranges.push(range);
    }
    ranges
}

/// UAX sentence bounds with Lab abbreviation, hardwrap and Markdown closer
/// policy. Unlike upstream `split_paragraphs`, pipe clauses are not sentences.
pub fn sentence_ranges(text: &str) -> Vec<ByteRange> {
    sentence_ranges_with_protected(text, &[])
}

/// As [`sentence_ranges`], but terminators in caller-supplied original byte
/// ranges (for example mdast inline code and destinations) cannot split prose.
pub fn sentence_ranges_with_protected(text: &str, protected: &[ByteRange]) -> Vec<ByteRange> {
    let mut out = Vec::new();
    for paragraph in paragraph_ranges(text) {
        let mut start = paragraph.start;
        let slice = &text[paragraph.start..paragraph.end];
        let mut ends: Vec<_> = slice
            .split_sentence_bound_indices()
            .map(|(offset, part)| paragraph.start + offset + part.len())
            .collect();
        for (offset, c) in slice.char_indices() {
            let pos = paragraph.start + offset;
            if !sentence_terminator(c) || is_protected(protected, pos) {
                continue;
            }
            let after = &text[pos + c.len_utf8()..paragraph.end];
            let rest = after.trim_start_matches(|c| closer(c) || matches!(c, '.' | '!' | '?'));
            let next = rest.trim_start();
            if rest.len() != next.len() && next.chars().next().is_some_and(markdown_opener) {
                ends.push(paragraph.end - next.len());
            }
        }
        ends.sort_unstable();
        ends.dedup();
        for raw_end in ends {
            if raw_end <= start {
                continue;
            }
            let mut end = start + text[start..raw_end].trim_end().len();
            // UAX doesn't know Markdown's closing emphasis markers.
            for c in text[end..paragraph.end].chars() {
                if !closer(c) {
                    break;
                }
                end += c.len_utf8();
            }
            let terminator = text[start..end]
                .grapheme_indices(true)
                .rev()
                .filter_map(|(pos, g)| g.chars().next().map(|c| (pos, c)))
                .find(|(_, c)| !closer(*c));
            let boundary = terminator.is_some_and(|(pos, c)| {
                let pos = start + pos;
                sentence_terminator(c)
                    && !(c == '.' && is_abbreviation(text, start, pos))
                    && !is_protected(protected, pos)
            });
            if boundary || raw_end == paragraph.end {
                if let Some(range) = trim(text, start, end) {
                    out.push(range);
                }
                start = end;
                while start < paragraph.end {
                    let Some(c) = text[start..].chars().next() else {
                        break;
                    };
                    if !c.is_whitespace() {
                        break;
                    }
                    start += c.len_utf8();
                }
            }
        }
        if let Some(range) = trim(text, start, paragraph.end) {
            out.push(range);
        }
    }
    out
}

/// Range at a scalar-safe byte caret. At the exact end of a word/sentence the
/// preceding unit is selected. Invalid byte offsets and empty targets return None.
pub fn range_at(text: &str, byte: usize, unit: TextUnit) -> Option<ByteRange> {
    if byte > text.len() || !text.is_char_boundary(byte) {
        return None;
    }
    match unit {
        TextUnit::Word => crate::words::word_spans(text)
            .map(|(start, end)| ByteRange { start, end })
            .find(|r| r.start <= byte && byte <= r.end),
        TextUnit::Sentence => sentence_ranges(text)
            .into_iter()
            .find(|r| r.start <= byte && byte <= r.end),
        TextUnit::Paragraph => paragraph_ranges(text)
            .into_iter()
            .find(|r| r.start <= byte && byte <= r.end),
    }
}

/// Infer the kind of an explicit selection using the same shared boundaries
/// and counter, preserving the existing up-to-four-word phrase convention.
pub fn infer_kind(text: &str) -> TextUnit {
    let text = text.trim();
    if text.contains('\n') || sentence_ranges(text).len() > 1 {
        return TextUnit::Paragraph;
    }
    let closing = text.trim_end_matches(closer).chars().next_back();
    if crate::words::count_words(text) <= 4 && !closing.is_some_and(sentence_terminator) {
        TextUnit::Word
    } else {
        TextUnit::Sentence
    }
}
