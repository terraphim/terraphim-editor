# Browser recording harness

## Recording Context

Prepared 2026-10-09; **not recorded or browser-qualified**. Parent must review,
commit this harness, finish remediation, rebuild and qualify immutable assets
before authorising execution. No final video exists. No Zed harness or GUI
automation is implemented here.

One behaviour per 60–90 second clip: **Unicode-safe local alternatives editing
and one-step undo**. Scripts: `record-browser.py`, `record-safari.py`, shared
`browser_demo_common.py`; fixture: `fixtures/alternatives.json`. The fixture is
fully synthetic and its small local KG is public, not a private corpus.
Fixture document/role loading uses the public editor API before the storyboard;
this is not an OS file-picker demonstration. All subsequent editing uses actual
keyboard and pointer controls; API reads assert exact body text.

Parent supplies a schema-1 JSON manifest, not an auto-selected newest build:

```json
{"schema":1,"sourceHead":"COMMITTED_HEAD","assets":{"index.html":"SHA256", "relative-production-path.js":"SHA256"}}
```

Include every production HTML/JS/WASM/CSS asset, recursive WASM snippet import,
local vendor dependency/font and licence; the production checker additionally
checks its asset contract. Only `index.html` is permitted as HTML; do not include
test-result or component-harness pages. Root glue and graph-worker bytes are
required. The recording serves only manifest-listed paths from a read-once
scratch stage, verifies delivery/stage hashes before and after, and checks
committed scoped source fingerprints and HEAD. Only the exact validated MP4
reservation and its JSON sidecar are excluded, using literal Git pathspecs in
both status and tracked-file hashing. The recording UUID and these two paths
are recorded; no `demos/out` directory or filename-glob exemption is used.
Unexpected files elsewhere in `demos/out`, the fixture tree or harness scope
still fail the gate, as do fixture, README, runner, test, input and HEAD changes.
Unrelated protected user dirt outside the source scopes is not a blocker.
Symlinked output paths/directories are refused and failed-capture cleanup only
removes the recorder's own reserved regular-file inode, never a replacement.
Missing assets/encoder, source
drift, wrong selectors, page errors or failed assertions abort the clip.

Run only when the parent explicitly authorises final recording, one browser at
a time while acceptance tests are idle. Use the existing Python environment:

```sh
DEMO_PYTHON=/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode/safari-perf-runner-venv/bin/python
FIREFOX_DEMO_PYTHON=/opt/homebrew/Caskroom/miniconda/base/bin/python
export PLAYWRIGHT_BROWSERS_PATH=/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode/rolegraph-playwright-browsers
export PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
"$DEMO_PYTHON" demos/record-browser.py --browser chrome --delivery "$FINAL_DELIVERY" --manifest "$FINAL_MANIFEST" --output "demos/out/chrome-alternatives-guided-demo-$(date +%F).mp4" --duration 64 --authorised-final-recording
"$FIREFOX_DEMO_PYTHON" demos/record-browser.py --browser firefox --delivery "$FINAL_DELIVERY" --manifest "$FINAL_MANIFEST" --output "demos/out/firefox-alternatives-guided-demo-$(date +%F).mp4" --duration 64 --authorised-final-recording
"$DEMO_PYTHON" demos/record-safari.py --delivery "$FINAL_DELIVERY" --manifest "$FINAL_MANIFEST" --output "demos/out/safari-alternatives-guided-demo-$(date +%F).mp4" --duration 64 --authorised-final-recording
```

An existing filename is refused. Use an explicit lowercase suffix for a new
take, e.g. `chrome-alternatives-guided-demo-2026-10-09-review.mp4`. Failure removes
the reserved MP4 and leaves an incomplete JSON sidecar. No overwrite, download,
install, remote upload or Safari settings change is performed. Chrome uses the
installed `channel=chrome`; Firefox uses its existing Playwright runtime. Native
Safari uses `/usr/bin/safaridriver` and only its own automation session, never
personal tabs, desktop capture, osascript or Playwright WebKit. If permission is
blocked, report it rather than enabling Remote Automation.

Before Safari editor readiness, viewport capture or storyboard, the recorder
prints and flushes this fixed notice (including on an owner-confirmed final run):

> Safari automation window ready: foreground the dedicated automation window now; waiting up to 10 seconds for visible, focused content. Do not foreground personal Safari tabs; no terminal acknowledgement required.

The owner may foreground only the dedicated automation window. The recorder
does not activate Safari/OS windows, change settings, enumerate or switch tabs,
or request terminal acknowledgement. It pins the current window of its own
WebDriver session in memory and checks that same handle before and after
read-only `document.visibilityState` / `document.hasFocus()` observations.
Both must be true within a 10-second monotonic deadline; each request receives
only the remaining timeout. Missing/changed context, malformed observations,
endpoint failure or exhaustion abort before any screenshot or storyboard.
Sidecars retain finite elapsed precondition timing, attempt count and boolean
observations, not handles, page content or URLs; native capability metadata is
limited to browser name/version and platform name. This is an initial DOM
visibility/focus precondition, not proof of OS occlusion or continuous focus.
The earlier hidden/unfocused dialog timeout has no proven cause; owner-focused
preliminary offline success is not final demo qualification. Source preparation
requires independent review before any final recording; see
`../docs/verification/demo-safari-visibility-guard.md`.

Chrome uses the existing Playwright 1.63 controller; Firefox uses the existing
1.57 controller matching cached Firefox revision 1497. Do not use the 1.63
controller with that older cache or rename cache entries. Registry/API checks
are static readiness only; Firefox capture still requires runtime qualification.
See `../docs/verification/demo-final-controller-readiness.md`.

