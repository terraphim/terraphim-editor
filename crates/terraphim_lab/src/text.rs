//! Document structure on the **original** body: words, paragraphs, sentences
//! and protected ranges.
//!
//! Nothing here builds a normalised copy of the text. Every offset is a byte
//! offset into the body exactly as the caller passed it (hard-wrapped lines,
//! `\r\n` line endings and all), so a mark computed here can be converted to
//! UTF-16 and handed to the editor without an offset map.
//!
//! Markdown structure (what is prose and what is protected) comes from the
//! `markdown` crate's mdast; this module does not re-implement block or
//! inline parsing.

use markdown::mdast::{self, Node};
use markdown::{Constructs, ParseOptions};

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

/// Parse options: GitHub-flavoured Markdown plus front matter, the dialect
/// the editor writes.
fn parse_options() -> ParseOptions {
    ParseOptions {
        constructs: Constructs {
            frontmatter: true,
            ..Constructs::gfm()
        },
        ..ParseOptions::gfm()
    }
}

/// Byte range of a node in the original body. `markdown` reports positions
/// as byte offsets into the string it parsed.
fn span(node: &Node) -> Option<(usize, usize)> {
    node.position().map(|p| (p.start.offset, p.end.offset))
}

/// True for a link whose visible text is its destination: an autolink
/// (`<https://...>`) or a GFM literal autolink (`https://...`, `www.x.org`,
/// `a@b.org`). Those are protected whole.
fn is_autolink(link: &mdast::Link, text: &str, (start, end): (usize, usize)) -> bool {
    let source = &text[start..end];
    !source.starts_with('[') || link.children.is_empty()
}

