//! One block of tests per R-8.2 action, on fixture text with expected spans.

mod common;

use common::{covered, range16, role_config, summary};
use terraphim_lab::{LabAction, LabConfig, MarkKind, StyleCategory, StyleLists, TypoList, mark};

fn defaults() -> LabConfig {
    LabConfig::with_defaults().unwrap()
}

fn s(x: &str) -> String {
    x.to_string()
}

// ------------------------------------------------- Fix punctuation and typos

#[test]
fn typos_are_marked_with_case_matched_proposals() {
    let body = "I recieve teh letter. Teh end of Tehran, and TEH END. Wierd.";
    let marks = mark(body, &defaults(), LabAction::TyposAndPunctuation);
    assert_eq!(
        summary(body, &marks),
        vec![
            (MarkKind::Typo, s("recieve"), Some(s("receive"))),
            (MarkKind::Typo, s("teh"), Some(s("the"))),
            (MarkKind::Typo, s("Teh"), Some(s("The"))),
            (MarkKind::Typo, s("TEH"), Some(s("THE"))),
            (MarkKind::Typo, s("Wierd"), Some(s("Weird"))),
        ]
    );
    // Spans are exact: "teh" never matches inside "Tehran".
    assert_eq!((marks[1].start, marks[1].end), range16(body, "teh", 0));
    assert_eq!((marks[2].start, marks[2].end), range16(body, "Teh", 0));
    assert!(marks.iter().all(|m| m.reason.starts_with("typo:")));
}

#[test]
fn punctuation_rules_propose_fixes() {
    let body = "Hello , world. It is fine,really. Wait;;now. The the cat sat. Two  spaces here. We had had 1,000 items.  Fine.";
    let marks = mark(body, &defaults(), LabAction::TyposAndPunctuation);
    assert_eq!(
        summary(body, &marks),
        vec![
            (MarkKind::Punctuation, s(" ,"), Some(s(","))),
            (MarkKind::Punctuation, s(","), Some(s(", "))),
            (MarkKind::Punctuation, s(";;"), Some(s(";"))),
            (MarkKind::Punctuation, s("The the"), Some(s("The"))),
            (MarkKind::Punctuation, s("  "), Some(s(" "))),
        ]
    );
    assert_eq!((marks[0].start, marks[0].end), range16(body, " ,", 0));
    let comma = range16(body, ",really", 0).0;
    assert_eq!((marks[1].start, marks[1].end), (comma, comma + 1));
    // Conservative: "1,000", "had had" and two spaces after a full stop are
    // left alone.
    assert_eq!(marks.len(), 5);
    assert_eq!(marks[4].start, range16(body, "Two  spaces", 0).0 + 3);
}

#[test]
fn typo_list_from_kg_markdown_and_custom_thesaurus() {
    // A KG concept file is a typo entry: the concept is the correction.
    let typos =
        TypoList::from_kg_markdown([("Wednesday", "synonyms:: wensday, wednsday")]).unwrap();
    let config = LabConfig {
        typos,
        ..defaults()
    };
    let body = "See you on wensday.";
    let marks = mark(body, &config, LabAction::TyposAndPunctuation);
    assert_eq!(
        summary(body, &marks),
        vec![(MarkKind::Typo, s("wensday"), Some(s("Wednesday")))]
    );
}

// ------------------------------------------------- Mark sentences that run long

#[test]
fn long_sentences_at_the_limit_are_marked() {
    let thirty = String::from("Word") + &" word".repeat(28) + " end.";
    let twenty_nine = String::from("Word") + &" word".repeat(27) + " end.";
    let body = format!("{thirty} {twenty_nine}");
    let marks = mark(&body, &defaults(), LabAction::LongSentences);
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].kind, MarkKind::LongSentence);
    assert_eq!((marks[0].start, marks[0].end), (0, thirty.len()));
    assert_eq!(marks[0].reason, "30 words (limit 30)");
    assert!((marks[0].score - 1.0).abs() < 1e-12);
}

#[test]
fn long_sentence_limit_is_configurable() {
    let mut config = defaults();
    config.options.long_sentence_words = 5;
    let body = "One two three four five. One two three.";
    let marks = mark(body, &config, LabAction::LongSentences);
    assert_eq!(
        summary(body, &marks),
        vec![(MarkKind::LongSentence, s("One two three four five."), None)]
    );
}

// ------------------------------------------------- Mark convoluted sentences

