//! Round-trip and malformed-block acceptance tests against real fixture files.
//!
//! Fixtures are embedded with `include_str!` so the tests need no file system
//! access and can also run under wasm-bindgen-test later.

use terraphim_alternatives::{
    Alternative, Anchor, BlockErrorKind, Document, Source, SpanKind, parse, write,
};

const FULL: &str = include_str!("fixtures/full.md");
const GHOST_ONLY: &str = include_str!("fixtures/ghost_only.md");
const OVERFLOW_ONLY: &str = include_str!("fixtures/overflow_only.md");
const PLAIN: &str = include_str!("fixtures/plain.md");
const EXAMPLE_BLOCK_AT_END: &str = include_str!("fixtures/example_block_at_end.md");
const TWO_EXAMPLE_BLOCKS: &str = include_str!("fixtures/two_example_blocks.md");

const TRUNCATED: &str = include_str!("fixtures/malformed/truncated.md");
const INVALID_JSON: &str = include_str!("fixtures/malformed/invalid_json.md");
const UNKNOWN_VERSION: &str = include_str!("fixtures/malformed/unknown_version.md");
const DUPLICATE_IDS: &str = include_str!("fixtures/malformed/duplicate_ids.md");
const OVERLAPPING: &str = include_str!("fixtures/malformed/overlapping.md");
const OVERLAPPING_GHOSTS: &str = include_str!("fixtures/malformed/overlapping_ghosts.md");
const DUPLICATE_GHOST_ID: &str = include_str!("fixtures/malformed/duplicate_ghost_id.md");

const FIXTURES: [(&str, &str); 6] = [
    ("full", FULL),
    ("ghost_only", GHOST_ONLY),
    ("overflow_only", OVERFLOW_ONLY),
    ("plain", PLAIN),
    ("example_block_at_end", EXAMPLE_BLOCK_AT_END),
    ("two_example_blocks", TWO_EXAMPLE_BLOCKS),
];

#[test]
fn fixtures_are_byte_stable() {
    for (name, source) in FIXTURES {
        let doc = parse(source).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(write(&doc), source, "{name}: write(parse(f)) != f");
    }
}

#[test]
fn parse_of_write_is_identity_for_fixture_documents() {
    for (name, source) in FIXTURES {
        let doc = parse(source).unwrap();
        assert_eq!(parse(&write(&doc)).unwrap(), doc, "{name}");
    }
}

#[test]
fn full_fixture_has_the_expected_content() {
    let doc = parse(FULL).unwrap();
    assert!(doc.body.starts_with("# Why isn't everything obvious?\n"));
    assert!(
        doc.body.ends_with("repeats itself.\n"),
        "separator stripped exactly"
    );
    assert!(!doc.body.contains("terraphim-alternatives"));
    let spans = &doc.annotations.spans;
    assert_eq!(spans.len(), 3);
    assert_eq!(spans[0].kind, SpanKind::Sentence);
    assert_eq!(spans[1].active_alternative().text, "struggle");
    assert_eq!(
        spans[1].active_alternative().model.as_deref(),
        Some("llama3")
    );
    // Ghost layer: g1 covers the word span s3 ("eraser") and its context, g2
    // a whole paragraph that has no span at all.
    let ghosts = &doc.annotations.ghosts;
    assert_eq!(ghosts.len(), 2);
    assert_eq!(ghosts[0].anchor.text, "draft 𝄞 is an eraser");
    assert!(ghosts[0].anchor.start < spans[2].anchor.start);
    assert_eq!(ghosts[0].anchor.end, spans[2].anchor.end);
    assert!(ghosts[1].anchor.text.starts_with("This whole paragraph"));
    assert!(
        doc.annotations.overflow.contains("```rust"),
        "backticks unescaped on read"
    );
    // Every stored anchor matches the body at its UTF-16 offsets.
    let units: Vec<u16> = doc.body.encode_utf16().collect();
    let anchors = spans
        .iter()
        .map(|s| (&s.id, &s.anchor))
        .chain(ghosts.iter().map(|g| (&g.id, &g.anchor)));
    for (id, anchor) in anchors {
        let text = String::from_utf16(&units[anchor.start..anchor.end]).unwrap();
        assert_eq!(text, anchor.text, "{id}");
    }
}

#[test]
fn full_fixture_exports_without_ghosted_text() {
    let doc = parse(FULL).unwrap();
    assert_eq!(
        doc.export(),
        "# Why isn't everything obvious?\n\nThe struggle in a café holding `code` together.\n\n"
    );
}

