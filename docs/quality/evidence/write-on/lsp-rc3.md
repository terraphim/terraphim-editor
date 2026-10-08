# terraphim-lsp tests and coverage at rc.3 (`aafcf6f5e`)

Tag `terraphim_lsp-v1.22.0-rc.3`, commit `aafcf6f5e212de55b5c5696f87bb7a7a463b80a8`. Run on 2026-10-08 with `cargo llvm-cov -p terraphim_lsp -p terraphim_lsp_core --summary-only`, which runs every test in both crates.

Tests: 24 binaries, 383 passed, 0 failed, 0 ignored.

Every test result line:

```
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.84s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.42s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.63s
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

Coverage summary (regions, functions, lines, branches):

```
Filename                                  Regions    Missed Regions     Cover   Functions  Missed Functions  Executed       Lines      Missed Lines     Cover    Branches   Missed Branches     Cover
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
terraphim_lsp/src/commands.rs                  65                 0   100.00%           7                 0   100.00%          44                 0   100.00%           0                 0         -
terraphim_lsp/src/completion.rs               166                 3    98.19%          15                 2    86.67%         104                 2    98.08%           0                 0         -
terraphim_lsp/src/convert.rs                  103                 1    99.03%          11                 0   100.00%          82                 1    98.78%           0                 0         -
terraphim_lsp/src/diagnostics.rs              188                 0   100.00%          20                 0   100.00%         130                 0   100.00%           0                 0         -
terraphim_lsp/src/handshake.rs                260                 1    99.62%          31                 0   100.00%         216                 0   100.00%           0                 0         -
terraphim_lsp/src/kg_analysis.rs              450                 0   100.00%          45                 0   100.00%         267                 0   100.00%           0                 0         -
terraphim_lsp/src/lib.rs                        3                 0   100.00%           1                 0   100.00%           3                 0   100.00%           0                 0         -
terraphim_lsp/src/main.rs                      32                 1    96.88%           2                 0   100.00%          24                 1    95.83%           0                 0         -
terraphim_lsp/src/server.rs                  2719               140    94.85%         267                17    93.63%        1742               100    94.26%           0                 0         -
terraphim_lsp/src/settings.rs                 159                 3    98.11%          13                 0   100.00%          93                 1    98.92%           0                 0         -
terraphim_lsp/src/thesaurus.rs                359                16    95.54%          27                 2    92.59%         187                 8    95.72%           0                 0         -
terraphim_lsp_core/src/alternative.rs         133                 3    97.74%           9                 0   100.00%          87                 1    98.85%           0                 0         -
terraphim_lsp_core/src/block.rs               277                 3    98.92%          20                 0   100.00%         163                 0   100.00%           0                 0         -
terraphim_lsp_core/src/case.rs                152                 1    99.34%          12                 0   100.00%          83                 1    98.80%           0                 0         -
terraphim_lsp_core/src/diagnostic.rs           84                 0   100.00%           3                 0   100.00%          89                 0   100.00%           0                 0         -
terraphim_lsp_core/src/engine.rs              289                11    96.19%          25                 0   100.00%         178                 2    98.88%           0                 0         -
terraphim_lsp_core/src/export.rs               19                 0   100.00%           2                 0   100.00%          14                 0   100.00%           0                 0         -
terraphim_lsp_core/src/ghost.rs               226                 9    96.02%          17                 1    94.12%         128                 4    96.88%           0                 0         -
terraphim_lsp_core/src/lab.rs                 285                 1    99.65%          26                 0   100.00%         180                 1    99.44%           0                 0         -
terraphim_lsp_core/src/offset.rs              317                 7    97.79%          30                 1    96.67%         206                 3    98.54%           0                 0         -
terraphim_lsp_core/src/overflow.rs            371                 8    97.84%          14                 2    85.71%         188                 6    96.81%           0                 0         -
terraphim_lsp_core/src/reanchor.rs             51                 0   100.00%           3                 0   100.00%          29                 0   100.00%           0                 0         -
terraphim_lsp_core/src/stored.rs              251                14    94.42%          17                 2    88.24%         139                 6    95.68%           0                 0         -
-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
TOTAL                                        6959               222    96.81%         617                27    95.62%        4376               137    96.87%           0                 0         -
```