Offline helper checks (no browser launched):

```sh
python3 -B demos/test_recording.py
```

## Storyboard

1/4 This synthetic draft contains Unicode before a local knowledge-graph term.

2/4 Select the term and request its local alternatives with the visible controls.

3/4 Choose eraser in the alternatives panel; only the selected term changes.

4/4 One undo restores the exact draft, including all Unicode characters.

The same numbered sentences appear as an on-page caption, bottom-right away
from the left alternatives panel. Ctrl+End and Shift+Left select the final term;
Ctrl+Shift+A opens Write_On alternatives, Escape returns focus to the selected
surface, and Ctrl+Shift+G requests the configured local provider. ArrowDown in
the first panel line chooses the next alternative;
Ctrl+Z restores the body in one step. These control semantics require focused
qualification against final assets; **they have not yet been runtime-tested**.

## Verification

Pending execution. JSON sidecars retain source/fixture/manifest identities,
actual user agent/version or native Safari capabilities, and ffprobe codec,
dimensions, fps, duration and bytes. Encoder checks require existing FFmpeg
libx264 and Playwright's cached recording encoder before launch. Output must be
H.264, yuv420p, 1140×720, 15fps encoding, faststart, 60–90 seconds.

Initial static preflight on 2026-10-09 found existing FFmpeg/libx264 and ffprobe
but lacked `ffmpeg-1011/ffmpeg-mac`. The parent subsequently qualified the
already-installed Homebrew FFmpeg 9.0.2 with the actual Playwright 1.63 Matroska
MJPEG stdin-to-VP8 encoding flags, then created only the approved scratch-cache
symlink to `/opt/homebrew/bin/ffmpeg` (no install/download or existing-path
overwrite). This resolves **encoder preflight only**, not browser capture or
actual-control qualification. Parent compatibility records are
`/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode/demo-encoder-compatibility-cfqosw2a/results.json`
and `encoder.json` in that directory. The recorded resolved binary SHA-256 is
`1ede5c7fd4e97d832ec69c97c78231875de7d69c55423158f5f4a4d923227dd5`.

Future sidecars disclose the Playwright registry encoder path, symlink target,
resolved target, binary SHA-256 and actual version as an externally supplied
system encoder, **not a bundled Playwright encoder**. Its identity is checked
again after capture. Eighteen offline helper tests pass (fourteen-test baseline); no browser launch or
runtime control result is implied. Lifecycle tests use isolated Git repositories,
unit-only synthetic assets/checker seams and non-media sentinel output bytes;
they prove output retention and drift cleanup, not production qualification or
video encoding. The P1 regression first failed with `Source/manifest drift
during recording` after writing only the owned sentinel output, before the fix.
Production graph-worker readiness and exact UTF-16
selection offsets are asserted by the runner before the shown replacement.

Safari is explicitly a **sampled viewport video**: actual WebDriver PNGs have
monotonic start/completion times and hashes retained in raw JSON, with measured
average cadence and maximum gaps. Less than two actual frames/second fails.
FFmpeg repeats previous screenshots for their genuine elapsed intervals; 15fps
encoding does not mean 15 fresh Safari captures/second. Raw PNGs are transient
scratch inputs, not invented animation or substitutions.

Review frames for unexpected content before accepting or committing anything.
Clips over 2 MB remain local and flag `storageReviewRequired`; ask the parent
for a storage decision. No automatic remote release upload or pixel-quality
truncation. Parent records reviewed byte sizes and branch/issue/commit context
here when real evidence exists.

## Coverage Shown

Planned, not yet demonstrated:

- A synthetic non-BMP character and combining accent precede a local KG term.
- Real public alternatives controls replace only `rubber` with `eraser`.
- One native undo restores the complete exact original Unicode body.

## Scope Boundary

No claim of full Write_On conformance, protected trim, persistence, OS pickers,
LLM provider execution, offline acceptance or Zed GUI integration. Chrome and
Firefox abort non-loopback page requests; Safari serves loopback assets but
has no asserted browser-wide network denial. Video alone is not acceptance
testing, installed-extension provenance or performance evidence. Final native
Safari screenshot availability/cadence and all browser controls remain gated
on the parent's review and focused runtime qualification.

## Approved expanded walkthrough (2026-10-10)

Alex approved `docs/plans/alternative-control-full-functionality-demos.md`, one independent review agent and window-only Zed capture. `--full-functionality --duration 240` selects the chaptered browser storyboard instead of the original 64-second example. The expanded path permits 180–1800-second H.264 clips and records actual output checks, independent Python word counts/UTF-16 cut application, and explicit uncovered criteria. It does not promote a passed check to full requirement acceptance. Setup API use and API save/reopen bridge are disclosed in captions; neither is an OS picker demonstration.

`demos/test_full_demo.py` tests the independent output-oracle helpers. Native `record-zed-window.swift` uses ScreenCaptureKit's desktop-independent window filter for only the dedicated public `demo — writing.md` or `demo — trim-long.md` Zed window, with no system/microphone audio or child windows. A watchdog bounds discovery/start/stop; a 100ms title check stops on missing/unapproved title. Sampling has residual gaps, so a public-only window and complete footage privacy audit remain required. Only CUA supplies native editor input. The native self-test checks title/bundle selection; a five-second real capture succeeded on 2026-10-10 and was probed/visually inspected. This is capture readiness, not the requested completed Zed walkthrough.

The host shell sandbox denies capture and loopback test servers. Approved scoped capture/test execution requires sandbox escalation, without changing macOS settings. A profile avatar in the Zed window must be masked in final media, with the edit disclosed. Failed/partial captures are never accepted as demos.
