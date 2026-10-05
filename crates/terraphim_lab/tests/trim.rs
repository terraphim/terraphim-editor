//! Trim engine (issue #15): levels hit their targets, nest, never touch
//! protected structure, and "Make the cuts" deletes exactly the faded spans
//! plus a documented tidy-up. Real KG lists, real rolegraph, no mocks.

mod common;

use common::{FIXTURES, role_config};
use terraphim_lab::{
    Cut, CutId, EditKind, LabConfig, MadeCuts, Tier, TrimLevel, TrimPlan, make_cuts,
    make_cuts_from_ranges, trim_plan,
};

const CUTTING: [TrimLevel; 4] = [
    TrimLevel::Slight,
    TrimLevel::Tighten,
    TrimLevel::Sharper,
    TrimLevel::Half,
];

/// The whole of `docs/requirements/zed-plugin-fit.md` (tables, lists, code):
/// a code-dense document.
const CODE_DENSE: &str = include_str!("fixtures/zed-plugin-fit-full.md");

fn units(body: &str) -> Vec<u16> {
    body.encode_utf16().collect()
}

fn slice16(body: &str, start: usize, end: usize) -> String {
    String::from_utf16(&units(body)[start..end]).expect("char boundary")
}

/// The crate's word count of `text`, through the public API: a plan's
/// `total_words` uses the one word definition.
fn word_count(text: &str, config: &LabConfig) -> usize {
    trim_plan(text, config).total_words()
}

