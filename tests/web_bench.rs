//! Typing-latency benchmark for the editing surface (issues #27 and #29), in
//! its own test binary. Run with `wasm-pack test --headless --chrome`.
//!
//! It is the heaviest browser test by far, so it runs in its own
//! wasm-bindgen-test runner session (each test binary gets its own 20 s
//! budget) instead of sharing one with `tests/web.rs`, and it yields to the
//! event loop between timed blocks so the page never holds its main thread
//! long enough to starve the webdriver's result poll. What is measured is
//! unchanged: the same keystrokes on the same document through the same
//! paths, summed per round, medians over rounds.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Roughly 5,000 words of Markdown.
fn five_thousand_words() -> String {
    let paragraph = "The quick brown fox jumps over the lazy dog while **bold** words and \
        _italic_ phrases mingle with `inline code` and [links](https://example.com) \
        so that the parser has realistic work to do on every single keystroke typed \
        by the writer who is drafting a long document in this editor today.";
    let words = paragraph.split_whitespace().count();
    let mut out = String::new();
    let mut count = 0;
    let mut section = 0;
    while count < 5000 {
        if count % 500 == 0 {
            section += 1;
            out.push_str(&format!("## Section {section}\n\n"));
            count += 3;
        }
        out.push_str(paragraph);
        out.push_str("\n\n");
        count += words;
    }
    out
}

/// Keystrokes per measured round, per path. Medians over `ROUNDS` rounds.
/// Kept small (with a shorter warm-up) so this binary stays well inside the
/// runner's 20 s budget on a loaded host.
const KEYSTROKES: u32 = 20;
const ROUNDS: u32 = 3;
/// Keystrokes per path in the single warm-up round (not measured).
const WARMUP_KEYSTROKES: u32 = 10;
/// Keystrokes timed in one synchronous block before yielding to the event
/// loop. Each block holds the page's main thread; under host load a whole
/// benchmark run held it for 15-20 s, longer than the webdriver allows for
/// its result poll (`execute/sync timed out`). Yielding between blocks lets
/// the poll through without changing what is measured.
const KEYSTROKES_PER_BLOCK: u32 = 10;

/// Time `n` native keystrokes in one synchronous block.
fn time_keystrokes(n: u32) -> f64 {
    js_number(&format!(
        "(() => {{ const t0 = performance.now(); for (let i = 0; i < {n}; i++) document.execCommand('insertText', false, 'a'); return performance.now() - t0; }})()"
    ))
}

/// Time `n` keystrokes as the sum of blocks of `KEYSTROKES_PER_BLOCK`,
/// yielding to the event loop between blocks. Only for paths with no pending
/// timer (synchronous preview, textarea): a yield could otherwise let a
/// debounced render fire mid-round.
async fn time_keystrokes_yielding(n: u32) -> f64 {
    let mut total = 0.0;
    let mut left = n;
    while left > 0 {
        let n = left.min(KEYSTROKES_PER_BLOCK);
        total += time_keystrokes(n);
        left -= n;
        sleep(0).await;
    }
    total
}