#[test]
fn plain_markdown_parses_to_empty_annotations() {
    let doc = parse(PLAIN).unwrap();
    assert_eq!(doc.body, PLAIN);
    assert!(doc.annotations.is_empty());
}

#[test]
fn fenced_examples_of_the_format_stay_in_the_body() {
    // Issue #26: a document describing the format ends in a valid, empty
    // block. That block is body text, not annotations, and must not be lost.
    for (name, source) in [
        ("example_block_at_end", EXAMPLE_BLOCK_AT_END),
        ("two_example_blocks", TWO_EXAMPLE_BLOCKS),
    ] {
        let doc = parse(source).unwrap();
        assert_eq!(doc.body, source, "{name}: whole source is body");
        assert!(doc.annotations.is_empty(), "{name}");
    }
}

#[test]
fn annotating_a_document_with_a_trailing_example_keeps_the_example() {
    let mut doc = parse(EXAMPLE_BLOCK_AT_END).unwrap();
    // "# Annotation format": "format" is UTF-16 units 13..19.
    let id = doc.add_span(SpanKind::Word, 13, 19).unwrap();
    doc.add_alternative(&id, "layout", Source::Human, None)
        .unwrap();
    let written = write(&doc);
    let reread = parse(&written).unwrap();
    assert_eq!(reread, doc);
    assert_eq!(reread.body, EXAMPLE_BLOCK_AT_END);
}

#[test]
fn programmatic_documents_round_trip() {
    let mut doc = Document::new("A 𝄞 clef, a café and a paperclip.\r\n");
    // "A 𝄞 clef, a café and a " is 24 UTF-16 units (the clef is two).
    let id = doc.add_span(SpanKind::Word, 24, 33).unwrap();
    assert_eq!(doc.span(&id).unwrap().anchor.text, "paperclip");
    doc.add_alternative(&id, "eraser", Source::Human, None)
        .unwrap();
    doc.add_alternative(&id, "`tick`", Source::Ai, Some("m".into()))
        .unwrap();
    doc.set_active(&id, 2).unwrap();
    doc.annotations.overflow = "line one\nline two ``` end".into();
    let written = write(&doc);
    assert_eq!(parse(&written).unwrap(), doc);
    assert_eq!(write(&parse(&written).unwrap()), written);
}

fn assert_recoverable(source: &str, expected_body: &str) -> BlockErrorKind {
    let err = parse(source).unwrap_err();
    assert_eq!(err.body, expected_body, "body is kept");
    assert_eq!(
        format!("{}{}", err.body, err.raw_block),
        source,
        "body + raw_block must reproduce the source exactly"
    );
    assert!(err.raw_block.contains("```terraphim-alternatives"));
    assert!(!err.to_string().is_empty());
    err.kind
}

#[test]
fn truncated_block_is_recoverable() {
    let kind = assert_recoverable(TRUNCATED, "Body survives truncation.");
    assert_eq!(kind, BlockErrorKind::Truncated);
}

#[test]
fn invalid_json_is_recoverable_with_position() {
    let kind = assert_recoverable(INVALID_JSON, "Body survives bad JSON.");
    match kind {
        BlockErrorKind::InvalidJson { line, column, .. } => {
            assert_eq!(line, 3);
            assert!(column > 0);
        }
        other => panic!("expected InvalidJson, got {other:?}"),
    }
}

#[test]
fn unknown_version_is_reported_before_schema_checks() {
    let kind = assert_recoverable(UNKNOWN_VERSION, "Body survives a future version.");
    assert_eq!(kind, BlockErrorKind::UnknownVersion { found: 2 });
}

#[test]
fn duplicate_span_ids_are_rejected() {
    let kind = assert_recoverable(DUPLICATE_IDS, "The tension and the eraser.");
    assert_eq!(kind, BlockErrorKind::DuplicateId { id: "s1".into() });
}

#[test]
fn ghost_reusing_a_span_id_is_rejected() {
    let kind = assert_recoverable(DUPLICATE_GHOST_ID, "The tension and the eraser.");
    assert_eq!(kind, BlockErrorKind::DuplicateId { id: "s1".into() });
}

#[test]
fn overlapping_ghosts_are_reported_not_accepted() {
    let kind = assert_recoverable(OVERLAPPING_GHOSTS, "The tension and the eraser.");
    assert_eq!(
        kind,
        BlockErrorKind::OverlappingGhosts {
            first: "g1".into(),
            second: "g2".into(),
        }
    );
}

