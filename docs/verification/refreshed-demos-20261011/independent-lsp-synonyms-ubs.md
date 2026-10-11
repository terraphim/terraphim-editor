# Independent LSP synonym scanner review

11 October 2026. Read-only source review; no browser/native execution.

UBS is **not green**: targeted four-Rust-file scan exited1 with5 critical,281 warning and128 info findings. The hook's contradictory “No critical issues found” text must not replace these recorded results.

Verbose log `/private/tmp/lsp-synonyms-ubs-verbose-20261011.log`, SHA-256 `d1a2aaba5a15655ad1f80f87a8230c9a0b56323a5125b139c663806ba6b57135`; summary JSON `/private/tmp/lsp-synonyms-ubs-20261011.json`, SHA-256 `4fdec7e22d638cfb3cc7b76817fd2f4651c2be34040d1eae3b6f0cd97462b0f6`.

All five critical examples are intentional test-helper failure assertions in `crates/terraphim_lsp/tests/hover_provenance_tests.rs`: line81 missing fixture text,119 unexpected hover kind,165 unexpected unversioned edit shape,226 unexpected report shape,406 unexpected action shape. Each exact panic expression is present before commit `c94c71037`; none is introduced by its new synonym regression. These fail tests when output has the wrong shape, rather than adding production panic paths. Classify these five as test-context false positives for the generic “avoid panic in library code” rule, retaining scanner exit/counts.

Allocation warnings at server.rs821,1582,2346 concern an error message in the existing Lab retry flow, one human-readable Keep label per existing candidate, and three existing modal-button titles. They are heuristic performance observations on baseline code, not identified new synonym correctness failures. This is focused triage of reported examples; it does not certify all281 warnings as harmless or claim a clean scanner run. The targeted shadow workspace skipped Cargo-based checks, so UBS is not evidence of compiler/Clippy status.

Updated synonym regression now invokes the actual ADD_ALTERNATIVE server command, applies its versioned output, confirms no membership before didChange, then queries actual server hover after committed didChange. It checks same-server didClose/didOpen plus a fresh server, simulated undo/redo and removal. This improves the earlier fixture-only test. Its “saved” value is serializer output, not an actual filesystem save, and undo/redo are delivered didChange messages, not native gestures. Native Zed/visual/all41 acceptance remain pending.
