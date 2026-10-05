//! Benchmarks for the persistence bridge (issue #6): opening and saving an
//! annotated document, and mirroring a keystroke into the span model, which
//! runs on every edit in the browser.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use terraphim_editor::DocumentSession;

const FIXTURE: &str = include_str!("../tests/fixtures/persistence/full.md");

/// About 5,000 words of body followed by the fixture's annotations, so
/// offsets and splices are measured on a realistic article length.
fn large_source() -> String {
    let filler = "The quick brown fox jumps over the lazy dog again. ".repeat(500);
    format!("{filler}\n\n{FIXTURE}")
}

fn bench_document(c: &mut Criterion) {
    let source = large_source();

    c.bench_function("document_open_5k_words", |b| {
        b.iter(|| {
            let mut session = DocumentSession::new();
            black_box(session.open(black_box(&source)));
        })
    });

    let mut session = DocumentSession::new();
    session.open(&source);
    c.bench_function("document_save_5k_words", |b| {
        b.iter(|| black_box(session.save()))
    });
    c.bench_function("document_export_5k_words", |b| {
        b.iter(|| black_box(session.export()))
    });

    c.bench_function("document_keystroke_at_start_5k_words", |b| {
        let mut session = DocumentSession::new();
        session.open(&source);
        b.iter(|| {
            session.apply_edit(0, 0, black_box("x")).unwrap();
            session.apply_edit(0, 1, "").unwrap();
        })
    });
}

criterion_group!(benches, bench_document);
criterion_main!(benches);