#[test]
fn overlapping_spans_are_reported_not_accepted() {
    let kind = assert_recoverable(OVERLAPPING, "The tension and the eraser.");
    assert_eq!(
        kind,
        BlockErrorKind::OverlappingSpans {
            first: "s2".into(),
            second: "s1".into(),
        }
    );
}

#[test]
fn structurally_invalid_span_is_reported_with_its_id() {
    let mut doc = Document::new("The tension.");
    let id = doc.add_span(SpanKind::Word, 4, 11).unwrap();
    doc.annotations.spans[0]
        .alts
        .push(Alternative::new("pressure", Source::Human));
    doc.annotations.spans[0].active = 9;
    let err = parse(&write(&doc)).unwrap_err();
    assert!(matches!(err.kind, BlockErrorKind::InvalidSpan { id: ref got, .. } if *got == id));
}

#[test]
fn hand_edited_body_is_not_a_parse_error() {
    // Anchor drift is the re-anchoring code's job, not the parser's.
    let edited = FULL.replacen("# Why", "# So, why", 1);
    let doc = parse(&edited).unwrap();
    assert_eq!(doc.annotations.spans.len(), 3);
    assert_eq!(doc.annotations.ghosts.len(), 2);
}

// ----- context (decision 2026-10-05: context re-anchoring) -----------------

/// `full.md` and `ghost_only.md` as written before anchors stored context.
const LEGACY_FULL: &str = include_str!("fixtures/legacy_no_context.md");
const LEGACY_GHOST_ONLY: &str = include_str!("fixtures/legacy_ghost_only.md");
const CONTEXT_WRONG_TYPE: &str = include_str!("fixtures/malformed/context_wrong_type.md");

fn anchors(doc: &Document) -> Vec<&Anchor> {
    doc.annotations
        .spans
        .iter()
        .map(|s| &s.anchor)
        .chain(doc.annotations.ghosts.iter().map(|g| &g.anchor))
        .collect()
}

#[test]
fn fixtures_with_context_store_it_on_every_anchor() {
    for (name, source) in [("full", FULL), ("ghost_only", GHOST_ONLY)] {
        let doc = parse(source).unwrap();
        for anchor in anchors(&doc) {
            assert!(anchor.before.is_some() && anchor.after.is_some(), "{name}");
        }
    }
    let doc = parse(FULL).unwrap();
    let s1 = &doc.annotations.spans[0].anchor;
    assert_eq!(
        s1.before.as_deref(),
        Some("# "),
        "cut by the document start"
    );
    let g2 = &doc.annotations.ghosts[1].anchor;
    assert_eq!(g2.after.as_deref(), Some("\n"), "cut by the document end");
    let s3 = &doc.annotations.spans[2].anchor;
    assert_eq!(
        s3.before.as_deref(),
        Some(" in a café draft 𝄞 is an "),
        "multi-byte and astral text, trimmed to a word boundary"
    );
}

#[test]
fn legacy_files_load_without_context_and_gain_it_on_save() {
    for (name, legacy, current) in [
        ("full", LEGACY_FULL, FULL),
        ("ghost_only", LEGACY_GHOST_ONLY, GHOST_ONLY),
    ] {
        let doc = parse(legacy).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(anchors(&doc).iter().all(|a| !a.has_context()), "{name}");
        assert!(!legacy.contains("\"before\""), "{name}: legacy bytes");
        let saved = write(&doc);
        assert_eq!(saved, current, "{name}: saving stores context");
        assert_eq!(write(&parse(&saved).unwrap()), saved, "{name}: then stable");
    }
}

#[test]
fn legacy_file_reanchors_as_before_after_a_hand_edit() {
    let edited = LEGACY_FULL.replacen("# Why", "Preface.\n\n# Why", 1);
    let mut legacy = parse(&edited).unwrap();
    let report = legacy.reanchor();
    assert!(report.is_clean(), "{report:?}");
    assert_eq!(report.moved.len(), 5, "every anchor shifted by the edit");
    // The same edit on the file with context gives the same placements, and
    // the placed legacy anchors gain the same context.
    let mut current = parse(&FULL.replacen("# Why", "Preface.\n\n# Why", 1)).unwrap();
    assert!(current.reanchor().is_clean());
    assert_eq!(legacy, current);
}

#[test]
fn context_of_the_wrong_type_is_recoverable() {
    let kind = assert_recoverable(CONTEXT_WRONG_TYPE, "The tension and the eraser.");
    assert!(
        matches!(kind, BlockErrorKind::InvalidSchema { ref message } if message.contains("string")),
        "{kind:?}"
    );
}
