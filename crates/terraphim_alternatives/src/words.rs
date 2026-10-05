//! The editor's one word definition.
//!
//! Every word count in the editor goes through this module: the Write_On
//! counter ([`Document::counts`](crate::Document::counts), surfaced to
//! JavaScript as `document_counts`), the Lab marks and the trim status card
//! (`terraphim_lab`'s `TrimPlan::status`). R-2.1 and R-8.4 of
//! `docs/requirements/alternative-control.md` show the same `535` in the
//! counter and in the card's `535 -> 480 words`, so both must count the same
//! body the same way (issue #59).
//!
//! A word is a maximal run of alphanumerics or `_`, where an apostrophe
//! (`'` or `\u{2019}`), hyphen, full stop or slash joins two runs only when
//! it sits between word characters:
//!
//! * `hadn't`, `liver-pill`, `R-8.7`, `e.g` and `terraphim_lsp` are one word
//!   each;
//! * an em dash splits (`us\u{2014}George` is two words);
//! * Markdown syntax (`#`, `-`, `**`, backticks, code fences, `|`) and
//!   tokens of punctuation, symbols or emoji alone are never words;
//! * words inside headings, code blocks and ghosted text count like any other
//!   words: the counter counts the body as written.
//!
//! Offsets are byte offsets into the text passed in; callers working in
//! UTF-16 convert with [`byte_to_utf16`](crate::byte_to_utf16). Nothing here
//! is pattern matching, so `terraphim_automata` (Aho-Corasick over a
//! thesaurus) does not apply.
//!
//! # Performance
//!
//! One forward pass over the characters, with one character of look-ahead
//! after a joiner: `O(n)` in the length of the text, no allocation for
//! [`count_words`].

/// True for a character that can make up a word: alphanumeric or `_`.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True for a character that joins two runs of word characters into one word
/// when it sits between them: `'`, `\u{2019}`, `-`, `.` or `/`.
pub fn is_joiner(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-' | '.' | '/')
}

/// Iterator over the byte spans of the words in a text (see [`word_spans`]).
#[derive(Debug, Clone)]
pub struct WordSpans<'a> {
    text: &'a str,
    iter: std::iter::Peekable<std::str::CharIndices<'a>>,
}

impl Iterator for WordSpans<'_> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<(usize, usize)> {
        let (start, first) = self.iter.by_ref().find(|&(_, c)| is_word_char(c))?;
        let mut end = start + first.len_utf8();
        while let Some(&(pos, next)) = self.iter.peek() {
            if is_word_char(next) {
                end = pos + next.len_utf8();
                self.iter.next();
            } else if is_joiner(next) {
                // A joiner only counts when a word character follows it.
                let after = self.text[pos + next.len_utf8()..].chars().next();
                if after.is_some_and(is_word_char) {
                    self.iter.next();
                    end = pos + next.len_utf8();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        Some((start, end))
    }
}

/// The byte span `(start, end)` of every word in `text`, in order.
///
/// ```
/// use terraphim_alternatives::words::word_spans;
///
/// let text = "# Hadn't we - R-8.7?";
/// let words: Vec<&str> = word_spans(text).map(|(s, e)| &text[s..e]).collect();
/// assert_eq!(words, ["Hadn't", "we", "R-8.7"]);
/// ```
pub fn word_spans(text: &str) -> WordSpans<'_> {
    WordSpans {
        text,
        iter: text.char_indices().peekable(),
    }
}

/// The number of words in `text`.
///
/// ```
/// use terraphim_alternatives::words::count_words;
///
/// assert_eq!(count_words("- A list item , with `code`."), 5);
/// assert_eq!(count_words("```\n```"), 0);
/// ```
pub fn count_words(text: &str) -> usize {
    word_spans(text).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        word_spans(text).map(|(s, e)| &text[s..e]).collect()
    }

    #[test]
    fn joiners_join_only_between_word_characters() {
        assert_eq!(
            words("us\u{2014}George hadn\u{2019}t a liver-pill, R-8.7 `terraphim_lsp` **ok**."),
            [
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
        assert_eq!(
            words("word- -word 'quoted' end."),
            ["word", "word", "quoted", "end"]
        );
        assert_eq!(words("e.g. and/or 3.14"), ["e.g", "and/or", "3.14"]);
        assert_eq!(words("a--b a..b"), ["a", "b", "a", "b"]);
    }

    #[test]
    fn markdown_syntax_and_punctuation_are_not_words() {
        let lines = "# Heading\n\n- item , one\n\n```\ncode , here\n```\n\n| a | b |\n> quote\n";
        assert_eq!(
            words(lines),
            ["Heading", "item", "one", "code", "here", "a", "b", "quote"]
        );
        assert_eq!(count_words("** -- , ; ``` | # > ..."), 0);
    }

    #[test]
    fn symbols_and_emoji_are_not_words() {
        assert_eq!(count_words("a \u{1F600}"), 1);
        assert_eq!(count_words("Caf\u{e9} \u{1D11E} counts"), 2);
    }

    #[test]
    fn unicode_letters_and_digits_are_words() {
        assert_eq!(
            words("caf\u{e9} \u{65e5}\u{672c} \u{0661}\u{0662}"),
            ["caf\u{e9}", "\u{65e5}\u{672c}", "\u{0661}\u{0662}"]
        );
    }

    #[test]
    fn whitespace_and_line_endings_separate_words() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words(" \t\r\n "), 0);
        assert_eq!(
            words("one\r\ntwo\tthree\u{a0}four"),
            ["one", "two", "three", "four"]
        );
    }

    #[test]
    fn spans_are_byte_offsets_on_char_boundaries() {
        let text = "\u{1D11E}caf\u{e9}-x \u{2019}y";
        for (s, e) in word_spans(text) {
            assert!(text.is_char_boundary(s) && text.is_char_boundary(e));
        }
        assert_eq!(word_spans(text).collect::<Vec<_>>(), [(4, 11), (15, 16)]);
    }
}
