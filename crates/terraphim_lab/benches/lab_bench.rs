//! Per-action timings on a ~5,000-word document (the three fixtures repeated),
//! with the fixture role (thesaurus and rolegraph) selected.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use terraphim_automata::load_thesaurus_from_json;
use terraphim_lab::{LabAction, LabConfig, RoleGraphData, RoleKnowledge, mark, mark_all};

const FIXTURES: [&str; 3] = [
    include_str!("../tests/fixtures/three-men-ch1.md"),
    include_str!("../tests/fixtures/walden-economy.md"),
    include_str!("../tests/fixtures/zed-plugin-fit.md"),
];

fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}

/// Concatenate the fixtures until the document holds at least 5,000 words.
fn document() -> String {
    let mut body = String::new();
    'outer: loop {
        for f in FIXTURES {
            if !body.is_empty() {
                body.push_str("\n\n");
            }
            body.push_str(f);
            if word_count(&body) >= 5_000 {
                break 'outer;
            }
        }
    }
    body
}

fn build_config() -> LabConfig {
    let thesaurus =
        load_thesaurus_from_json(include_str!("../tests/fixtures/role-thesaurus.json")).unwrap();
    let graph = RoleGraphData::from_json(include_str!("../tests/fixtures/rolegraph.json")).unwrap();
    LabConfig::with_defaults()
        .unwrap()
        .with_role(RoleKnowledge::new(&thesaurus, graph).unwrap())
}

fn bench(c: &mut Criterion) {
    let body = document();
    let config = build_config();
    eprintln!(
        "lab_bench document: {} words, {} bytes",
        word_count(&body),
        body.len()
    );
    let mut group = c.benchmark_group("lab_5000_words");
    for action in LabAction::ALL {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{action:?}")),
            &action,
            |b, &action| b.iter(|| mark(black_box(&body), &config, action)),
        );
    }
    group.bench_function("mark_all", |b| {
        b.iter(|| mark_all(black_box(&body), &config))
    });
    group.finish();

    c.bench_function("config_with_role", |b| b.iter(build_config));
}

criterion_group!(benches, bench);
criterion_main!(benches);
