//! Tests for the Lab heuristics prototype. Real fixtures and real KG files;
//! no mocks.

use lab_heuristics::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn lists() -> Lists {
    Lists::load(&root().join("kg"))
}

fn fixtures() -> Vec<(String, Doc)> {
    let mut paths: Vec<PathBuf> = fs::read_dir(root().join("fixtures"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 3, "expected three fixtures");
    paths
        .into_iter()
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().to_string(),
                Doc::parse(&fs::read_to_string(&p).unwrap()),
            )
        })
        .collect()
}

// ------------------------------------------------------------ percentage maths

#[test]
fn target_words_rounds_to_nearest() {
    assert_eq!(target_words(401, 0.10), 40);
    assert_eq!(target_words(853, 0.50), 427); // 426.5 rounds up
    assert_eq!(target_words(641, 0.30), 192);
    assert_eq!(target_words(0, 0.30), 0);
}

#[test]
fn cut_percentage_is_words_cut_over_total() {
    assert!((cut_percentage(401, 40) - 9.97506).abs() < 1e-4);
    assert!((cut_percentage(200, 100) - 50.0).abs() < 1e-9);
    assert_eq!(cut_percentage(0, 0), 0.0);
}

#[test]
fn reported_cut_matches_text_after_make_the_cuts() {
    // The status-card number must equal what "Make the cuts" actually removes.
    let lists = lists();
    for (name, doc) in fixtures() {
        let res = trim(&doc, &lists);
        for lv in &res.levels {
            let ranges = merged_ranges(&res.candidates, &lv.selected);
            let after = apply_cuts(&doc.text, &ranges);
            assert_eq!(
                word_count(&after),
                res.total_words - lv.cut_words,
                "{name} {}: word count after cuts disagrees with reported cut",
                lv.label
            );
            assert_eq!(
                union_cut_words(&doc, &res.candidates, &lv.selected),
                lv.cut_words
            );
        }
    }
}

#[test]
fn small_document_trim_counts_union_not_sum() {
    let lists = lists();
    let doc = Doc::parse(
        "The plan is really very simple. We cut words. Perhaps we cut a whole sentence that is quite weak and just repeats the plan. The plan is simple.\n",
    );
    let res = trim(&doc, &lists);
    assert_eq!(res.total_words, doc.total_words());
    for lv in &res.levels {
        // union of selected spans, never double counting fillers inside a cut sentence
        let mut words = BTreeSet::new();
        for &i in &lv.selected {
            words.extend(doc.words_in(res.candidates[i].start, res.candidates[i].end));
        }
        assert_eq!(words.len(), lv.cut_words);
        assert_eq!(
            lv.achieved_pct,
            cut_percentage(res.total_words, lv.cut_words)
        );
    }
}

// ------------------------------------------------------------ acceptance

#[test]
fn every_fixture_level_within_three_points() {
    let lists = lists();
    for (name, doc) in fixtures() {
        let res = trim(&doc, &lists);
        for lv in &res.levels {
            let delta = lv.achieved_pct - lv.target_pct;
            assert!(
                delta.abs() <= TOLERANCE_PP,
                "{name} {}: target {:.0}% achieved {:.2}% (delta {:+.2}pp)",
                lv.label,
                lv.target_pct,
                lv.achieved_pct,
                delta
            );
        }
    }
}

#[test]
fn levels_are_nested() {
    let lists = lists();
    for (name, doc) in fixtures() {
        let res = trim(&doc, &lists);
        for w in res.levels.windows(2) {
            let lo: BTreeSet<_> = w[0].selected.iter().collect();
            let hi: BTreeSet<_> = w[1].selected.iter().collect();
            assert!(
                lo.is_subset(&hi),
                "{name}: {} not a subset of {}",
                w[0].label,
                w[1].label
            );
            assert!(w[0].cut_words <= w[1].cut_words);
        }
    }
}

