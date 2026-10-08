# Review index: the structural review of record on every merged PR

Built on 2026-10-08 from the Gitea API: each PR's comments were searched for the pi-rust (openai-codex/gpt-5.5, structural-pr-review) review, and each PR's last review comment was read for its outcome. All 31 PRs are merged. Each has at least one review comment.

In the last column, "Comments with dispositions" counts comments that use the words fixed, rejected or disposition. Where it is 0, the review itself reported no P0, P1 or P2 finding, or a round summary in the same comment records the fixes.

| Repository | PR | Merged | Review comments | First review comment | Comments with dispositions |
|---|---|---|---|---|---|
| terraphim-editor | #82 | yes | 1 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/82#issuecomment-95739 | 0 |
| terraphim-editor | #87 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/87#issuecomment-95987 | 2 |
| terraphim-editor | #88 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/88#issuecomment-95763 | 0 |
| terraphim-editor | #89 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/89#issuecomment-96063 | 0 |
| terraphim-editor | #91 | yes | 1 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/91#issuecomment-96153 | 0 |
| terraphim-editor | #92 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/92#issuecomment-96192 | 0 |
| terraphim-editor | #94 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/94#issuecomment-96212 | 0 |
| terraphim-editor | #95 | yes | 3 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/95#issuecomment-96459 | 0 |
| terraphim-editor | #97 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/97#issuecomment-96573 | 1 |
| terraphim-editor | #99 | yes | 3 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/99#issuecomment-96596 | 2 |
| terraphim-editor | #102 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/102#issuecomment-96633 | 1 |
| terraphim-editor | #103 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/103#issuecomment-96719 | 1 |
| terraphim-editor | #105 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/105#issuecomment-96693 | 1 |
| terraphim-editor | #108 | yes | 1 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/108#issuecomment-96777 | 1 |
| terraphim-editor | #109 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/109#issuecomment-96823 | 0 |
| terraphim-editor | #111 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/111#issuecomment-96899 | 2 |
| terraphim-editor | #113 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/113#issuecomment-96894 | 0 |
| terraphim-editor | #117 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-editor/pulls/117#issuecomment-96955 | 2 |
| terraphim-ai | #3450 | yes | 1 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3450#issuecomment-95683 | 2 |
| terraphim-ai | #3458 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3458#issuecomment-96028 | 0 |
| terraphim-ai | #3460 | yes | 3 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3460#issuecomment-96185 | 1 |
| terraphim-ai | #3462 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3462#issuecomment-96302 | 1 |
| terraphim-ai | #3463 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3463#issuecomment-96360 | 2 |
| terraphim-ai | #3466 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3466#issuecomment-96428 | 0 |
| terraphim-ai | #3471 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3471#issuecomment-96504 | 0 |
| terraphim-ai | #3472 | yes | 3 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3472#issuecomment-96532 | 2 |
| terraphim-ai | #3475 | yes | 2 | https://git.terraphim.cloud/terraphim/terraphim-ai/pulls/3475#issuecomment-96712 | 1 |
| zed-terraphim | #29 | yes | 1 | https://git.terraphim.cloud/terraphim/zed-terraphim/pulls/29#issuecomment-95632 | 0 |
| zed-terraphim | #30 | yes | 1 | https://git.terraphim.cloud/terraphim/zed-terraphim/pulls/30#issuecomment-95678 | 0 |
| zed-terraphim | #31 | yes | 1 | https://git.terraphim.cloud/terraphim/zed-terraphim/pulls/31#issuecomment-95961 | 0 |
| zed-terraphim | #32 | yes | 2 | https://git.terraphim.cloud/terraphim/zed-terraphim/pulls/32#issuecomment-96874 | 2 |

## Findings left as "being fixed" in a comment, checked in the merged code

| PR | Finding | Where it is resolved |
|---|---|---|
| terraphim-editor #97 | P2: traceability cited the context menu at 2:38 | `docs/requirements/traceability.md` at main has no `2:38` |
| terraphim-editor #102 | P1: web_overflow unreliable within its budget | split into web_overflow and web_overflow_layout (468338a), recorded in the last comment; both pass in the final run |
| terraphim-editor #105 | P2: document `terraphim_lab` in `[patch.terraphim]` | `docs/design/kg-alternatives.md` names `terraphim_lab` in the patch section |
| terraphim-editor #111 | one capped wait on CI; guard-test CI case; SC1091 | load-guard self-test output in `final-run-84cf119.md` ("CI: warn only ... benchmarks skipped with one wait"); `# shellcheck disable=SC1091` in `scripts/ci/browser-tests.sh` |
| terraphim-ai #3466 | P2: Export command carried the version | resolved in 6bc1ef188, with the regression test `a_cached_export_action_exports_the_current_text` |
| terraphim-ai #3475 | two P2s (token length in a message; stale proxy references) | fixed in aab2e7567, delta checked, recorded in the last comment |
| zed-terraphim #32 | P2: qualification still presented rc.2 as current | `docs/qualification.md` at `26fced4` gives rc.3 as current and keeps rc.2 as history |

Rejected findings, with the code evidence on the PR: #99's CRLF P2, #111's P1 (benchmarks skipped on CI under load is existing behaviour), zed-terraphim #32's P1 (checksum-driven candidate selection matches `pick_asset`), and #87's delta Medium (first rejected, then accepted and fixed after the owner rule, as the follow-up comment records).
