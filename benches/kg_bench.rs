//! Benchmarks for the knowledge-graph alternatives bridge (issue #13).
//!
//! The indicators read the derived KG spans on a debounce after typing, so
//! `kg_spans` on an unchanged body (cached) and after an edit (a fresh
//! analysis) are the paths that matter, plus one swap.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use terraphim_editor::{kg_spans, kg_swap, load_thesaurus, DocumentSession};

const THESAURUS: &str = include_str!("../tests/fixtures/kg/thesaurus.json");

/// About 5,000 words, one KG term in every five words or so.
fn large_body(variant: &str) -> String {
    let sentence = "Every choice we make is a judgment; use an eraser in a café when needed. ";
    format!("{variant}\n\n{}", sentence.repeat(360))
}

fn bench_kg(c: &mut Criterion) {
    load_thesaurus(THESAURUS).expect("fixture thesaurus loads");
    let mut a = DocumentSession::new();
    a.open(&large_body("A"));
    let mut b = DocumentSession::new();
    b.open(&large_body("B"));
    let terms = kg_spans(&a).len();
    assert!(terms > 1000, "{terms} KG spans");

    c.bench_function("kg_spans_5k_words_fresh", |bench| {
        // Alternating two bodies defeats the cache: every call analyses.
        let mut flip = false;
        bench.iter(|| {
            flip = !flip;
            black_box(kg_spans(black_box(if flip { &a } else { &b })).len())
        })
    });

    c.bench_function("kg_spans_5k_words_cached", |bench| {
        bench.iter(|| black_box(kg_spans(black_box(&a)).len()))
    });

    c.bench_function("kg_swap_5k_words", |bench| {
        let mut to = 0;
        bench.iter(|| {
            to = 1 - to;
            black_box(kg_swap(&mut a, "kg-2-0", to).expect("swap"))
        })
    });
}

criterion_group!(benches, bench_kg);
criterion_main!(benches);
