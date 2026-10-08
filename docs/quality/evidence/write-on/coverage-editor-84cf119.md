# terraphim-editor coverage at `84cf119`

Measured on 2026-10-08. Native: `cargo llvm-cov --workspace --lcov` at `84cf119`. wasm and JavaScript: `bash scripts/ci/wasm-coverage.sh` (all 51 instrumented browser suites, every one passing on the first attempt) on `84cf119` plus the #116 fix to that script (PR #117), which changes only `scripts/ci` and the CI workflow; `src` and `crates` are byte-identical to `84cf119`.

The union counts a line as instrumented if either export records it, and covered if either run executed it (`lcov_union.py`). The same calculation reproduces the earlier figures at `bb6e422` exactly.

## Rust (lines)

| File | Native lines | wasm lines | Native ∪ wasm lines | Instrumented lines |
|---|---|---|---|---|
| crates/terraphim_alternatives/src/article.rs | 100.00% | 88.89% | 100.00% | 106 |
| crates/terraphim_alternatives/src/block.rs | 100.00% | 84.97% | 100.00% | 376 |
| crates/terraphim_alternatives/src/context.rs | 98.97% | 94.71% | 98.97% | 292 |
| crates/terraphim_alternatives/src/document.rs | 99.26% | 88.63% | 99.85% | 675 |
| crates/terraphim_alternatives/src/model.rs | 98.00% | 62.82% | 98.00% | 150 |
| crates/terraphim_alternatives/src/offset.rs | 98.95% | 94.83% | 98.95% | 95 |
| crates/terraphim_alternatives/src/reanchor.rs | 97.73% | 89.15% | 97.73% | 352 |
| crates/terraphim_alternatives/src/words.rs | 100.00% | 100.00% | 100.00% | 69 |
| crates/terraphim_lab/src/actions.rs | 99.10% | 80.72% | 99.10% | 332 |
| crates/terraphim_lab/src/cuts.rs | 96.71% | 72.60% | 96.71% | 365 |
| crates/terraphim_lab/src/lib.rs | 98.78% | 79.27% | 98.78% | 82 |
| crates/terraphim_lab/src/lists.rs | 92.72% | 69.06% | 92.72% | 261 |
| crates/terraphim_lab/src/offset.rs | 100.00% | 96.97% | 100.00% | 45 |
| crates/terraphim_lab/src/rolegraph.rs | 98.08% | 0.00% | 98.08% | 104 |
| crates/terraphim_lab/src/text.rs | 98.35% | 85.08% | 98.35% | 486 |
| crates/terraphim_lab/src/trim.rs | 98.81% | 88.91% | 98.81% | 505 |
| crates/terraphim_lab/src/weak.rs | 100.00% | 95.42% | 100.00% | 147 |
| src/document.rs | 92.04% | 89.59% | 99.13% | 2286 |
| src/kg.rs | 89.34% | 91.54% | 97.49% | 319 |
| src/lab.rs | 90.71% | 96.18% | 99.04% | 312 |
| src/lib.rs | 40.10% | 91.16% | 93.23% | 192 |
| src/providers.rs | 87.20% | 89.63% | 98.17% | 164 |

| Crate | Native lines | wasm lines | Native ∪ wasm lines |
|---|---|---|---|
| crates/terraphim_alternatives | 99.05% | 88.13% | 99.24% |
| crates/terraphim_lab | 97.81% | 78.68% | 97.81% |
| src (terraphim-editor) | 88.36% | 90.46% | 98.56% |

## JavaScript (V8 precise coverage of public/js)

```
File                                 Lines  Missed    Cover   Funcs  Missed    Cover
------------------------------------------------------------------------------------
public/js/alert.js                       -       -not loaded
public/js/alternatives-panel.js        981      20   97.96%     112       1   99.11%
public/js/blocks.js                   1388      38   97.26%      90       7   92.22%
public/js/chrome.js                    448      17   96.21%      44       1   97.73%
public/js/config.js                     74       1   98.65%       0       0  100.00%
public/js/editor.js                   1778      95   94.66%     178       5   97.19%
public/js/indicators.js                694      12   98.27%     107       2   98.13%
public/js/lab.js                       454      11   97.58%      51       2   96.08%
public/js/overflow.js                  382      19   95.03%      55       7   87.27%
public/js/persistence.js              1209      70   94.21%     116      13   88.79%
public/js/selection-menu.js            514      25   95.14%      87       5   94.25%
public/js/terraphim-editor.js          188      38   79.79%      25       2   92.00%
public/js/trim.js                      458      24   94.76%      62       4   93.55%
------------------------------------------------------------------------------------
TOTAL                                 8568     370   95.68%     927      49   94.71%
```
