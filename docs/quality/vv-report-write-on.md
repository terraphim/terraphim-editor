# Verification and validation report: Write_On alternative control

Date: 2026-10-08. Scope: the requirements traced from the Write_On demo video (`docs/requirements/alternative-control.md`) through terraphim-editor, the terraphim-lsp server (terraphim-ai) and the zed-terraphim extension. Epic: terraphim/zed-terraphim#1.

## Verdict

**Verification (Phase 4): passed. Validation (Phase 5): passed, with stated deviations. Owner sign-off: pending.**

Every figure below names the commit it was measured on and the file that supports it: an evidence file under [`evidence/write-on/`](evidence/write-on/), or [`traceability.md`](../requirements/traceability.md) for the requirement verdicts.

| Measure | Result |
|---|---|
| terraphim-editor requirements (41) | 41 Done ([`traceability.md`](../requirements/traceability.md), summary table) |
| Zed requirements (41) | 15 Done (5 with rc.2, 10 with rc.3), 7 Partial, 2 Missing, 16 Not possible, 0 Untested, 1 N/A ([`traceability.md`](../requirements/traceability.md), summary table) |
| Acceptance test against the video (74 steps) | 73 Matches, 1 Deviates (by design), 0 Fails ([`acceptance-a5f3fbe.md`](evidence/write-on/acceptance-a5f3fbe.md)) |
| LSP protocol run with Zed's real client capabilities, against the released rc.3 binary | 32/32 ([`protocol-rc3-released.json`](evidence/write-on/protocol-rc3-released.json)); 31/32 on the pre-release build `5d7be04a1` ([`protocol-prerelease-5d7be04a1.json`](evidence/write-on/protocol-prerelease-5d7be04a1.json)), the failing exit step fixed in #3472 |
| Open P0/P1/P2 review findings | 0 across the 31 merged PRs ([`review-index.md`](evidence/write-on/review-index.md)) |

## Commits under test

