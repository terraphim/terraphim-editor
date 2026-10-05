//! One word count (issue #59), run natively.
//!
//! The Write_On counter (`document_counts`, from
//! [`DocumentSession::counts`]) and the Lab trim status card
//! (`TrimPlan::status`) must report the same number of words for the same
//! body (R-2.1 and R-8.4 of `docs/requirements/alternative-control.md` show
//! the same `535` in both). Every Markdown fixture in the repository is opened
//! as the editor opens it (annotation block split off) and both counts are
//! compared on the resulting body. The browser check is in `tests/web_trim.rs`.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

use terraphim_editor::DocumentSession;
use terraphim_lab::{trim_plan, LabConfig, TrimLevel};

/// Directories whose `.md` files (recursively) are compared.
const FIXTURE_DIRS: [&str; 3] = [
    "tests/fixtures",
    "crates/terraphim_lab/tests/fixtures",
    "crates/terraphim_alternatives/tests/fixtures",
];

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

fn all_fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in FIXTURE_DIRS {
        markdown_files(&root.join(dir), &mut files);
    }
    files.sort();
    files
}

/// `(counter words, card words_before)` for the body `source` opens to.
fn both_counts(source: &str, config: &LabConfig) -> (usize, usize, String) {
    let mut session = DocumentSession::new();
    let body = session.open(source).body;
    let counter = session.counts().words;
    let card = trim_plan(&body, config)
        .status(TrimLevel::Slight, &[])
        .words_before;
    (counter, card, body)
}

#[test]
fn counter_and_trim_card_agree_on_every_fixture() {
    let config = LabConfig::with_defaults().unwrap();
    let files = all_fixtures();
    for required in [
        "tests/fixtures/lab/lab.md",
        "tests/fixtures/indicators/indicators.md",
        "crates/terraphim_lab/tests/fixtures/three-men-ch1.md",
        "crates/terraphim_lab/tests/fixtures/walden-economy.md",
        "crates/terraphim_lab/tests/fixtures/zed-plugin-fit.md",
        "crates/terraphim_lab/tests/fixtures/zed-plugin-fit-full.md",
        "crates/terraphim_alternatives/tests/fixtures/full.md",
    ] {
        assert!(
            files.iter().any(|f| f.ends_with(required)),
            "fixture {required} not found"
        );
    }
    let mut disagreements = Vec::new();
    for file in &files {
        let source = fs::read_to_string(file).unwrap();
        let (counter, card, body) = both_counts(&source, &config);
        if counter != card {
            disagreements.push(format!(
                "{}: counter {counter}, card {card}",
                file.display()
            ));
        }
        assert!(
            body.trim().is_empty() || counter > 0,
            "{}: no words counted",
            file.display()
        );
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

/// The fixture from the issue: 166 (counter) against 160 (card) before the
/// fix. The six extra tokens were `#`, two code fences, the list marker and
/// two stand-alone commas; none of them is a word.
#[test]
fn lab_fixture_counts_160_words_in_both() {
    let config = LabConfig::with_defaults().unwrap();
    let source =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lab/lab.md"))
            .unwrap();
    assert_eq!(both_counts(&source, &config).0, 160);
    assert_eq!(both_counts(&source, &config).1, 160);
}

/// Edge cases of the shared definition, through both public entry points.
#[test]
fn edge_cases_agree() {
    let config = LabConfig::with_defaults().unwrap();
    for (text, words) in [
        ("", 0),
        ("   \n\t", 0),
        ("# Heading\n", 1),
        ("- item\n* item\n", 2),
        // An ordered-list number is a run of digits, so it is a word.
        ("1. item\n", 2),
        ("```rust\nlet x = 1;\n```\n", 4),
        ("Inline `code_span` here.", 3),
        ("Stand-alone , comma and -- dashes", 4),
        ("us\u{2014}George hadn\u{2019}t", 3),
        ("e.g. R-8.7 and/or terraphim_lsp", 4),
        ("**bold** _em_ ~~gone~~", 3),
        ("| a | b |\n|---|---|\n| 1 | 2 |\n", 4),
        ("a \u{1F600} \u{1D11E}", 1),
        ("line one\r\nline two\r\n", 4),
        ("[link text](https://example.com/page)", 4),
    ] {
        let (counter, card, _) = both_counts(text, &config);
        assert_eq!((counter, card), (words, words), "{text:?}");
    }
}
