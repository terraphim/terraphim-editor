# Browser and native Zed demo recording

Updated 2026-10-11. Four local application walkthroughs have been recorded: installed Chrome, cached Firefox, native Safari and the existing Zed extension. **Recorded media does not mean all 41 requirements pass.** Safari retains two failed native-drag receipt checks. The refreshed Zed capture qualifies finite immediate-save anchors, byte-identical repeated export and Done fading after reopen; complete acceptance is still outstanding. The Zed fork remains separate.

Current outputs and all four independent reviews are linked in the [refreshed checkpoint](../docs/verification/refreshed-demos-20261011/checkpoint.md). Historical media, JSON sidecars, requirement-time indices and retained failed attempts are linked in [delivery status](/Users/alex/.codex/visualizations/2026/10/09/01a1221d-a378-7a00-8943-4c42925945f0/full-functionality-demos-20261010/README.md). See the [41-requirement ledger](../docs/verification/independent-requirement-ledger-20261010.md), [media audit](../docs/verification/independent-final-demo-media-audit-20261010.md) and [admission report](../docs/verification/independent-public-byte-admission-20261010.md). Historical failed takes remain evidence; later successful operations do not erase failures.

## Browser harness

`record-browser.py` controls Chrome/Firefox; `record-safari.py` controls installed native Safari with `/usr/bin/safaridriver`. `browser_demo_common.py` supplies immutable-asset staging, source/manifest checks, output reservation and encoding. `full_demo.py` supplies the expanded walkthrough and independent Python word-count, UTF-16 cut and insertion oracles.

The original 60–90-second example demonstrates a Unicode-safe local KG replacement and undo. `--full-functionality --duration 180` selects the expanded 180–1800-second walkthrough: real alternatives creation/editing, article agreement, local-provider output, ghosts/revive, Overflow keyboard and drag actions, six Lab categories, five trim levels, Keep/Walk/Make cuts/Done, and persistence/export through the disclosed API bridge. Missing visual, semantic, shortcut, OS picker and ambiguity criteria remain explicit. Public fixtures and preloaded annotations are inputs; subsequent independently checked editing results are outputs.

Use the existing environments, one browser at a time:

```sh
DEMO_PYTHON=/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode/safari-perf-runner-venv/bin/python
FIREFOX_DEMO_PYTHON=/opt/homebrew/Caskroom/miniconda/base/bin/python
export PLAYWRIGHT_BROWSERS_PATH=/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode/rolegraph-playwright-browsers
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
"$DEMO_PYTHON" demos/record-browser.py --browser chrome --delivery "$FINAL_DELIVERY" --manifest "$FINAL_MANIFEST" --output "demos/out/chrome-alternatives-guided-demo-$(date +%F)-new.mp4" --duration 180 --full-functionality --authorised-final-recording
"$FIREFOX_DEMO_PYTHON" demos/record-browser.py --browser firefox --delivery "$FINAL_DELIVERY" --manifest "$FINAL_MANIFEST" --output "demos/out/firefox-alternatives-guided-demo-$(date +%F)-new.mp4" --duration 180 --full-functionality --authorised-final-recording
"$DEMO_PYTHON" demos/record-safari.py --delivery "$FINAL_DELIVERY" --manifest "$FINAL_MANIFEST" --output "demos/out/safari-alternatives-guided-demo-$(date +%F)-new.mp4" --duration 180 --full-functionality --authorised-final-recording
```

Chrome uses installed `channel=chrome` and the existing Playwright 1.63 controller. Firefox uses its cached runtime and matching 1.57 controller. Do not rename runtime caches or download replacements. Existing FFmpeg/ffprobe encode H.264/yuv420p, 1140×720, 15fps with faststart and no audio. Encoder identity is disclosed and checked again after capture. No upload, publication, package installation or Safari settings change is part of recording.

## Evidence and capture safeguards

Supply an explicit schema-1 manifest with committed source HEAD and SHA256 hashes for every production HTML/JS/WASM/CSS/font/vendor/license asset. Only `index.html` is served as HTML. The read-once scratch stage serves only manifest-listed assets. Delivery, staged files, manifest and committed scoped source fingerprints must match before and after capture. Only the exact validated MP4 reservation and JSON sidecar are excluded from source status; unrelated files in `demos/out` remain blockers. Unrelated protected user changes outside frozen scopes are preserved.

