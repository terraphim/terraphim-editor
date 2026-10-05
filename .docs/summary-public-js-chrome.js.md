# Summary: public/js/chrome.js

## Purpose
Write_On mode toggle and corner chrome (#7).

## Key points
- The dim `N words M chars` counter toggles `body[data-mode="write-on"]`; counts come from the Rust model (including ghosted text) when available, else the surface text.
- Corner controls (only in Write_On mode, inert otherwise) dispatch `te:open-panel`, `te:markdown`, `te:shortcuts`, `te:save`, `te:open`, `te:lab`, `te:overflow`; `te:mode-change` and `te:saved` report state.
- Shortcut reference `<dialog>`; mode persisted per document in localStorage keyed by `documentKey` or a content hash.