| Repository | Commit | Notes |
|---|---|---|
| terraphim-editor | `84cf119` (main at the final run; main is now `50757cc`, which adds only #117's change to `scripts/ci` and the CI workflow) | final verification run 2026-10-08 11:58–12:47 BST ([`final-run-84cf119.md`](evidence/write-on/final-run-84cf119.md)); Gitea Actions `ci / build (push)` on this commit: success |
| terraphim-ai | `aafcf6f5e`, tag `terraphim_lsp-v1.22.0-rc.3` | core 1.23.0 published to the private registry |
| zed-terraphim | `26fced4` (main, #32) | pins rc.3; provisions from `terraphim/terraphim-lsp-releases` (tailnet), then GitHub, then PATH |

## Phase 4: verification

### terraphim-editor

- Static analysis at `84cf119` ([`ubs-final.md`](evidence/write-on/ubs-final.md)): 11 critical findings, all false positives. The Phase 4 scan at `0e5d8af` had 12; the twelfth, a script file handle, was real and was fixed in #103.
- Quality at `84cf119`: `cargo fmt --check` and workspace clippy with `-D warnings` clean ([`final-run-84cf119.md`](evidence/write-on/final-run-84cf119.md), as are the next three items).
- Native tests at `84cf119`: 393 passed, 0 failed across 76 test binaries (`cargo test --workspace`).
- Browser tests at `84cf119`: two full passes of `scripts/browser-tests.sh`, each 53 binaries and 171 tests, 0 failures; the load guard held each binary until the Mac's load fell below 8, and no binary was killed. The guard's own self-test passed.
- Coverage at `84cf119` ([`coverage-editor-84cf119.md`](evidence/write-on/coverage-editor-84cf119.md)), native and wasm lines combined: `src/lib.rs` 93.23%, `document.rs` 99.13%, `kg.rs` 97.49%, `providers.rs` 98.17%, `lab.rs` 99.04%, the editor's `src` as a whole 98.56%; the alternatives and Lab crates 99.24% and 97.81%; JavaScript 95.68% of lines. Re-measuring found that `scripts/ci/wasm-coverage.sh` could not run on main and would delete the tracked `webdriver.json`; fixed in #117 (#116).
- Traceability at `84cf119`: every test cited in the 41 editor rows exists and passed ([`traceability-check-84cf119.md`](evidence/write-on/traceability-check-84cf119.md); 82 citations across all 41 rows; 81 matched automatically by `trace_check.py`, and the wildcard `document::tests::pull_back_*` (R-6.4) was confirmed by hand: 11 tests, all passed; 0 failures).
- Defects found and fixed: D-1 to D-6 (#103), including the wasm-side coverage gap; #116 (the coverage script and the tracked `webdriver.json`, #117).

### terraphim-lsp and zed-terraphim

- UBS ([`ubs-final.md`](evidence/write-on/ubs-final.md)): no critical findings in product code. terraphim-lsp at rc.3 has 77, all in `tests/` (74 `panic!` assertions, 3 reader threads); zed-terraphim at `26fced4` has 3, all in its `#[cfg(test)]` module.
- Tests at rc.3 (`aafcf6f5e`, [`lsp-rc3.md`](evidence/write-on/lsp-rc3.md)): 383 pass and 0 fail across 24 test binaries in `terraphim_lsp` and `terraphim_lsp_core`; line coverage 96.87% (server.rs 94.26%, stored.rs 95.68%, overflow.rs 96.81%, reanchor.rs 100%, ghost.rs 96.88%, block.rs 100%).
- zed-terraphim at `26fced4` ([`zed-terraphim-26fced4.md`](evidence/write-on/zed-terraphim-26fced4.md), re-run with the CI steps; Gitea CI green): fmt and clippy (native and wasm32-wasip2) clean, 74 tests, release wasm 357,877 of 450,000 bytes, guards and the 20 packaging tests pass.
- Protocol ([`protocol-rc3-released.json`](evidence/write-on/protocol-rc3-released.json)): the release binary over stdio, driven with the client capabilities from Zed's own `lsp.rs`: initialize, hover with provenance, every code-action kind, Replace, Ghost it, Revive, Lab mark and clear, Stash, Pull back, Export, folding, save re-anchor, shutdown. One failure on the pre-release build (the server did not exit on `exit` with stdin open) was fixed in #3472 with a real-binary test. Re-run on 2026-10-08 against the released rc.3 universal macOS binary, downloaded anonymously from the internal channel and verified ([`rc3-release-verification.md`](evidence/write-on/rc3-release-verification.md): `shasum -c`, SHA-256 `e3488ea18e9c9502c243c57fd6feb90e4f06b8e4d63767e349def479ae4b652d`): 32 PASS, 0 FAIL, including the exit with stdin open and pull back as a copy with separation.
- Defects found and fixed: D1 exit (#3469), D2 pull-back separation (#3470), D4 README (zed-terraphim #32).

## Phase 5: validation

Method: the real editor build driven in headless Chrome over WebDriver at 1140x900, real input, each step screenshotted and compared with the matching video frame. Final run on `a5f3fbe` (all acceptance fixes merged; `git diff a5f3fbe 84cf119` touches no file under `src`, `crates`, `public` or `templates`). Step-by-step results: [`acceptance-a5f3fbe.md`](evidence/write-on/acceptance-a5f3fbe.md).

| Scenario | Result |
|---|---|
| S1 Write_On look, S2 underlines and dots, S3 cycling and a/an | all match |
| S4 alternatives panel (including live inline add) | all match |
| S5 Ghost it and Revive; S5b AI alternatives with a provider | 13 match; S5.1 deviates by design (no provider loaded, so no AI menu item) |
| S6 Overflow: Ctrl+Shift+D, full-height panel, stash, pull back | all match |
| S7 Lab popover and trim card; S8 save, open, export; S9 plain mode; S10 counter | all match |

Defects found by the first acceptance run and fixed: D1 dark plain mode, D2 paragraph rule in the gutter, D5 panel tabs and status, D9 Lab trim row (#109); D4 live inline add (#108); D6 and D6b Overflow placement and shortcut (#102); D7 pull-back separation (#99).

## Decisions taken during the work

| Decision | By | Where recorded |
|---|---|---|
| Pull back copies (the Overflow keeps the text), editor and LSP alike | orchestrator, from the video transcript | #99, #3472, R-6.4 |
| Stash over an alternative: the editor sets the span aside; the LSP refuses | orchestrator, from the 4:06 frame | R-6.3 |
| Panels are 18% wide; Overflow is full height and flush right; timestamps re-derived | measured from frames | #97, #102, `video-measurements.md` |
| Keep system fonts: no web font is bundled or loaded; the display stack names Caveat first and falls back to system cursive fonts | owner | #109, R-4.1, R-4.2, tokens.md |
| rc.3 tagged on Gitea only; nothing published to GitHub | owner | tag `terraphim_lsp-v1.22.0-rc.3` |
| Internal release channel through a public, release-only Gitea repo | owner | zed-terraphim #32 |
| Commits and tags as Dr Alexander Mikhalev &lt;alex@metacortex.engineer&gt; | owner | terraphim-ai local config corrected |

## What could not be validated

- A live rc.3 run inside Zed (the macOS Screen Recording permission is not granted, and the owner's Zed settings were not edited). The Zed verdicts rest on the LSP tests, the protocol run with Zed's capabilities and the provisioning unit tests.
- Users off the tailnet: the internal channel is tailnet-only; they fall back to `terraphim-lsp` on PATH. A public channel first needs a review of the terraphim-ai commits that exist only on Gitea.
- Drag and drop of Overflow text, and the native Open and Save pickers, which headless Chrome cannot drive.
- Pixel fidelity of colours and dots, beyond what a compressed recording shows.

## Out of scope (by the requirements)

X and LinkedIn sharing (requirements section 1).

## Pull requests merged

terraphim-editor: #82, #87, #88, #89, #91, #92, #94, #95, #97, #99, #102, #103, #105, #108, #109, #111, #113, #117.
terraphim-ai: #3450, #3458, #3460, #3462, #3463, #3466, #3471, #3472, #3475.
zed-terraphim: #29, #30, #31, #32.

Each PR's structural review with pi-rust (openai-codex/gpt-5.5) is posted as a comment on that PR, with the outcome of every P0, P1 and P2 finding (fixed, or rejected with code evidence) recorded beside it. [`review-index.md`](evidence/write-on/review-index.md) links each review and checks, in the merged code, every finding a comment left as "being fixed".

## Evidence

| File | What it holds | Commit |
|---|---|---|
| [`final-run-84cf119.md`](evidence/write-on/final-run-84cf119.md) | fmt, clippy, load-guard self-test, native totals, both browser passes per binary, CI state | terraphim-editor `84cf119` |
| [`traceability-check-84cf119.md`](evidence/write-on/traceability-check-84cf119.md) | every citation in the 41 editor rows, found and passed | terraphim-editor `84cf119` |
| [`coverage-editor-84cf119.md`](evidence/write-on/coverage-editor-84cf119.md) | native, wasm, union and JavaScript line coverage | terraphim-editor `84cf119` |
| [`lsp-rc3.md`](evidence/write-on/lsp-rc3.md) | terraphim-lsp test totals and line coverage | terraphim-ai `aafcf6f5e` (rc.3) |
| [`protocol-rc3-released.json`](evidence/write-on/protocol-rc3-released.json) | 32 protocol checks against the released rc.3 binary | rc.3 release asset |
| [`protocol-prerelease-5d7be04a1.json`](evidence/write-on/protocol-prerelease-5d7be04a1.json) | the same checks on the pre-release build | terraphim-ai `5d7be04a1` |
| [`acceptance-a5f3fbe.md`](evidence/write-on/acceptance-a5f3fbe.md) | the 74 acceptance steps against the video | terraphim-editor `a5f3fbe` |
| [`ubs-final.md`](evidence/write-on/ubs-final.md) | UBS totals and the triage of every critical finding | all three, at the commits above |
| [`zed-terraphim-26fced4.md`](evidence/write-on/zed-terraphim-26fced4.md) | fmt, clippy, tests, wasm size, guards, packaging tests, CI state | zed-terraphim `26fced4` |
| [`rc3-release-verification.md`](evidence/write-on/rc3-release-verification.md) | anonymous download of the rc.3 macOS binary, `shasum -c` against `checksums.txt`, identity with the tested binary | rc.3 release assets |
| [`../requirements/traceability.md`](../requirements/traceability.md) | the verdict of each of the 41 requirements, editor and Zed, and the summary counts | terraphim-editor `84cf119` |
| [`review-index.md`](evidence/write-on/review-index.md) | the review of record on each of the 31 PRs and how its findings were resolved | Gitea PR comments |

Local paths in the evidence are shortened to `<work>/`. Screenshots and video frames are not committed.

## Sign-off

| Approver | Role | Decision | Date |
|---|---|---|---|
| Dr Alexander Mikhalev | Owner | pending | |
