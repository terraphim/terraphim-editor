# Independent requirement/output ledger

2026-10-10. Review only: no UI, capture, source edits or concurrent browser. **All 41 requirements are not accepted across the browser and existing stock-Zed extension.** Chrome and Firefox completed recordings have independently inspected finite passing outputs; Safari take8 is a finished180.2s recording with112 PASS checks, two drag FAIL checks, six semantic observations and nine dependent checks NOT EXECUTED; acceptance remains FAIL. Zed now has a finished900.33s chaptered walkthrough assembled from disclosed partial takes plus the new export chapter; full acceptance is not passed. Stock Zed supplies useful LSP/code-action behavior, not the video's custom UI. The Zed fork remains separate.

Evidence identity:

- Requirements `docs/requirements/alternative-control.md`: SHA-256 `54b4ecf346639a9684bb6e50ee1bcdde5b3fc46a35407a3cedbaf06a0392ce17`.
- Chrome `/private/tmp/full-demo-preflight9-20261010/report.json`: `8f4959909ab28cf51d26041c4634f5d3af7cc4d1f903fc7d411b28473844a2af`; **120 PASS assertions + six semantic observations**, not126 PASS assertions or a completed video.
- Zed `/private/tmp/zed-full-output-checks-20261010.json`: `270086978ffa983410d739043ba914a1d188041d8a2b715756cdb09f53eae476`; extension commit `26fced425584fca66fc01499fa7fd6151004953c` as recorded by parent.
- Zed `/private/tmp/zed-trim-output-20261010.json`: `0570adc6be504c63972e7ee791c1900318d8211071bc0f468ad1d564153df8cf`.
- Zed `/private/tmp/zed-export-output-20261010.json`: `4ab335d5ab68c460474cd080a5cd582c272bda4bcc8c15a847835778c65491b5`.

These are parent-produced actual-output records, independently inspected here; this reviewer did not operate their native UI execution, but subsequently inspected both completed browser videos and partial native footage as documented in the final media audit. Baseline text, preloaded alternatives and preloaded ghost g1 are **inputs**, not demonstrations that those annotations were created. Browser chapter fixtures are similarly disclosed inputs; subsequently typed alternatives, swaps, ghosts and trusted drag/drop receipts are outputs.

## Material native failures and evidence limits

Repeated export into an already open clean file is also **FAIL**: the new67s native chapter visibly duplicates the body, undo leaves it empty, and a second export plus explicit owned-file overwrite recovers a literal matching single copy. Recovery does not establish robust R-9.3 behavior. The final media audit records this footage and the source-level hypothesis.

The immediate-save Zed attempt is **FAIL**: saved anchors use146146/155157 and119119/135135, which do not address their recorded text. Independently checked every recorded annotated body: those two anchors fail, while all anchors in the settled retry and later ghost/revive/stash/pull/typo records match their UTF-16 slices. A later successful settled save does not erase the persistence defect. The parent separately reports native undo/redo hash equality, but this artifact set lacks per-action timestamp receipts; those claims are not promoted to proved behavior here. An asynchronous-edit/save race is a plausible explanation recorded by the parent, not an independently established root cause. This requires a reproducible regression/fix before claiming robust R-2.5/R-9.2 completion.

Six Zed records with expectedBody compare exactly. The unclassified typo-action record independently differs from the preceding body only by first `recieve`→`receive`; it alone does not establish all six mark actions; subsequent partial-video inspection shows all six finite diagnostic categories, with no semantic-quality guarantee. Trim's actual363 words match its displayed expectation and the kept literal survives; no independent original-range splice, all-character oracle, protected-occurrence preservation is supplied by that JSON. Partial video separately shows finite undo changes and late transient presentation disappearing after reopen; no timestamped exact undo/redo hash receipts exist. Export actual equals its literal expected text and visibly omits the ghosted aside/block/Overflow; this does not prove general export behavior. The trim fixture contains historical statements such as “None of the features below exist yet.” Those are document content, never current product-status evidence.

## All 41 criteria

Each browser row describes **finite Chrome and Firefox run evidence** (the final media audit links sampled output frames), followed by the exact remaining acceptance gap. “Zed equivalent” means an observed code action/model outcome, never full visual parity. “UI gap” refers to the current stock extension boundary documented in local Zed fit research; this ledger does not adopt that historical document's obsolete repository-status claims.

