use criterion::{black_box, criterion_group, criterion_main, Criterion};
use markdown::Options;
use terraphim_editor;

const BENCHMARK_TEXT: &str = r#"# Heading 1
## Heading 2
### Heading 3

This is a paragraph with **bold** and *italic* text.

- List item 1
- List item 2
  - Nested item 1
  - Nested item 2

1. Ordered item 1
2. Ordered item 2

> This is a blockquote

```rust
fn hello_world() {
    println!("Hello, World!");
}
```

[Link](https://example.com)

| Table | Header |
|-------|--------|
| Cell 1 | Cell 2 |
"#;

fn benchmark_markdown(c: &mut Criterion) {
    let options = Options::default();
    
    c.bench_function("markdown_conversion", |b| {
        b.iter(|| {
            markdown::to_html_with_options(black_box(BENCHMARK_TEXT), &options)
        })
    });
}

criterion_group!(benches, benchmark_markdown);
criterion_main!(benches);