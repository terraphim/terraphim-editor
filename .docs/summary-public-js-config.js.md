# Summary: public/js/config.js

## Purpose
Editor configuration: formatting shortcuts, `/` palette commands and initial content.

## Key points
- The only shortcut source the code reads. `editor_config.toml` is not read by the code (its keys are checked against the Write_On shortcuts by `tests/web_shortcuts.rs`); the legacy `public/shortcuts.json` and `public/js/shortcuts.js` were removed in #103.
