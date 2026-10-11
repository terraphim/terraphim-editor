# Safari take8: all41 requirement evidence index

2026-10-11. Finished recording; **acceptance FAIL**.112 finite PASS checks, two trusted-drag receipt FAIL checks, six semantic observations, nine dependent checks NOT EXECUTED. A PARTIAL row means some finite checks passed, never full requirement acceptance. Existing criterion gaps below are retained from the independently reviewed ledger.

Video SHA-256 `0eb37401d8faa1b0363ccb44f595631069953ae859faed7226cada9889e6c908`; sidecar `0213e4f49f6ce449370105459656096b20df1226bf14aa6910e23667d447351d`. Captions and checks use storyboard time, not exact video PTS. Initial actual captions suggest about+0.1s offset, with screenshot cadence delaying visibility. Use approximately±3.1s conservative navigation uncertainty (maximum capture interval2.7107s); exact inspected-frame PTS are separately listed. No exact input-event timing claimed.

| ID | Safari status | Approx video range (s) | Finite evidence / missing criteria |
|---|---|---|---|
| R-2.1 | PARTIAL | 2.5–2.5 | Counter toggles both modes; startup default, only-text/count visibility and dim placement not fully audited. |
| R-2.2 | PARTIAL | 9.0–124.4 | Typed original-first word list and active body; arbitrary contiguous spans/full inactive rendering not exhaustively checked. |
| R-2.3 | PARTIAL | 20.4–26.2 | Sentence and paragraph replacements; headline alternative creation/cycling missing. |
| R-2.4 | PARTIAL | 17.7–17.8 | Word hover Up/Down changes actual body; all granularities/boundary cycling missing. |
| R-2.5 | PARTIAL | 133.3–133.3 | Full created choice/ghost/Overflow API roundtrip; OS save/open and later-session test missing. |
| R-2.6 | PARTIAL | 9.0–129.0 | Reversible paperclip/eraser/thumbtack body; wider vowel-sound and untouched-outside-article cases missing. |
| R-2.7 | PARTIAL | 35.1–35.1 | Undo generated span removes model span; manual delete-all and actual dots/underline removal with independent ghost preservation missing. |
| R-3.1 | PARTIAL | 17.7–17.7 | Actual word underline1px; full span width/baseline/color/no-hover-change audit missing. |
| R-3.2 | PARTIAL | 17.7–20.4 | Word centering, sentence-right geometry and counts; dot diameter/spacing, multiline and headline geometry missing. |
| R-3.3 | PARTIAL | 17.7–26.2 | Active1 full lavender, inactive alpha0.4, one active; wider transition/theme/pixel review missing. |
| R-3.4 | PARTIAL | 26.2–26.2 | Rule1px×22px,20px gutter, left dots/no underline; multiline/scroll/zoom cases missing. |
| R-3.5 | PARTIAL | 2.5–2.5 | Headline exists; exact face/color/bold/size and headline indicators missing. |
| R-3.6 | PARTIAL | 17.7–17.7 | Real span hover triggers cycle; passive dots/direct-jump inferred behavior not checked. |
| R-4.1 | NOT DEMONSTRATED | — | NOT DEMONSTRATED: full-height18% clamp, shortcut/icon toggle and main-text dimming. |
| R-4.2 | PARTIAL | 20.4–26.2 | Real tabs select sentence/paragraph kind; all labels/fonts/colors/population behavior missing. |
| R-4.3 | PARTIAL | 9.0–13.5 | Typed first keystroke/Enter/ordered list/Escape; edit-existing-line and full list styling missing. |
| R-4.4 | PARTIAL | 34.1–34.1 | Actual generated source=`ai`; rendered robot/human glyphs and active text/glyph styling missing. |
| R-4.5 | PARTIAL | 13.3–13.4 | Panel Up/Down exact word output; edge/wrap/multigranularity missing. |
| R-4.6 | PARTIAL | 34.1–35.1 | Actual generated-span undo; bot-line and indicator DOM disappearance not checked. |
| R-5.1 | PARTIAL | 41.2–41.2 | Real ghost preserves text/counts, actual color alpha0.1; right-click Ghost item/selectability still incomplete. |
| R-5.2 | PARTIAL | 41.2–142.2 | Revive removes ghost and menu shows Revive; actual opacity restoration/right-click sequence not fully checked. |
| R-5.3 | PARTIAL | 116.5–146.3 | Swap-driven resize, combined save/reopen, overlap merge, interior split and full deletion observed; adjacent merge, partial alternative overlap and ordinary interior typing missing. |
| R-6.1 | PARTIAL | 47.6–47.6 | Overflow used;18%/clamp/full-height/right edge/title fonts/XYZ highlight/fixed corner glyphs not checked. |
| R-6.2 | PARTIAL | 50.8–133.3 | Exact typed note reaches model; font and notes/URLs/outlines variety missing. |
| R-6.3 | FAIL | 47.6–67.1 | Shortcut stash output passed; native stash receipts empty (FAIL), four dependent checks NOT EXECUTED. Context-menu/visual criteria remain incomplete. |
| R-6.4 | FAIL | 50.8–67.1 | Keyboard pull-back output passed; native pull records trusted dragstart only (FAIL), five dependent checks NOT EXECUTED. No drag insertion/undo/redo success claimed. |
| R-6.5 | PARTIAL | 133.3–133.3 | Typed Overflow exact API save/reopen; OS and later-session persistence missing. |
| R-7.1 | NOT DEMONSTRATED | — | NOT DEMONSTRATED: full-bleed,70-char centered column/top margin and toolbar absence. |
| R-7.2 | PARTIAL | 2.5–2.5 | Counter used; all corner positions/icons/tooltips/reference and intentionally blocked posts not audited. |
| R-7.3 | PARTIAL | 142.2–142.2 | Actual selection menu Revive item exercised; all four items, shortcuts, rounded/colors/hover styling missing. |
| R-7.4 | NOT DEMONSTRATED | — | NOT DEMONSTRATED: existing bold/italic/code/link/heading collision regression. |
| R-8.1 | PARTIAL | 75.6–90.8 | Six actions execute; anchored Lab popover/header/action-layout geometry missing. |
| R-8.2 | PARTIAL | 75.6–90.8 | All six output marks and unchanged body; target ranges align/exclude fixture heading/code. Semantic precision/recall/ranking/tone suitability unaccepted. |
| R-8.3 | PARTIAL | 93.8–102.1 | Five levels clicked, body preserved; exact labels/borders/selected styling missing. |
| R-8.4 | PARTIAL | 93.8–102.1 | Independent counts160→146/130/113/82 with9/19/29/49% cards and unchanged body; ghost rendering/card layout/text/buttons still need full visual audit. |
| R-8.5 | PARTIAL | 106.2–116.5 | Initial Walk marker, Keep id, changed/count-checked cuts, literal-body-preserving Done/exact promoted ghosts; Next/Previous lifecycle, faded-span click keep and literal every-character tidy oracle missing. |
| R-8.6 | PARTIAL | 34.1–34.1 | Real local generation appends exact ordered list with source=ai; rendered bot glyph and wider selection cases missing. |
| R-8.7 | PARTIAL | 34.1–34.1 | Local role-backed generation observed; provider switching/abstraction and offline runtime proof not in these artifacts. LLM optional per decision. |
| R-9.1 | PARTIAL | 117.5–133.3 | Created annotations save trailing block and reopen exact body/model via API; actual picker/preview exclusion missing. |
| R-9.2 | PARTIAL | 133.3–133.3 | Exact created annotations roundtrip; displaced hints/context best-match thresholds/ambiguity/set-aside/legacy cases missing. |
| R-9.3 | PARTIAL | 121.6–133.3 | Visible export value equals independent body-minus-ghost output, block/Overflow omitted; OS exported file and broader ranges/tidy cases missing. |
