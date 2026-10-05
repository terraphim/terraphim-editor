//! Golden fixture for the inline indicators (issue #8), run natively.
//!
//! `tests/fixtures/indicators/indicators.md` is built here through
//! `terraphim_alternatives` (so every anchor is correct by construction) and
//! compared with the committed file. The browser tests in
//! `tests/web_indicators.rs` and the reference screenshots in
//! `tests/fixtures/visual/` use that same file. Set `UPDATE_GOLDENS=1` to
//! rewrite it after an intended change, then review the diff.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::Path;

use terraphim_alternatives::{write, Document, Source, SpanKind};
use terraphim_editor::DocumentSession;

const HEADLINE: &str = "Shouldn't everything be obvious?";
const WORD: &str = "tension";
const SENTENCE: &str = "Short sentences help.";
const PARAGRAPH: &str = "This paragraph has alternatives of its own. It shows a rule in the left \
gutter and a column of dots instead of an underline, and it is long enough to wrap onto a \
second line in the writing column.";

fn body() -> String {
    format!(
        "# {HEADLINE}\n\nThere is a {WORD} between clarity and brevity in every draft. {SENTENCE}\n\n{PARAGRAPH}\n"
    )
}

fn utf16_index(haystack: &str, needle: &str) -> usize {
    let byte = haystack.find(needle).expect("needle in body");
    haystack[..byte].encode_utf16().count()
}

fn add(doc: &mut Document, kind: SpanKind, text: &str, alts: &[&str], active: usize) {
    let body = doc.body.clone();
    let start = utf16_index(&body, text);
    let end = start + text.encode_utf16().count();
    let id = doc.add_span(kind, start, end).expect("span");
    for alt in alts {
        doc.add_alternative(&id, *alt, Source::Human, None)
            .expect("alternative");
    }
    if active > 0 {
        doc.set_active(&id, active).expect("set active");
    }
}

/// The fixture document: a 3-alternative headline (original active), a
/// 7-alternative word with the fifth alternative active, a 2-alternative
/// sentence and a 3-alternative paragraph with the second active.
fn build() -> String {
    let mut doc = Document::new(body());
    add(
        &mut doc,
        SpanKind::Sentence,
        HEADLINE,
        &[
            "Why isn't everything obvious?",
            "Everything should be obvious.",
        ],
        0,
    );
    add(
        &mut doc,
        SpanKind::Word,
        WORD,
        &[
            "pressure",
            "challenge",
            "conflict",
            "struggle",
            "friction",
            "strain",
        ],
        4,
    );
    add(
        &mut doc,
        SpanKind::Sentence,
        SENTENCE,
        &["Short sentences are easier to follow."],
        0,
    );
    add(
        &mut doc,
        SpanKind::Paragraph,
        PARAGRAPH,
        &[
            "This paragraph was rewritten once. The rule in the gutter shows that it has \
             alternatives, and the lit dot shows which one is active right now.",
            "A shorter take on the same paragraph.",
        ],
        1,
    );
    write(&doc)
}

#[test]
fn indicators_fixture_matches_its_golden_file() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/indicators/indicators.md");
    let actual = build();
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &actual).unwrap();
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{e} (run with UPDATE_GOLDENS=1 to create it)"));
    assert_eq!(
        actual, expected,
        "indicators.md differs from its golden file"
    );

    // The fixture opens cleanly: four live spans, nothing set aside.
    let mut session = DocumentSession::new();
    let opened = session.open(&expected);
    assert_eq!(opened.warning, None);
    assert_eq!(opened.unresolved, 0);
    let spans = &session.document().annotations.spans;
    let shape: Vec<(String, usize, usize)> = spans
        .iter()
        .map(|s| (format!("{:?}", s.kind), s.alts.len(), s.active))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("Sentence".to_string(), 3, 0),
            ("Word".to_string(), 7, 4),
            ("Sentence".to_string(), 2, 0),
            ("Paragraph".to_string(), 3, 1),
        ]
    );
    assert!(session.body().contains("There is a struggle between"));
}