#[test]
fn convoluted_sentences_are_marked_but_enumerations_are_not() {
    let convoluted = "The plan, which we drafted in May (after the review, which ran late), failed; although nobody said so, it was clear.";
    let list = "We bought apples, pears, plums, figs, dates and limes.";
    let body = format!("{list} {convoluted}");
    let marks = mark(&body, &defaults(), LabAction::ConvolutedSentences);
    assert_eq!(
        summary(&body, &marks),
        vec![(MarkKind::ConvolutedSentence, s(convoluted), None)]
    );
    assert!(
        marks[0].reason.contains("subordinate"),
        "{}",
        marks[0].reason
    );
    assert!(marks[0].score >= 1.0);
}

// ------------------------------------------------- Mark words that don't fit the tone

#[test]
fn off_tone_words_come_from_the_tone_list() {
    let body = "The results are awesome and we have lots of stuff to ship.";
    let marks = mark(body, &defaults(), LabAction::OffTone);
    assert_eq!(
        summary(body, &marks),
        vec![
            (MarkKind::OffTone, s("awesome"), None),
            (MarkKind::OffTone, s("lots of"), None),
            (MarkKind::OffTone, s("stuff"), None),
        ]
    );
    assert_eq!(marks[0].reason, "off-tone \"awesome\"");
}

#[test]
fn tone_list_comes_from_the_role_kg() {
    // An academic role brings its own register list.
    let style = StyleLists::from_kg_markdown([("lab-tone", "synonyms:: per se, utilise")]).unwrap();
    let config = LabConfig {
        style,
        ..defaults()
    };
    let body = "We utilise stuff per se.";
    let marks = mark(body, &config, LabAction::OffTone);
    assert_eq!(
        summary(body, &marks),
        vec![
            (MarkKind::OffTone, s("utilise"), None),
            (MarkKind::OffTone, s("per se"), None)
        ]
    );
}

// ------------------------------------------------- Mark hedges and filler

#[test]
fn hedges_and_filler_are_marked() {
    let body = "This is basically fine and perhaps this seems to work, sort of.";
    let marks = mark(body, &defaults(), LabAction::HedgesAndFiller);
    assert_eq!(
        summary(body, &marks),
        vec![
            (MarkKind::Filler, s("basically"), None),
            (MarkKind::Hedge, s("perhaps"), None),
            (MarkKind::Hedge, s("seems to"), None),
            (MarkKind::Filler, s("sort of"), None),
        ]
    );
    assert_eq!((marks[3].start, marks[3].end), range16(body, "sort of", 0));
}

#[test]
fn multi_word_terms_match_across_hard_wraps_on_original_offsets() {
    // Three Men in a Boat wraps "all\nof a sudden" across lines.
    let body = common::THREE_MEN;
    let marks = mark(body, &defaults(), LabAction::HedgesAndFiller);
    let m = marks
        .iter()
        .find(|m| covered(body, m).ends_with("of a sudden"))
        .expect("all of a sudden is marked");
    assert_eq!(covered(body, m), "all\nof a sudden");
    assert_eq!((m.start, m.end), range16(body, "all\nof a sudden", 0));
}

#[test]
fn custom_style_terms() {
    let style = StyleLists::from_terms([(StyleCategory::Filler, "needless")]).unwrap();
    let config = LabConfig {
        style,
        ..defaults()
    };
    let marks = mark("A needless word.", &config, LabAction::HedgesAndFiller);
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].kind, MarkKind::Filler);
}

// ------------------------------------------------- Mark the weakest sentences

#[test]
fn weakest_sentences_use_the_role_rolegraph() {
    // Eight sentences, two paragraphs. Ceil(15% of 8) = 2 marks.
    let body = "\
The language server shows diagnostics for each span. \
Perhaps it is basically fine, really. \
The span model keeps alternatives for the language server. \
We met on a Tuesday and talked for a while.

The thesaurus feeds the span model and the knowledge graph. \
Zed loads a WebAssembly module through the zed extension. \
It was quite a long day, I suppose. \
Diagnostics and code actions use the annotation format.";
    let marks = mark(body, &role_config(), LabAction::WeakestSentences);
    assert_eq!(
        summary(body, &marks),
        vec![
            (
                MarkKind::WeakSentence,
                s("Perhaps it is basically fine, really."),
                None
            ),
            (
                MarkKind::WeakSentence,
                s("It was quite a long day, I suppose."),
                None
            ),
        ]
    );
    assert!(marks[0].reason.contains("touches no role concepts"));
    assert!(marks[0].reason.contains("hedges/filler"));
}

