# Final verification run: terraphim-editor `84cf119`

Commit: `84cf1192b6bf21d12cfdc38ca5d2d7f54e1ad456`. Started 2026-10-08 11:58 BST, finished 2026-10-08 12:47 BST. Host: the shared development Mac (load 4 to 14 during the run).

CI on the same commit: Gitea Actions `ci / build (push)`, state **success** (updated 2026-10-08 13:01:58 +02:00).

## Quality

```
cargo fmt --all -- --check: exit 0
cargo clippy --workspace --all-targets -- -D warnings:
    Finished `dev` profile [optimized] target(s) in 12.25s
exit 0
```

## Load-guard self-test

```
browser-tests load guard: ok (local: exit 4 after the 6s cap; benchmark skipped; default limit 8; CI: warn only, binaries run, benchmarks skipped with one wait)
exit 0
```

## Native tests (`cargo test --workspace`)

```
binaries: 76   passed: 393   failed: 0
exit 0
```

## Browser pass 1 (`scripts/browser-tests.sh`)

Totals: 53 binaries, 171 tests passed, 0 failed.

```
  tokens: ok (12s wall, finished in 0.31s, load 7.73) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_focus: ok (17s wall, finished in 8.82s, load 7.60) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_keys: ok (17s wall, finished in 8.96s, load 7.01) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_capacity: ok (18s wall, finished in 9.49s, load 7.76) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_limit: ok (15s wall, finished in 6.63s, load 7.73) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_refused: ok (20s wall, finished in 12.29s, load 7.75) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_undo: ok (21s wall, finished in 12.36s, load 7.81) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live: ok (20s wall, finished in 11.33s, load 7.95) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_tabs: ok (26s wall, finished in 16.34s, load 7.79) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_width: ok (13s wall, finished in 4.69s, load 7.85) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel: ok (20s wall, finished in 10.85s, load 7.95) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_bench_logged: ok (18s wall, finished in 9.51s, load 4.90) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_bench: ok (22s wall, finished in 12.52s, load 4.98) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_blocks_edit: ok (20s wall, finished in 13.28s, load 5.29) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_blocks_move: ok (18s wall, finished in 11.32s, load 5.40) test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out
  web_blocks: ok (22s wall, finished in 15.36s, load 4.73) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_chrome: ok (20s wall, finished in 12.76s, load 5.99) test result: ok. 12 passed; 0 failed; 0 ignored; 0 filtered out
  web_commands: ok (14s wall, finished in 6.48s, load 5.41) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_cycling: ok (20s wall, finished in 11.56s, load 5.93) test result: ok. 6 passed; 0 failed; 0 ignored; 0 filtered out
  web_edit_location: ok (13s wall, finished in 4.65s, load 5.85) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_embed: ok (15s wall, finished in 9.21s, load 5.81) test result: ok. 10 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_drafts: ok (11s wall, finished in 3.70s, load 5.84) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_handles: ok (15s wall, finished in 8.05s, load 5.56) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_identity: ok (15s wall, finished in 6.50s, load 5.81) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_markdown: ok (12s wall, finished in 4.87s, load 5.75) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_open: ok (13s wall, finished in 6.54s, load 5.47) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_race: ok (13s wall, finished in 5.75s, load 6.74) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_replace: ok (12s wall, finished in 5.11s, load 7.35) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_restore: ok (12s wall, finished in 4.91s, load 6.83) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_saveas: ok (15s wall, finished in 8.33s, load 6.92) test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out
  web_files: ok (13s wall, finished in 5.76s, load 7.49) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_ghosts: ok (18s wall, finished in 10.52s, load 7.41) test result: ok. 8 passed; 0 failed; 0 ignored; 0 filtered out
  web_headings: ok (15s wall, finished in 8.93s, load 7.57) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_indicators_registry: ok (17s wall, finished in 9.92s, load 7.06) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_indicators: ok (17s wall, finished in 9.56s, load 7.73) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_kg_case: ok (13s wall, finished in 3.93s, load 7.40) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_kg_menu: ok (15s wall, finished in 7.84s, load 6.80) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_kg: ok (13s wall, finished in 6.71s, load 6.97) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_lab: ok (15s wall, finished in 7.35s, load 6.69) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_move: ok (10s wall, finished in 3.12s, load 5.79) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_overflow_layout: ok (10s wall, finished in 3.80s, load 5.57) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_overflow: ok (17s wall, finished in 10.50s, load 5.42) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence_chrome: ok (14s wall, finished in 5.82s, load 5.32) test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence_edits: ok (13s wall, finished in 6.12s, load 5.63) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence_reattach: ok (13s wall, finished in 6.23s, load 5.19) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence: ok (14s wall, finished in 6.73s, load 5.10) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_providers: ok (19s wall, finished in 10.31s, load 6.07) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_shortcuts: ok (13s wall, finished in 6.45s, load 7.14) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_surface: ok (14s wall, finished in 6.87s, load 6.66) test result: ok. 6 passed; 0 failed; 0 ignored; 0 filtered out
  web_trim_cuts: ok (16s wall, finished in 9.05s, load 6.51) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_trim_done: ok (13s wall, finished in 6.21s, load 6.70) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_trim: ok (14s wall, finished in 7.05s, load 7.12) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web: ok (14s wall, finished in 7.31s, load 6.97) test result: ok. 13 passed; 0 failed; 0 ignored; 0 filtered out
exit 0
```

