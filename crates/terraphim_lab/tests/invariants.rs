//! Properties every action must keep: protected text is never marked,
//! offsets index the original body in UTF-16, results are deterministic, and
//! nothing is ever rewritten.

mod common;

use common::{FIXTURES, covered, range16, role_config, summary};
use terraphim_lab::{LabAction, LabConfig, MarkKind, mark, mark_all, mark_many};

/// A body whose protected parts are full of things every action would mark
/// in prose: typos, hedges, filler, off-tone words and long, convoluted
/// sentences.
const PROTECTED_BODY: &str = "\
# Teh heading is basically awesome, perhaps

Prose with `teh basically awesome code` and a [link](https://example.org/teh-basically) here.

```text
Perhaps teh fenced block is basically awesome stuff, which (as noted, which is odd) runs on; and on, because it must, although nobody, really, asked for it, at all, ever, in any way, shape or form, today or tomorrow.
```

| teh | basically awesome |
|-----|-------------------|
| perhaps | stuff |

> Perhaps teh quote is basically awesome stuff.

    indented teh code is basically awesome

<div>teh basically awesome</div>

Final prose recieve is basically fine.
";

/// Every byte range of `body` that must never be marked.
fn protected_ranges16(body: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for needle in [
        "# Teh heading is basically awesome, perhaps",
        "`teh basically awesome code`",
        "(https://example.org/teh-basically)",
        "| teh | basically awesome |",
        "| perhaps | stuff |",
        "> Perhaps teh quote is basically awesome stuff.",
        "    indented teh code is basically awesome",
        "<div>teh basically awesome</div>",
    ] {
        out.push(range16(body, needle, 0));
    }
    let fence_start = body.find("```text").unwrap();
    let fence_end = body[fence_start + 3..].find("```").unwrap() + fence_start + 6;
    let s = body[..fence_start].encode_utf16().count();
    out.push((s, s + body[fence_start..fence_end].encode_utf16().count()));
    out
}

#[test]
fn protected_text_is_never_marked() {
    let config = role_config();
    let marks = mark_all(PROTECTED_BODY, &config);
    let protected = protected_ranges16(PROTECTED_BODY);
    for m in &marks {
        for &(ps, pe) in &protected {
            assert!(
                m.end <= ps || m.start >= pe,
                "{:?} {:?} overlaps protected range {ps}..{pe}",
                m.kind,
                covered(PROTECTED_BODY, m)
            );
        }
    }
    // Prose outside the protected parts is still marked.
    let s = summary(PROTECTED_BODY, &marks);
    assert!(s.contains(&(MarkKind::Typo, "recieve".into(), Some("receive".into()))));
    assert!(s.contains(&(MarkKind::Filler, "basically".into(), None)));
    // The prose "teh"/"basically"/"awesome" inside inline code are not.
    assert_eq!(
        s.iter().filter(|(k, _, _)| *k == MarkKind::Filler).count(),
        1,
        "{s:?}"
    );
}

#[test]
fn sentence_marks_split_around_inline_code() {
    let mut config = LabConfig::with_defaults().unwrap();
    config.options.long_sentence_words = 8;
    let body = "Call `teh_function()` when the long sentence needs a few more words today.";
    let marks = mark(body, &config, LabAction::LongSentences);
    assert_eq!(
        summary(body, &marks),
        vec![
            (MarkKind::LongSentence, "Call".into(), None),
            (
                MarkKind::LongSentence,
                "when the long sentence needs a few more words today.".into(),
                None
            ),
        ]
    );
    assert_eq!(marks[0].reason, marks[1].reason);
}

#[test]
fn offsets_are_utf16_into_the_original_body() {
    // Multi-byte (é, ü), astral (𝄞, 2 UTF-16 units), CRLF line endings and a
    // hard wrap inside a multi-word hedge.
    let body = "Caf\u{e9} \u{1d11e} owners recieve m\u{fc}sic.\r\nIt is basically\r\nfine, sort\r\nof.\r\n\r\nTeh end.";
    let config = LabConfig::with_defaults().unwrap();
    let marks = mark_all(body, &config);
    let s = summary(body, &marks);
    assert!(
        s.contains(&(MarkKind::Typo, "recieve".into(), Some("receive".into()))),
        "{s:?}"
    );
    assert!(
        s.contains(&(MarkKind::Filler, "basically".into(), None)),
        "{s:?}"
    );
    assert!(
        s.contains(&(MarkKind::Filler, "sort\r\nof".into(), None)),
        "{s:?}"
    );
    assert!(
        s.contains(&(MarkKind::Typo, "Teh".into(), Some("The".into()))),
        "{s:?}"
    );
    let typo = marks.iter().find(|m| m.kind == MarkKind::Typo).unwrap();
    // "Café 𝄞 owners " is 5 + 3 + 7 = 15 UTF-16 units but 18 bytes.
    assert_eq!((typo.start, typo.end), (15, 22));
    assert_eq!((typo.start, typo.end), range16(body, "recieve", 0));
}

#[test]
fn every_mark_is_in_bounds_and_on_char_boundaries() {
    let config = role_config();
    for (name, body) in FIXTURES
        .iter()
        .copied()
        .chain([("protected", PROTECTED_BODY)])
    {
        let len16 = body.encode_utf16().count();
        for m in mark_all(body, &config) {
            assert!(m.start < m.end && m.end <= len16, "{name}: {m:?}");
            // `covered` panics if a mark splits a surrogate pair.
            let text = covered(body, &m);
            assert!(!text.trim().is_empty(), "{name}: empty mark {m:?}");
            assert!(m.score.is_finite() && m.score >= 0.0, "{name}: {m:?}");
            assert!(!m.reason.is_empty());
            match m.kind {
                MarkKind::Typo | MarkKind::Punctuation => assert!(m.proposal.is_some()),
                _ => assert!(m.proposal.is_none(), "{name}: {m:?}"),
            }
        }
    }
}

#[test]
fn marks_are_deterministic() {
    // Two configs built independently: their AHashMap-backed thesauri have
    // different iteration orders, so this also checks matcher construction
    // does not depend on hash order.
    let a = role_config();
    let b = role_config();
    for (name, body) in FIXTURES {
        let first = mark_all(body, &a);
        let second = mark_all(body, &b);
        let third = mark_all(body, &a);
        assert_eq!(first, second, "{name}");
        assert_eq!(first, third, "{name}");
        assert!(!first.is_empty());
    }
}

#[test]
fn no_action_mutates_the_input() {
    let config = role_config();
    for (name, body) in FIXTURES
        .iter()
        .copied()
        .chain([("protected", PROTECTED_BODY)])
    {
        let before = body.to_string();
        for action in LabAction::ALL {
            let marks = mark(body, &config, action);
            assert!(
                marks.iter().all(|m| m.kind.action() == action),
                "{name} {action:?}"
            );
        }
        let _ = mark_all(body, &config);
        assert_eq!(body, before, "{name}");
    }
}

#[test]
fn mark_all_is_the_union_of_single_actions() {
    let config = role_config();
    for (name, body) in FIXTURES {
        let mut union: Vec<_> = LabAction::ALL
            .iter()
            .flat_map(|&a| mark(body, &config, a))
            .collect();
        union.sort_by(|x, y| {
            (x.start, x.end, x.kind)
                .cmp(&(y.start, y.end, y.kind))
                .then(x.reason.cmp(&y.reason))
        });
        assert_eq!(mark_all(body, &config), union, "{name}");
        // Duplicated actions are ignored.
        assert_eq!(
            mark_many(body, &config, &[LabAction::OffTone, LabAction::OffTone]),
            mark(body, &config, LabAction::OffTone)
        );
    }
}

#[test]
fn empty_and_whitespace_bodies_yield_no_marks() {
    let config = role_config();
    for body in ["", "   ", "\n\n\r\n", "# Only a heading", "```\ncode\n```"] {
        assert!(mark_all(body, &config).is_empty(), "{body:?}");
    }
}

#[test]
fn labels_and_kinds_cover_all_six_actions() {
    let labels: Vec<&str> = LabAction::ALL.iter().map(|a| a.label()).collect();
    assert_eq!(
        labels,
        vec![
            "Fix punctuation and typos",
            "Mark the weakest sentences",
            "Mark sentences that run long",
            "Mark convoluted sentences",
            "Mark words that don't fit the tone",
            "Mark hedges and filler",
        ]
    );
    let json = serde_json::to_string(&mark(
        "We recieve it.",
        &role_config(),
        LabAction::TyposAndPunctuation,
    ))
    .unwrap();
    assert_eq!(
        json,
        r#"[{"kind":"typo","start":3,"end":10,"score":1.0,"reason":"typo: \"recieve\" -> \"receive\"","proposal":"receive"}]"#
    );
}
