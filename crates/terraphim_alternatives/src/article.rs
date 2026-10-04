//! Indefinite-article fix-up for swaps (R-2.6).
//!
//! When the active alternative changes, an `a`/`an` that *immediately*
//! precedes the span (only whitespace between them, at most one line break) is
//! switched to match the new text. The article is derived purely from the new
//! text, so cycling back restores the original article: the rule is
//! reversible by construction. Nothing else in the body is touched.
//!
//! The vowel-sound test is a heuristic: a leading vowel letter means "an",
//! with small exception lists for words that start with a "you"/"w" sound
//! ("a unicorn", "a one-off") or a silent "h" ("an hour"). Text whose first
//! meaningful character is not a letter (digits, symbols) leaves the article
//! alone.

/// The article the given text should take, or `None` when the first
/// meaningful character is not a letter.
///
/// Leading Markdown emphasis and opening quotes/brackets are skipped.
///
/// ```
/// use terraphim_alternatives::{article_for, Article};
/// assert_eq!(article_for("eraser"), Some(Article::An));
/// assert_eq!(article_for("*unicorn*"), Some(Article::A));
/// assert_eq!(article_for("hour"), Some(Article::An));
/// assert_eq!(article_for("8-bit"), None);
/// ```
pub fn article_for(text: &str) -> Option<Article> {
    let trimmed = text.trim_start_matches(|c: char| {
        matches!(
            c,
            '*' | '_' | '"' | '\'' | '(' | '[' | '\u{201C}' | '\u{2018}'
        )
    });
    let first = trimmed.chars().next()?;
    if !first.is_alphabetic() {
        return None;
    }
    let word: String = trimmed
        .chars()
        .take_while(|c| c.is_alphabetic())
        .flat_map(char::to_lowercase)
        .collect();
    Some(if wants_an(&word) {
        Article::An
    } else {
        Article::A
    })
}

/// An English indefinite article.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Article {
    /// "a"
    A,
    /// "an"
    An,
}

/// Words starting with a vowel letter that still take "a".
const A_PREFIXES: &[&str] = &[
    "eu", "ewe", "ubiq", "ufo", "unanim", "uni", "uran", "uri", "use", "usu", "uten", "uti", "utop",
];
/// Exact words starting with a vowel letter that take "a".
const A_WORDS: &[&str] = &["one", "once"];
/// Prefixes that override `A_PREFIXES` back to "an" ("an unidentified").
const AN_OVERRIDES: &[&str] = &["unid", "unim", "unin"];
/// Silent-h prefixes that take "an".
const SILENT_H: &[&str] = &["heir", "honest", "honor", "honour", "hour"];

fn wants_an(word: &str) -> bool {
    let starts = |list: &[&str]| list.iter().any(|p| word.starts_with(p));
    if starts(AN_OVERRIDES) || starts(SILENT_H) {
        return true;
    }
    if starts(A_PREFIXES) || A_WORDS.contains(&word) {
        return false;
    }
    matches!(word.chars().next(), Some('a' | 'e' | 'i' | 'o' | 'u'))
}

/// Byte range of an article immediately before `span_start` in `body`.
pub(crate) fn preceding_article(body: &str, span_start: usize) -> Option<(usize, usize)> {
    let before = &body[..span_start];
    let article_end = before.trim_end_matches(char::is_whitespace).len();
    let gap = &before[article_end..];
    if gap.is_empty() || gap.matches('\n').count() > 1 {
        return None;
    }
    let head = &before[..article_end];
    let article_start = head
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_alphanumeric())
        .map_or(0, |(i, c)| i + c.len_utf8());
    matches!(&head[article_start..], "a" | "an" | "A" | "An" | "AN")
        .then_some((article_start, article_end))
}

/// Spells `wanted` in the case style of `existing` ("A" -> "An", "AN" -> "A").
pub(crate) fn respell(existing: &str, wanted: Article) -> &'static str {
    let upper = existing.starts_with(|c: char| c.is_uppercase());
    let shouting = existing.len() > 1 && existing.chars().all(char::is_uppercase);
    match (wanted, upper, shouting) {
        (Article::A, true, _) => "A",
        (Article::A, false, _) => "a",
        (Article::An, true, true) => "AN",
        (Article::An, true, false) => "An",
        (Article::An, false, _) => "an",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vowel_letters_take_an() {
        for w in ["eraser", "apple", "idea", "orange", "umbrella", "Eraser"] {
            assert_eq!(article_for(w), Some(Article::An), "{w}");
        }
        for w in ["paperclip", "thumbtack", "tension", "house"] {
            assert_eq!(article_for(w), Some(Article::A), "{w}");
        }
    }

    #[test]
    fn exceptions_follow_sound_not_spelling() {
        for w in [
            "unicorn",
            "university",
            "user",
            "European",
            "one",
            "utility",
        ] {
            assert_eq!(article_for(w), Some(Article::A), "{w}");
        }
        for w in [
            "hour",
            "honest",
            "heir",
            "unidentified",
            "unimportant",
            "onerous",
        ] {
            assert_eq!(article_for(w), Some(Article::An), "{w}");
        }
    }

    #[test]
    fn non_letters_leave_the_article_alone() {
        assert_eq!(article_for("8-bit"), None);
        assert_eq!(article_for(""), None);
        assert_eq!(article_for("**"), None);
    }

    #[test]
    fn finds_only_an_immediately_preceding_article() {
        let body = "It is a paperclip.";
        assert_eq!(preceding_article(body, 8), Some((6, 7)));
        let body = "It is a big paperclip.";
        assert_eq!(preceding_article(body, 12), None);
        let body = "Banana paperclip";
        assert_eq!(preceding_article(body, 7), None, "word ending in a");
        let body = "a\npaperclip";
        assert_eq!(preceding_article(body, 2), Some((0, 1)));
        let body = "a\n\nParagraph";
        assert_eq!(preceding_article(body, 3), None, "blank line between");
        let body = "apaperclip";
        assert_eq!(preceding_article(body, 1), None, "no whitespace");
    }

    #[test]
    fn respell_preserves_case() {
        assert_eq!(respell("a", Article::An), "an");
        assert_eq!(respell("A", Article::An), "An");
        assert_eq!(respell("An", Article::A), "A");
        assert_eq!(respell("AN", Article::An), "AN");
        assert_eq!(respell("AN", Article::A), "A");
    }
}
