# Blocks view fixtures (issue #19)

Markdown sources for the Blocks view round-trip tests in `tests/web_blocks.rs`.
`BlocksView.serialise(BlocksView.parse(text))` must reproduce each file byte
for byte, and the block types must match the expectations in the test.

- `mixed.md`: leading and trailing blank lines, ATX and setext headings,
  paragraphs, tight and loose lists, a code fence holding blank and `#` lines,
  a blockquote with lazy continuation, a pipe table, a thematic break,
  indented code and non-BMP text with trailing spaces.
- `edge.md`: no trailing newline, a heading interrupting a paragraph and an
  unclosed tilde fence running to the end of the file.

The persistence fixtures in `tests/fixtures/persistence/` are reused for the
annotation-preservation tests.