/// Sorted, merged UTF-16 ranges (touching ranges merge), as `make_cuts`
/// merges them.
fn merged(cuts: &[Cut]) -> Vec<(usize, usize)> {
    let mut r: Vec<(usize, usize)> = cuts.iter().map(|c| (c.start, c.end)).collect();
    r.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (s, e) in r {
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

/// Apply edits to `body` from last to first (as an editor would).
fn apply(body: &str, made: &MadeCuts) -> String {
    let mut u = units(body);
    for e in made.edits.iter().rev() {
        let ins: Vec<u16> = e.insert.encode_utf16().collect();
        u.splice(e.start..e.end, ins);
    }
    String::from_utf16(&u).unwrap()
}

fn active(plan: &TrimPlan, level: TrimLevel, kept: &[CutId]) -> Vec<Cut> {
    plan.active(level, kept)
}

#[test]
fn every_fixture_level_within_three_points() {
    let config = role_config();
    let mut table = String::new();
    for (name, body) in FIXTURES {
        let plan = trim_plan(body, &config);
        for level in CUTTING {
            let st = plan.status(level, &[]);
            let delta = st.percent - st.target_percent;
            table.push_str(&format!(
                "{name} {:?}: {} -> {} words, {:.1}% (target {:.0}%, {delta:+.1}pp)\n",
                level, st.words_before, st.words_after, st.percent, st.target_percent
            ));
            assert!(
                delta.abs() <= 3.0,
                "{name} {level:?} misses: {:.1}% vs {:.0}%\n{table}",
                st.percent,
                st.target_percent
            );
        }
    }
    println!("{table}");
}

/// Every document the plan-level properties run on.
fn documents() -> Vec<(&'static str, &'static str)> {
    let mut d = FIXTURES.to_vec();
    d.push(("zed-plugin-fit-full", CODE_DENSE));
    d
}

#[test]
fn code_dense_document_reports_the_achieved_percentage_honestly() {
    let config = role_config();
    let plan = trim_plan(CODE_DENSE, &config);
    let mut previous = 0.0;
    for level in CUTTING {
        let st = plan.status(level, &[]);
        let made = make_cuts(CODE_DENSE, &active(&plan, level, &[]));
        println!(
            "code-dense {level:?}: {} ({:.1}% vs target {:.0}%)",
            st.card_text(),
            st.percent,
            st.target_percent
        );
        // The card's numbers are the text's numbers.
        assert_eq!(st.words_after, word_count(&made.text, &config), "{level:?}");
        let expected = 100.0 * (st.words_before - st.words_after) as f64 / st.words_before as f64;
        assert!((st.percent - expected).abs() < 1e-9);
        assert!(
            st.percent >= previous,
            "levels are nested, so cuts only grow"
        );
        previous = st.percent;
    }
    // Protected code, tables and code-holding sentences cap the cut: "Cut in
    // half" reports its shortfall rather than pretending to reach 50%.
    assert!(plan.status(TrimLevel::Half, &[]).percent < 47.0);
}

#[test]
fn levels_are_nested() {
    let config = role_config();
    for (name, body) in documents() {
        let plan = trim_plan(body, &config);
        assert_eq!(plan.faded(TrimLevel::Original).count(), 0);
        for pair in TrimLevel::ALL.windows(2) {
            let lower: Vec<CutId> = plan.faded(pair[0]).map(|c| c.id).collect();
            let higher: Vec<CutId> = plan.faded(pair[1]).map(|c| c.id).collect();
            assert!(
                lower.iter().all(|id| higher.contains(id)),
                "{name}: {:?} is not a superset of {:?}",
                pair[1],
                pair[0]
            );
        }
        assert!(
            plan.cuts()
                .iter()
                .all(|c| c.first_level != TrimLevel::Original)
        );
    }
}

#[test]
fn walk_through_order_is_document_order_and_ids_index_the_plan() {
    let config = role_config();
    for (_, body) in documents() {
        let plan = trim_plan(body, &config);
        let faded: Vec<&Cut> = plan.faded(TrimLevel::Half).collect();
        assert!(
            faded
                .windows(2)
                .all(|w| (w[0].start, w[0].end) <= (w[1].start, w[1].end))
        );
        for c in plan.cuts() {
            assert_eq!(plan.get(c.id), Some(c));
        }
    }
}

/// Characters in UTF-16 range `a..b` of `body`.
fn chars_in(units: &[u16], a: usize, b: usize) -> usize {
    char::decode_utf16(units[a..b].iter().copied()).count()
}

/// The golden invariant from the spike review (#15): the faded ranges are
/// exactly the ranges `make_cuts` deletes; every other edit is a documented
/// fix-up within two characters of a cut boundary (emptied blocks: only
/// whitespace and list markers, touching a cut).
fn assert_golden(name: &str, body: &str, cuts: &[Cut], made: &MadeCuts) {
    let u = units(body);
    let ranges = merged(cuts);
    let cut_edits: Vec<(usize, usize)> = made
        .edits
        .iter()
        .filter(|e| e.kind == EditKind::Cut)
        .map(|e| (e.start, e.end))
        .collect();
    assert_eq!(
        cut_edits, ranges,
        "{name}: cut edits differ from faded ranges"
    );
    assert!(
        made.edits.windows(2).all(|w| w[0].end <= w[1].start),
        "{name}: edits overlap or are unsorted"
    );
    for e in made.edits.iter().filter(|e| e.kind != EditKind::Cut) {
        let covered = String::from_utf16(&u[e.start..e.end]).expect("char boundary");
        if e.kind == EditKind::EmptyBlock {
            assert!(e.insert.is_empty());
            assert!(
                covered.chars().all(|c| c.is_whitespace()
                    || c.is_ascii_digit()
                    || matches!(c, '-' | '*' | '+' | '.' | ')' | '[' | ']' | 'x' | 'X')),
                "{name}: empty-block edit removes text {covered:?}"
            );
            assert!(
                ranges.iter().any(|&(s, en)| s == e.end || en == e.start),
                "{name}: empty-block edit does not touch a cut"
            );
            continue;
        }
        // Within two characters: at most two characters lie between the
        // edit's farthest character and a cut boundary.
        let near = ranges.iter().any(|&(s, en)| {
            (e.end <= s && chars_in(&u, e.start, s) <= 3)
                || (e.start >= en && chars_in(&u, en, e.end) <= 3)
        });
        assert!(
            near,
            "{name}: {:?} edit {covered:?} is not next to a cut",
            e.kind
        );
        match e.kind {
            EditKind::Capitalisation => assert_eq!(e.insert, covered.to_uppercase(), "{name}"),
            EditKind::Punctuation => {
                assert!(e.insert.is_empty());
                assert!(
                    covered.chars().all(|c| matches!(c, ',' | ';' | ':')),
                    "{name}: {covered:?}"
                );
            }
            _ => {
                assert!(e.insert.is_empty());
                assert!(
                    covered.chars().all(char::is_whitespace),
                    "{name}: {covered:?}"
                );
            }
        }
    }
    assert_eq!(
        apply(body, made),
        made.text,
        "{name}: edits do not reproduce the text"
    );
}

#[test]
fn make_cuts_deletes_exactly_the_faded_spans_plus_documented_fix_ups() {
    let config = role_config();
    for (name, body) in documents() {
        let plan = trim_plan(body, &config);
        for level in TrimLevel::ALL {
            let cuts = active(&plan, level, &[]);
            let made = make_cuts(body, &cuts);
            assert_golden(&format!("{name} {level:?}"), body, &cuts, &made);
            assert_eq!(
                word_count(&made.text, &config),
                plan.status(level, &[]).words_after,
                "{name} {level:?}: status card disagrees with the text"
            );
        }
        assert_eq!(make_cuts(body, &[]).text, body);
    }
}

/// Structure full of words every rule would cut in prose.
const PROTECTED_BODY: &str = "\
# Perhaps this heading is really quite basically padded

Really, this is quite a padded note, which is basically filler, and it says nothing at all. It is just padding (really quite padded). Perhaps it is fine.

```text
Perhaps the fenced block is really quite basically padded, which is odd.
```

| really | quite basically |
|--------|-----------------|
| perhaps | just filler |

> Perhaps the quote is really quite basically padded, which is odd.

<div>Perhaps really quite basically padded.</div>

See [really quite basic](https://example.org/really-quite) and `really quite basically` code here.

Another paragraph that is basically filler, which is quite padded. It is quite literally padding, of course. Honestly it is really just very padded.[^n]

[^n]: Perhaps the footnote is really quite basically padded.
";

const PROTECTED_NEEDLES: [&str; 9] = [
    "# Perhaps this heading is really quite basically padded",
    "```text\nPerhaps the fenced block is really quite basically padded, which is odd.\n```",
    "| really | quite basically |\n|--------|-----------------|\n| perhaps | just filler |",
    "> Perhaps the quote is really quite basically padded, which is odd.",
    "<div>Perhaps really quite basically padded.</div>",
    "](https://example.org/really-quite)",
    "`really quite basically`",
    "[^n]: Perhaps the footnote is really quite basically padded.",
    "[^n]",
];

#[test]
fn protected_structure_is_never_faded() {
    let config = role_config();
    let plan = trim_plan(PROTECTED_BODY, &config);
    assert!(
        plan.faded(TrimLevel::Half).count() > 0,
        "prose is still trimmed"
    );
    for needle in PROTECTED_NEEDLES {
        let byte = PROTECTED_BODY.find(needle).unwrap();
        let s = PROTECTED_BODY[..byte].encode_utf16().count();
        let e = s + needle.encode_utf16().count();
        for c in plan.cuts() {
            assert!(
                c.end <= s || c.start >= e,
                "cut {:?} overlaps protected {needle:?}",
                slice16(PROTECTED_BODY, c.start, c.end)
            );
        }
    }
    let made = make_cuts(PROTECTED_BODY, &active(&plan, TrimLevel::Half, &[]));
    for needle in PROTECTED_NEEDLES {
        assert!(
            made.text.contains(needle),
            "{needle:?} lost:\n{}",
            made.text
        );
    }
    // On the fixtures, no cut holds inline code or a table.
    for (name, body) in documents() {
        for c in trim_plan(body, &config).cuts() {
            let text = slice16(body, c.start, c.end);
            assert!(!text.contains('`'), "{name}: cut holds code: {text:?}");
            assert!(!text.contains('|'), "{name}: cut holds a table: {text:?}");
        }
    }
}

#[test]
fn openers_and_short_sentences_only_go_at_half() {
    let config = role_config();
    let mut seen = 0;
    for (name, body) in documents() {
        for c in trim_plan(body, &config).cuts() {
            if c.reason == "paragraph-opening sentence" || c.reason == "short sentence" {
                seen += 1;
                assert_eq!(c.first_level, TrimLevel::Half, "{name}: {:?}", c.reason);
            }
        }
    }
    assert!(seen > 0, "the rule is exercised");
}

#[test]
fn cut_list_items_leave_no_empty_bullets() {
    let config = role_config();
    let body = "Intro sentence that stays here.\n\n- First item is really quite padded and weak.\n- Second item stays because it names the span model.\n- Third item is basically just filler too.\n";
    let plan = trim_plan(body, &config);
    for level in CUTTING {
        let made = make_cuts(body, &active(&plan, level, &[]));
        for line in made.text.lines() {
            let t = line.trim();
            assert!(
                t != "-" && t != "*",
                "empty bullet at {level:?}:\n{}",
                made.text
            );
        }
        assert_eq!(
            word_count(&made.text, &config),
            plan.status(level, &[]).words_after
        );
    }
}

#[test]
fn word_cuts_are_whole_words_outside_emphasis() {
    let config = role_config();
    let body = "The quite-good result is **quite** clear to everyone here today. It reads well, and the rest is ordinary prose for the reader.";
    let plan = trim_plan(body, &config);
    for c in plan.cuts() {
        let text = slice16(body, c.start, c.end);
        assert!(
            !(c.tier == Tier::Word && text.contains("quite")),
            "{text:?}"
        );
    }
}

#[test]
fn a_relative_clause_takes_its_closing_comma() {
    let config = LabConfig::with_defaults().unwrap();
    let body = "The editor, which owns its DOM, is the only target here today.";
    let plan = trim_plan(body, &config);
    let aside = plan
        .cuts()
        .iter()
        .find(|c| c.reason == "aside \"which\"")
        .expect("aside selected at some level");
    assert_eq!(
        slice16(body, aside.start, aside.end),
        ", which owns its DOM,"
    );
    let made = make_cuts(body, std::slice::from_ref(aside));
    assert_eq!(made.text, "The editor is the only target here today.");
}

#[test]
fn kept_spans_reduce_the_cut_and_the_status() {
    let config = role_config();
    let body = common::THREE_MEN;
    let plan = trim_plan(body, &config);
    let level = TrimLevel::Tighten;
    let base = plan.status(level, &[]);
    let inside = |outer: &Cut, inner: &Cut| {
        outer.id != inner.id && outer.start <= inner.start && inner.end <= outer.end
    };
    // A faded sentence with a faded cut nested inside it.
    let sentence = plan
        .faded(level)
        .find(|c| c.tier == Tier::Sentence && plan.faded(level).any(|w| inside(c, w)))
        .expect("a sentence with a nested cut");
    let kept = [sentence.id];
    let st = plan.status(level, &kept);
    assert_eq!(st.words_after, base.words_after + sentence.words);
    assert!(st.percent < base.percent);
    let remaining = active(&plan, level, &kept);
    // Keeping the sentence keeps everything inside it.
    assert!(
        remaining
            .iter()
            .all(|c| !(sentence.start <= c.start && c.end <= sentence.end))
    );
    let made = make_cuts(body, &remaining);
    assert_golden("kept", body, &remaining, &made);
    assert_eq!(word_count(&made.text, &config), st.words_after);

    // Keeping a word cut that no faded cut covers.
    let word = plan
        .faded(level)
        .find(|w| w.tier == Tier::Word && !plan.faded(level).any(|c| inside(c, w)))
        .expect("a free-standing word cut");
    let st = plan.status(level, &[word.id]);
    assert_eq!(st.words_after, base.words_after + word.words);
    // Unknown ids are ignored.
    assert_eq!(plan.status(level, &[CutId(u32::MAX)]), base);
}

/// True when `inner` lies inside `outer` (and is a different cut).
fn inside(outer: &Cut, inner: &Cut) -> bool {
    outer.id != inner.id && outer.start <= inner.start && inner.end <= outer.end
}

/// The keep-always-wins checks for one `kept` set: no `Cut` edit touches a
/// kept range, the golden invariant holds on the effective (post-keep)
/// ranges, and the card's numbers are the text's numbers.
fn assert_keep_wins(
    config: &LabConfig,
    name: &str,
    body: &str,
    plan: &TrimPlan,
    level: TrimLevel,
    kept: &[CutId],
) {
    let effective = active(plan, level, kept);
    let made = make_cuts(body, &effective);
    assert_golden(name, body, &effective, &made);
    for k in kept.iter().filter_map(|&id| plan.get(id)) {
        for e in made.edits.iter().filter(|e| e.kind == EditKind::Cut) {
            assert!(
                e.end <= k.start || e.start >= k.end,
                "{name}: kept {:?} deleted",
                slice16(body, k.start, k.end)
            );
        }
    }
    let st = plan.status(level, kept);
    assert_eq!(word_count(&made.text, config), st.words_after, "{name}");
    assert!(
        st.words_after >= plan.status(level, &[]).words_after,
        "{name}"
    );
}

#[test]
fn keeping_a_word_inside_a_faded_sentence_keeps_the_word() {
    let config = role_config();
    let level = TrimLevel::Half;
    let mut checked = 0;
    for (name, body) in documents() {
        let plan = trim_plan(body, &config);
        let base = plan.status(level, &[]);
        for c in plan.faded(level).filter(|c| c.tier == Tier::Sentence) {
            let Some(w) = plan
                .faded(level)
                .find(|w| w.tier == Tier::Word && inside(c, w))
            else {
                continue;
            };
            let kept = [w.id];
            let st = plan.status(level, &kept);
            assert_eq!(st.words_after, base.words_after + w.words, "{name}");
            let made = make_cuts(body, &active(&plan, level, &kept));
            let word = slice16(body, w.start, w.end)
                .trim_matches(|ch: char| ch.is_whitespace() || ch == ',')
                .to_lowercase();
            assert!(
                made.text.to_lowercase().contains(&word),
                "{name}: kept {word:?} lost"
            );
            assert_keep_wins(&config, name, body, &plan, level, &kept);
            checked += 1;
        }
    }
    assert!(checked > 0, "a word nested in a faded sentence exists");
}

#[test]
fn keeping_a_clause_inside_a_faded_sentence_keeps_the_clause() {
    let config = role_config();
    let level = TrimLevel::Half;
    let mut checked = 0;
    for (name, body) in documents() {
        let plan = trim_plan(body, &config);
        let base = plan.status(level, &[]);
        for c in plan.faded(level).filter(|c| c.tier == Tier::Sentence) {
            let Some(k) = plan
                .faded(level)
                .find(|k| k.tier == Tier::Clause && inside(c, k))
            else {
                continue;
            };
            let st = plan.status(level, &[k.id]);
            assert_eq!(st.words_after, base.words_after + k.words, "{name}");
            assert_keep_wins(&config, name, body, &plan, level, &[k.id]);
            checked += 1;
        }
    }
    assert!(checked > 0, "a clause nested in a faded sentence exists");
}

#[test]
fn keeping_two_nested_items() {
    let config = role_config();
    let level = TrimLevel::Half;
    let body = common::THREE_MEN;
    let plan = trim_plan(body, &config);
    let base = plan.status(level, &[]);
    // Two disjoint cuts inside one faded sentence: both survive.
    let (sentence, a, b) = plan
        .faded(level)
        .filter(|c| c.tier == Tier::Sentence)
        .find_map(|c| {
            let inner: Vec<&Cut> = plan
                .faded(level)
                .filter(|w| {
                    inside(c, w) && !plan.faded(level).any(|m| inside(c, m) && inside(m, w))
                })
                .collect();
            inner
                .windows(2)
                .find(|p| p[0].end <= p[1].start)
                .map(|p| (c, p[0], p[1]))
        })
        .expect("a sentence holding two disjoint cuts");
    let st = plan.status(level, &[a.id, b.id]);
    assert_eq!(st.words_after, base.words_after + a.words + b.words);
    assert_keep_wins(&config, "two inner", body, &plan, level, &[a.id, b.id]);
    // The outer cut kept with an inner one: the same as keeping the outer.
    assert_eq!(
        plan.status(level, &[sentence.id, a.id]),
        plan.status(level, &[sentence.id])
    );
    assert_eq!(
        active(&plan, level, &[sentence.id, a.id]),
        active(&plan, level, &[sentence.id])
    );
    assert_keep_wins(
        &config,
        "outer and inner",
        body,
        &plan,
        level,
        &[sentence.id, a.id],
    );
}

/// A small deterministic generator (SplitMix64) for the property test.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[test]
fn invariants_hold_with_random_keeps() {
    let config = role_config();
    let mut rng = SplitMix(0x15_7215);
    for (name, body) in documents() {
        let plan = trim_plan(body, &config);
        for level in CUTTING {
            let ids: Vec<CutId> = plan.faded(level).map(|c| c.id).collect();
            for round in 0..12 {
                // Keep each faded cut with probability 1/4 (and sometimes one
                // cut that this level does not fade).
                let mut kept: Vec<CutId> = ids
                    .iter()
                    .copied()
                    .filter(|_| rng.next().is_multiple_of(4))
                    .collect();
                if round % 3 == 0 && !plan.cuts().is_empty() {
                    let any = (rng.next() % plan.cuts().len() as u64) as u32;
                    kept.push(CutId(any));
                }
                assert_keep_wins(
                    &config,
                    &format!("{name} {level:?} round {round}"),
                    body,
                    &plan,
                    level,
                    &kept,
                );
            }
        }
    }
}

#[test]
fn trim_is_deterministic() {
    for (_, body) in documents() {
        let a = trim_plan(body, &role_config());
        let b = trim_plan(body, &role_config());
        assert_eq!(a, b);
        let cuts = active(&a, TrimLevel::Half, &[]);
        assert_eq!(make_cuts(body, &cuts), make_cuts(body, &cuts));
    }
}

#[test]
fn status_card_text() {
    let config = role_config();
    let plan = trim_plan(common::WALDEN, &config);
    assert_eq!(
        plan.status(TrimLevel::Original, &[]).card_text(),
        "670 \u{2192} 670 words \u{b7} \u{2212}0%"
    );
    let st = plan.status(TrimLevel::Half, &[]);
    assert_eq!(st.words_cut(), st.words_before - st.words_after);
    assert!(
        st.card_text().ends_with("\u{2212}50%"),
        "{}",
        st.card_text()
    );
    assert_eq!(TrimLevel::Half.label(), "Cut in half");
    assert_eq!(TrimLevel::Slight.target_words(853), 85);
}

// ------------------------------------------------------------ tidy-up

/// `make_cuts` with cuts given as the UTF-16 ranges of `needles` (first
/// occurrence each).
fn cut_needles(body: &str, needles: &[&str]) -> MadeCuts {
    let ranges: Vec<(usize, usize)> = needles
        .iter()
        .map(|n| {
            let byte = body.find(n).unwrap_or_else(|| panic!("{n:?} not in body"));
            let s = body[..byte].encode_utf16().count();
            (s, s + n.encode_utf16().count())
        })
        .collect();
    let made = make_cuts_from_ranges(body, ranges.iter().copied());
    assert_eq!(apply(body, &made), made.text);
    made
}

fn kinds(made: &MadeCuts) -> Vec<EditKind> {
    made.edits.iter().map(|e| e.kind).collect()
}

#[test]
fn tidy_collapses_a_doubled_comma() {
    let made = cut_needles("We need a, many, b here.", &["many"]);
    assert_eq!(made.text, "We need a, b here.");
    assert_eq!(
        kinds(&made),
        vec![EditKind::Cut, EditKind::Punctuation, EditKind::Whitespace]
    );
}

#[test]
fn tidy_drops_a_comma_left_after_a_conjunction() {
    let made = cut_needles(
        "It was impertinent, but, considering it, natural.",
        &[", considering it"],
    );
    assert_eq!(made.text, "It was impertinent, but natural.");
    let made = cut_needles(
        "It was odd, and, frankly speaking, fine.",
        &[" frankly speaking,"],
    );
    assert_eq!(made.text, "It was odd, and fine.");
}

#[test]
fn tidy_capitalises_a_new_sentence_start() {
    let made = cut_needles("It works. Perhaps these pages help.", &["Perhaps "]);
    assert_eq!(made.text, "It works. These pages help.");
    assert_eq!(made.edits[1].kind, EditKind::Capitalisation);
    let made = cut_needles("It works. Perhaps, these pages help.", &["Perhaps"]);
    assert_eq!(made.text, "It works. These pages help.");
    let made = cut_needles("Perhaps these help.\n\nNext one.", &["Perhaps "]);
    assert_eq!(made.text, "These help.\n\nNext one.");
    // Mid-sentence cuts never capitalise.
    let made = cut_needles("It seemed somehow to be a slight.", &[" somehow"]);
    assert_eq!(made.text, "It seemed to be a slight.");
    assert_eq!(kinds(&made), vec![EditKind::Cut]);
}

#[test]
fn tidy_leaves_clean_joins_alone() {
    let body = "Keep this. Drop this sentence. Keep that, of course.";
    let made = cut_needles(body, &[" Drop this sentence.", ", of course"]);
    assert_eq!(made.text, "Keep this. Keep that.");
    assert_eq!(kinds(&made), vec![EditKind::Cut, EditKind::Cut]);
}

#[test]
fn tidy_removes_space_before_punctuation_and_leading_space() {
    let made = cut_needles("It is quite. Fine.", &["quite"]);
    assert_eq!(made.text, "It is. Fine.");
    let made = cut_needles("First one. Second two.\n\nNext one.", &["First one."]);
    assert_eq!(made.text, "Second two.\n\nNext one.");
}

#[test]
fn tidy_on_crlf_and_multibyte_text() {
    let body = "Le caf\u{e9} est bon.\r\nPeut-\u{ea}tre, \u{e9}lan compte.\r\n\r\n\u{1d11e} Music first. Perhaps\r\n\u{fc}nits matter.\r\n";
    let made = cut_needles(body, &["Peut-\u{ea}tre", "Perhaps\r\n"]);
    assert_eq!(
        made.text,
        "Le caf\u{e9} est bon.\r\n\u{c9}lan compte.\r\n\r\n\u{1d11e} Music first. \u{dc}nits matter.\r\n"
    );
}

#[test]
fn cuts_at_document_start_and_end() {
    let made = cut_needles("Perhaps this works. Yes it does.", &["Perhaps "]);
    assert_eq!(made.text, "This works. Yes it does.");
    let made = cut_needles("Keep this. Drop the end.", &[" Drop the end."]);
    assert_eq!(made.text, "Keep this.");
    let made = cut_needles("All of it.", &["All of it."]);
    assert_eq!(made.text, "");
}

#[test]
fn invalid_cuts_are_ignored() {
    let body = "\u{1d11e} keep everything.";
    let made = make_cuts_from_ranges(body, [(5, 3), (0, 999), (1, 4), (4, 4)]);
    assert_eq!(made.text, body);
    assert!(made.edits.is_empty());
}

#[test]
fn an_emptied_list_item_loses_its_bullet() {
    let body = "Intro stays.\n\n- One gone. Two gone.\n- Three stays.\n";
    let made = cut_needles(body, &["One gone.", " Two gone."]);
    assert_eq!(made.text, "Intro stays.\n\n- Three stays.\n");
    assert!(kinds(&made).contains(&EditKind::EmptyBlock));
}

#[test]
fn abbreviations_ellipses_and_decimals_do_not_start_a_sentence() {
    // Each cut removes the end of one sentence and the start of the next,
    // leaving the text before it ending in a full stop that does not end a
    // sentence: no capital.
    for (body, cut, expected) in [
        (
            "Use e.g. the red one. Perhaps this option is best.",
            " the red one. Perhaps",
            "Use e.g. this option is best.",
        ),
        (
            "Use i.e. the red one. Perhaps this option is best.",
            " the red one. Perhaps",
            "Use i.e. this option is best.",
        ),
        (
            "Ask Dr. the right one. Perhaps jones knows.",
            " the right one. Perhaps",
            "Ask Dr. jones knows.",
        ),
        (
            "As in Fig. the left one. Perhaps three shows it.",
            " the left one. Perhaps",
            "As in Fig. three shows it.",
        ),
        (
            "It rose to 3.5 percent. Perhaps more came later.",
            "5 percent. Perhaps ",
            "It rose to 3.more came later.",
        ),
        (
            "Wait... the red one. Perhaps this works.",
            " the red one. Perhaps",
            "Wait... this works.",
        ),
    ] {
        let made = cut_needles(body, &[cut]);
        assert_eq!(made.text, expected, "{body:?}");
        assert!(
            !kinds(&made).contains(&EditKind::Capitalisation),
            "{body:?}"
        );
    }
    // A real sentence end still capitalises.
    let made = cut_needles(
        "Use the red one. Then the blue. Perhaps this option is best.",
        &[" Then the blue. Perhaps"],
    );
    assert_eq!(made.text, "Use the red one. This option is best.");
}