Existing evidence paths are refused. Symlinked output paths are refused; inode checks prevent writing over a replaced reservation. Capture failure removes only the owned reserved regular-file inode and preserves an incomplete JSON sidecar. After recording, copy and hash-verify the owned pair into the evidence directory before removing that pair from the recorder scope. Source changes require a newly reviewed commit and admission for any subsequent capture; documentation updates after a take do not change its recorded source identity.

Safari captures actual viewport PNGs from its own visible, focused automation window; it never substitutes Playwright WebKit or records personal tabs. Its initial ten-second visibility/focus guard aborts before screenshot/storyboard if unmet. Do not bypass the Mac lock or change remote-automation settings. PNG timestamps/hashes, average actual cadence and maximum gaps are retained. An average of at least two actual frames/second is required; 15fps encoding duplicates real screenshots for their elapsed intervals and is not 15 fresh captures/second. Chrome/Firefox deny non-loopback page requests; Safari's loopback asset server is not proof of browser-wide network isolation.

The failure-visible Safari recording uses `--record-failed-drag-chapters`: only a failed first trusted drag-start/drop receipt assertion may continue. The exact FAIL and receipts remain, dependent checks are NOT EXECUTED, a FAILED caption appears, and independent chapters resume from a new disclosed public fixture. Requirement failures remain sticky; `acceptanceStatus` stays FAIL even though `status=complete` describes a finished recording. Payload/body/history assertions, source/asset drift, transport, privacy and other failures remain fatal. This is evidence collection, not a passing acceptance gate.

The optional Safari `--native-drag-handoff` uses fresh bounded file coordination for actual CUA input and does not substitute an acknowledgement for output checks. It is mutually exclusive with failed-drag continuation. A real attempt was blocked by Safari's remote-control warning; no gesture acknowledgement was written and the take timed out. No security setting was bypassed.

## Native Zed

`record-zed-window.swift` captures only a desktop-independent ScreenCaptureKit window whose bundle is `dev.zed.Zed` and exact title is `demo — writing.md`, `demo — trim-long.md` or `demo — writing.clean.md`. It records no system/microphone audio or child windows. A watchdog bounds capture and a 100ms title check stops on missing/unapproved titles. Only CUA supplies native editing input. The source has six native selection self-tests; compiled recorder identity and actual installed extension/server identity are retained with evidence.

The historical 15-minute walkthrough concatenates three genuine native takes at disclosed boundaries, before lock/title-guard failures, and masks the window avatar. It contains actual failure and recovery footage, not synthesized successful UI. Saved-output literal comparisons supplement sampled media inspection. Stock Zed's LSP/code actions are not full visual parity with the browser; custom dots, panels, opacity and Lab shell belong to separately deferred fork work.

## Verification

Offline helper tests launch no browser:

```sh
python3 -m unittest discover -s demos -p 'test_*.py'
```

The current recorder module has 24 passing helper tests. Actual refreshed recording evidence remains separate: Chrome/Firefox each have 134 finite PASS checks plus six semantic observations; Safari has 122 PASS, two FAIL and six semantic observations. The fresh Zed capture is one continuous window-only take of approximately 679 seconds, with the finite output checks and qualifications in the refreshed checkpoint. Review media hashes, probe, cadence, chapter transitions, actual output frames and unrelated/private pixels before acceptance. Sampled frame review is not every-frame certification. Sidecar times are storyboard-relative; requirement indices disclose capture-offset uncertainty. No source-video requirement may be marked passed merely because a control, fixture, menu or caption appears.

## Short storyboard captions

These describe the original short recording, not acceptance results:

1/4 This synthetic draft contains Unicode before a local knowledge-graph term.

2/4 Select the term and request its local alternatives with the visible controls.

3/4 Choose eraser in the alternatives panel; only the selected term changes.

4/4 One undo restores the exact draft, including all Unicode characters.

The refreshed full storyboard also queries newly authored alternatives through the actual shared KG synonym index, records their `synonyms::` output and typed concept IDs, verifies generated-group removal on Undo, and queries the reconstructed group after reopen.
