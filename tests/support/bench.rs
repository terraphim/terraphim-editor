//! Typing-latency benchmark helpers, shared by `tests/web_bench.rs` (the
//! asserted comparison) and `tests/web_bench_logged.rs` (logged-only paths).
//! Both binaries must use the same document, the same textarea baseline and
//! the same keystroke timer, or the logged ratios stop being comparable with
//! the asserted figure.

use super::*;

/// Roughly 5,000 words of Markdown (5,043 words, 30,464 characters).
pub fn five_thousand_words() -> String {
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

/// Keystrokes timed in one synchronous block before yielding to the event
/// loop. Each block holds the page's main thread; under host load a whole
/// benchmark run held it for 15-20 s, longer than the webdriver allows for
/// its result poll (`execute/sync timed out`). Yielding between blocks lets
/// the poll through without changing what is measured.
pub const KEYSTROKES_PER_BLOCK: u32 = 10;

/// Time `n` native keystrokes in one synchronous block.
pub fn time_keystrokes(n: u32) -> f64 {
    js_number(&format!(
        "(() => {{ const t0 = performance.now(); for (let i = 0; i < {n}; i++) document.execCommand('insertText', false, 'a'); return performance.now() - t0; }})()"
    ))
}

/// Time `n` keystrokes as the sum of blocks of `KEYSTROKES_PER_BLOCK`,
/// yielding to the event loop between blocks. Only for paths with no pending
/// timer (synchronous preview, textarea): a yield could otherwise let a
/// debounced render fire mid-round.
pub async fn time_keystrokes_yielding(n: u32) -> f64 {
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

/// The benchmark baseline: a plain textarea holding `doc_text`, wired to the
/// same Rust conversion on every `input` (the main-branch behaviour before
/// the editing surface), next to the full editor. Both the textarea and the
/// editing surface get the same scrolling box, as in the app (the test page
/// does not load index.html's layout CSS).
pub fn bench_textarea_baseline(document: &Document, doc_text: &str) -> HtmlTextAreaElement {
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
    textarea.set_value(doc_text);

    let size = "width: 600px; height: 300px; overflow-y: auto; font-family: monospace; padding: 1rem; box-sizing: border-box;";
    textarea.set_attribute("style", size).unwrap();
    surface_element(document)
        .set_attribute("style", size)
        .unwrap();
    textarea
}

/// Let the work the previous path left behind finish outside the timed
/// region: one frame (paint of its 5,000-word preview or textarea, idle
/// tasks) on a real timer, then any style and layout still pending. Without
/// it the first keystrokes of a round paid for the other path's deferred
/// relayout and paint, so a path's figure depended on which path ran
/// before it. Run by every path, so neither is favoured.
pub async fn bench_settle() {
    sleep(BENCH_SETTLE_MS).await;
    js_eval("void document.body.offsetHeight");
}

/// Untimed pause before each benchmark round (roughly one frame).
pub const BENCH_SETTLE_MS: u32 = 20;

/// Time `n` keystrokes typed into the middle (`at`) of the baseline textarea,
/// after `preroll` untimed keystrokes typed immediately before them.
pub async fn bench_run_textarea(
    textarea: &HtmlTextAreaElement,
    at: u32,
    preroll: u32,
    n: u32,
) -> f64 {
    textarea.focus().unwrap();
    textarea.set_selection_range(at, at).unwrap();
    bench_settle().await;
    if preroll > 0 {
        time_keystrokes(preroll);
    }
    time_keystrokes_yielding(n).await
}

/// Load `doc_text` into the editing surface and run the render `setText`
/// scheduled, so it cannot fire during a later yield and be counted as a
/// keystroke render.
pub fn bench_load_surface(doc_text: &str) {
    js_eval(&format!(
        "teTest.surface().setText({})",
        js_string_literal(doc_text)
    ));
    flush_preview();
}
