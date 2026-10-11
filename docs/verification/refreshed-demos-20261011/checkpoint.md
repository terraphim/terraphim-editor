# Refreshed output verification — 11 October 2026

**Current work is incomplete. All 41 requirements are not accepted across all four hosts.** Two refreshed browser recordings are complete; native Safari and Zed recordings remain blocked by the locked Mac. Earlier recordings remain historical evidence and are not promoted to verification of today's fixes. No Zed fork changes were made.

## Completed outputs

Chrome: 181.866667 seconds, 134 PASS assertions and six explicitly unqualified Lab observations. Firefox: 181 seconds, 134 PASS assertions and six observations. Both recordings use independently admitted snapshot `86f1291d36df50a694293a4c670c0f4961b83c7a` and asset manifest SHA-256 `b4287a48bae6211fce7c7fe350c96b6722b694f18f3f548d3329029f87dcce42`. They show real UI-created alternatives, article swaps, sentence/paragraph alternatives, AI alternatives and Undo, ghosts, Overflow moves and copies, six Lab actions, five trim levels, review/Keep/cuts/Done, API save/reopen and clean export. The API persistence bridge is disclosed; it is not native file-picker proof.

- [Chrome video](</Users/alex/.codex/visualizations/2026/10/09/01a1221d-a378-7a00-8943-4c42925945f0/refreshed-functionality-demos-20261011/chrome-alternatives-guided-demo-2026-10-11-synonyms-take2.mp4>) — SHA-256 `9d3cc0ec0b79219e9bf6aaa298d3ef159f3db201456a7da65e4c8f5a6d5fe923`.
- [Firefox video](</Users/alex/.codex/visualizations/2026/10/09/01a1221d-a378-7a00-8943-4c42925945f0/refreshed-functionality-demos-20261011/firefox-alternatives-guided-demo-2026-10-11-synonyms.mp4>) — SHA-256 `bbf7b9e6b2a6e254f5067201ea77fca9243f7929650ea3d27f70e646de95d245`.
- [Independent Chrome 41-row review](independent-all-41-chrome.md) and [Firefox 41-row review](independent-all-41-firefox.md) distinguish finite checked outputs from every unmet clause. Sampled source-video comparisons and approximate navigation timestamps are included. Exact capture lead-in was not recorded; timestamps are not claimed as exact synchronization.

Both recordings prove `synonyms:: paperclip, eraser, thumbtack` after actual UI creation. Queries through the real shared WASM return all three aliases under the same typed, span-local concept. Undo removes the authored group and its membership; a separate role-derived indicator can remain. Reopen restores the authored membership. Registration is document-scoped and derived from committed annotations, rather than an unacknowledged proposal or global role mutation.

The additional [Chrome](chrome-literal-lab-final-20261011.json) and [Firefox](firefox-literal-lab-final-20261011.json) reports each contain twelve passing literal UI checks: one positive and one negative example per Lab action, exact UTF-16 ranges/text and unchanged source. These independently authored examples cover typo boundaries, long-sentence threshold, complex clauses versus enumerations, professional-register terms, hedge/filler lists and protected paragraph openers. They establish finite deterministic behavior, not universal semantic suitability. The runner is `demos/verify-lab-outputs.py`; its three contract tests pass. Reports retain the executed runner hash and admitted asset hash.

## LSP synonym integration fixed

[Terraphim AI draft PR 3484](https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3484) includes `c94c71037` (actual stored document synonym groups in LSP hover) and `f2e004ea7` (real filesystem save/reopen regression). The existing Terraphim alternatives crate is pinned to editor commit `9a1b901`; the implementation uses the crate's `synonyms::` parsing and Terraphim types. No parallel synonym representation was introduced.

The regression invokes the actual server add-alternative command, verifies proposed edits do not register membership, applies its versioned edit through didChange, and checks actual hover output. It writes a real `.md` file and reads its bytes before same-server close/open and fresh-server reopen. Simulated Undo/Redo use didChange and remove/restore membership; native Zed history gestures remain a separate acceptance step. [Returned hover](zed-synonyms-actual-server-output-20261011.log) includes both `decision → concept 1` and `judgment → concept 1` with literal `synonyms:: decision, Judgment`.

The full server/core run before the file-only regression strengthening passed 416 tests with none failed or ignored. The final focused server run passed 13 tests; formatting and warnings-denied Clippy passed. Required repository hooks also passed, including workspace tests with the local Docker backend available. [Full final hook output](lsp-filesystem-commit-hook-20261011.log) is retained.

UBS itself is not green: five critical findings, 281 warnings, 128 informational findings and exit 1. Independent review classifies all five critical examples as pre-existing intentional test assertion panics, not introduced production panic paths. It does not clear all 281 warnings. See [scanner triage](independent-lsp-synonyms-ubs.md). The hook's contradictory success text is not used to override the scanner result.

## Remaining execution

Native retries continued to report a locked Mac; no new native input or recording success is claimed. The isolated public Zed demo project and reviewed server binary are prepared, and the window recorder allowlist self-test passed five checks. After unlock, qualify immediate-save anchors, Done/save/reopen ghost retention, repeated clean export into an already open target, and authored synonym hover/history in the existing extension. Then record native Safari, reproducing and fixing or faithfully retaining its trusted-drag failures. The user's recording/review approvals already apply.

Stock Zed still has unsupported custom visual UI requirements. Equivalent LSP output is not a pass for dots, side panels, opacity or a Lab popover. The separate fork remains outside this delivery. The historical [cross-host ledger](../independent-requirement-ledger-20261010.md) retains the native failures and unsupported clauses until actual new native outputs replace them.

Failed Chrome take 1, the first incomplete literal-Lab run (panel toggle harness error), the missing-test-dependency compile failure, and scanner findings remain beside the successful evidence. None is relabeled as a product success.
