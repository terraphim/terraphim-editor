# Independent final demo media audit

2026-10-10. Read-only media review; no browser or native UI launched. **Four finished application recordings: Chrome, Firefox, Safari and chaptered native Zed. Safari and Zed retain explicit failures; this is not acceptance of all41 requirements.**

## Exact media identity

- chrome: video SHA-256 `1e7056f56727db005f521c1106b93e4be499df16be88172a5b27b15dffeb9e46`; sidecar `c60b52929b730b54771bf74522b335cb7404d8643c2d55d3a2a77df8b70e38bb`; 181.400000s, 1140×720, h264/yuv420p, 15/1fps, 1 video-only stream. Independently counted120 PASS and six semantic observations.
- firefox: video SHA-256 `b1dade34d9ac574559a5483ec69d3ee236512b6fb50a3a4bc4a54257cb52437b`; sidecar `367bee0226dcbabbd303e1d972b63dd7cc7d7e8b0e7f387bb262399c048aac20`; 181.066667s, 1140×720, h264/yuv420p, 15/1fps, 1 video-only stream. Independently counted120 PASS and six semantic observations.

Source/build: both sidecars identify `0d07b5d9b1696f38d22a712f84d90d5d054d8b3d` and admitted demo manifest `0ead0fcfab463c00ae020b232a1fe35d629f2be77fee83ef3188fccbbebeecf0`. Exact hashes, probe and extracted-frame hashes are saved in the `independent-chrome-chapter-audit/audit.json` and `independent-firefox-chapter-audit/audit.json` artifacts beside the videos.

## Frame observations and timing limits

Viewed Chrome55 earlier key-output frames plus29 chapter/extra frames; Firefox75 chapter/key-output frames and75 revised samples. Also inspected native35 coarse frames plus54 five-second samples/OCR, with selected full-resolution diagnostics. These are sampled frame inspections, not every-frame privacy certification. Public fixtures and explicit setup captions remain inputs, not successful operation outputs.

Neither browser sidecar contains its promised capture-lead-in field. Initial caption transitions independently sampled/OCR establish approximate offsets from storyboard to video: Chrome +0.4s (about ±0.15s); Firefox +1.1s (about ±0.2s). Constant exact alignment over the whole recording is not established. Use the actual extracted-frame times below; do not claim exact action timing from sidecar milliseconds. Some short-lived output checks finish before a later sampled frame, so labels in the frame manifest identify the intended sample, not an independent assertion that the named operation is visible in that frame.

| Actual video frames (seconds) | Observed output | Requirement scope, not full acceptance |
|---|---|---|
| Chrome10–14; Firefox9.70/14.61 | Real partial typing, a/an article change, restored paperclip; alternative panel visible. | R-2.2–2.6, R-4.3/4.5 |
| Chrome20.32/26.75; Firefox24.95/32.67 | Sentence and paragraph replacements with original/typed list. | R-2.3, R-4.2/4.3 |
| Chrome34.12; Firefox36.88 | Generated rubber/eraser list, provenance marker. | R-8.6/8.7; no provider-quality guarantee |
| Chrome39–41; Firefox43–44 | Selected ghost text remains visible/dim. | R-5.1/5.2 |
| Chrome46–49,57,64; Firefox58.94,66.27–67.37 | Overflow output, body insertion, source removal and appended paragraph. | R-6.2–6.4 |
| Chrome68–85 incl78; Firefox72.97/76.02/79.07/82.12/85.17/88.23 | Six Lab mark categories and visible target marks. | R-8.2 finite observations; semantic correctness not proved |
| Chrome90–110; Firefox108.78/115.20 | Trim card/fades, changed cut body and Done text retained. | R-8.3–8.5; literal punctuation-tidy oracle remains limited |
| Chrome113.08/115.40; Firefox116.22/120.33 | Reopened annotations and visible Markdown export dialog. | R-9.1/9.3; API roundtrip, not OS picker proof |
| Chrome125/130; Firefox132.66 | Created choice/ghost/Overflow combination, reopened body. | R-2.5, R-5.3, R-6.5 |
| Chrome134–142; Firefox138.80/140.98/143.03 | Merged ghost coverage, Revive menu/interior selection, subsequent empty body. | R-5.3 finite lifecycle |

