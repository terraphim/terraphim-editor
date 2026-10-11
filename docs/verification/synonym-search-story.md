# Synonym sharing verification — 2026-10-11

User-approved story: create Risk/Concern alternatives, swap words, share actual document graph, search the exported brief by Concern, compare synthesis off/on.

Shared-crate export tests:4pass, including separate IDs, collision refusal, exact typed comma/multiline alternatives and direct-parse stale attachment. Session tests:2pass for swap/reopen/snapshot restoration/detachment and malformed blocks. Existing110library tests and4actual RoleGraph registration tests pass. Strict native Clippy passed. WASM/worker release build succeeded; Trunk emits an existing deprecated address-field warning.

Actual Chrome interaction test tests/browser/synonym-sharing.cjs: add through visible panel, native Ctrl+Z/Ctrl+Shift+Z removes/restores real graph membership, swap both ways, reopen annotated source, inspect actual synonyms:: Risk, Concern, download graph JSON and clean Markdown, assert graph identity and absence of literal Concern in saved prose. PASS. This browser history result is distinct from the Rust snapshot-restoration test.

Actual CLI binary from reviewed clients commit0898db6fcaf617757669fda8bf746e7b75cde207: old role0matches; downloaded new graph3actual excerpts; --answer produces an answer from the same3chunks. A first one-risk draft was correctly refused as insufficient evidence; the authored final example contains3distinct risk/decision pairs. Final answer grounding review and new media verification are still pending.

Source review found and resolved a shared-crate stale-anchor export issue. No remaining source blocker reported. Zed fork untouched; this work does not establish all41host requirements.
