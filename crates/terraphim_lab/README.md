# terraphim_lab

The deterministic writing-analysis engine used by [Terraphim Editor](https://github.com/terraphim/terraphim-editor) and its language server. It proposes typo corrections, highlights long or convoluted sentences, applies role-specific tone/hedge lists, and plans trims using knowledge-graph concepts and rolegraph scores.

Analysis borrows the original Markdown. It returns proposed spans and edits; the host decides which to apply. Offsets use UTF-16 code units so native and browser hosts can exchange the same ranges. Protected Markdown structures are excluded from prose edits.

```rust
use terraphim_lab::{LabAction, LabConfig, MarkKind, mark};

let config = LabConfig::with_defaults().unwrap();
let marks = mark("I recieve it.", &config, LabAction::TyposAndPunctuation);
assert_eq!(marks[0].kind, MarkKind::Typo);
assert_eq!(marks[0].proposal.as_deref(), Some("receive"));
```

Role lists support the Terraphim/Logseq-compatible `synonyms::` syntax. `LabConfig` accepts an existing role thesaurus and graph. The built-in lists and test fixtures are included in the package. The runtime has no DOM or network dependency and builds for native Rust and WebAssembly; native conformance tests use the real `terraphim_rolegraph` implementation.

Run `cargo test -p terraphim_lab` to check actions, rolegraph conformance, trim targets, protected ranges and text invariants.

Licensed under MIT; see [LICENSE](LICENSE).
