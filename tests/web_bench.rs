//! Asserted typing-latency benchmark (issues #28 and #47), in its own test
//! binary. Run with `wasm-pack test --headless --chrome`.
//!
//! wasm-bindgen-test-runner gives each test binary a fixed wall-clock budget
//! (20 s from page load) to print its result; a binary that overruns fails
//! with "Failed to detect test as having been run". The harness's own
//! `finished in` figure is what must stay comfortably inside that budget, so
//! this binary holds only what is asserted:
//!
//! * the textarea baseline (a plain textarea plus synchronous Rust
//!   conversion, the main-branch behaviour), and
//! * the shipped editing surface with the debounced preview,
//!
//! on the same 5,043-word document, one warm-up round of 10 keystrokes then
//! three measured rounds of 20 keystrokes per path, interleaved, compared by
//! median per keystroke (#28 acceptance: debounced surface <= textarea baseline).
//!
//! The paths that are only logged for comparison (surface without debounce,
//! the #4 regression guard; surface with `EditorSurface.sync` bypassed;
//! conversion alone) live in `tests/web_bench_logged.rs`, which has its own
//! runner budget. The document, the textarea baseline and the keystroke
//! timer are shared through `tests/support/mod.rs`, so the two binaries'
//! numbers stay comparable.
//!
//! Setup steps and timed blocks yield to the event loop between them so the
//! page never holds its main thread long enough to starve the webdriver's
//! result poll, and pending layout is settled (untimed) before each round so
//! one path's deferred relayout is never charged to the other path. Each
//! measured round also starts with 5 untimed pre-roll keystrokes on both
//! paths: the first keystrokes after focus moves between the two editors
//! carry a one-off cost that is a benchmark artefact. The original single
//! binary absorbed it for the surface by accident (other surface paths ran
//! just before each debounced round) but not for the textarea; now both
//! paths measure steady-state typing.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types, the editor API and the benchmark helpers used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Keystrokes per measured round, per path. Medians over `ROUNDS` rounds.
const KEYSTROKES: u32 = 20;
/// Measured rounds; the median of three is stable against one noisy round.
const ROUNDS: u32 = 3;
/// Warm-up rounds (not measured) and keystrokes per path in each.
const WARMUP_ROUNDS: u32 = 1;
const WARMUP_KEYSTROKES: u32 = 10;
/// Untimed keystrokes typed immediately before each measured round's timed
/// keystrokes, in the same synchronous block (so the debounce cannot fire
/// between them), on both paths. The first keystrokes after focus moves from
/// one editor to the other carry a one-off cost (often several hundred ms on
/// a loaded host) that is a benchmark artefact, not typing latency; both
/// paths are measured in steady-state typing.
const PREROLL_KEYSTROKES: u32 = 5;

