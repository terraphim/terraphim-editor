//! Benchmarks for the hot paths: parse/write of the block, re-anchoring after
//! an edit, and swapping the active alternative.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use terraphim_alternatives::{Document, Source, SpanKind, parse, write};

/// A ~3,000-word document with one span every 20 words.
fn large_document() -> Document {
    let sentence = "The tension between speed and care is a paperclip holding the draft together. ";
    let body = sentence.repeat(220);
    let mut doc = Document::new(body);
    let sentence_units = sentence.encode_utf16().count();
    for i in 0..220 {
        let start = i * sentence_units + 4;
        let id = doc.add_span(SpanKind::Word, start, start + 7).unwrap();
        doc.add_alternative(&id, "pressure", Source::Human, None)
            .unwrap();
        doc.add_alternative(&id, "struggle", Source::Ai, Some("bench".into()))
            .unwrap();
    }
    doc.annotations.overflow = "Stashed paragraph. ".repeat(50);
    doc
}

fn benches(c: &mut Criterion) {
    let doc = large_document();
    let saved = write(&doc);

    c.bench_function("write_220_spans", |b| b.iter(|| write(black_box(&doc))));
    c.bench_function("parse_220_spans", |b| {
        b.iter(|| parse(black_box(&saved)).unwrap())
    });

    c.bench_function("reanchor_after_prefix_insert", |b| {
        b.iter_batched(
            || {
                let mut d = doc.clone();
                d.body.insert_str(0, "Inserted heading.\n\n");
                d
            },
            |mut d| d.reanchor(),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("set_active_first_span", |b| {
        b.iter_batched(
            || doc.clone(),
            |mut d| d.set_active("s1", 1).unwrap(),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("export_with_ghosts", |b| {
        let mut d = doc.clone();
        for i in (1..=220).step_by(3) {
            d.ghost_span(&format!("s{i}")).unwrap();
        }
        b.iter(|| black_box(&d).export())
    });
}

criterion_group!(alternatives, benches);
criterion_main!(alternatives);