#[test]
fn trim_is_deterministic() {
    let lists = lists();
    for (_, doc) in fixtures() {
        let a = trim(&doc, &lists);
        let b = trim(&doc, &lists);
        for (x, y) in a.levels.iter().zip(b.levels.iter()) {
            assert_eq!(x.selected, y.selected);
        }
    }
}

#[test]
fn paragraph_openers_survive_below_half() {
    let lists = lists();
    for (name, doc) in fixtures() {
        let res = trim(&doc, &lists);
        for (i, c) in res.candidates.iter().enumerate() {
            if c.tier == 3 {
                if let Some(l) = res.level_of[i] {
                    assert_eq!(
                        l,
                        LEVELS.len() - 1,
                        "{name}: opener cut below 50%: {}",
                        &doc.text[c.start..c.end]
                    );
                }
            }
        }
    }
}

#[test]
fn cut_list_items_leave_no_empty_bullets() {
    let lists = lists();
    let raw = "Intro sentence that stays.\n\n- First item is really quite padded and weak.\n- Second item stays because it names the span model.\n- Third item is basically just filler too.\n";
    let doc = Doc::parse(raw);
    let res = trim(&doc, &lists);
    let lv = res.levels.last().unwrap();
    let after = apply_cuts(&doc.text, &merged_ranges(&res.candidates, &lv.selected));
    for line in after.lines() {
        let t = line.trim();
        assert!(t != "-" && t != "*", "empty bullet left behind:\n{after}");
    }
    assert_eq!(word_count(&after), res.total_words - lv.cut_words);
}

#[test]
fn headings_code_and_tables_are_never_cut() {
    let lists = lists();
    let raw = "# Really very important heading\n\nThis is really a very short note, which is quite padded. It just says nothing at all. Perhaps it is fine.\n\n```\nlet really = very(just);\n```\n\n| really | very |\n|---|---|\n| just | quite |\n\nAnother paragraph that is basically filler. It is quite literally padding.\n";
    let doc = Doc::parse(raw);
    let res = trim(&doc, &lists);
    for lv in &res.levels {
        for &i in &lv.selected {
            let c = &res.candidates[i];
            assert!(
                !doc.protected[c.start..c.end].iter().any(|&p| p),
                "cut touches protected text: {:?}",
                &doc.text[c.start..c.end]
            );
        }
        let after = apply_cuts(&doc.text, &merged_ranges(&res.candidates, &lv.selected));
        assert!(after.contains("# Really very important heading"));
        assert!(after.contains("let really = very(just);"));
        assert!(after.contains("| really | very |\n|---|---|\n| just | quite |"));
    }
}

// ------------------------------------------------------------ words and sentences

#[test]
fn word_definition_on_fixture_lines() {
    // em-dash splits words; curly apostrophes and hyphens join
    assert_eq!(
        word_count("There were four of us\u{2014}George, and William Samuel Harris, and myself,"),
        12
    );
    assert_eq!(word_count("us\u{2014}George"), 2);
    assert_eq!(
        word_count("Why hadn\u{2019}t I got housemaid\u{2019}s knee?"),
        6
    );
    assert_eq!(word_count("a patent liver-pill circular"), 4);
    // Markdown syntax is not a word
    assert_eq!(
        word_count("**No.** The Zed plan (`terraphim/zed-terraphim#1`)"),
        6
    );
    assert_eq!(word_count("`terraphim_lsp` (R-8.7)"), 2);
}

#[test]
fn sentence_splitter_handles_abbreviations_and_markup() {
    let doc = Doc::parse("I turned up St. Vitus\u{2019}s Dance and stopped. Then I left (e.g. home). **No.** The end.\n");
    let s: Vec<&str> = (0..doc.sentences.len())
        .map(|i| doc.sentence_text(i))
        .collect();
    assert_eq!(
        s,
        vec![
            "I turned up St. Vitus\u{2019}s Dance and stopped.",
            "Then I left (e.g. home).",
            "**No.**",
            "The end."
        ]
    );
}

