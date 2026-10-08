# Write_On video measurements

Evidence for the panel widths and the Overflow timings cited in [alternative-control.md](alternative-control.md) (issue #96). Source: the Write_On demo recording, 3096x1952 pixels, 60 fps, 415.4 s. The recording is the whole app window, so a percentage of the frame width is a percentage of the window width. The video and its frames are not committed.

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

## Not yet reflected in the implementation

- The `Ctrl+Shift+D` toggle shortcut shown in the 3:52 tooltip is not bound.
- In the video the Overflow panel is full height and flush right, with the keyboard glyph and the XYZ tag inside it; ours floats between those corner controls. Its width matches. R-6.1 asks only for a right-hand panel, so this is a difference from the demo, not a failed requirement.
- The CSS clamps both widths to 12rem–20rem, so they are exactly 18% only for windows from 1067px to 1778px wide; narrower windows keep a usable 12rem panel and wider ones stop at 20rem.
