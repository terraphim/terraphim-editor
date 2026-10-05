//! Logged-only typing-latency comparisons (issues #4, #27 and #47), in their
//! own test binary so that they do not share a wasm-bindgen-test-runner
//! budget (20 s per binary) with the asserted comparison in
//! `tests/web_bench.rs`. Run with `wasm-pack test --headless --chrome`.
//!
//! Nothing here is asserted on timing: under host load the headless timing
//! of these paths is too noisy to gate on. They are logged next to the same
//! textarea baseline so the ratios stay readable:
//!
//! * the surface without the debounce (synchronous preview), the #4
//!   regression guard; it no longer ships (#28);
//! * the same with `EditorSurface.sync` bypassed, isolating the cost of the
//!   surface's model synchronisation;
//! * conversion plus preview update alone.
//!
//! Same 5,043-word document, textarea baseline and keystroke timer as the
//! asserted binary (shared through `tests/support/mod.rs`), but fewer
//! keystrokes (three rounds of 3, no separate warm-up: the median of three
//! discards a cold first round, and the log line carries the per-round
//! totals) because the synchronous surface costs several times the baseline
//! per keystroke. Five keystrokes per round still overran the runner budget
//! at load 18 (issue #53); the figures are per keystroke, so they stay
//! comparable. The correctness checks (every keystroke landed, canonical
//! surface, no full DOM rebuilds, preview render count) are still asserted.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types, the editor API and the benchmark helpers used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Keystrokes per measured round, per path. Medians over `ROUNDS` rounds.
const KEYSTROKES: u32 = 3;
const ROUNDS: u32 = 3;

#[wasm_bindgen_test]
async fn bench_typing_latency_logged_paths() {
    let doc_text = five_thousand_words();
    let word_count = doc_text.split_whitespace().count();
    let middle = (doc_text.len() / 2) as u32;

    let document = fresh_full_editor();
    let textarea = bench_textarea_baseline(&document, &doc_text);
    sleep(0).await;

    // The surface with a synchronous preview (`delay` 0), optionally with
    // `EditorSurface.sync` bypassed; there is no pending timer, so the
    // keystrokes are timed in yielding blocks.
    let run_surface = |bypass_sync: bool, n: u32| async move {
        set_preview_delay(0);
        js_eval(&format!(
            "(() => {{ const s = teTest.surface(); s.focus(); s.setSelectionOffsets({middle}); s.programmatic = {bypass_sync}; }})()"
        ));
        bench_settle().await;
        let t = time_keystrokes_yielding(n).await;
        // Resynchronise the model after a bypassed round.
        js_eval(
            "(() => { const s = teTest.surface(); s.programmatic = false; s.sync('typing'); })()",
        );
        assert!(
            !flush_preview(),
            "synchronous rounds leave no render pending"
        );
        sleep(0).await;
        t
    };
    let preview = document
        .query_selector(".markdown-preview")
        .unwrap()
        .unwrap();
    let run_conversion_only = |n: u32| {
        let t0 = now();
        for _ in 0..n {
            preview.set_inner_html(&terraphim_editor::render_markdown(&doc_text).unwrap());
        }
        now() - t0
    };

    bench_load_surface(&doc_text);
    sleep(0).await;
    let renders_before = js_number("teTest.surface().renderCount");
    let preview_renders_before = preview_render_count();

    // Alternate measured rounds and take medians; a cold first round is
    // discarded by the median.
    let mut textarea_runs = Vec::new();
    let mut surface_runs = Vec::new();
    let mut native_runs = Vec::new();
    let mut conversion_runs = Vec::new();
    for _ in 0..ROUNDS {
        textarea_runs.push(bench_run_textarea(&textarea, middle, 0, KEYSTROKES).await);
        surface_runs.push(run_surface(false, KEYSTROKES).await);
        native_runs.push(run_surface(true, KEYSTROKES).await);
        conversion_runs.push(run_conversion_only(KEYSTROKES));
        sleep(0).await;
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
        "textarea {} ms, surface {} ms, sync bypassed {} ms, conversion {} ms",
        raw(&textarea_runs),
        raw(&surface_runs),
        raw(&native_runs),
        raw(&conversion_runs)
    );
    let per_key = |runs: &mut Vec<f64>| median(runs) / f64::from(KEYSTROKES);
    let per_textarea = per_key(&mut textarea_runs);
    let per_surface = per_key(&mut surface_runs);
    let per_native_only = per_key(&mut native_runs);
    let per_conversion = per_key(&mut conversion_runs);

    // Every path really received every keystroke (ROUNDS rounds; the
    // surface ran two paths).
    let typed = (ROUNDS * KEYSTROKES) as usize;
    assert_eq!(textarea.value().len(), doc_text.len() + typed);
    let surface_len = js_number("teTest.surface().getText().length") as usize;
    assert_eq!(surface_len, doc_text.encode_utf16().count() + 2 * typed);
    // Both synchronous surface paths render on every keystroke: bypassing
    // `EditorSurface.sync` does not detach the Rust preview listener.
    assert_eq!(
        preview_render_count() - preview_renders_before,
        (2 * typed) as u32,
        "synchronous rounds must render on every keystroke"
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
    assert!(word_count >= 5000, "document has {word_count} words");

    web_sys::console::log_1(
        &format!(
            "Typing latency (logged only) on {word_count} words ({} chars), median of {ROUNDS} rounds x {KEYSTROKES} keystrokes \
             (no warm-up), ms per keystroke incl. Rust conversion: \
             textarea baseline (sync conversion) {per_textarea:.3}; \
             surface without debounce {per_surface:.3}; \
             surface without debounce, EditorSurface sync bypassed {per_native_only:.3}; \
             conversion + preview update alone {per_conversion:.3}; \
             surface/textarea ratio {:.3}; per-round totals {raw_rounds}",
            doc_text.len(),
            per_surface / per_textarea
        )
        .into(),
    );
}
