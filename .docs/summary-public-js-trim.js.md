# Summary: public/js/trim.js

## Purpose
Trim levels, faded preview and status card (#15, R-8.3 to R-8.5).

## Key points
- `TeLabTrim` (`editor.lab.trim`), created by `TeLabPopover` in its `data-slot="trim"`: five level radios (Original, Slight trim ~10%, Tighten more ~20%, Even sharper ~30%, Cut in half ~50%).
- Preview: `lab-trim` decoration layer (`te-trim-faded`, ghost fade token, never model ghosts, no attributes) fed by the `src/lab.rs` bridge (`trim_plan_json`, `trim_status`, `trim_make_cuts`; stale plans refused).
- Bottom-centre status card: level, engine `card_text`, hint, Make the cuts / Walk through / Done; `aria-live` numbers.
- Click keeps the outermost faded cut. Make the cuts applies the engine's typed edits through `EditorSurface.replaceRanges` as one undo step; undo restores text and annotations.
- Walk through only navigates and keeps (Previous/Next, Keep/K, Stop/Escape). Done closes the card and leaves the preview (no ghosts written). Any non-trim edit ends the trim. Original, leaving Write_On and destroy clear everything.
- Tests: `tests/web_trim.rs` and native tests in `src/lab.rs`.
