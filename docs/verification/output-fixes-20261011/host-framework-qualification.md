# Host framework qualification — 11 October 2026

This ledger retains all 41 original requirements. No requirement is promoted from an input fixture, compilation, or a protocol test to full visual acceptance. Zed fork work is excluded.

Existing browser and stock-Zed findings remain in [the independent output ledger](../independent-requirement-ledger-20261010.md); Safari has two retained trusted-drag failures. The local server fixes have 415 passing real tests and reviewed commits 40421f8e0 / 7b9ffd339, but native Zed qualification is still incomplete.

Shared host facade: 14 real Rust tests. Bridge: 14 tests against actual generated WASM, including queued edits, native command ordering, multiple views, stale ACKs and exact conflict drafts. Editor native suite: 497 passing tests; no browser-gated tests are inferred from native zero-test binaries. Strict all-target Clippy passes. Native VS Code startup smoke passes activation, registration, exact CRLF/Unicode preservation and opening the actual custom provider. Native r8 UI typing and exact saved output are separately verified below; startup checks alone do not prove interactions.

| ID | Requirement | VS Code actual evidence / remaining proof |
|---|---|---|
| R-2.1 | Plain mode by default. | Native r8 starts with text and counter; clicking counter exposes Write_On controls. Complete layout/token audit remains pending. |
| R-2.2 | Span-anchored alternatives. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-2.3 | Three granularities. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-2.4 | In-place cycling. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-2.5 | Persistence. | Real core/bridge persistence and conflict tests pass; native editor create/save/reopen with full annotations pending. |
| R-2.6 | Grammar fix-ups on swap. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-2.7 | Emptying removes the indicator. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-3.1 | Underline. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-3.2 | Dot row (word/sentence/headline). | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-3.3 | Active-dot highlight. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-3.4 | Paragraph indicator. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-3.5 | Headline rendering. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-3.6 | Hover target. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-4.1 | Placement and toggle. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-4.2 | Tabs. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-4.3 | Alternative list. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-4.4 | Provenance marker. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-4.5 | Keyboard navigation. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-4.6 | Undo. | Native surface Undo changed apple→a and Redo restored the exact annotated saved file. Earlier panel Undo required two Redos; reviewer identified an input-history bypass, now fixed in source with native retest pending. Bot-line/indicator acceptance remains pending. |
| R-5.1 | Ghost. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-5.2 | Revive. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-5.3 | Ghosting is an independent layer, persisted with the document | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-6.1 | Placement and toggle. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-6.2 | Content. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-6.3 | Stash. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-6.4 | Pull back. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-6.5 | Persistence. | Real core/bridge persistence and conflict tests pass; native editor create/save/reopen with full annotations pending. |
| R-7.1 | Layout. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-7.2 | Corner controls (Write_On mode on) | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-7.3 | Context menu on selection | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-7.4 | Existing shortcuts | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.1 | Lab popover. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.2 | Mark actions | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.3 | Trim levels. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.4 | Trim behaviour. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.5 | Review. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.6 | AI alternatives. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-8.7 | Provider abstraction. | Native UI/output acceptance pending; shared UI is included in the local VSIX. |
| R-9.1 | Document = Markdown + annotations. | Real core/bridge persistence and conflict tests pass; native editor create/save/reopen with full annotations pending. |
| R-9.2 | Annotation schema (proposal). | Real core/bridge persistence and conflict tests pass; native editor create/save/reopen with full annotations pending. |
| R-9.3 | Export. | Real core exact export test passes; native picker/output bytes and repeated exports still pending. |

## Evidence and limits

Local VSIX: `/private/tmp/terraphim-it-0.0.4-output-fixes-r8.vsix`. Build includes 122 assets from a stable 244-file source fingerprint. The prior r2 build deliberately refused qualification after source changed during build; r8 completed with a stable fingerprint. Packaging reports a size/file-count advisory, not a compiler warning. No Marketplace publication occurred.

Independent source review cleared the five integration defects plus the completion-before-sync draft-loss race. This does not establish native or all-41 acceptance.

## Native VS Code r8 actual outputs

Dedicated public fixture began as `a banana. Café 😀\r\nSecond line.\r\n`. It was unannotated input. Selecting banana and typing apple in the new alternative line produced `an apple. Café 😀\r\nSecond line.\r\n` and an original-first [banana, apple] list, active index 1. The full saved output is [retained here](vscode-alternative-saved-actual-r8.md); independent byte/UTF-16 checks and source hash are in [the actual output receipt](vscode-alternative-output-r8.json). Screenshot: [actual native UI](vscode-alternative-actual-r8.png).

Native surface Undo produced `an a`, then Redo restored the complete previous file byte for byte, SHA-256 `6c7793933d4cd2381adbd39d4dc866220f0971885ad5903151f714b6725bb0c4`. An earlier panel-focused Undo returned the initial unannotated source and needed two native Redos; this is retained as a history-ownership gap, not a single-step pass. Review identified that accepted panel input beforeinput could bypass host history. The latest source routes accepted input history through the host, but has no new native retest yet.

The first CRLF attempt entered conflict after typing `a`; its actual failed saved output and screenshot remain retained. Structured CRLF/LF annotation conversion fixes this: 14 real Rust tests cover multiline alternatives/ghosts, contexts, malformed raw blocks, unresolved hints, split CRLF boundaries and exact output validation. These are not visual proof for every range.

Latest review also found and fixed export reading an unacknowledged draft and an obsolete rejection canceling a newer proposal. Three new regressions failed before the fixes and all 14 actual-WASM bridge tests now pass. Export now reads the acknowledged engine export and holds its command barrier until the picker/write lifecycle completes. Native export/reopen/split-view and recording remain unqualified. The Mac locked at the reload step; no persistence success is inferred from that attempted action.

Source review cleared the final export refusal/error lifecycle and accepted-input history routing; exact provider/adapter/bridge identities are recorded in host-review-identities.json. These changes postdate the r8 native checks. r9 packaging is a separate source identity and has no native UI pass yet.
