//! Benchmarks for the hot paths: parse/write of the block, re-anchoring after
//! an edit, and swapping the active alternative.

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use terraphim_alternatives::{Document, Source, SpanKind, parse, utf16_len, write};

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

/// 220 sentences, each holding a span over a word no other sentence uses, plus
/// a ghost over every third sentence. Re-anchoring after a prefix insert moves
/// every item through rule 2, the case where one automaton pass over the body
/// replaces one scan per distinct anchor text.
fn distinct_document() -> Document {
    let mut body = String::new();
    let mut spans = Vec::new();
    let mut sentences = Vec::new();
    for i in 0..220 {
        let sentence_start = body.encode_utf16().count();
        body.push_str("Sentence ");
        let start = body.encode_utf16().count();
        let word = format!("token{i:03}x");
        body.push_str(&word);
        spans.push((start, start + word.len()));
        body.push_str(" carries a unique word. ");
        sentences.push((sentence_start, body.encode_utf16().count() - 1));
    }
    let mut doc = Document::new(body);
    for (start, end) in spans {
        let id = doc.add_span(SpanKind::Word, start, end).unwrap();
        doc.add_alternative(&id, "marker", Source::Human, None)
            .unwrap();
    }
    for &(start, end) in sentences.iter().step_by(3) {
        doc.ghost(start, end).unwrap();
    }
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
            // Returned, so dropping the document is not timed.
            |mut d| {
                let report = d.reanchor();
                (d, report)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    let distinct = distinct_document();
    {
        // The scenario must exercise the placing path, not the error path.
        let mut d = distinct.clone();
        d.body.insert_str(0, "Inserted heading.\n\n");
        let report = d.reanchor();
        assert!(report.is_clean());
        assert_eq!(report.moved.len(), 220 + 74);
    }
    c.bench_function("reanchor_distinct_220_spans_74_ghosts", |b| {
        b.iter_batched(
            || {
                let mut d = distinct.clone();
                d.body.insert_str(0, "Inserted heading.\n\n");
                d
            },
            // Returned, so dropping the document is not timed.
            |mut d| {
                let report = d.reanchor();
                (d, report)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("set_active_first_span", |b| {
        b.iter_batched(
            || doc.clone(),
            |mut d| {
                d.set_active("s1", 1).unwrap();
                d
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // A keystroke just before a span in the middle of the document: later
    // anchors shift and the context of the anchors near it is refreshed.
    c.bench_function("apply_edit_keystroke_near_span", |b| {
        let at = doc.span("s110").unwrap().anchor.start - 1;
        b.iter_batched(
            || doc.clone(),
            |mut d| {
                d.apply_edit(at, at, "x").unwrap();
                d
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Block reordering (issue #44): move the first 1,000 code units (about 13
    // sentences, each with a span) to the end of the ~17,000-unit document,
    // shifting every other anchor and refreshing context over the whole body.
    c.bench_function("move_range_block_to_end", |b| {
        let end = doc.span("s14").unwrap().anchor.start - 4;
        let to = utf16_len(&doc.body);
        b.iter_batched(
            || doc.clone(),
            |mut d| {
                d.move_range(0, end, to).unwrap();
                d
            },
            criterion::BatchSize::SmallInput,
        )
    });

    // Moving one sentence past its neighbour: the common Blocks view case.
    c.bench_function("move_range_sentence_down", |b| {
        let start = doc.span("s110").unwrap().anchor.start - 4;
        let end = doc.span("s111").unwrap().anchor.start - 4;
        let to = doc.span("s112").unwrap().anchor.start - 4;
        b.iter_batched(
            || doc.clone(),
            |mut d| {
                d.move_range(start, end, to).unwrap();
                d
            },
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

    // The Write_On counter runs on every edit (issue #59): one pass over the
    // ~3,000-word body with the shared word definition.
    c.bench_function("counts_3000_words", |b| b.iter(|| black_box(&doc).counts()));
}

criterion_group!(alternatives, benches);
criterion_main!(alternatives);