| ID | Chrome actual evidence; missing criteria | Existing Zed actual evidence / gap |
|---|---|---|
| R-2.1 | Counter toggles both modes; startup default, only-text/count visibility and dim placement not fully audited. | No Write_On chrome/toggle equivalent shown; UI gap. |
| R-2.2 | Typed original-first word list and active body; arbitrary contiguous spans/full inactive rendering not exhaustively checked. | Stored binder-clip swap observed; original list was preloaded, creation not proved. |
| R-2.3 | Sentence and paragraph replacements; headline alternative creation/cycling missing. | Word swap only; sentence/paragraph alternatives unobserved. |
| R-2.4 | Word hover Up/Down changes actual body; all granularities/boundary cycling missing. | Code-action swap observed; hover-arrow UI gap. |
| R-2.5 | Full created choice/ghost/Overflow API roundtrip; OS save/open and later-session test missing. | Settled saved active choice observed; immediate-save corruption remains FAIL. |
| R-2.6 | Reversible paperclip/eraser/thumbtack body; wider vowel-sound and untouched-outside-article cases missing. | KG rubber→eraser action changes `a`→`an` exactly; reverse swap not shown. |
| R-2.7 | Undo generated span removes model span; manual delete-all and actual dots/underline removal with independent ghost preservation missing. | No removal/indicator proof. |
| R-3.1 | Actual word underline1px; full span width/baseline/color/no-hover-change audit missing. | Custom precise underline UI gap. |
| R-3.2 | Word centering, sentence-right geometry and counts; dot diameter/spacing, multiline and headline geometry missing. | Dot row UI gap. |
| R-3.3 | Active1 full lavender, inactive alpha0.4, one active; wider transition/theme/pixel review missing. | Custom active dots UI gap. |
| R-3.4 | Rule1px×22px,20px gutter, left dots/no underline; multiline/scroll/zoom cases missing. | Paragraph gutter/dot UI gap. |
| R-3.5 | Headline exists; exact face/color/bold/size and headline indicators missing. | Host Markdown styling is not verified custom parity. |
| R-3.6 | Real span hover triggers cycle; passive dots/direct-jump inferred behavior not checked. | Span hover/key interception UI gap. |
| R-4.1 | NOT DEMONSTRATED: full-height18% clamp, shortcut/icon toggle and main-text dimming. | Full alternatives panel UI gap. |
| R-4.2 | Real tabs select sentence/paragraph kind; all labels/fonts/colors/population behavior missing. | Custom tabs UI gap. |
| R-4.3 | Typed first keystroke/Enter/ordered list/Escape; edit-existing-line and full list styling missing. | Stored list preloaded; editable side-list UI gap. |
| R-4.4 | Actual generated source=`ai`; rendered robot/human glyphs and active text/glyph styling missing. | No provenance glyph/hover presentation proof. |
| R-4.5 | Panel Up/Down exact word output; edge/wrap/multigranularity missing. | Code-action chooser is not panel Up/Down parity. |
| R-4.6 | Actual generated-span undo; bot-line and indicator DOM disappearance not checked. | AI-list undo not observed. |
| R-5.1 | Real ghost preserves text/counts, actual color alpha0.1; right-click Ghost item/selectability still incomplete. | New paragraph ghost/code action saved without body rewrite; exact10% rendering/selectability not proved. |
| R-5.2 | Revive removes ghost and menu shows Revive; actual opacity restoration/right-click sequence not fully checked. | Revive removes newly added g2, preserves g1/body; UI opacity parity not proved. |
| R-5.3 | Swap-driven resize, combined save/reopen, overlap merge, interior split and full deletion observed; adjacent merge, partial alternative overlap and ordinary interior typing missing. | Saved independent g2 creation/revive observed; preloaded g1 not creation proof; merge/split/elastic lifecycle missing. |
| R-6.1 | Overflow used;18%/clamp/full-height/right edge/title fonts/XYZ highlight/fixed corner glyphs not checked. | Custom Overflow panel UI gap. |
| R-6.2 | Exact typed note reaches model; font and notes/URLs/outlines variety missing. | Stashed paragraph stored; free-form panel UI gap. |
| R-6.3 | Shortcut stash and trusted surface→Overflow drop exact body/append/undo/redo; right-click Stash item missing. | Actual code-action move/expectedOverflow observed; custom menu missing. |
| R-6.4 | Chrome/Firefox Ctrl+Enter and trusted Overflow→surface copy exact output/undo/redo; Safari take5 trusted-drop FAIL and take6 handoff unexecuted; hint, ordinary copy/paste and all caret boundaries missing. | Actual code-action pull restores body and keeps Overflow; native Ctrl+Enter/drag parity not shown. |
| R-6.5 | Typed Overflow exact API save/reopen; OS and later-session persistence missing. | Saved Overflow present after stash/pull; close/reopen native test missing. |
| R-7.1 | NOT DEMONSTRATED: full-bleed,70-char centered column/top margin and toolbar absence. | Stock host chrome; UI gap. |
| R-7.2 | Counter used; all corner positions/icons/tooltips/reference and intentionally blocked posts not audited. | Custom corner controls UI gap. |
| R-7.3 | Actual selection menu Revive item exercised; all four items, shortcuts, rounded/colors/hover styling missing. | Native code-action menu differs; custom selection menu UI gap. |
| R-7.4 | NOT DEMONSTRATED: existing bold/italic/code/link/heading collision regression. | Host/custom key collision suite missing. |
| R-8.1 | Six actions execute; anchored Lab popover/header/action-layout geometry missing. | Code actions/diagnostics replace popover; UI gap. |
| R-8.2 | All six output marks and unchanged body; target ranges align/exclude fixture heading/code. Semantic precision/recall/ranking/tone suitability unaccepted. | All six finite diagnostic categories visible in partial video10–250s; typo fix separately observed. Weak diagnostic says no role selected; semantic quality and unchanged-body proof for every action remain unaccepted. |
| R-8.3 | Five levels clicked, body preserved; exact labels/borders/selected styling missing. | All five native levels/actions and notifications observed280–320s; browser-style visual buttons remain a UI gap. |
| R-8.4 | Independent counts160→146/130/113/82 with9/19/29/49% cards and unchanged body; ghost rendering/card layout/text/buttons still need full visual audit. | Trim count363 observed; original whole-body/range/candidate diagnostic evidence incomplete; custom card UI gap. |
| R-8.5 | Initial Walk marker, Keep id, changed/count-checked cuts, literal-body-preserving Done/exact promoted ghosts; Next/Previous lifecycle, faded-span click keep and literal every-character tidy oracle missing. | Make cuts after Keep retains literal/count; finite visual undo/Done/reopen observed. Done presentation lost after reopen is an unresolved persistence limitation. Exact splice and timestamped undo/redo hashes missing. |
| R-8.6 | Real local generation appends exact ordered list with source=ai; rendered bot glyph and wider selection cases missing. | KG replacement action observed; not evidence of appending a bot-tagged list. |
| R-8.7 | Local role-backed generation observed; provider switching/abstraction and offline runtime proof not in these artifacts. LLM optional per decision. | Local KG action observed; pluggable/offline/provider boundary unqualified here. |
| R-9.1 | Created annotations save trailing block and reopen exact body/model via API; actual picker/preview exclusion missing. | Saved raw block observed; native re-open/preview-stripping not tested. |
| R-9.2 | Exact created annotations roundtrip; displaced hints/context best-match thresholds/ambiguity/set-aside/legacy cases missing. | Settled anchors independently valid; immediate-save corruption FAIL; context/reanchor/ambiguity tests missing. |
| R-9.3 | Visible export value equals independent body-minus-ghost output, block/Overflow omitted; OS exported file and broader ranges/tidy cases missing. | First public clean file and later recovery equal literal expected; new export chapter shows duplication on repeat export into an open file (FAIL), empty state after undo, then recovery. General repeat/overlapping/reanchored export unaccepted. |

