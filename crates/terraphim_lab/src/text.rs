//! Document structure on the **original** body: words, paragraphs, sentences
//! and protected ranges.
//!
//! Nothing here builds a normalised copy of the text. Every offset is a byte
//! offset into the body exactly as the caller passed it (hard-wrapped lines,
//! `\r\n` line endings and all), so a mark computed here can be converted to
//! UTF-16 and handed to the editor without an offset map.

/// A paragraph-level block of prose (a paragraph or a list item).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Paragraph {
    /// First byte of prose (after any list marker).
    pub start: usize,
    /// One past the last byte of prose on the block's last line (line ending
    /// excluded).
    pub end: usize,
}

/// One sentence inside a [`Paragraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sentence {
    pub start: usize,
    pub end: usize,
    /// Index into [`Doc::paragraphs`].
    pub para: usize,
    /// True for the paragraph's opening sentence (never marked weakest).
    pub first_in_para: bool,
}

/// The analysed body.
#[derive(Debug, Clone)]
pub(crate) struct Doc<'a> {
    pub text: &'a str,
    pub paragraphs: Vec<Paragraph>,
    pub sentences: Vec<Sentence>,
    /// Sorted, non-overlapping byte ranges that must never be marked:
    /// headings, fenced and indented code, tables, block quotes, HTML
    /// blocks, inline code, link destinations and bare URLs.
    pub protected: Vec<(usize, usize)>,
    /// Every word in the body, in order (see [`word_spans`]).
    pub words: Vec<(usize, usize)>,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_joiner(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-' | '.' | '/')
}

/// The one word definition used by every action (and by the status card):
/// a maximal run of alphanumerics or `_`, where an apostrophe, hyphen, full
/// stop or slash joins two runs only when it sits between word characters.
/// `hadn't`, `liver-pill`, `R-8.7` and `terraphim_lsp` are one word each;
/// Markdown syntax (`**`, backticks, `#`) is never a word.
pub(crate) fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(text.len() / 5);
    let mut iter = text.char_indices().peekable();
    while let Some((start, c)) = iter.next() {
        if !is_word_char(c) {
            continue;
        }
        let mut end = start + c.len_utf8();
        while let Some(&(pos, next)) = iter.peek() {
            if is_word_char(next) {
                end = pos + next.len_utf8();
                iter.next();
            } else if is_joiner(next) {
                // A joiner only counts when a word character follows it.
                let after = text[pos + next.len_utf8()..].chars().next();
                if after.is_some_and(is_word_char) {
                    iter.next();
                    end = pos + next.len_utf8();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        out.push((start, end));
    }
    out
}

/// Lower-case abbreviations that end in a full stop but do not end a sentence.
const ABBREVIATIONS: [&str; 12] = [
    "e.g.", "i.e.", "st.", "mr.", "mrs.", "ms.", "dr.", "vs.", "cf.", "no.", "fig.", "etc.",
];

/// Characters that may trail a terminator and still belong to the sentence.
fn is_closer(c: char) -> bool {
    matches!(
        c,
        '.' | '!' | '?' | '"' | '\u{201D}' | '\u{2019}' | ')' | ']' | '*' | '`' | '_'
    )
}

/// Characters that may open a new sentence after whitespace.
fn opens_sentence(c: char) -> bool {
    c.is_uppercase()
        || c.is_ascii_digit()
        || matches!(
            c,
            '\u{201C}' | '"' | '(' | '*' | '`' | '_' | '[' | '\u{2018}'
        )
}

/// Split `text[start..end]` into sentences. Terminators inside protected
/// ranges (inline code, URLs) never split.
fn split_sentences(
    text: &str,
    start: usize,
    end: usize,
    protected: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let slice = &text[start..end];
    let chars: Vec<(usize, char)> = slice.char_indices().collect();
    let mut out = Vec::new();
    let mut s_start = 0usize;
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if !matches!(c, '.' | '!' | '?') || in_ranges(protected, start + pos) {
            i += 1;
            continue;
        }
        if c == '.' {
            let tok_start = slice[..pos]
                .rfind(char::is_whitespace)
                .map(|p| p + 1)
                .unwrap_or(0);
            let tok = slice[tok_start..=pos]
                .trim_start_matches(|ch: char| !ch.is_alphanumeric())
                .to_lowercase();
            // "**No.**" is a sentence, not the abbreviation "no.".
            let bold = slice[tok_start..].starts_with("**");
            if ABBREVIATIONS.contains(&tok.as_str()) && !bold {
                i += 1;
                continue;
            }
        }
        let mut j = i + 1;
        while j < chars.len() && is_closer(chars[j].1) {
            j += 1;
        }
        if j < chars.len() && chars[j].1.is_whitespace() {
            let mut k = j;
            while k < chars.len() && chars[k].1.is_whitespace() {
                k += 1;
            }
            if k < chars.len() && opens_sentence(chars[k].1) {
                out.push((start + s_start, start + chars[j].0));
                s_start = chars[k].0;
                i = k;
                continue;
            }
        }
        i = j;
    }
    if s_start < slice.len() {
        let tail = slice[s_start..].trim_end();
        if !tail.is_empty() {
            out.push((start + s_start, start + s_start + tail.len()));
        }
    }
    out
}

/// True when byte `pos` lies inside one of the sorted `ranges`.
pub(crate) fn in_ranges(ranges: &[(usize, usize)], pos: usize) -> bool {
    let i = ranges.partition_point(|r| r.1 <= pos);
    ranges.get(i).is_some_and(|r| r.0 <= pos)
}

/// True when `start..end` overlaps one of the sorted `ranges`.
pub(crate) fn overlaps(ranges: &[(usize, usize)], start: usize, end: usize) -> bool {
    let i = ranges.partition_point(|r| r.1 <= start);
    ranges.get(i).is_some_and(|r| r.0 < end)
}

/// Byte length of a list marker (`- `, `* `, `+ `, `12. `, `3) `) at the start
/// of an already left-trimmed line, including the following space.
fn list_marker_len(t: &str) -> Option<usize> {
    let b = t.as_bytes();
    if b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' {
        // `- [ ] ` task-list boxes are part of the marker.
        for task in ["[ ] ", "[x] ", "[X] "] {
            if t[2..].starts_with(task) {
                return Some(2 + task.len());
            }
        }
        return Some(2);
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits)
        && b.len() > digits + 1
        && matches!(b[digits], b'.' | b')')
        && b[digits + 1] == b' '
    {
        return Some(digits + 2);
    }
    None
}

