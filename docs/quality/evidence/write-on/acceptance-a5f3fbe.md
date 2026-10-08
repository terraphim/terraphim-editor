# Phase 5 acceptance testing: terraphim-editor against the Write_On video (final)

Date: 2026-10-08. Evidence only; no product code was changed.

**Final run: commit `a5f3fbe`** (gitea/main, "Fix #107: UAT visual fidelity (paragraph rule, dark plain mode, panel tabs and status, Lab trim row) (#109)"), which includes #99 (pull back copies, with separation), #102 (Ctrl+Shift+D, full-height Overflow), #108 (live inline add, focus return) and #109. Built with `trunk build` and served from `target/trunk-dist` on 127.0.0.1:8093. All 74 step results below are from `a5f3fbe`; the first run, on `cd84e6d`, is summarised in the history section.

Result: **73 Matches, 1 Deviates, 0 Fails**. The one Deviates (S5.1) is by design and its provider case (S5b) matches.

## Method

- Real Chrome 155 (headless, new mode) driven by chromedriver 155.0.8059.39 from the wasm-pack cache, over the plain W3C WebDriver HTTP protocol with Python `urllib` (`uat/wd.py`). Viewport 1140x900, device scale factor 1.
- Input is real: pointer clicks, right-clicks, hover, and WebDriver key actions (`Ctrl+Shift+D`, `Ctrl+Enter`, `Ctrl+/`, `Alt+arrows`, typed text). Assertions are read back from the DOM, the surface text, the save event and the export textarea.
- The document is `tests/fixtures/persistence/full.md`, loaded with `terraphimEditor.openDocument(text, name)` because the Open control uses the native file picker; the control itself is clicked and its `te:open` event checked (S8). S4b uses a longer two-paragraph document typed in the same way; S5b loads `tests/fixtures/kg/thesaurus.json` with `loadThesaurus` and registers a host provider through `registerAlternativesProvider` with a real in-page function.
- For each step: a screenshot (`uat/shots/<scenario>-<n>.png`), the assertion, and the video frame at the stated time (`uat/frames/t_M-SS.png`, extracted from the local-only `writeon.mp4` with ffmpeg, scaled to 1140px). Side-by-side composites (ours left, video right) are in `uat/cmp-f3/`. Nothing from the video is committed.
- Verdicts: **Matches**, **Deviates** (works, differs from the video or the requirement; described), **Fails** (requirement not met).
- Reproduce: `python3 -I uat/run_uat.py http://127.0.0.1:8093 results.json`; then `uat/mkcmp.py results.json outdir`. Results: `uat/results-final3.json`.

## Scenario summary (commit `a5f3fbe`)

| Scenario | Steps | Matches | Deviates | Fails |
|---|---|---|---|---|
| S1 | 5 | 5 | 0 | 0 |
| S2 | 7 | 7 | 0 | 0 |
| S3 | 8 | 8 | 0 | 0 |
| S4 | 10 | 10 | 0 | 0 |
| S4b | 1 | 1 | 0 | 0 |
| S5 | 7 | 6 | 1 | 0 |
| S5b | 7 | 7 | 0 | 0 |
| S6 | 10 | 10 | 0 | 0 |
| S7 | 7 | 7 | 0 | 0 |
| S8 | 4 | 4 | 0 | 0 |
| S9 | 4 | 4 | 0 | 0 |
| S10 | 4 | 4 | 0 | 0 |

S1 Write_On look; S2 underlines and dots; S3 hover cycling and a/an; S4 alternatives panel (S4b paragraph indicator on a long paragraph); S5 Ghost it and Revive (S5b AI alternatives with a provider); S6 Overflow; S7 Lab popover and trim card; S8 save, open and export; S9 plain mode and Ctrl+Shift+L; S10 word counter.

## Defects: found on `cd84e6d`, status on `a5f3fbe`