Both recorded run sidecars independently report120 PASS checks and six observations, rather than126 semantic PASS outcomes. Native pullback differs visibly: Chrome `FPublic note.irst paragraph.` at drop offset1, Firefox `Public note.\n\nFirst paragraph.` at offset0. Both satisfy the independent insertion oracle at the actual destination; this does not demonstrate identical caret placement or prose quality. Browser §10 visual token/pixel checks and the exact gaps in the41-row ledger remain outstanding.

## Native Zed partial and failure boundaries

Raw `/private/tmp/zed-remaining-raw-20261010.mp4`: SHA-256 `1672969eb897206e93dddfb9020e469c11fcb016a371f888d626a78742232ac7`, creation19:09:47Z, duration446.85s. Its exact-title guard correctly terminated as export opened another title. Independently hashed/probed public derivative SHA-256 `7590b9f04b9a45a92565be6ae1038b4a156ccbdcec974309b91655b8f7d09515`,445.0s,one video-only stream. It keeps445s and masks the top-right avatar; this remains partial, not a final full demo.

Observed finite native outputs: typo diagnostic/fix around80s, hedge diagnostics10–70s, weak-sentence115s (`no role selected; 1 hedge/filler`), long-sentence155–205s, convoluted230–235s (clause load8, limit5), off-tone245–250s (`stuff`). These establish all six diagnostic categories, not mark quality or full body-preservation across every action. Trim notifications around280–320s show Original/Slight/Tighten/Sharper/Half and counts713→643/572/481/356. Keep/Make cuts and visual undo changes appear340–375s. Exact per-action undo/redo hash receipts are absent.

Done/reopen limitation: pre-reopen diagnostics/fading are visible in the raw late sequence; reopened trim-long at415s has no trim toast/diagnostics and ordinary-dark text. Parent reports the completed Done fades disappear on reopen; the sampled frames establish that the late transient presentation is lost, not an independently decoded saved-ghost count. Treat persistence parity as FAIL/unresolved, never a successful durable Done demonstration. Saved363-word output receipt19:15:32 lies within capture; export receipt19:17:55 is after raw capture end (~19:17:13.85), so export output is independently verified from JSON/file evidence, not demonstrated in this partial video.

**Immediate-save corruption remains unresolved FAIL**: malformed146146/155157 and119119/135135 anchors in the actual output record. A successful settled retry does not erase that failure. The cause is unproved; separate investigation documents candidate asynchronous edit/reanchor paths. Stock-Zed custom dots/panels/opacity/Lab chrome remain visual gaps; fork work is separate.

## Safari

Retained final sidecar is incomplete: visible=true, focused=false,89 attempts over10.00087s; capture did not start. Mac lock prevented focus/native continuation. This is an operational block, not Safari functionality success or a completed video.

## Subsequent native export chapter (22:05Z)

Mac unlocked; parent resumed recording. Independently inspected all23 three-second sample frames of `/private/tmp/zed-export-final-20261010.mp4`. SHA-256 `ec1debce16d9ebb2d79caf27595e26369286542b5855076757f6b206c4e0b32c`,67.325s,1140×720,H264,one video-only stream,creation22:05:48Z. Log records WINDOW_RECORDING_STARTED followed by normal WINDOW_RECORDING_FINISHED. Sampled frames contain the public Zed window only; avatar is visible top-right and needs the existing mask before user-facing assembly. Public exact file titles remain within the previously reviewed allowlist; no evidence of guard bypass. Sampling/log review cannot independently prove every guard poll.

**Repeat-export FAIL is directly visible:** initial clean output exists0–9s; native Export action12–15s; open output contains duplicated text18–24s; undo leaves the output empty27–39s. Second source Export42–45s yields a single copy48–51s; a dimmed save-conflict state appears54–57s, and the single-copy body remains60–66s. Parent reports selecting Overwrite on the owned public file; the three-second sampling does not clearly resolve the button click. Saved retry receipt22:06:55.287Z independently matches its literal expected text and SHA-256 `ca827b7b0328ee29b7966dd0ce57d941e56fb46721f742b1b3a0d99937cee9a9`. This is successful recovery after a failed repeated export, not robust repeat-export acceptance.