fn is_heading(t: &str) -> bool {
    let hashes = t.bytes().take_while(|&c| c == b'#').count();
    (1..=6).contains(&hashes) && t[hashes..].chars().next().is_none_or(char::is_whitespace)
}

/// A thematic break or setext underline: three or more of one of `-`, `*`,
/// `_`, `=`, optionally separated by spaces.
fn is_rule_line(t: &str) -> Option<char> {
    let first = t.chars().next()?;
    if !matches!(first, '-' | '*' | '_' | '=') {
        return None;
    }
    let mut n = 0;
    for c in t.chars() {
        if c == first {
            n += 1;
        } else if c != ' ' && c != '\t' {
            return None;
        }
    }
    (n >= 3).then_some(first)
}

fn is_html_block(t: &str) -> bool {
    let mut it = t.chars();
    it.next() == Some('<')
        && it
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!')
}

/// `[label]: destination` link reference definitions.
fn is_link_definition(t: &str) -> bool {
    t.starts_with('[') && t.find("]:").is_some_and(|p| p > 1)
}

struct Builder {
    paragraphs: Vec<Paragraph>,
    protected: Vec<(usize, usize)>,
    open: Option<Paragraph>,
}

impl Builder {
    fn close(&mut self) {
        if let Some(p) = self.open.take()
            && p.end > p.start
        {
            self.paragraphs.push(p);
        }
    }
}

