# Summary: crates/terraphim_alternatives/src/words.rs

## Purpose
The editor's one word definition (#59).

## Key points
- A word is a run of letters, digits or `_`; `'`, `’`, `-`, `.` and `/` join two runs only between word characters.
- `word_spans` (allocation-free iterator of byte spans on character boundaries) and `count_words`.
- Used by `Document::counts` (Write_On counter, `document_counts`) and by `terraphim_lab`'s `word_spans` (Lab actions, trim card `TrimPlan::status`); mirrored as `WORD_RE` in `public/js/chrome.js` for the surface-text fallback.
- O(n); about 25 µs per 3,000 words.