Pinned rc3 LSP source (`aafcf6f5e212de55b5c5696f87bb7a7a463b80a8`) explains a plausible interaction: `write_export`, server.rs1219–1240, sends CreateFile(overwrite=true), then an unversioned TextDocumentEdit whose Range::default() inserts at0:0. It does not explicitly replace the previous open buffer's entire contents or version it. Correctness therefore depends on the client resetting existing buffer text when it applies CreateFile. Observed duplication is consistent with insertion into a retained open buffer while disk was truncated; this is a hypothesis, not a proved client/LSP root cause. Request/response and client buffer/resource-operation traces are required. No production edits were made.

A parent-assembled chaptered video may combine the genuine recorded segments with disclosed cuts, title cards, avatar masks and retained failure/recovery chapters. It must not relabel partial source segments or recovered export as all41-requirement completion.

## Assembled native Zed recording

Independently hashed final assembly `zed-existing-extension-guided-demo-2026-10-10.mp4`: `c075a88916e4dd6e7727d5e326f1caeecf690d8ee566fe3152e22474b5c08c60`, duration900.333333s. ffprobe confirms three actual MP4 chapters:0–388s alternatives/ghost/Overflow with retained save failure;388–833s Lab/trim;833–900.333s export failure and recovery. Boundary frames387/388 and832/833/834 plus export850/860/880/899 were extracted. The inspected transition frames and export samples show real public Zed content, with the account-avatar region masked. This is a disclosed concatenation of three separately captured segments, not continuous execution or full41-requirement acceptance; all native failures above remain. Chapter boundaries and probe saved beside media in `independent-zed-assembled-frame-audit/`.

## Final assembled Zed sidecar review

The final sidecar SHA-256 is `c67b9b961c1d004a08d341d607cfd4821a38768d11bedb31eb06013e8cf5eae4`. Independent file/hash/probe checks match its7299592-byte video and all three constituent segment hashes/durations, whose sum matches900.333333s. Sidecar truthfully labels a finished chaptered walkthrough assembled from three takes, distinguishes preloaded input, retains immediate-save corruption, Done/reopen loss and repeated-export duplication, and separates recovered-export success. No blocking disclosure issue found. Its first-segment approximate time index uses corrected actual18:20:47Z capture origin rather than the journal's stale18:20:10 estimate; subsequent segments correctly add388 and833 seconds. Do not infer exact input-event timestamps from file-save receipts or sampled frames. The final assembly is ready as a recorded walkthrough; full requirement acceptance remains not passed.

## Latest Safari attempt review

Take3 sidecar `16a76af2df35dbb944e563605bb68ab880b6214f94081a5e97e61493c0bc5f2c` is incomplete, with49 PASS entries,195 actual screenshot sample records and two native-secondary-button input receipts; parent log reports subsequent HTTP400 at the Overflow-control click. This is useful finite output evidence, not a qualified full demo. The precise HTTP error body was unavailable; focus/setup remains a hypothesis.

Take4 sidecar `9ec2675668b3e7ec0844b09def55ab922caa162187d4d745d329ce919437addc` independently matches the retained file. It identifies source `c7622b48215e85a8451bd1c96cc805be4a455b30` and manifest `1e045a7846a18224d27e018902d2f7bca4b5389a4f50f6c63254ba56385bfde5`; Safari26.6.2. It contains no output checks or raw frames. Visibility/focus guard failed visible=false/focused=false after82 attempts in10.0014s; `/private/tmp/safari-full-take4-20261010.log` explicitly states no capture or storyboard started. Parent observed Mac locked separately. No Safari MP4 is qualified; no take4 output or functionality success is inferred.

Documentation observation: `demos/README.md` still includes historical “No final video exists”, pending-execution text, the original64-second scope, and a two-title Zed description that omits the later allowlisted writing.clean.md. Its expanded section is also behind the finished native walkthrough. This stale overview must not override current exact sidecars/ledger. No source or README files changed in this read-only review.

## Safari takes5–7: latest independently checked evidence

