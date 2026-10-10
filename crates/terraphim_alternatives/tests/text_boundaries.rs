use terraphim_alternatives::text::{
    ByteRange, TextUnit, paragraph_ranges, range_at, sentence_ranges,
};
use terraphim_alternatives::words::{count_words, word_spans};

#[test]
fn combining_sequences_belong_to_the_word_not_a_standalone_mark() {
    let text = "cafe\u{301}x e\u{301}-mail \u{301} 日本 𐐀x 😀";
    let words: Vec<_> = word_spans(text).map(|(s, e)| &text[s..e]).collect();
    assert_eq!(words, ["cafe\u{301}x", "e\u{301}-mail", "日本", "𐐀x"]);
    assert_eq!(count_words(text), 4);
    // Some spacing marks have Unicode Alphabetic=true; that is not a base.
    assert_eq!(count_words("\u{093e} \u{0903} \u{0301}"), 0);
}

#[test]
fn joiners_and_cjk_keep_existing_counter_semantics() {
    let text = "hadn’t liver-pill R-8.7 e.g. and/or 3.14 a--b 日本";
    assert_eq!(count_words(text), 9);
    assert_eq!(
        word_spans(text)
            .map(|(s, e)| &text[s..e])
            .collect::<Vec<_>>(),
        [
            "hadn’t",
            "liver-pill",
            "R-8.7",
            "e.g",
            "and/or",
            "3.14",
            "a",
            "b",
            "日本"
        ]
    );
}

#[test]
fn repeated_sentences_have_distinct_original_ranges() {
    assert_eq!(
        sentence_ranges("Same. Same."),
        [
            ByteRange { start: 0, end: 5 },
            ByteRange { start: 6, end: 11 }
        ]
    );
    for byte in 6..=11 {
        assert_eq!(
            range_at("Same. Same.", byte, TextUnit::Sentence),
            Some(ByteRange { start: 6, end: 11 })
        );
    }
}

#[test]
fn sentence_policy_retains_hardwraps_abbreviations_and_markdown_closers() {
    for (text, expected) in [
        (
            "Mr. Who sees Fig. 2. End.",
            vec!["Mr. Who sees Fig. 2.", "End."],
        ),
        (
            "Use e.g. This example. **No.** Then.",
            vec!["Use e.g. This example.", "**No.**", "Then."],
        ),
        (
            "First line of a\nwrapped sentence. Second\r\none here.",
            vec!["First line of a\nwrapped sentence.", "Second\r\none here."],
        ),
        (
            "why? next | fragment! end",
            vec!["why?", "next | fragment!", "end"],
        ),
        ("日本。次！本？", vec!["日本。", "次！", "本？"]),
        ("مرحبا؟ التالي.", vec!["مرحبا؟", "التالي."]),
        (
            "One‼ Next.\nWrapped⁉\nAfter.",
            vec!["One‼", "Next.", "Wrapped⁉", "After."],
        ),
        ("lower. next. Upper.", vec!["lower. next.", "Upper."]),
        (
            "Wait... Then 3.14 ends.",
            vec!["Wait...", "Then 3.14 ends."],
        ),
    ] {
        assert_eq!(
            sentence_ranges(text)
                .iter()
                .map(|r| &text[r.start..r.end])
                .collect::<Vec<_>>(),
            expected,
            "{text}"
        );
    }
}

#[test]
fn paragraphs_handle_crlf_and_whitespace_blank_lines() {
    let text = "First\r\nwrapped.\r\n \t\r\nLast.";
    assert_eq!(
        paragraph_ranges(text)
            .iter()
            .map(|r| &text[r.start..r.end])
            .collect::<Vec<_>>(),
        ["First\r\nwrapped.", "Last."]
    );
}

#[test]
fn word_caret_at_end_targets_preceding_word_and_invalid_bytes_refuse() {
    let text = "cafe\u{301} 😀";
    assert_eq!(
        range_at(text, 6, TextUnit::Word),
        Some(ByteRange { start: 0, end: 6 })
    );
    assert_eq!(range_at(text, 5, TextUnit::Word), None);
    assert_eq!(range_at(text, text.len() + 1, TextUnit::Sentence), None);
    assert_eq!(range_at("", 0, TextUnit::Word), None);
}

#[test]
fn markdown_openers_split_lowercase_code_emphasis_and_links() {
    for (text, expected) in [
        (
            "Built there. `terraphim-editor` owns its DOM.",
            vec!["Built there.", "`terraphim-editor` owns its DOM."],
        ),
        (
            "User edits. `terraphim_lsp` must preserve annotations.",
            vec!["User edits.", "`terraphim_lsp` must preserve annotations."],
        ),
        (
            "First. *next* sentence. [lower](url) sentence.",
            vec!["First.", "*next* sentence.", "[lower](url) sentence."],
        ),
    ] {
        assert_eq!(
            sentence_ranges(text)
                .iter()
                .map(|r| &text[r.start..r.end])
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn shared_markdown_policy_preserves_exact_zed_fixture_positions() {
    let text = include_str!("../../terraphim_lab/tests/fixtures/zed-plugin-fit.md");
    let ranges = sentence_ranges(text);
    for expected in [
        ByteRange {
            start: 108,
            end: 283,
        },
        ByteRange {
            start: 284,
            end: 391,
        },
        ByteRange {
            start: 2230,
            end: 2329,
        },
        ByteRange {
            start: 2330,
            end: 2563,
        },
    ] {
        assert!(ranges.contains(&expected), "missing {expected:?}");
    }
}

#[test]
fn protected_inline_code_does_not_gain_opener_boundaries_inside_it() {
    use terraphim_alternatives::text::sentence_ranges_with_protected;
    let text = "Use `code. *next*` here. `lower` next.";
    let protected = [ByteRange { start: 4, end: 18 }];
    let ranges = sentence_ranges_with_protected(text, &protected);
    assert_eq!(
        ranges
            .iter()
            .map(|r| &text[r.start..r.end])
            .collect::<Vec<_>>(),
        ["Use `code. *next*` here.", "`lower` next."]
    );
}