## Completion boundary

Chrome and Firefox final video identities, probes and sampled chapter/output frames were independently inspected in [the media audit](independent-final-demo-media-audit-20261010.md). Each has120 PASS checks plus six semantic observations. The finished native900.33s assembly has three reviewed chapters (0/388/833s), with its source-take interruptions disclosed; it is ready as a walkthrough, not all-requirement acceptance. Safari take8 is finished; the earlier takes5–7 failures remain retained. A finished Safari take8 MP4 now exists with explicit acceptance FAIL. Browser §10 token/pixel review and stock Zed's custom UI gaps prevent visual parity claims even where data-model equivalents work. Remaining acceptance requires final per-browser actual-output reports and inspected videos, filling the listed missing criteria, and resolving/qualifying the native immediate-save corruption. A separate Zed fork may address unsupported visual UI; it is not part of this current extension completion claim.

Actual Chrome/Firefox frame timestamps and native finite diagnostic/trim observations are detailed in [the media audit](independent-final-demo-media-audit-20261010.md). These refine evidence, not the acceptance thresholds or unresolved gaps in the41 rows.

Subsequent67s native export footage is now inspected: real repeated-export failure and successful recovery are separately retained. Safari take2 later failed after33 PASS checks at its Shift+F10 menu path; the context-menu fallback was subsequently independently reviewed.

