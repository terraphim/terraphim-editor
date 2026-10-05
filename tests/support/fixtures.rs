//! Persistence fixtures (issue #6) and their golden preview/export, shared
//! by `tests/web_persistence.rs` and `tests/web_persistence_chrome.rs`.

use super::*;

pub const FIXTURE_NAMES: [&str; 3] = ["plain", "full", "malformed"];

/// Fixture documents and their golden preview/export (see
/// `tests/fixtures/persistence/README.md`).
pub fn fixture(name: &str) -> (&'static str, &'static str, &'static str) {
    match name {
        "plain" => (
            include_str!("../fixtures/persistence/plain.md"),
            include_str!("../fixtures/persistence/plain.preview.html"),
            include_str!("../fixtures/persistence/plain.export.md"),
        ),
        "full" => (
            include_str!("../fixtures/persistence/full.md"),
            include_str!("../fixtures/persistence/full.preview.html"),
            include_str!("../fixtures/persistence/full.export.md"),
        ),
        "malformed" => (
            include_str!("../fixtures/persistence/malformed.md"),
            include_str!("../fixtures/persistence/malformed.preview.html"),
            include_str!("../fixtures/persistence/malformed.export.md"),
        ),
        other => panic!("unknown fixture {other}"),
    }
}

/// Make the fixtures available to JavaScript as `window.teFixtures`.
pub fn install_fixtures() {
    let mut src = String::from("window.teFixtures = {");
    for name in FIXTURE_NAMES {
        let (md, preview, export) = fixture(name);
        src.push_str(&format!(
            "{name}: {{ md: {}, preview: {}, export: {} }},",
            js_string_literal(md),
            js_string_literal(preview),
            js_string_literal(export)
        ));
    }
    src.push_str("}; 'ok'");
    assert_eq!(js_string(&src), "ok");
}