#[wasm_bindgen_test]
async fn bench_typing_latency_textarea_vs_surface() {
    let doc_text = five_thousand_words();
    let word_count = doc_text.split_whitespace().count();
    let middle = doc_text.len() / 2;

    // Baseline: a plain textarea wired to the same Rust conversion.
    let document = fresh_full_editor();
    let textarea = document
        .create_element("textarea")
        .unwrap()
        .dyn_into::<HtmlTextAreaElement>()
        .unwrap();
    textarea.set_class_name("te-bench");
    let baseline_preview = document.create_element("div").unwrap();
    baseline_preview.set_class_name("te-bench");
    let body = document.body().unwrap();
    body.append_child(&textarea).unwrap();
    body.append_child(&baseline_preview).unwrap();
    let ta = textarea.clone();
    let out = baseline_preview.clone();
    let handler = Closure::wrap(Box::new(move |_e: web_sys::Event| {
        out.set_inner_html(&terraphim_editor::render_markdown(&ta.value()).unwrap());
    }) as Box<dyn FnMut(_)>);
    textarea
        .add_event_listener_with_callback("input", handler.as_ref().unchecked_ref())
        .unwrap();
    handler.forget();
    textarea.set_value(&doc_text);

    // Give both editors the same scrolling box, as in the app (the test page
    // does not load index.html's layout CSS).
    let size = "width: 600px; height: 300px; overflow-y: auto; font-family: monospace; padding: 1rem; box-sizing: border-box;";
    textarea.set_attribute("style", size).unwrap();
    surface_element(&document)
        .set_attribute("style", size)
        .unwrap();

    let run_textarea = |n: u32| {
        let textarea = textarea.clone();
        async move {
            textarea.focus().unwrap();
            textarea
                .set_selection_range(middle as u32, middle as u32)
                .unwrap();
            time_keystrokes_yielding(n).await
        }
    };
    // `delay` 0 reproduces the previous synchronous preview; anything else
    // is the debounced preview, whose keystrokes are timed in one block so
    // that the 120 ms debounce cannot fire mid-round.
    let run_surface = |bypass_sync: bool, delay: u32, n: u32| async move {
        set_preview_delay(delay);
        js_eval(&format!(
            "(() => {{ const s = teTest.surface(); s.focus(); s.setSelectionOffsets({middle}); s.programmatic = {bypass_sync}; }})()"
        ));
        let t = if delay == 0 {
            time_keystrokes_yielding(n).await
        } else {
            time_keystrokes(n)
        };
        // Resynchronise the model after a bypassed round.
        js_eval(
            "(() => { const s = teTest.surface(); s.programmatic = false; s.sync('typing'); })()",
        );
        // Run the trailing render (if any) now and time it separately.
        let t_flush = now();
        let flushed = flush_preview();
        let flush_ms = now() - t_flush;
        assert_eq!(
            flushed,
            delay != 0,
            "debounced rounds leave one render pending"
        );
        set_preview_delay(0);
        sleep(0).await;
        (t, flush_ms)
    };
    let preview = document
        .query_selector(".markdown-preview")
        .unwrap()
        .unwrap();
    let run_conversion_only = || {
        let performance = web_sys::window().unwrap().performance().unwrap();
        let t0 = performance.now();
        for _ in 0..KEYSTROKES {
            preview.set_inner_html(&terraphim_editor::render_markdown(&doc_text).unwrap());
        }
        performance.now() - t0
    };

    js_eval(&format!(
        "teTest.surface().setText({})",
        js_string_literal(&doc_text)
    ));

    // setText scheduled a debounced render; run it now so it cannot fire
    // during a yield below and be counted as a keystroke render.
    flush_preview();

    let renders_before = js_number("teTest.surface().renderCount");

    // Warm up every path, then alternate measured rounds and take medians.
    let renders_rust_before = preview_render_count();
    run_textarea(WARMUP_KEYSTROKES).await;
    run_surface(false, 0, WARMUP_KEYSTROKES).await;
    run_surface(true, 0, WARMUP_KEYSTROKES).await;
    run_surface(false, DEFAULT_PREVIEW_DELAY_MS, WARMUP_KEYSTROKES).await;
    run_conversion_only();
    sleep(0).await;
    let mut textarea_runs = Vec::new();
    let mut surface_runs = Vec::new();
    let mut native_runs = Vec::new();
    let mut debounced_runs = Vec::new();
    let mut flush_runs = Vec::new();
    let mut conversion_runs = Vec::new();
    for _ in 0..ROUNDS {
        textarea_runs.push(run_textarea(KEYSTROKES).await);
        surface_runs.push(run_surface(false, 0, KEYSTROKES).await.0);
        native_runs.push(run_surface(true, 0, KEYSTROKES).await.0);
        let (debounced, flush_ms) = run_surface(false, DEFAULT_PREVIEW_DELAY_MS, KEYSTROKES).await;
        debounced_runs.push(debounced);
        flush_runs.push(flush_ms);
        conversion_runs.push(run_conversion_only());
        sleep(0).await;
    }
    let per_key = |runs: &mut Vec<f64>| median(runs) / f64::from(KEYSTROKES);
    let per_textarea = per_key(&mut textarea_runs);
    let per_surface = per_key(&mut surface_runs);
    let per_native_only = per_key(&mut native_runs);
    let per_debounced = per_key(&mut debounced_runs);
    let trailing_render = median(&mut flush_runs);
    let per_conversion = per_key(&mut conversion_runs);

    // Every path really received every keystroke (one warm-up round plus
    // ROUNDS measured rounds; the surface ran three paths).
    let rounds = (ROUNDS + 1) as usize;
    let typed = (WARMUP_KEYSTROKES + ROUNDS * KEYSTROKES) as usize;
    assert_eq!(textarea.value().len(), doc_text.len() + typed);
    let surface_len = js_number("teTest.surface().getText().length") as usize;
    assert_eq!(surface_len, doc_text.encode_utf16().count() + 3 * typed);
    // Synchronous rounds render on every keystroke; each debounced round
    // renders exactly once (its flushed trailing render).
    assert_eq!(
        preview_render_count() - renders_rust_before,
        (2 * typed + rounds) as u32,
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
            "Typing latency on {word_count} words ({} chars), median of {ROUNDS} rounds x {KEYSTROKES} keystrokes, ms per keystroke incl. Rust conversion: \
             textarea baseline (sync conversion) {per_textarea:.3}; \
             surface without debounce {per_surface:.3}; \
             surface without debounce, EditorSurface sync bypassed {per_native_only:.3}; \
             surface with {DEFAULT_PREVIEW_DELAY_MS} ms debounce {per_debounced:.3}; \
             debounced trailing render (once per burst) {trailing_render:.3} ms; \
             conversion + preview update alone {per_conversion:.3}; \
             surface/textarea ratio {:.3}; debounced surface/textarea ratio {:.3}",
            doc_text.len(),
            per_surface / per_textarea,
            per_debounced / per_textarea
        )
        .into(),
    );
    assert!(word_count >= 5000, "document has {word_count} words");
    // The non-debounced surface no longer ships (#28), so its ratio to the
    // textarea is logged above for comparison but not asserted: under host
    // load the headless timing of that path is too noisy to gate on.
    // Issue #28 acceptance: the debounced surface is no worse per keystroke
    // than the main-branch baseline (textarea plus synchronous conversion).
    assert!(
        per_debounced <= per_textarea,
        "debounced surface {per_debounced:.3} ms vs textarea {per_textarea:.3} ms per keystroke"
    );
}