/// Walk the mdast, collecting prose paragraphs and protected ranges.
fn collect(
    node: &Node,
    text: &str,
    paragraphs: &mut Vec<Paragraph>,
    protected: &mut Vec<(usize, usize)>,
) {
    match node {
        // Never prose, never marked: protect the whole node and stop.
        Node::Html(_)
        | Node::Code(_)
        | Node::InlineCode(_)
        | Node::Heading(_)
        | Node::Table(_)
        | Node::Blockquote(_)
        | Node::Definition(_)
        | Node::ThematicBreak(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::Yaml(_)
        | Node::Toml(_)
        | Node::Math(_)
        | Node::InlineMath(_)
        | Node::FootnoteReference(_)
        | Node::FootnoteDefinition(_) => {
            if let Some(r) = span(node) {
                protected.push(r);
            }
            return;
        }
        Node::Link(link) => {
            if let Some((start, end)) = span(node) {
                let children_end = link.children.last().and_then(span).map(|(_, e)| e);
                match children_end {
                    // `[text](destination)`: the text is prose, the
                    // `](destination)` tail is not.
                    Some(ce) if !is_autolink(link, text, (start, end)) => {
                        protected.push((start, start + 1));
                        protected.push((ce, end));
                    }
                    _ => {
                        protected.push((start, end));
                        return;
                    }
                }
            }
        }
        Node::LinkReference(lr) => {
            // `[text][label]`: protect the brackets and label, keep the text.
            if let Some((start, end)) = span(node) {
                match lr.children.last().and_then(span) {
                    Some((_, ce)) => {
                        protected.push((start, start + 1));
                        protected.push((ce, end));
                    }
                    None => {
                        protected.push((start, end));
                        return;
                    }
                }
            }
        }
        Node::Paragraph(_) => {
            if let Some((start, end)) = span(node)
                && end > start
            {
                paragraphs.push(Paragraph { start, end });
            }
        }
        _ => {}
    }
    if let Some(children) = node.children() {
        for child in children {
            collect(child, text, paragraphs, protected);
        }
    }
}

impl<'a> Doc<'a> {
    /// Analyse `text` without copying or normalising it. Block and inline
    /// structure comes from the `markdown` crate's mdast (GFM plus front
    /// matter); sentences and words are segmented here, on prose only.
    pub(crate) fn parse(text: &'a str) -> Doc<'a> {
        let mut paragraphs = Vec::new();
        let mut protected = Vec::new();
        // Plain Markdown (not MDX) never fails to parse; if it ever did, the
        // document would simply have no prose and so no marks.
        if let Ok(root) = markdown::to_mdast(text, &parse_options()) {
            collect(&root, text, &mut paragraphs, &mut protected);
        }
        paragraphs.sort_unstable_by_key(|p| (p.start, p.end));
        let protected = merge(protected);

        let mut sentences = Vec::new();
        for (para, p) in paragraphs.iter().enumerate() {
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
            paragraphs,
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
        assert!(prot.contains(&"| a | b |\n|---|---|"));
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
            vec!["[", "](https://example.org/teh-page)", "https://x.org/a.b"]
        );
    }

    /// Prose sentences of `text`, in order.
    fn sentences(text: &str) -> Vec<&str> {
        let doc = Doc::parse(text);
        doc.sentences
            .iter()
            .map(|s| &text[s.start..s.end])
            .collect()
    }

    /// True when every byte of `needle` (first occurrence) is protected.
    fn fully_protected(text: &str, needle: &str) -> bool {
        let doc = Doc::parse(text);
        let start = text.find(needle).expect("needle in text");
        (start..start + needle.len()).all(|b| in_ranges(&doc.protected, b))
    }

    #[test]
    fn multi_line_html_block_is_protected_on_every_line() {
        let text = "Before it.\n\n<div>\nInner teh line one.\nInner line two.\n</div>\n\nAfter it.";
        assert!(fully_protected(
            text,
            "<div>\nInner teh line one.\nInner line two.\n</div>"
        ));
        assert_eq!(sentences(text), vec!["Before it.", "After it."]);
    }

    #[test]
    fn html_comment_spanning_lines_is_protected() {
        let text =
            "Before it.\n\n<!--\nA commented teh sentence.\n\nStill a comment.\n-->\n\nAfter it.";
        assert!(fully_protected(
            text,
            "A commented teh sentence.\n\nStill a comment."
        ));
        assert_eq!(sentences(text), vec!["Before it.", "After it."]);
    }

    #[test]
    fn pipe_less_gfm_table_is_protected() {
        let text =
            "Before it.\n\nName | Value\n--- | ---\nteh basically | awesome stuff\n\nAfter it.";
        assert!(fully_protected(text, "teh basically | awesome stuff"));
        assert!(fully_protected(text, "Name | Value"));
        assert_eq!(sentences(text), vec!["Before it.", "After it."]);
    }

    #[test]
    fn table_with_alignment_row_is_protected() {
        let text = "| Left | Centre | Right |\n|:-----|:------:|------:|\n| teh | basically | stuff |\n\nAfter it.";
        assert!(fully_protected(text, "| teh | basically | stuff |"));
        assert!(fully_protected(text, "|:-----|:------:|------:|"));
        assert_eq!(sentences(text), vec!["After it."]);
    }

    #[test]
    fn setext_heading_is_protected_whole() {
        let text = "Title words that\nwrap\n---\n\nBody text.";
        assert!(fully_protected(text, "Title words that\nwrap\n---"));
        assert_eq!(sentences(text), vec!["Body text."]);
    }

    #[test]
    fn indented_code_is_protected() {
        let text = "Before it.\n\n    teh code line one.\n    Basically code two.\n\nAfter it.";
        assert!(fully_protected(
            text,
            "teh code line one.\n    Basically code two."
        ));
        assert_eq!(sentences(text), vec!["Before it.", "After it."]);
    }

    #[test]
    fn nested_list_with_fenced_code_is_protected() {
        let text = "- Outer item.\n  - Inner item.\n\n    ```\n    teh fenced. Code here\n    ```\n\n  - Last item.";
        assert!(fully_protected(text, "teh fenced. Code here"));
        assert_eq!(
            sentences(text),
            vec!["Outer item.", "Inner item.", "Last item."]
        );
    }

    #[test]
    fn block_quote_with_lazy_continuation_is_protected() {
        let text = "> Quoted teh line\nlazy continuation basically.\n\nAfter it.";
        assert!(fully_protected(
            text,
            "> Quoted teh line\nlazy continuation basically."
        ));
        assert_eq!(sentences(text), vec!["After it."]);
    }

    #[test]
    fn autolinks_and_reference_links() {
        let text = "Mail <https://example.org/teh> or see [the docs][ref] today.\n\n[ref]: https://example.org/teh";
        let doc = Doc::parse(text);
        let prot: Vec<&str> = doc.protected.iter().map(|&(s, e)| &text[s..e]).collect();
        assert_eq!(
            prot,
            vec![
                "<https://example.org/teh>",
                "[",
                "][ref]",
                "[ref]: https://example.org/teh"
            ]
        );
        assert_eq!(
            sentences(text),
            vec!["Mail <https://example.org/teh> or see [the docs][ref] today."]
        );
    }

    #[test]
    fn footnote_definitions_are_protected() {
        let single = "Before it.\n\n[^x]: teh footnote content.\n\nAfter it.";
        assert!(fully_protected(single, "[^x]: teh footnote content."));
        assert_eq!(sentences(single), vec!["Before it.", "After it."]);

        let multi = "Body with a note.[^n]\n\n[^n]: teh first line\n    and a teh second line.\n\nAfter it.";
        assert!(fully_protected(
            multi,
            "[^n]: teh first line\n    and a teh second line."
        ));
        assert_eq!(sentences(multi), vec!["Body with a note.[^n]", "After it."]);
    }

    #[test]
    fn front_matter_is_protected() {
        let text = "---\ntitle: teh basically awesome\n---\n\nBody text.";
        assert!(fully_protected(text, "title: teh basically awesome"));
        assert_eq!(sentences(text), vec!["Body text."]);
    }

    #[test]
    fn mdast_offsets_are_bytes_on_multibyte_text() {
        let text = "Caf\u{e9} \u{1d11e} `c\u{f6}de` here.\r\n\r\n# H\u{e9}ading";
        let doc = Doc::parse(text);
        let prot: Vec<&str> = doc.protected.iter().map(|&(s, e)| &text[s..e]).collect();
        assert_eq!(prot, vec!["`c\u{f6}de`", "# H\u{e9}ading"]);
        assert_eq!(
            sentences(text),
            vec!["Caf\u{e9} \u{1d11e} `c\u{f6}de` here."]
        );
    }
}
