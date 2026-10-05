# Visual references: inline indicators (issue #8)

Reference screenshots of the Write_On inline indicators, rendered from `../indicators/indicators.md` with a release Trunk build in headless Chromium (agent-browser, 1280x800 viewport, device scale factor 2):

| File | Case |
|---|---|
| `indicators-word.png` | word `struggle`: underline, 7 dots centred, fifth lit |
| `indicators-headline.png` | headline: bold and larger, underline, 3 dots right-aligned, first (original) lit |
| `indicators-paragraph.png` | paragraph: gutter rule over its height, 3-dot column to its left, second lit, no underline |
| `indicators-page.png` | the whole Write_On page with every case |

These are reference images for review, not pixel-diff targets: fonts differ between hosts. The assertions of record are the geometry tests in `tests/web_indicators.rs`.

## Regenerating

1. `trunk build --release --dist <scratch>/site`, then restore `dist/` (never commit it).
2. Copy `<scratch>/site/index.html` to `shot.html` and add a script before `</body>` that waits for `window.terraphimEditor.documentApi()`, then calls `terraphimEditor.openDocument(<fixture text>, 'indicators-fixture')`, `terraphimEditor.chrome.setMode('write-on')` and `terraphimEditor.indicators.flush()`. Then add fixed, transparent `#shot-word`, `#shot-headline` and `#shot-paragraph` boxes over each span's decoration and dot rects, with 12px padding.
3. Serve the directory (`python3 -m http.server`), then:
   `agent-browser set viewport 1280 800 2`, `agent-browser open http://127.0.0.1:<port>/shot.html`, `agent-browser wait "#shot-paragraph"`, and `agent-browser screenshot "#shot-word" indicators-word.png` (likewise for the other cases), plus a plain `agent-browser screenshot indicators-page.png`.
