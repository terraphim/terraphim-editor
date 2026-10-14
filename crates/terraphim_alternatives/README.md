# terraphim_alternatives

The shared document model used by [Terraphim Editor](https://github.com/terraphim/terraphim-editor) and its language server. It stores authored alternatives, ghosted text and overflow in a trailing Markdown annotation block, with UTF-16 offsets and re-anchoring after edits.

Alternatives use the existing Terraphim `synonyms::` parser and typed thesaurus entries. Export creates an explicit sharing snapshot; ambiguous terms in separate groups are rejected rather than silently combining concepts.

```rust
use terraphim_alternatives::{Document, Source, SpanKind, parse, write};
use terraphim_types::NormalizedTermValue;

let mut document = Document::new("Risk matters.");
let span = document.add_span(SpanKind::Word, 0, 4).unwrap();
document.add_alternative(&span, "Concern", Source::Human, None).unwrap();
let reopened = parse(&write(&document)).unwrap();
let graph = reopened.export_synonym_thesaurus().unwrap();
let risk = graph.get(&NormalizedTermValue::from("risk")).unwrap();
let concern = graph.get(&NormalizedTermValue::from("concern")).unwrap();
assert_eq!(risk.id, concern.id);
```

The crate has no DOM or network runtime and builds for native Rust and WebAssembly. Hosts own their history and persistence; the model supplies serialized state and mutation operations. Run `cargo test -p terraphim_alternatives` for annotation round trips, synonym registration/export, text boundaries, moves and re-anchoring.

Licensed under MIT; see [LICENSE](LICENSE).
