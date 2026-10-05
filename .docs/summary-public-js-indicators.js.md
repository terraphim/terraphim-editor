# Summary: public/js/indicators.js

## Purpose
Decoration registry and Write_On inline indicators (#8, spec R-3.1 to R-3.6). Design notes: `docs/design/indicators.md`.

## Key points
- `TeDecorationRegistry` (`editor.decorations`): named layers (`set`, `clear`, `get`, `current`, `names`, `batch`, `onApply`) merged into one `EditorSurface.setDecorations` call. Ids are namespaced `<layer>:<id>`, decorations the registry does not own are kept, and an identical merged list causes no re-render. The ghost layer for #11 plugs in here.
- `TeIndicatorLayer` (`editor.indicators`), active only in Write_On mode: a CSS underline on word, sentence and headline spans; a dot row (centred for words, right-aligned for sentences and headlines) with one lit dot for the active alternative; a gutter rule plus dot column for paragraphs; bold, larger headlines. Spans with only their original get nothing.
- Dots live in an out-of-flow, `aria-hidden` overlay positioned from live decoration rects, at most once per frame. The model (`editor.annotations()`) is read only on a 120 ms trailing debounce; `flush()` forces a refresh.
- Every span carries an `aria-describedby` summary ("Word: 7 alternatives, 5 of 7 active"). Dot clicks dispatch `te:dot` and do not change the document.
- Loaded after `chrome.js` and before `editor.js`. Tests: `tests/web_indicators.rs` and `tests/indicators_fixture.rs`.