#[wasm_bindgen_test]
async fn bench_typing_latency_textarea_vs_debounced_surface() {
    let doc_text = five_thousand_words();
    let word_count = doc_text.split_whitespace().count();
    let middle = (doc_text.len() / 2) as u32;

    let document = fresh_full_editor();
    let textarea = bench_textarea_baseline(&document, &doc_text);
    sleep(0).await;

    // The shipped surface: the preview debounced by the default delay. Its
    // keystrokes are timed in one block so that the debounce cannot fire
    // mid-round; the trailing render is then flushed and timed separately.
    let run_debounced = |preroll: u32, n: u32| async move {
        set_preview_delay(DEFAULT_PREVIEW_DELAY_MS);
        js_eval(&format!(
            "(() => {{ const s = teTest.surface(); s.focus(); s.setSelectionOffsets({middle}); }})()"
        ));
        bench_settle().await;
        if preroll > 0 {
            time_keystrokes(preroll);
        }
        let t = time_keystrokes(n);
        let t_flush = now();
        let flushed = flush_preview();
        let flush_ms = now() - t_flush;
        assert!(flushed, "a debounced round leaves one render pending");
        set_preview_delay(0);
        sleep(0).await;
        (t, flush_ms)
    };

    bench_load_surface(&doc_text);
    sleep(0).await;
    let renders_before = js_number("teTest.surface().renderCount");
    let preview_renders_before = preview_render_count();

    // Warm up both paths, then alternate measured rounds and take medians.
    for _ in 0..WARMUP_ROUNDS {
        bench_run_textarea(&textarea, middle, 0, WARMUP_KEYSTROKES).await;
        run_debounced(0, WARMUP_KEYSTROKES).await;
    }
    let mut textarea_runs = Vec::new();
    let mut debounced_runs = Vec::new();
    let mut flush_runs = Vec::new();
    for _ in 0..ROUNDS {
        textarea_runs
            .push(bench_run_textarea(&textarea, middle, PREROLL_KEYSTROKES, KEYSTROKES).await);
        let (debounced, flush_ms) = run_debounced(PREROLL_KEYSTROKES, KEYSTROKES).await;
        debounced_runs.push(debounced);
        flush_runs.push(flush_ms);
    }
    // Raw per-round totals, in run order, for the log line (the medians
    // below sort the vectors in place).
    let raw = |runs: &[f64]| {
        runs.iter()
            .map(|ms| format!("{ms:.0}"))
            .collect::<Vec<_>>()
            .join("/")
    };
    let raw_rounds = format!(
        "textarea {} ms, debounced {} ms",
        raw(&textarea_runs),
        raw(&debounced_runs)
    );
    let per_key = |runs: &mut Vec<f64>| median(runs) / f64::from(KEYSTROKES);
    let per_textarea = per_key(&mut textarea_runs);
    let per_debounced = per_key(&mut debounced_runs);
    let trailing_render = median(&mut flush_runs);

    // Both paths really received every keystroke (WARMUP_ROUNDS warm-up
    // rounds plus ROUNDS measured rounds).
    let rounds = WARMUP_ROUNDS + ROUNDS;
    let typed =
        (WARMUP_ROUNDS * WARMUP_KEYSTROKES + ROUNDS * (PREROLL_KEYSTROKES + KEYSTROKES)) as usize;
    assert_eq!(textarea.value().len(), doc_text.len() + typed);
    let surface_len = js_number("teTest.surface().getText().length") as usize;
    assert_eq!(surface_len, doc_text.encode_utf16().count() + typed);
    // The textarea's handler converts directly, never through the preview
    // scheduler; each debounced round renders exactly once (its flushed
    // trailing render).
    assert_eq!(
        preview_render_count() - preview_renders_before,
        rounds,
        "debounced rounds must render once each"
    );
    assert_eq!(
        js_string("String(teTest.canonical())"),
        "true",
        "surface must stay canonical"
    );
    assert_eq!(
        js_number("teTest.surface().renderCount"),
        renders_before,
        "native typing must not trigger full DOM rebuilds"
    );

    web_sys::console::log_1(
        &format!(
            "Typing latency (asserted) on {word_count} words ({} chars), median of {ROUNDS} rounds x {KEYSTROKES} keystrokes \
             after {WARMUP_ROUNDS} warm-up round(s) x {WARMUP_KEYSTROKES} keystrokes and {PREROLL_KEYSTROKES} untimed pre-roll keystrokes per round, ms per keystroke incl. Rust conversion: \
             textarea baseline (sync conversion) {per_textarea:.3}; \
             surface with {DEFAULT_PREVIEW_DELAY_MS} ms debounce {per_debounced:.3}; \
             debounced trailing render (once per burst) {trailing_render:.3} ms; \
             debounced surface/textarea ratio {:.3}; per-round totals {raw_rounds}",
            doc_text.len(),
            per_debounced / per_textarea
        )
        .into(),
    );
    assert!(word_count >= 5000, "document has {word_count} words");
    // Issue #28 acceptance: the debounced surface is no worse per keystroke
    // than the main-branch baseline (textarea plus synchronous conversion).
    assert!(
        per_debounced <= per_textarea,
        "debounced surface {per_debounced:.3} ms vs textarea {per_textarea:.3} ms per keystroke"
    );
}
