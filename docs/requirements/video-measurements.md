# Write_On video measurements

Evidence for the panel widths, the Overflow timings and placement, and the other video citations in [alternative-control.md](alternative-control.md) (issues #96 and #101). Source: the Write_On demo recording, 3096x1952 pixels, 60 fps, 415.4 s. The recording is the whole app window, so a percentage of the frame width is a percentage of the window width. The video and its frames are not committed.

## Method

- Frames were extracted with `ffmpeg -ss <s> -i writeon.mp4 -frames:v 1` at native resolution, and at one frame per second (four per second around the open and close transitions) for 3:20 to 4:30 and 6:05 to 6:30.
- A panel edge is the column where the per-column median colour of rows 20% to 80% steps to the 1px panel border (luminance sum about 45 on the page, about 115 on the border). Text is sparse, so the median is the background.
- Widths are given in native pixels and as a percentage of the 3096px frame. At 1140 CSS pixels, 18% is 205px.

## Alternatives panel (R-4.1)

The panel is flush with the left edge and full height; its right border is the measured edge.

| Time | Right border (x of 3096) | Width | % of window |
|---|---|---|---|
| 1:17 | 556 | first frame fully open (opens 1:16.75 to 1:17.00) | n/a |
| 1:30 | 557-559 | 558px | 18.0% |
| 1:40 | 557-559 | 558px | 18.0% |
| 3:20 | 557-559 | 558px | 18.0% |
| 3:30 | 557-559 | 558px | 18.0% |

The previous figure, "~29% of a 1140px window, about 330px", does not match any frame.

## Overflow panel (R-6.1)

The panel is flush with the right edge and full height (its background runs from x=2536 to x=3095 at rows 5, 300, 1300 and 1945); its left border is the measured edge.

| Time | Left border (x of 3096) | Width | % of window |
|---|---|---|---|
| 4:05 | 2535-2537 | 560px | 18.1% |
| 4:20 | 2535-2537 | 560px | 18.1% |
| 6:15 | 2535-2537 | 560px | 18.1% |
| 6:20 | 2535-2537 | 560px | 18.1% |

Both panels are 18% of the window, so the "same width class" in R-6.1 holds exactly.

## Overflow timings (R-6.1 to R-6.4)

| Time | Observation |
|---|---|
| 2:36-2:37 | Selection context menu with all four items, including "Stash this in Overflow Ctrl+Shift+X"; "Ghost it" is chosen and the text is ghosted by 2:38 |
| 3:45 | Overflow panel closed (the doc cited a frame here) |
| 3:48-3:57 | Transcript: "if you click on this button down here, you go into the overflow panel" |
| 3:52 | Cursor on the bottom-right XYZ tag; tooltip above it reads `Ctrl+Shift+D` |
| 3:52.75-3:53.25 | Panel opens (border first visible at 3:52.75, fully drawn at 3:54) |
| 3:57-4:03 | Transcript: "the overflow panel just has whatever you want to put in it" |
| 3:59-4:10 | Transcript: "select something here and I can stash it in overflow… over there to the right hand side" |
| 4:04-4:05 | Context menu with "Stash this in Overflow Ctrl+Shift+X" hovered, panel open; footer hint `ctrl+⏎ or drag into the page to use` visible |
| 4:14-4:25 | Document scrolled, panel still open |
| 4:22-4:29 | Transcript: "Copy, paste, drag stuff over or whatever… You've got the overflow" |
| 4:27.25-4:27.50 | Panel closes |
| 4:29-4:48 | Transcript: save, open, post to X, LinkedIn blocked |
| 6:13 | Panel opens again (fully drawn by 6:14) |
| 6:10-6:22 | Transcript recap: "I can stash extra writing in the sidebar" |
| 6:23.50 | Panel closed |

## Overflow placement and the corner controls (R-6.1, R-7.2)

Compared at 3:48 (closed) and 4:05 (open), cropping x=2396 to 3095 of the full height:

| Time | Observation |
|---|---|
| 3:48, 4:05 | The top-right keyboard glyph is at the same place closed and open; open, it sits over the top of the panel, above the title |
| 3:48, 4:05 | The bottom-right XYZ tag is at the same place closed and open; open, it sits over the bottom of the panel, below the footer hint, and is drawn lavender (border and text) instead of dim |
| 4:05 | The panel has no buttons of its own: the title "Overflow", the text, and the footer hint; its content starts about 23 CSS px in from its left border |
| 4:05 | The title sits well below the keyboard glyph and the footer hint well above the XYZ tag |

The implementation matches: the panel runs full height against the right edge, the corner controls stay on top of it (and keep working), the XYZ tag is lit while it is open, and Ctrl+Shift+D toggles it. It keeps a close button beside the title, so the panel can be dismissed from inside it; Escape and the XYZ tag also close it.

The CSS clamps both widths to 12rem–20rem, so they are exactly 18% only for windows from 1067px to 1778px wide; narrower windows keep a usable 12rem panel and wider ones stop at 20rem.

## Other citations re-derived (issue #101)

Transcript times are from the recording's own transcript; frames were extracted every three seconds over the Lab (4:48 to 6:00) and twice a second around the transitions.

| Requirement | Was | Now | Evidence |
|---|---|---|---|
| §1 no AI rewriting | T 5:10–5:20 | T 5:38–5:46 | "So it's not rewriting anything for me. I don't want it to do that" |
| R-2.2 alternatives | T 0:35–1:00 | T 0:33–1:11 | from "written alternate versions of this headline" to "I wrote these alternatives" |
| R-2.3 granularities | T 0:55, 1:55; T 2:00 | T 0:56, 1:52; T 2:08–2:15 | "tension" at 0:56, "paperclip" at 1:52; "Same thing is true at the paragraph level" at 2:08 |
| R-2.4 cycling | T 0:40–0:50, 1:25–1:35 | T 0:40–0:53, 1:28–1:37 | "mouse over it and use my arrow key"; "If I arrow up… see them in line" |
| R-2.5 persistence | T 1:00, 1:40 | T 0:53, 1:37–1:43 | "keep it like this because that's what I prefer"; "leave struggle there for a while" |
| R-2.6 a/an | T 1:50–2:00, F 1:55 | T 1:52–2:08, F 2:05 | at 1:55 the text still reads "a paperclip"; "an eraser" is on screen at 2:05 |
| R-3.3 lit dot | T 1:00–1:10 | T 1:40–1:52 | "the dot in the middle is highlighted… not the original word" |
| R-5.1 ghost | T 2:15–2:45, F 2:40 | T 2:15–2:48, F 2:38 | "dim something back" at 2:15, "ghost it… about 10%" at 2:37; the text is ghosted by 2:38 |
| R-5.2 revive | T 2:45–2:50 | T 2:48–2:52 | "right click it and say revive" |
| R-8.1 Lab popover | F 5:00 | T 4:48–4:59, F 4:56, F 5:00 | popover fades in at 4:55.5 and is fully open at 4:56 |
| R-8.2 mark actions | T 4:15–4:35 | T 4:59–5:15 | "mark the convoluted sentences. Find words that don't fit the tone" |
| R-8.3 trim levels | F 5:00 | T 5:15–5:27, F 5:00 | "slightly trim this, tighten it some more, even sharper, or cut it in half"; Slight trim hovered 5:25–5:26.5 |
| R-8.4 trim behaviour | T 4:45–5:20 | T 5:22–5:46, F 5:27, F 5:35 | "AI scans the whole document… uses the dim feature"; the status card is up from 5:27 |
| R-8.5 review | T 5:20–5:30 | T 5:41–5:49 | "I can walk through the changes, I can make the cuts, or I can say, 'Eh, maybe not.'" |
| Evidence index | 0:55–1:10, 1:50–2:00, 2:15–2:50, 4:10–5:35 | 0:55–1:11 and 1:40–1:52, 1:52–2:15, 2:15–2:52, 4:48–5:58 | as above; the Lab runs from "the last thing… the lab" (4:48) to "cut it in half" (5:58) |

Frames checked and found right: F 0:05 (plain mode, counter only), F 0:25 (headline underline and dots), F 0:55 ("tension" underlined), F 1:45 ("struggle"), F 2:00–2:10 (paragraph gutter rule), F 2:35 (all corner controls), F 5:00 (Lab popover; "Shouldn't everything be obvious?" with its dots), F 5:35 (trim status card).

## Acceptance-test visual defects (issue #107)

Measured on native frames (3096x1952; CSS pixels are half the native ones).

| Defect | Time | Observation |
|---|---|---|
| D1 plain mode | 0:00–0:05 | Plain mode is already the dark page: background about rgb(5, 9, 28), body text about rgb(236, 200, 181) (the recording's rendering of the `--te-color-bg` and `--te-color-text` tokens), text in the same centred column and at the same height as in Write_On; the counter top-left at y=60–70 native (30–35 CSS px), where Write_On has it at 0:25 |
| D2 paragraph rule | 1:30, 2:00, 2:08 | The rule is 2–3 native px wide at x=728–731, the paragraph's text starts at x=769: a gap of about 40 native px (20 CSS px, hence `--te-ind-gutter: 1.25rem`). The dot column is at x=715–721, just left of the rule, at y=1645–1680 while the rule runs y=1466–1802: level with its middle, not its top |
| D5 alternatives panel | 1:30 | The tabs are one line, x=48–440 of the 558px panel (70% of its width), y=365–399, just above the document's first line (y=445); the list follows at y=455; no status line; the key hint "return adds · esc closes" at y=1795, above the bottom-left icons (y=1870–1905). The UAT's "vertically centred" list is the list starting at the page's top margin, not centred in the panel |
| D9 Lab popover | 4:56–5:30 (frame 5:00) | Header row, the six actions as text only (no icons), then one row of five equal trim buttons (about 230 native px each), each with the target under the name; no description line under the actions and no "TRIM" label. The popover is x=930–2165 (about 40% of the window) |