- Take5 sidecar SHA-256 `8b0acc168edb297ee236cd516caca91b98f27d739b86200affb6b36bca39bb43`: incomplete,52 PASS checks plus one FAIL,203 raw screenshot records. Focus/viewport preconditions passed. Failed R-6.4 check records only trusted `dragstart` from Overflow, with the trusted surface `drop` absent. No insertion/undo/redo success follows that failure.
- Passive diagnostic SHA-256 `da8649b3eca0f282131a9b36e5c96966f9349e0be5bfa01d51ad958803321dd2`: requested start(967.8125,114), destination(89.09375,100.390625) and actual DOM endpoint is the rendered editor. Trusted dragover events remain(968,114), then leave(1071,955)/end(1084,-304), with no surface event. This supports native WebDriver coordinate divergence in this attempt; it does not establish an underlying product defect or a general Safari limitation.
- Take6 sidecar SHA-256 `7c6a2ae21633017fe35c9304fa4e2eef4104167e929e5e85552b7743618d5aab`: incomplete,52 earlier PASS checks,416 raw screenshots, one handoff request marked `requested; not proof of output`. Parent observed Safari's remote-control modal preventing manual/native interaction and chose Continue Session. No gesture acknowledgement was written; log confirms the60-second acknowledgement timeout. Earlier52 checks are finite outputs, but there is no native-drag output proof from this handoff. Modal interaction was not bypassed.
- Take7 sidecar SHA-256 `5fdeb6ebd61cb6006c65c57d34f7565e0e11405180c8c73e1149525ca3f5167e`: incomplete, acceptance NOT EVALUATED, zero checks/screenshots/native inputs. Visibility/focus false after83 attempts/10.0017s; log explicitly refuses capture/storyboard. Parent observed relocked Mac separately.

No take above qualifies a completed Safari video. Take8 is prepared on the independently admitted9f996cc failure-visible slot, with explicit retained drag FAIL/NOT EXECUTED dependency policy; it has not yet produced evidence for this review. Four completed deliverables and all41-requirement acceptance are distinct goals; the latter remains unaccepted.

## Final native Safari take8 (2026-10-11)

Independently verified completed MP4 SHA-256 `0eb37401d8faa1b0363ccb44f595631069953ae859faed7226cada9889e6c908`,940626bytes; sidecar `0213e4f49f6ce449370105459656096b20df1226bf14aa6910e23667d447351d`. Actualffprobe180.2s,H264/yuv420p1140×720,15fps,one video-only stream. Safari26.6.2. All285 recorded source hashes independently match captured9f996ccHEAD; all128 admitted assets match recorder manifest19bd999...8bbb. This is historical source association, unaffected by later documentation updates.

Counted112 PASS, two FAIL, six semantic observations, nine dependent checks NOT EXECUTED; sidecarstatuscomplete describes encoded media while acceptanceStatusFAIL remains explicit. Pull drag has only trusted Overflow dragstart; stash receipts empty. Body/payload/undo/redo dependent checks were not executed, and requirement coverageR-6.3/R-6.4 staysFAIL. FAILED captions are present/readable at actual sampled video57.77s and67.58s. Independent Lab/trim/export/persistence chapters then proceed from disclosed new public fixtures; neither fixture setup nor continuation is promoted to failed-drag success.

Viewed47 independently extracted full-viewport samples, covering every chapter transition, both failure captions, allsix Lab category outputs, cut/Done presentation, export dialog121.38s, combined reopen133.09s and interiorRevive menu141.91s. No private/unrelated content found in sampled frames. This is sampled review, not every-frame privacy certification or pixel-perfect visual parity. Main text and captions are readable at1140×720; small annotation UI, intentional ghost dimming and scrolling mean individual frames do not show every long-body character. Extracted-frame hashes and source checks are retained beside media in `independent-safari-take8-frame-audit/audit.json` and probe.json.

Independently recomputed actual screenshot cadence from monotonic completions: average3.710398885frames/s, maximum interval2.710740083s.15fps encoding duplicates actual screenshots over genuine elapsed intervals; it does not supply15fresh captures/s. Initial caption transitions imply approximate+0.1s storyboard-to-video offset, with quantization/sampling delays; maxgap supports conservative±3.1s navigation uncertainty. Exact whole-record action alignment is unproved. [All41 requirement rows/time index](independent-safari-requirement-time-index-20261011.md) and sibling JSON `safari-requirement-time-index-20261011.json` retain evidence/gaps rather than blanketPASS. Safari loopback serving is not browser-wide network isolation. Four media deliverables are complete; full requirements remain unaccepted.
