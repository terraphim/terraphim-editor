# Persistence golden files (issue #6)

Each `<name>.md` is a document as stored on disk. For every one:

* `<name>.preview.html` is the Markdown preview of the body (the annotation
  block is never rendered);
* `<name>.export.md` is the clean export (active alternatives, ghosted text
  dropped, no overflow, no block).

Both are asserted natively by `tests/persistence.rs` and in a real browser by
`tests/web.rs`. To regenerate after an intended change, run
`UPDATE_GOLDENS=1 cargo test --test persistence` and review the diff.