## Browser pass 2 (`scripts/browser-tests.sh`)

Totals: 53 binaries, 171 tests passed, 0 failed.

```
  tokens: ok (7s wall, finished in 0.18s, load 5.95) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_focus: ok (12s wall, finished in 5.98s, load 5.88) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_keys: ok (13s wall, finished in 6.72s, load 5.25) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_capacity: ok (12s wall, finished in 5.28s, load 5.14) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_limit: ok (13s wall, finished in 5.52s, load 5.25) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_refused: ok (15s wall, finished in 8.01s, load 4.91) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live_undo: ok (14s wall, finished in 7.24s, load 5.80) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_live: ok (14s wall, finished in 6.98s, load 5.78) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_tabs: ok (14s wall, finished in 7.93s, load 6.10) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel_width: ok (10s wall, finished in 3.25s, load 6.15) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_alt_panel: ok (13s wall, finished in 6.56s, load 6.34) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_bench_logged: ok (15s wall, finished in 8.77s, load 4.72) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_bench: ok (16s wall, finished in 9.74s, load 4.83) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_blocks_edit: ok (19s wall, finished in 12.46s, load 4.67) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_blocks_move: ok (18s wall, finished in 11.17s, load 5.16) test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out
  web_blocks: ok (21s wall, finished in 13.42s, load 5.22) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_chrome: ok (16s wall, finished in 9.74s, load 6.65) test result: ok. 12 passed; 0 failed; 0 ignored; 0 filtered out
  web_commands: ok (14s wall, finished in 6.11s, load 6.35) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_cycling: ok (18s wall, finished in 10.59s, load 6.31) test result: ok. 6 passed; 0 failed; 0 ignored; 0 filtered out
  web_edit_location: ok (12s wall, finished in 4.42s, load 6.14) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_embed: ok (15s wall, finished in 8.63s, load 5.88) test result: ok. 10 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_drafts: ok (10s wall, finished in 3.14s, load 6.21) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_handles: ok (13s wall, finished in 6.82s, load 6.09) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_identity: ok (13s wall, finished in 5.74s, load 6.23) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_markdown: ok (11s wall, finished in 4.77s, load 6.27) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_open: ok (14s wall, finished in 7.12s, load 5.78) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_race: ok (12s wall, finished in 4.51s, load 5.25) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_replace: ok (11s wall, finished in 4.40s, load 4.91) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_restore: ok (12s wall, finished in 4.52s, load 5.10) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_files_saveas: ok (14s wall, finished in 6.62s, load 6.69) test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out
  web_files: ok (11s wall, finished in 4.63s, load 6.06) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_ghosts: ok (14s wall, finished in 7.88s, load 5.58) test result: ok. 8 passed; 0 failed; 0 ignored; 0 filtered out
  web_headings: ok (15s wall, finished in 8.04s, load 5.08) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_indicators_registry: ok (13s wall, finished in 6.85s, load 4.62) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_indicators: ok (15s wall, finished in 8.03s, load 4.52) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_kg_case: ok (12s wall, finished in 3.52s, load 4.05) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_kg_menu: ok (13s wall, finished in 7.07s, load 4.34) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_kg: ok (12s wall, finished in 5.28s, load 4.44) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_lab: ok (15s wall, finished in 7.31s, load 4.64) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_move: ok (11s wall, finished in 3.31s, load 4.43) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_overflow_layout: ok (10s wall, finished in 3.94s, load 4.90) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_overflow: ok (16s wall, finished in 8.89s, load 4.68) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence_chrome: ok (13s wall, finished in 5.41s, load 4.62) test result: ok. 3 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence_edits: ok (12s wall, finished in 5.26s, load 4.79) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence_reattach: ok (12s wall, finished in 5.96s, load 4.97) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_persistence: ok (13s wall, finished in 5.77s, load 4.60) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_providers: ok (18s wall, finished in 10.67s, load 4.24) test result: ok. 5 passed; 0 failed; 0 ignored; 0 filtered out
  web_shortcuts: ok (12s wall, finished in 5.52s, load 4.06) test result: ok. 4 passed; 0 failed; 0 ignored; 0 filtered out
  web_surface: ok (13s wall, finished in 6.06s, load 4.64) test result: ok. 6 passed; 0 failed; 0 ignored; 0 filtered out
  web_trim_cuts: ok (13s wall, finished in 7.05s, load 4.34) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web_trim_done: ok (11s wall, finished in 4.37s, load 4.60) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out
  web_trim: ok (13s wall, finished in 6.99s, load 4.66) test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out
  web: ok (15s wall, finished in 7.90s, load 4.60) test result: ok. 13 passed; 0 failed; 0 ignored; 0 filtered out
exit 0
```