impl<'a> Doc<'a> {
    /// Analyse `text` without copying or normalising it.
    pub(crate) fn parse(text: &'a str) -> Doc<'a> {
        let mut b = Builder {
            paragraphs: Vec::new(),
            protected: Vec::new(),
            open: None,
        };
        // (fence character, fence length) while inside a fenced block.
        let mut fence: Option<(char, usize)> = None;
        let mut pos = 0usize;
        for line in text.split_inclusive('\n') {
            let line_start = pos;
            pos += line.len();
            let content = line.trim_end_matches(['\n', '\r']);
            let content_end = line_start + content.len();
            let trimmed = content.trim_start();
            let indent = content.len() - trimmed.len();

            if let Some((fc, flen)) = fence {
                b.protected.push((line_start, content_end));
                let run = trimmed.chars().take_while(|&c| c == fc).count();
                if run >= flen && trimmed[run * fc.len_utf8()..].trim().is_empty() {
                    fence = None;
                }
                continue;
            }
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                b.close();
                let fc = trimmed.chars().next().unwrap_or('`');
                fence = Some((fc, trimmed.chars().take_while(|&c| c == fc).count()));
                b.protected.push((line_start, content_end));
                continue;
            }
            if trimmed.is_empty() {
                b.close();
                continue;
            }
            if indent >= 4 && b.open.is_none() {
                // Indented code block.
                b.protected.push((line_start, content_end));
                continue;
            }
            if let Some(rule) = is_rule_line(trimmed) {
                // A setext underline turns the open paragraph into a heading.
                if matches!(rule, '=' | '-')
                    && let Some(p) = b.open.take()
                {
                    b.protected.push((p.start, p.end));
                }
                b.close();
                b.protected.push((line_start, content_end));
                continue;
            }
            if is_heading(trimmed)
                || trimmed.starts_with('|')
                || trimmed.starts_with('>')
                || is_html_block(trimmed)
                || is_link_definition(trimmed)
            {
                b.close();
                b.protected.push((line_start, content_end));
                continue;
            }
            if let Some(m) = list_marker_len(trimmed) {
                b.close();
                b.protected.push((line_start, line_start + indent + m));
                b.open = Some(Paragraph {
                    start: line_start + indent + m,
                    end: content_end,
                });
                continue;
            }
            match b.open.as_mut() {
                Some(p) => p.end = content_end,
                None => {
                    b.open = Some(Paragraph {
                        start: line_start + indent,
                        end: content_end,
                    })
                }
            }
        }
        b.close();

        let mut protected = b.protected;
        for p in &b.paragraphs {
            inline_protected(text, p.start, p.end, &mut protected);
        }
        let protected = merge(protected);

        let mut sentences = Vec::new();
        for (para, p) in b.paragraphs.iter().enumerate() {
            for (k, (s, e)) in split_sentences(text, p.start, p.end, &protected)
                .into_iter()
                .enumerate()
            {
                sentences.push(Sentence {
                    start: s,
                    end: e,
                    para,
                    first_in_para: k == 0,
                });
            }
        }
        Doc {
            text,
            paragraphs: b.paragraphs,
            sentences,
            protected,
            words: word_spans(text),
        }
    }

    /// Number of words whose first byte lies in `start..end`.
    pub(crate) fn word_count(&self, start: usize, end: usize) -> usize {
        let lo = self.words.partition_point(|w| w.0 < start);
        let hi = self.words.partition_point(|w| w.0 < end);
        hi - lo
    }

    /// Words whose first byte lies in `start..end`.
    pub(crate) fn words_in(&self, start: usize, end: usize) -> &[(usize, usize)] {
        let lo = self.words.partition_point(|w| w.0 < start);
        let hi = self.words.partition_point(|w| w.0 < end);
        &self.words[lo..hi]
    }

    /// True when `start..end` touches protected text.
    pub(crate) fn is_protected(&self, start: usize, end: usize) -> bool {
        overlaps(&self.protected, start, end)
    }

    /// The unprotected pieces of `start..end`, each trimmed of surrounding
    /// whitespace and dropped when it holds no word. A sentence that contains
    /// inline code is marked as the prose around the code, never over it.
    pub(crate) fn unprotected_segments(&self, start: usize, end: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut cursor = start;
        let i = self.protected.partition_point(|r| r.1 <= start);
        for &(ps, pe) in &self.protected[i..] {
            if ps >= end {
                break;
            }
            if ps > cursor {
                self.push_segment(cursor, ps, &mut out);
            }
            cursor = cursor.max(pe);
        }
        if cursor < end {
            self.push_segment(cursor, end, &mut out);
        }
        out
    }