#[test]
fn gutenberg_hard_wraps_are_unwrapped() {
    let doc = Doc::parse("One line\nwraps here. Second\nsentence.\n\nNew paragraph.\n");
    assert_eq!(
        doc.text,
        "One line wraps here. Second sentence.\n\nNew paragraph."
    );
    assert_eq!(doc.sentences.len(), 3);
    assert!(doc.sentences[2].first_in_para);
}

// ------------------------------------------------------------ KG matching

#[test]
fn matcher_respects_word_boundaries() {
    let terms = vec![
        Term {
            term: "very".into(),
            concept: "lab-filler".into(),
        },
        Term {
            term: "just".into(),
            concept: "lab-filler".into(),
        },
        Term {
            term: "so".into(),
            concept: "x".into(),
        },
    ];
    let m = Matcher::new(&terms);
    assert!(m.find("every justice is also good").is_empty());
    let hits = m.find("Very just, so very.");
    let terms: Vec<&str> = hits.iter().map(|h| h.term.as_str()).collect();
    assert_eq!(terms, vec!["very", "just", "so", "very"]);
}

#[test]
fn matcher_handles_curly_apostrophes() {
    let terms = vec![Term {
        term: "i'm not sure".into(),
        concept: "lab-hedge-phrase".into(),
    }];
    let m = Matcher::new(&terms);
    assert_eq!(m.find("Well, I\u{2019}m not sure.").len(), 1);
    assert_eq!(m.find("Well, I'm not sure.").len(), 1);
}

#[test]
fn kg_markdown_parses_synonyms_line() {
    let t = parse_kg_markdown(
        "lab-hedge",
        "# lab-hedge\n\nprose\n\nsynonyms:: Perhaps, maybe , i think\n",
        false,
    );
    let terms: Vec<&str> = t.iter().map(|x| x.term.as_str()).collect();
    assert_eq!(terms, vec!["perhaps", "maybe", "i think"]);
    let with_name = parse_kg_markdown("zed", "synonyms:: zed extension", true);
    assert_eq!(with_name[0].term, "zed");
}

#[test]
fn shipped_lists_load_and_compile_to_thesaurus_json() {
    let l = lists();
    let concepts: BTreeSet<&str> = l.style_terms.iter().map(|t| t.concept.as_str()).collect();
    for c in ["lab-filler", "lab-hedge", "lab-hedge-phrase", "lab-tone"] {
        assert!(concepts.contains(c), "missing KG list {c}");
    }
    // every entry is at least terraphim_automata's MIN_FIND_PATTERN_LENGTH (2)
    assert!(l.style_terms.iter().all(|t| t.term.len() >= 2));
    let json = thesaurus_json("Lab", &l.style_terms);
    assert!(json.contains("\"perhaps\": {\"id\": "));
    assert!(json.contains("\"nterm\": \"lab-hedge\""));
}

#[test]
fn domain_kg_contributes_concept_coverage() {
    let l = lists();
    let (_, doc) = fixtures()
        .into_iter()
        .find(|(n, _)| n == "zed-plugin-fit")
        .unwrap();
    let scores = score_sentences(&doc, &l);
    assert!(scores.iter().any(|s| s.kg_coverage > 0.0));
    let walden = fixtures()
        .into_iter()
        .find(|(n, _)| n == "walden-economy")
        .unwrap()
        .1;
    assert!(score_sentences(&walden, &l)
        .iter()
        .all(|s| s.kg_coverage == 0.0));
}

#[test]
fn short_emphatic_sentences_are_never_marked_weak() {
    let l = lists();
    for (name, doc) in fixtures() {
        let scores = score_sentences(&doc, &l);
        for &si in rank_weakest(&scores)
            .iter()
            .take(weakest_count(scores.len()))
        {
            assert!(
                scores[si].words >= SHORT_SENTENCE_WORDS,
                "{name}: short sentence marked weak: {}",
                doc.sentence_text(si)
            );
        }
    }
}

#[test]
fn weakest_count_is_fifteen_percent_at_least_one() {
    assert_eq!(weakest_count(0), 0);
    assert_eq!(weakest_count(3), 1);
    assert_eq!(weakest_count(19), 3);
    assert_eq!(weakest_count(47), 8);
}