Final Zed assembly and truthful sidecar independently verified: SHA-256 `c075a88916e4dd6e7727d5e326f1caeecf690d8ee566fe3152e22474b5c08c60`,900.333333s,three chapters. Saved outputs and all retained failures remain distinct. Safari take8 is now recorded with acceptance FAIL; prior attempts remain historical evidence.

## Latest Safari state (take4)

Independently inspected take3 sidecar SHA-256 `16a76af2df35dbb944e563605bb68ab880b6214f94081a5e97e61493c0bc5f2c`: incomplete,49 actual PASS entries and two logged native secondary-button inputs, then parent-retained HTTP400 at the Overflow-control click. Focus/setup is a hypothesis, not an established product cause. Independently inspected take4 sidecar SHA-256 `9ec2675668b3e7ec0844b09def55ab922caa162187d4d745d329ce919437addc`: c7622b4 admitted slot, zero checks, zero raw samples, zero context-menu inputs; visible=false and focused=false over82 attempts/10.0014s. Log explicitly refuses capture/storyboard. Parent separately observed Mac locked. Thus take4 is an operational precondition failure and establishes no Safari product output. No failed take is relabelled PASS, and all41 requirement rows retain their unmet criteria.

## Safari takes5–7: latest independently checked evidence

- Take5 sidecar SHA-256 `8b0acc168edb297ee236cd516caca91b98f27d739b86200affb6b36bca39bb43`: incomplete,52 PASS checks plus one FAIL,203 raw screenshot records. Focus/viewport preconditions passed. Failed R-6.4 check records only trusted `dragstart` from Overflow, with the trusted surface `drop` absent. No insertion/undo/redo success follows that failure.
- Passive diagnostic SHA-256 `da8649b3eca0f282131a9b36e5c96966f9349e0be5bfa01d51ad958803321dd2`: requested start(967.8125,114), destination(89.09375,100.390625) and actual DOM endpoint is the rendered editor. Trusted dragover events remain(968,114), then leave(1071,955)/end(1084,-304), with no surface event. This supports native WebDriver coordinate divergence in this attempt; it does not establish an underlying product defect or a general Safari limitation.
- Take6 sidecar SHA-256 `7c6a2ae21633017fe35c9304fa4e2eef4104167e929e5e85552b7743618d5aab`: incomplete,52 earlier PASS checks,416 raw screenshots, one handoff request marked `requested; not proof of output`. Parent observed Safari's remote-control modal preventing manual/native interaction and chose Continue Session. No gesture acknowledgement was written; log confirms the60-second acknowledgement timeout. Earlier52 checks are finite outputs, but there is no native-drag output proof from this handoff. Modal interaction was not bypassed.
- Take7 sidecar SHA-256 `5fdeb6ebd61cb6006c65c57d34f7565e0e11405180c8c73e1149525ca3f5167e`: incomplete, acceptance NOT EVALUATED, zero checks/screenshots/native inputs. Visibility/focus false after83 attempts/10.0017s; log explicitly refuses capture/storyboard. Parent observed relocked Mac separately.

No take above qualifies a completed Safari video. Take8 is prepared on the independently admitted9f996cc failure-visible slot, with explicit retained drag FAIL/NOT EXECUTED dependency policy; it has not yet produced evidence for this review. Four completed deliverables and all41-requirement acceptance are distinct goals; the latter remains unaccepted.

## Final Safari take8 and four-recording status

Four application recordings now exist and have independently sampled media audits: Chrome, Firefox, Safari and finished chaptered native Zed. **All41 requirements are not accepted.** Safari take8 source `9f996ccd38d3b4d67ad6820aa49350d6126b4195`, admitted asset manifest `19bd999068cbaf3d09b3f6e7d5667863324b5e90e2e47d9b281560dec8c58bbb`, actualSafari26.6.2,180.2s. It retains112 PASS checks, two drag FAIL checks, six semantic observations and nine unexecuted dependent checks; acceptanceStatusFAIL. R-6.3/R-6.4 remain failed despite later finite successes. All285 source records independently match captured committedHEAD and all128 staged asset hashes match admission. [All41 Safari entries and approximate time index](independent-safari-requirement-time-index-20261011.md) retain exact missing criteria; video/frame navigation uncertainty is disclosed. Immediate-save corruption, repeated-export failure and Done/reopen limitation in Zed remain unresolved. The fork remains separate.
