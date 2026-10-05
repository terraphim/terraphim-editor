//! Golden-file tests for the persistence bridge (issue #6), run natively.
//!
//! For every `tests/fixtures/persistence/<name>.md` the preview of the opened
//! body and the clean export must match `<name>.preview.html` and
//! `<name>.export.md`. The same files are asserted in a real browser by
//! `tests/web.rs`. Set `UPDATE_GOLDENS=1` to rewrite them after an intended
//! change, then review the diff.
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

use terraphim_alternatives::words::count_words;
use terraphim_editor::{render_markdown, DocumentSession};

const FIXTURES: [&str; 3] = ["plain", "full", "malformed"];

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/persistence")
}

fn read(name: &str) -> String {
    fs::read_to_string(fixture_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn check_golden(name: &str, actual: &str) {
    let path = fixture_dir().join(name);
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name}: {e} (run with UPDATE_GOLDENS=1 to create it)"));
    assert_eq!(actual, expected, "{name} differs from its golden file");
}

#[test]
fn preview_and_export_match_golden_files() {
    for name in FIXTURES {
        let mut session = DocumentSession::new();
        let opened = session.open(&read(&format!("{name}.md")));
        let preview = render_markdown(&opened.body).expect("preview renders");
        assert!(
            !preview.contains("terraphim-alternatives"),
            "{name}: the block must never reach the preview"
        );
        check_golden(&format!("{name}.preview.html"), &preview);
        check_golden(&format!("{name}.export.md"), &session.export());
    }
}

#[test]
fn every_fixture_saves_back_byte_for_byte() {
    for name in FIXTURES {
        let source = read(&format!("{name}.md"));
        let mut session = DocumentSession::new();
        session.open(&source);
        // `full.md` has a non-canonical separator before its block; the
        // writer normalises it, after which saving is stable.
        let saved = session.save();
        let mut reopened = DocumentSession::new();
        reopened.open(&saved);
        assert_eq!(reopened.save(), saved, "{name}: save is not stable");
        assert_eq!(reopened.document(), session.document(), "{name}");
        if name != "full" {
            assert_eq!(
                saved, source,
                "{name}: plain and malformed files are untouched"
            );
        }
    }
}

#[test]
fn only_the_malformed_fixture_warns() {
    for name in FIXTURES {
        let mut session = DocumentSession::new();
        let opened = session.open(&read(&format!("{name}.md")));
        assert_eq!(opened.warning.is_some(), name == "malformed", "{name}");
        assert_eq!(opened.unresolved, 0, "{name}");
    }
}

#[test]
fn counts_of_the_full_fixture_include_ghosted_text() {
    let mut session = DocumentSession::new();
    let opened = session.open(&read("full.md"));
    let counts = session.counts();
    assert_eq!(counts.words, count_words(&opened.body));
    assert_eq!(counts.chars, opened.body.chars().count());
    let export = session.export();
    assert!(count_words(&export) < counts.words);
}
