# UBS static analysis at the final commits

Run on 2026-10-08 with UBS Meta-Runner v5.3.13, default profile. Totals are from `ubs --format=jsonl`; locations from the text report and checked with `rg` against the source.

| Repository and scope | Commit | Critical | Warning | Info | Critical in product code |
|---|---|---|---|---|---|
| terraphim-editor `src crates public/js` | `84cf119` | 11 | 2708 | 2733 | 0 |
| terraphim-ai `crates/terraphim_lsp crates/terraphim_lsp_core` | `aafcf6f5e` (rc.3) | 77 | 3109 | 468 | 0 |
| zed-terraphim `src` | `26fced4` | 3 | 302 | 94 | 0 |

## terraphim-editor (11)

The same findings as the Phase 4 scan at `0e5d8af` (12), less U-12 (the fixture script's file handle), which #103 fixed. Line numbers have moved.

| Finding | Location | Verdict |
|---|---|---|
| Loose equality | public/js/editor.js:1336 | False positive: `text == null` deliberately matches null and undefined. |
| Secret compared with `===` (2) | public/js/indicators.js:255, :986 | False positive: render-cache signatures and a layout guard, no credential. |
| `panic!` (6) | crates/terraphim_alternatives/tests/block_round_trip.rs:37, :178; crates/terraphim_alternatives/tests/reanchor_matcher.rs:228; crates/terraphim_lab/tests/trim.rs:622, :676; crates/terraphim_lab/tests/common/mod.rs:45 | False positive: all in test code. |
| JWT decode or validation bypass (2) | crates/terraphim_alternatives/src/block.rs:225, :323 | False positive: `decode()` parses the annotation block's JSON; no JWT exists. |

## terraphim-ai, terraphim-lsp crates (77)

- 74 `panic!` macros, all in `crates/terraphim_lsp*/tests/` (assertion helpers). `rg` over both crates' `src/` finds one match, inside a doc-comment example at `crates/terraphim_lsp_core/src/reanchor.rs:61`.
- 3 "thread spawned without join" findings: `tests/lifecycle_tests.rs:35`, `tests/stdio_tests.rs:31`, `:160`. These are reader threads that end on EOF when the test stops the child process.

## zed-terraphim (3)

- 3 `panic!` macros at `src/provision.rs:1027`, `:1485`, `:1827`. All three come after `#[cfg(test)]` at line 787, so they are test assertions.