| ID | Requirement | Defect (as found) | Status |
|---|---|---|---|
| D1 | R-2.1, R-7.1 | Plain mode was a white page; the video's is dark | **Fixed (#109)**: plain mode is dark navy with cream text and the dim counter (`cmp-f3/S1-01_0-05.png`) |
| D2 | R-3.4 | Paragraph rule and dot column drawn over the first letters | **Fixed (#109)**: `te-ind-rule` sits left of the paragraph and `te-ind-col` left of it (S4.10, S4b.1 assert the geometry) |
| D3 | R-7.3 | Menu has three items, the video four | **Not a defect (by design, #83)**; with the KG thesaurus or a host provider the fourth item "AI alternatives for selection  Ctrl+Shift+G" appears, works, and one Ctrl+Z undoes it (S5b, all 7 pass). S5.1 stays Deviates because that scenario loads no provider |
| D4 | R-4.3 | A typed alternative did not show in the document | **Fixed (#108)**: live inline add; S4.5, S4.6 pass |
| D5 | R-4.1, R-4.2 | Panel tabs wrapped, status line, top-aligned list, hint position | **Fixed (#109)**: one-line tabs, centred list, hint at the foot (`cmp-f3/S4-01_1-30.png`) |
| D6, D6b | R-6.1 | Overflow was a floating card; Ctrl+Shift+D unbound | **Fixed (#102)**: full height, flush right, Ctrl+Shift+D toggles (S6.1, S6.2, S6.10) |
| D7 | R-6.4 | Pull back glued the text to its neighbours | **Fixed (#99)**: copy, with blank-line separation at a line start; the pulled paragraph is its own paragraph (`cmp-f3/S6-07_4-23.png`, S6.9) |
| D8 | R-7.2 | X and LinkedIn icons absent | **Not a defect**: out of scope (requirements section 1, recorded in the traceability) |
| D9 | R-8.1 | Lab popover extra text and wrapped trim row | **Fixed (#109)**: no description line, one row of five trim buttons (`cmp-f3/S7-01_5-00.png`) |

Observation (not a defect): `Escape` closes the alternatives panel and returns focus to the control that opened it, as documented, so `Ctrl+Z` straight afterwards does nothing until the page is focused again (S5b.7 clicks into the text first).

## History: first run on `cd84e6d`

60 PASS and 7 FAIL by assertion; by visual verdict 54 Matches, 8 Deviates, 5 Fails (S4.10 paragraph rule, S4b.1, S6.1, S6.9, S6.10 failed; the rest deviated). The defect table above lists what changed.

## Cannot be validated here

- The native Open and Save pickers (File System Access API): the Open control's event is checked, the dialog is not. Save is checked through its `te:saved` text; the download path was not exercised.
- Drag and drop of Overflow text into the page and of a page selection onto the panel (R-6.4): WebDriver cannot perform HTML5 drag and drop reliably in headless Chrome.
- The X and LinkedIn icons (out of scope).
- Pixel fidelity of the dots and colours: the video is a compressed 3096x1952 recording, and the dots at 1140px are a few pixels; structure, counts, positions and computed colours were checked, not individual pixels.
- Cursive and monospace faces versus the video's (ours: Caveat).
- Live Zed provisioning (a different suite: the zed-terraphim PR #32).

## Step table (commit `a5f3fbe`)

Columns: step id, requirement, step, assertion result with the value read back, screenshot (relative to `uat/`), video time of the compared frame, verdict.

| Step | Req | Step | Assertion | Screenshot | Video | Verdict |
|---|---|---|---|---|---|---|
| S1.1 | R-2.1 | plain mode: only the text and the word/char counter are visible | pass: `counter='26 words 164 chars' corners={'dots': 'hidden', 'lab': 'hidden', 'save': 'hidden', 'xyz': 'h` | `shots/S1-01.png` | 0:05 | **Matches** |
| S1.2 | R-2.1 | clicking the counter turns Write_On mode on (page goes dark) | pass: `['write-on', 'write-on']` | `shots/S1-02.png` | 0:25 | **Matches** |
| S1.3 | R-10 | design tokens: bg #0a0d1c, cream text, monospace, line height 1.75 | pass: `{'bg': 'rgb(10, 13, 28)', 'color': 'rgb(232, 217, 196)', 'font': '"JetBrains Mono", "IBM Plex Mono",` | `shots/S1-03.png` | 2:35 | **Matches** |
| S1.4 | R-7.2 | corner controls: counter TL, ●●● and M↓ top-centre, keyboard TR, save/open BL, LAB bottom-centre, XYZ BR | pass: `{'counter': [24, 20, 189, 38], 'dots': [524, 20, 558, 48], 'kbd': [1086, 20, 1116, 48], 'lab': [541,` | `shots/S1-04.png` | 2:35 | **Matches** - All corner controls match. The video's X and LinkedIn icons are out of scope (requirements section 1; recorded in the traceability). |
| S1.5 | R-7.1 | text column about 70 characters wide, centred, generous top margin | pass: `cols=74.7 left=189 right_gap=189 first_line_top=112` | `shots/S1-05.png` | 2:35 | **Matches** |
| S2.1 | R-3.2 | one dot per alternative (original included): sentence 3, word 5, word 3 | pass: `dots=[(3, 1), (5, 3), (3, 1)] spans=[{'k': 'te-heading te-ind-sentence te-ind-headline', 't': "Why i` | `shots/S2-01.png` | 0:55 | **Matches** |
| S2.2 | R-3.3 | exactly one lit dot per row, at the active index (1, 3, 1) | pass: `[(3, 1), (5, 3), (3, 1)]` | `shots/S2-02.png` | 1:00 | **Matches** |
| S2.3 | R-3.1 | a thin lavender-grey underline under the span | pass: `{'bb': '0px none rgb(232, 217, 196)', 'bs': 'none', 'color': 'rgb(232, 217, 196)', 'td': 'underline'` | `shots/S2-03.png` | 0:55 | **Matches** |
| S2.4 | R-3.2 | word dots centred under the word; sentence/headline dots right-aligned at the end of the underline | pass: `word dots cx=295 vs word cx=295; headline dots right=576 vs span right=576` | `shots/S2-04.png` | 0:25 | **Matches** |
| S2.5 | R-3.5 | headline bold, slightly larger, same colour as body | pass: `{'bcolor': 'rgb(232, 217, 196)', 'bfs': '17px', 'color': 'rgb(232, 217, 196)', 'fs': '19.55px', 'w':` | `shots/S2-05.png` | 0:25 | **Matches** |
| S2.6 | R-3.1 | hover leaves the underline unchanged (screenshot while hovering 'struggle') | pass: `visual` | `shots/S2-06.png` | 0:55 | **Matches** |
| S2.7 | R-3.6 | (inferred) clicking a dot jumps to that alternative: struggle -> pressure | pass: `The pressure in a café draft 𝄞 is an eraser holding `code` t` | `shots/S2-07.png` | 1:45 | **Matches** |
| S3.1 | R-2.4 | hover 'struggle' + ArrowDown cycles in place to the next alternative (friction) | pass: `The friction in a café draft 𝄞 is an eraser holdin` | `shots/S3-01.png` | 0:45 | **Matches** |
| S3.2 | R-2.4 | ArrowUp twice walks back through the alternatives (struggle -> challenge) | pass: `The challenge in a café draft 𝄞 is an eraser holdi` | `shots/S3-02.png` | 0:50 | **Matches** |
| S3.3 | R-3.3 | the lit dot follows the active alternative (5-dot row: challenge = index 2) | pass: `[2]` | `shots/S3-03.png` | 1:00 | **Matches** |
| S3.4 | R-2.4 | Alt+ArrowDown with the caret in the word cycles too (keyboard path) | pass: `The struggle in a café draft 𝄞 is an eraser holdin` | `shots/S3-04.png` | 0:50 | **Matches** |
| S3.5 | R-2.6 | a/an fix-up: 'an eraser' -> 'a thumbtack' when cycling | pass: `is a thumbtack` | `shots/S3-05.png` | 1:55 | **Matches** |
| S3.6 | R-2.6 | and back: 'a paperclip' -> reversible | pass: `is a paperclip` | `shots/S3-06.png` | 1:55 | **Matches** |
| S3.7 | R-2.6 | 'an eraser' restored | pass: `is an eraser` | `shots/S3-07.png` | 1:55 | **Matches** |
| S3.8 | R-2.4 | headline (sentence span) cycles in place: ArrowUp -> original 'Shouldn't everything be obvious?' | pass: `# Shouldn't everything be obvious?` | `shots/S3-08.png` | 0:40 | **Matches** |
| S4.1 | R-4.1 | ●●● opens a full-height left panel about 18% of the window (205px at 1140) | pass: `{'b': 900, 'h': 900, 'r': 205.1875, 'w': 205.1875, 'x': 0, 'y': 0}` | `shots/S4-01.png` | 1:30 | **Matches** |
| S4.2 | R-4.2 | tabs Word · Sentence · Paragraph in a display face; Word active (lavender) | pass: `[{'c': 'rgb(140, 134, 230)', 'ff': 'Caveat, "Segoe Script", "Bradley Hand", ', 'sel': 'true', 't': '` | `shots/S4-02.png` | 1:30 | **Matches** |
| S4.3 | R-4.3 | list: one alternative per line (tension, pressure, challenge, struggle, friction) plus an empty add line | pass: `['tension', 'pressure', 'challenge', 'struggle', 'friction', '']` | `shots/S4-03.png` | 1:30 | **Matches** |
| S4.4 | R-4.4 | provenance glyphs: dim dot for original/human, robot for AI (struggle, friction) | pass: `['dot', 'dot', 'dot', 'bot', 'bot', 'dot']` | `shots/S4-04.png` | 3:20 | **Matches** |
| S4.5 | R-4.3 | typing 'obstacle' + Enter adds an alternative at once as a new dot (6) in the document | pass: `dots=[3, 3, 6]; document still shows: 'The obstacle in a café draft 𝄞'` | `shots/S4-05.png` | 1:25 | **Matches** |
| S4.6 | R-4.3 | the new alternative is also shown in the document as the active candidate (video: 'it immediately appears') | pass: `The obstacle in a café draft 𝄞 is an era` | `shots/S4-06.png` | 1:25 | **Matches** |
| S4.7 | R-4.5 | ArrowUp in the panel moves the active alternative and the document updates live (to 'obstacle') | pass: `The obstacle in a café draft 𝄞 is an era` | `shots/S4-07.png` | 1:35 | **Matches** |
| S4.8 | R-4.5 | ArrowUp again (to 'friction') | pass: `The friction in a café draft 𝄞 is an era` | `shots/S4-08.png` | 1:35 | **Matches** |
| S4.9 | R-4.1 | the main text dims slightly while the panel is open | pass: `{'filter': 'none', 'op': '0.7', 'root': ''}` | `shots/S4-09.png` | 1:30 | **Matches** |
| S4.10 | R-2.3/R-3.4 | Paragraph tab: a paragraph alternative gets a thin gutter rule and a dot column to its left, no underline | pass: `{'col': {'h': 9, 'r': 189, 'w': 3, 'x': 186, 'y': 240.953125}, 'para': {'h': 22, 'r': 355.8125, 'w':` | `shots/S4-10.png` | 2:00 | **Matches** |
| S4b.1 | R-3.4 | a paragraph with alternatives: no underline; thin vertical rule in the gutter spanning the paragraph, dot column left of it | pass: `{'others': ['te-indicator-host {"x":0,"y":0,"width":1140,"height":900,"top":0,"right":1140,"bottom":` | `shots/S4b-01.png` | 2:00 | **Matches** |
| S5.1 | R-7.3 | right-click on a selection opens the menu: Alternatives, AI alternatives, Ghost it, Stash (with shortcuts) | FAIL: `sel='in a café' items=['Alternatives for selection Ctrl+Shift+A', 'Ghost it Ctrl+/', 'Stash this in ` | `shots/S5-01.png` | 2:36 | **Deviates** - Three items without a provider: by design (#83) the AI item is offered only on KG terms or with a host provider. With a provider it appears and matches the video (S5b.1, S5b.4). |
| S5.2 | R-5.1 | 'Ghost it' dims the selection (about 10% opacity); the text stays in the document | pass: `ghosts 3->4 {'color': 'color(srgb 0.909804 0.85098 0.768627 / 0.1)', 'opacity': '1'}` | `shots/S5-02.png` | 2:40 | **Matches** |
| S5.3 | R-5.2 | right-click on ghosted text offers Revive | pass: `['Alternatives for selection Ctrl+Shift+A', 'Revive Ctrl+/', 'Stash this in Overflow Ctrl+Shift+X']` | `shots/S5-03.png` | 2:45 | **Matches** |
| S5.4 | R-5.2 | Revive restores full opacity: no ghost left over 'café' | pass: `['draft 𝄞 is an ', 'eraser', 'This whole paragraph might go. It hedges and repeats itself.']` | `shots/S5-04.png` | 2:50 | **Matches** |
| S5.5 | R-5.1 | Ctrl+/ ghosts the selection ('holding') | pass: `['draft 𝄞 is an ', 'eraser', 'holding', 'This whole paragraph might go. It hedges and repeats itself` | `shots/S5-05.png` | 2:40 | **Matches** |
| S5.6 | R-5.2 | Ctrl+/ again revives | pass: `['draft 𝄞 is an ', 'eraser', 'This whole paragraph might go. It hedges and repeats itself.']` | `shots/S5-06.png` | 2:48 | **Matches** |
| S5.7 | R-5.3 | ghosted text stays in the document and is counted (counter unchanged) | pass: `26 words 164 chars ←` | `shots/S5-07.png` | 2:40 | **Matches** |
| S5b.1 | R-7.3 | with the KG thesaurus loaded, selecting the KG term 'café' offers the fourth item 'AI alternatives for selection  Ctrl+Shift+G' | pass: `{'concepts': 7, 'name': 'writing-fixture', 'skipped': []} items=['Alternatives for selection Ctrl+Sh` | `shots/S5b-01.png` | 2:37 | **Matches** |
| S5b.2 | R-8.6 | 'AI alternatives' appends the thesaurus synonyms to 'café' as bot-marked (ai) alternatives; the document text is unchanged | pass: `[{"alts":[["café","original",null],["coffee shop","ai","kg"],["coffeehouse","ai","kg"]],"active":0}]` | `shots/S5b-02.png` | 3:20 | **Matches** |
| S5b.3 | R-4.6 | one Ctrl+Z removes the AI alternatives again (the span disappears, nothing else changes) | pass: `café spans after undo: 0` | `shots/S5b-03.png` | 3:30 | **Matches** |
| S5b.4 | R-7.3/R-8.7 | with a host provider registered, any selection ('holding', not a KG term) offers 'AI alternatives for selection' | pass: `['Alternatives for selection Ctrl+Shift+A', 'AI alternatives for selection Ctrl+Shift+G', 'Ghost it ` | `shots/S5b-04.png` | 2:37 | **Matches** |
| S5b.5 | R-8.6/R-8.7 | Ctrl+Shift+G asks the host provider with the selected text and appends its alternatives as ai lines with their models | pass: `[{"alts":[["holding","original",null],["carrying","ai","uat-model"],["bearing","ai","uat-model"],["b` | `shots/S5b-05.png` | 3:20 | **Matches** |
| S5b.6 | R-4.4 | in the panel the provider's lines carry the robot glyph, the original a dim dot | pass: `['holding', 'carrying', 'bearing', 'binding', ''] ['dot', 'bot', 'bot', 'bot', 'dot']` | `shots/S5b-06.png` | 3:20 | **Matches** |
| S5b.7 | R-4.6 | one Ctrl+Z (page focused again) removes the provider's alternatives | pass: `holding spans after undo: 0; focus after Esc was on 'te-chrome-control te-chrome-dots'` | `shots/S5b-07.png` | 3:35 | **Matches** |
| S6.1 | R-6.1 | Ctrl+Shift+D toggles the Overflow panel (shortcut shown in the XYZ tooltip) | pass: `open=True` | `shots/S6-01.png` | 3:52 | **Matches** |
| S6.2 | R-6.1 | right-hand panel: same 18% width as the alternatives panel, full height, title 'Overflow' | pass: `{'b': 900, 'h': 900, 'r': 1140, 'w': 205.1875, 'x': 934.8125, 'y': 0} title='Overflow'` | `shots/S6-02.png` | 3:55 | **Matches** |
| S6.3 | R-6.4 | footer hint reads 'ctrl+⏎ or drag into the page to use' | pass: `ctrl+⏎ or drag into the page to use` | `shots/S6-03.png` | 4:05 | **Matches** |
| S6.4 | R-6.2/R-6.5 | the overflow loaded from the document block is shown (Stashed idea…) | pass: `Stashed idea.  ```rust fn main() {} ``` https://example.com/` | `shots/S6-04.png` | 4:05 | **Matches** |
| S6.5 | R-6.3 | context menu offers 'Stash this in Overflow  Ctrl+Shift+X' | pass: `['Alternatives for selection Ctrl+Shift+A', 'Ghost it Ctrl+/', 'Stash this in Overflow Ctrl+Shift+X'` | `shots/S6-05.png` | 4:05 | **Matches** |
| S6.6 | R-6.3 | stash MOVES the text: gone from the page, appended to the end of the overflow after a blank line | pass: `'``rust\nfn main() {}\n```\nhttps://example.com/a?b=c\n\nin a café'` | `shots/S6-06.png` | 4:10 | **Matches** |
| S6.7 | R-6.4 | Ctrl+Enter inserts the overflow selection ('Stashed idea.') at the document caret | pass: `' eraser holding `code` together.\n\nStashed idea.\n\nThis whole paragraph might go. It hedges and r` | `shots/S6-07.png` | 4:23 | **Matches** |
| S6.8 | R-6.4 | pull back COPIES: the overflow still holds the paragraph | pass: `'Stashed idea.\n\n```rust\nfn main() {}\n```\n'` | `shots/S6-08.png` | 4:23 | **Matches** |
| S6.9 | R-6.4 | pulled text is set off as its own paragraph (blank line before/after) when pulled into a line start | pass: `'r.\n\nStashed idea.\n\nThis'` | `shots/S6-09.png` | 4:23 | **Matches** |
| S6.10 | R-6.1 | Ctrl+Shift+D closes it again | pass: `True` | `shots/S6-10.png` | 4:27 | **Matches** |
| S7.1 | R-8.1 | LAB opens a popover above the bottom-centre control: 'The Lab guide…' / 'what each idea does' | pass: `{'b': 842, 'h': 327.9375, 'r': 826, 'w': 512, 'x': 314, 'y': 514.0625} title='The Lab guide…' sub='w` | `shots/S7-01.png` | 5:00 | **Matches** |
| S7.2 | R-8.2 | six mark actions | pass: `['Fix punctuation and typos', 'Mark the weakest sentences', 'Mark sentences that run long', 'Mark co` | `shots/S7-02.png` | 5:00 | **Matches** |
| S7.3 | R-8.3 | five trim levels: Original, Slight trim ~10%, Tighten more ~20%, Even sharper ~30%, Cut in half ~50% | pass: `['Original', 'Slight trim ~10%', 'Tighten more ~20%', 'Even sharper ~30%', 'Cut in half ~50%']` | `shots/S7-03.png` | 5:00 | **Matches** |
| S7.4 | R-8.2 | 'Mark hedges and filler' marks (nothing is rewritten): body unchanged | pass: `marks/chips=3` | `shots/S7-04.png` | 5:00 | **Matches** |
| S7.5 | R-8.4 | choosing a level shows the status card bottom-centre: level, 'N → M words · −X%', hint, Make the cuts · Walk through · Done | pass: `'Slight trim 26 → 21 words · −19% Faded words would go. Click one to keep it. Make the cuts Walk thr` | `shots/S7-05.png` | 5:35 | **Matches** |
| S7.6 | R-8.4 | spans proposed for dropping are shown with the ghost treatment (nothing rewritten) | pass: `cuts=1` | `shots/S7-06.png` | 5:35 | **Matches** |
| S7.7 | R-8.5 | Done dismisses the card | pass: `` | `shots/S7-07.png` | 5:30 | **Matches** |
| S8.1 | R-9.1/R-2.5 | save: the file is the Markdown body plus the trailing terraphim-alternatives block (annotations, ghosts, overflow) | pass: `events=['te:saved'] len=2804` | `shots/S8-01.png` | 4:35 | **Matches** |
| S8.2 | R-9.3 | export view: only active alternatives, no ghosted text, no annotation block, no overflow | pass: `"# Why isn't everything obvious?\n\nThe struggle in a café holding `code` together.\n\n"` | `shots/S8-02.png` | 4:35 | **Matches** |
| S8.3 | R-9.3 | export equals the committed full.export.md fixture | pass: `got="# Why isn't everything obvious?\n\nThe struggle in a café holding `code` together." want="# Why` | `-` | 4:35 | **Matches** |
| S8.4 | R-7.2 | the folder control fires te:open (the picker itself is native and cannot be driven by WebDriver) | pass: `['te:open']` | `shots/S8-04.png` | 4:35 | **Matches** - The native file picker itself cannot be driven by WebDriver. |
| S9.1 | R-2.1 | clicking the counter again returns to plain mode with no toolbar or preview | pass: `[None, 'off']` | `shots/S9-01.png` | 0:05 | **Matches** |
| S9.2 | R-7.1 | Ctrl+Shift+L shows the toolbar and live preview in plain mode | pass: `['on', True]` | `shots/S9-02.png` | 0:05 | **Matches** - The legacy toolbar and preview appear; the video has no such view to compare. |
| S9.3 | R-7.1 | Ctrl+Shift+L again hides them | pass: `off` | `shots/S9-03.png` | 0:05 | **Matches** |
| S9.4 | R-7.1 | in Write_On the toolbar and preview stay hidden even with Ctrl+Shift+L | pass: `tools=on (visual)` | `shots/S9-04.png` | 2:35 | **Matches** - Visual. |
| S10.1 | R-2.1 | counter 'N words M chars' in the top-left, dim monospace | pass: `'26 words 164 chars' (body: 28 whitespace words, 164 chars)` | `shots/S10-01.png` | 0:05 | **Matches** |
| S10.2 | R-5.1/D3 | ghosted text counts (ghosts are in the body) | pass: `('26', '164') vs chars=164` | `-` | 0:05 | **Matches** |
| S10.3 | R-2.1 | typing updates the counter live (+3 words, +19 chars) | pass: `26 words 164 chars -> 29 words 183 chars` | `shots/S10-03.png` | 0:05 | **Matches** |
| S10.4 | R-2.1 | the counter stays visible in Write_On mode (with the toggle arrow) | pass: `29 words 183 chars ←` | `shots/S10-04.png` | 2:35 | **Matches** |