    /// Push `start..end` trimmed of whitespace and of the punctuation left
    /// dangling next to removed code: a leading `)`, `,` or closing quote, a
    /// trailing `(` or opening quote ("That contract (" becomes "That
    /// contract").
    fn push_segment(&self, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
        let seg = &self.text[start..end];
        let trimmed_start = seg.trim_start_matches(|c: char| {
            c.is_whitespace() || matches!(c, ')' | ']' | ',' | ';' | ':' | '\u{201D}' | '\u{2019}')
        });
        let trimmed = trimmed_start.trim_end_matches(|c: char| {
            c.is_whitespace() || matches!(c, '(' | '[' | '\u{201C}' | '\u{2018}')
        });
        let lead = seg.len() - trimmed_start.len();
        let (s, e) = (start + lead, start + lead + trimmed.len());
        if e > s && self.word_count(s, e) > 0 {
            out.push((s, e));
        }
    }
}

/// Protect inline code spans, link destinations and bare URLs inside one
/// paragraph. Inline code may span line breaks within the paragraph.
fn inline_protected(text: &str, start: usize, end: usize, out: &mut Vec<(usize, usize)>) {
    let slice = &text[start..end];
    let bytes = slice.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = bytes[i..].iter().take_while(|&&c| c == b'`').count();
                // Find a closing run of exactly the same length.
                let mut j = i + run;
                let mut closed = None;
                while j < bytes.len() {
                    if bytes[j] == b'`' {
                        let r = bytes[j..].iter().take_while(|&&c| c == b'`').count();
                        if r == run {
                            closed = Some(j + r);
                            break;
                        }
                        j += r;
                    } else {
                        j += 1;
                    }
                }
                match closed {
                    Some(e) => {
                        out.push((start + i, start + e));
                        i = e;
                    }
                    None => i += run,
                }
            }
            b']' if bytes.get(i + 1) == Some(&b'(') => {
                // Link destination: `](...)`, balanced parentheses.
                let mut depth = 0i32;
                let mut j = i + 1;
                while j < bytes.len() {
                    match bytes[j] {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        b'\n' => break,
                        _ => {}
                    }
                    j += 1;
                }
                let e = (j + 1).min(bytes.len());
                out.push((start + i + 1, start + e));
                i = e;
            }
            b'h' if slice[i..].starts_with("http://") || slice[i..].starts_with("https://") => {
                let len = slice[i..]
                    .find(|c: char| c.is_whitespace() || matches!(c, '>' | ')' | '"'))
                    .unwrap_or(slice.len() - i);
                // A trailing full stop or comma ends the sentence, not the URL.
                let url = slice[i..i + len].trim_end_matches(['.', ',', ';', ':']);
                out.push((start + i, start + i + url.len()));
                i += len.max(1);
            }
            _ => i += 1,
        }
    }
}

/// A whitespace-folded view of the body for KG matching, with a map back to
/// original offsets.
///
/// Multi-word KG terms ("all of a sudden", "language server") must match
/// across hard line wraps, `\r\n` endings and indentation, but marks must
/// index the original body. Every run of whitespace that is not a single
/// space becomes one space here; `map[i]` is the original byte offset of
/// folded byte `i`. A match that crosses a paragraph break maps to a range
/// spanning two paragraphs, which the actions reject.
#[derive(Debug, Clone)]
pub(crate) struct Folded {
    pub text: String,
    map: Vec<usize>,
}

impl Folded {
    pub(crate) fn new(original: &str) -> Folded {
        let mut text = String::with_capacity(original.len());
        let mut map = Vec::with_capacity(original.len() + 1);
        let mut chars = original.char_indices().peekable();
        while let Some((pos, c)) = chars.next() {
            if c.is_whitespace() {
                text.push(' ');
                map.push(pos);
                while chars.peek().is_some_and(|&(_, n)| n.is_whitespace()) {
                    chars.next();
                }
            } else {
                let mut buf = [0u8; 4];
                text.push_str(c.encode_utf8(&mut buf));
                map.extend(pos..pos + c.len_utf8());
            }
        }
        map.push(original.len());
        Folded { text, map }
    }