#[test]
fn concept_rank_orders_otherwise_equal_sentences() {
    // Both candidates touch exactly one concept and no other: "span model"
    // (rank 31 in the fixture rolegraph) and "wasm" (rank 4). With one mark,
    // the low-ranked concept's sentence is the weaker.
    let mut config = role_config();
    config.options.weakest_share = 0.01;
    let body = "\
The language server shows diagnostics for each span and the zed extension.

Our plan relies on the span model today. \
Our plan relies on the wasm toolchain today.";
    let marks = mark(body, &config, LabAction::WeakestSentences);
    assert_eq!(
        summary(body, &marks),
        vec![(
            MarkKind::WeakSentence,
            s("Our plan relies on the wasm toolchain today."),
            None
        )]
    );
    assert!(marks[0].reason.contains("wasm"), "{}", marks[0].reason);
}

#[test]
fn paragraph_openers_are_never_marked_weakest() {
    // Every opener is the weakest sentence of its paragraph (hedged, no role
    // concepts); none may be marked.
    let body = "\
Perhaps it is basically fine, really, I suppose.
The language server shows diagnostics for each span model change.

Maybe it is sort of quite okay, I think.
The thesaurus feeds the span model and the knowledge graph.

Honestly it is just very fine, basically.
Zed loads a WebAssembly module through the zed extension today.";
    let marks = mark(body, &role_config(), LabAction::WeakestSentences);
    assert_eq!(marks.len(), 1, "ceil(15% of 6) = 1");
    for m in &marks {
        let text = covered(body, m);
        assert!(
            !["Perhaps", "Maybe", "Honestly"]
                .iter()
                .any(|o| text.starts_with(o)),
            "opener marked: {text}"
        );
    }
}

#[test]
fn only_openers_means_no_weakest_marks() {
    let body = "Perhaps it is basically fine, really.\n\nMaybe it is sort of quite okay, I think.";
    assert!(mark(body, &role_config(), LabAction::WeakestSentences).is_empty());
}

#[test]
fn weakest_is_fifteen_percent_of_sentences_on_fixtures() {
    let config = role_config();
    for (name, body) in common::FIXTURES {
        let marks = mark(body, &config, LabAction::WeakestSentences);
        // Distinct sentences: the segments of one sentence (split around
        // inline code) share score and reason, and no sentence boundary lies
        // between them.
        let units: Vec<u16> = body.encode_utf16().collect();
        let mut sentences = 0;
        let mut last: Option<&terraphim_lab::LabMark> = None;
        for m in &marks {
            let same = last.is_some_and(|l| {
                let gap = String::from_utf16(&units[l.end..m.start]).unwrap();
                l.score == m.score
                    && l.reason == m.reason
                    && !gap.contains(". ")
                    && !gap.contains(".\n")
            });
            if !same {
                sentences += 1;
            }
            last = Some(m);
        }
        let expected = match name {
            "three-men-ch1" => 8,  // ceil(0.15 * sentences)
            "walden-economy" => 3, // ceil(0.15 * 20 sentences)
            "zed-plugin-fit" => 4, // ceil(0.15 * 24 sentences)
            _ => unreachable!(),
        };
        assert_eq!(sentences, expected, "{name}");
        // Never a paragraph opener: some sentence text precedes it in the
        // same paragraph.
        for m in &marks {
            let before16: Vec<u16> = body.encode_utf16().take(m.start).collect();
            let before = String::from_utf16(&before16).unwrap();
            let para_start = before.rfind("\n\n").map(|p| p + 2).unwrap_or(0);
            assert!(
                before[para_start..].trim().chars().count() > 0,
                "{name}: opener marked: {}",
                covered(body, m)
            );
        }
    }
}

#[test]
fn without_a_role_weakness_falls_back_to_hedges_and_text() {
    let body = "Opening line of the note here. Perhaps it is basically fine, really. The engine ranks each sentence by its concepts.";
    let marks = mark(body, &defaults(), LabAction::WeakestSentences);
    assert_eq!(
        summary(body, &marks),
        vec![(
            MarkKind::WeakSentence,
            s("Perhaps it is basically fine, really."),
            None
        )]
    );
    assert!(marks[0].reason.starts_with("no role selected"));
}