    /// Original byte range of folded range `start..end` (a match never
    /// starts or ends on folded whitespace, because KG terms are trimmed).
    pub(crate) fn to_original(&self, start: usize, end: usize) -> (usize, usize) {
        let s = self.map[start];
        let e = if end > start {
            self.map[end - 1] + 1
        } else {
            s
        };
        // `map[end - 1]` is the last byte of the final character, so `+ 1`
        // lands on the next char boundary.
        (s, e)
    }
}

/// Sort and merge byte ranges.
fn merge(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.retain(|r| r.1 > r.0);
    ranges.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (s, e) in ranges {
        if let Some(last) = out.last_mut()
            && s <= last.1
        {
            last.1 = last.1.max(e);
            continue;
        }
        out.push((s, e));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        word_spans(text).iter().map(|&(s, e)| &text[s..e]).collect()
    }

    #[test]
    fn word_definition_matches_the_status_card() {
        assert_eq!(
            words("us\u{2014}George hadn\u{2019}t a liver-pill, R-8.7 `terraphim_lsp` **ok**."),
            vec![
                "us",
                "George",
                "hadn\u{2019}t",
                "a",
                "liver-pill",
                "R-8.7",
                "terraphim_lsp",
                "ok"
            ]
        );
    }

    #[test]
    fn hard_wrapped_paragraphs_keep_original_offsets() {
        let text = "First line of a\nwrapped sentence. Second\r\none here.\r\n\r\nNext para.";
        let doc = Doc::parse(text);
        assert_eq!(doc.paragraphs.len(), 2);
        let s: Vec<&str> = doc
            .sentences
            .iter()
            .map(|s| &text[s.start..s.end])
            .collect();
        assert_eq!(
            s,
            vec![
                "First line of a\nwrapped sentence.",
                "Second\r\none here.",
                "Next para."
            ]
        );
        assert!(doc.sentences[0].first_in_para);
        assert!(!doc.sentences[1].first_in_para);
        assert!(doc.sentences[2].first_in_para);
    }

    #[test]
    fn structure_is_protected() {
        let text = "# Heading here\n\nProse with `code. Here` inside.\n\n```\nfenced. Code\n```\n\n| a | b |\n|---|---|\n\n> quoted\n\n- item one. Item two\n";
        let doc = Doc::parse(text);
        let prot: Vec<&str> = doc.protected.iter().map(|&(s, e)| &text[s..e]).collect();
        assert!(prot.contains(&"# Heading here"));
        assert!(prot.contains(&"`code. Here`"));
        assert!(prot.iter().any(|p| p.contains("fenced. Code")));
        assert!(prot.contains(&"| a | b |"));
        assert!(prot.contains(&"> quoted"));
        let s: Vec<&str> = doc
            .sentences
            .iter()
            .map(|s| &text[s.start..s.end])
            .collect();
        // The `.` inside inline code does not split the sentence.
        assert_eq!(
            s,
            vec!["Prose with `code. Here` inside.", "item one.", "Item two"]
        );
    }

    #[test]
    fn setext_heading_paragraph_is_protected() {
        let text = "Title words\n===\n\nBody text.";
        let doc = Doc::parse(text);
        assert!(doc.is_protected(0, 5));
        assert_eq!(doc.sentences.len(), 1);
    }

    #[test]
    fn unprotected_segments_skip_code() {
        let text = "Use `foo` and then `bar` today.";
        let doc = Doc::parse(text);
        let segs: Vec<&str> = doc
            .unprotected_segments(0, text.len())
            .iter()
            .map(|&(s, e)| &text[s..e])
            .collect();
        assert_eq!(segs, vec!["Use", "and then", "today."]);
    }

    #[test]
    fn urls_and_link_destinations_are_protected() {
        let text = "See [the docs](https://example.org/teh-page) or https://x.org/a.b, then stop.";
        let doc = Doc::parse(text);
        let prot: Vec<&str> = doc.protected.iter().map(|&(s, e)| &text[s..e]).collect();
        assert_eq!(
            prot,
            vec!["(https://example.org/teh-page)", "https://x.org/a.b"]
        );
    }
}
