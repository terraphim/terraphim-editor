//! Helpers shared by the browser test binaries (`tests/web*.rs`): the real
//! editor scripts, the real exported document API on `window.wasmBindings`,
//! and small JavaScript interop utilities. `bench` holds the typing-latency
//! benchmark helpers and `fixtures` the persistence fixtures, each shared by
//! the two binaries their tests are split across. Nothing is mocked.
#![allow(dead_code, unused_imports)]

pub use wasm_bindgen::prelude::*;
pub use wasm_bindgen::JsCast;
pub use wasm_bindgen_futures::{js_sys::Promise, JsFuture};
pub use wasm_bindgen_test::*;
pub use web_sys::{Document, HtmlElement, HtmlTextAreaElement};

mod bench;
mod fixtures;
pub use bench::*;
pub use fixtures::*;

pub use terraphim_editor::{
    apply_edit, document_annotations, document_body, document_counts, export_document,
    flush_preview, open_document, preview_delay, preview_pending, preview_render_count,
    save_document, set_preview_delay, sync_document_body, DEFAULT_PREVIEW_DELAY_MS,
};

pub const CONFIG_JS: &str = include_str!("../../public/js/config.js");
pub const EDITOR_JS: &str = include_str!("../../public/js/editor.js");
pub const CHROME_JS: &str = include_str!("../../public/js/chrome.js");
pub const INDICATORS_JS: &str = include_str!("../../public/js/indicators.js");
pub const TOKENS_CSS: &str = include_str!("../../public/css/tokens.css");
pub const WRITE_ON_CSS: &str = include_str!("../../public/css/write-on.css");

/// Small helpers shared by the JavaScript snippets below.
pub const TEST_HELPERS_JS: &str = r##"
window.teTest = {
  surface() { return window.__teEditor.surface; },
  root() { return window.__teEditor.surface.root; },
  key(target, key, opts) {
    const ev = new KeyboardEvent('keydown', Object.assign({ key, bubbles: true, cancelable: true }, opts || {}));
    target.dispatchEvent(ev);
    return ev.defaultPrevented;
  },
  // The preview is debounced; flush any pending render (the same path a
  // save or export uses) before reading it.
  preview() {
    window.teFlushPreview();
    return document.querySelector('.markdown-preview').innerHTML;
  },
  menu() { return window.__teEditor.commandMenu; },
  resetWriteOn() {
    for (let i = localStorage.length - 1; i >= 0; i -= 1) {
      const k = localStorage.key(i);
      if (k && k.startsWith(window.WRITE_ON_STORAGE_PREFIX)) localStorage.removeItem(k);
    }
    delete document.body.dataset.mode;
  },
  // Rendered and not hidden by CSS (display, visibility or the hidden attribute).
  visible(el) {
    return !!el && el.isConnected && el.getClientRects().length > 0 &&
      getComputedStyle(el).visibility !== 'hidden';
  },
  chrome() { return window.__teEditor.chrome; },
  control(name) { return document.querySelector('.te-chrome [data-control="' + name + '"]'); },
  canonical() {
    const s = window.__teEditor.surface;
    return s.isCanonical().ok && s.root.textContent === s.getText();
  },
  // Run `act` on a fresh document `text` with decorations `decos` and the
  // selection [selStart, selEnd), then report the single change it caused,
  // the decorations afterwards, the caret and canonical state.
  editCase(text, decos, selStart, selEnd, act) {
    const s = window.__teEditor.surface;
    s.setText(text);
    s.setDecorations(decos);
    s.focus();
    s.setSelectionOffsets(selStart, selEnd);
    const changes = [];
    const off = s.onChange((c) => changes.push(c.edit));
    act(s);
    off();
    return {
      text: s.getText(),
      changes,
      decorations: s.getDecorations().map((d) => [d.id, d.start, d.end]),
      selection: s.getSelectionOffsets(),
      canonical: teTest.canonical(),
    };
  },
  // Compare an editCase result with the expected values; returns problems.
  expectEdit(name, r, want) {
    const out = [];
    if (r.text !== want.text) out.push(name + ': text ' + JSON.stringify(r.text));
    if (r.changes.length !== 1) {
      out.push(name + ': ' + r.changes.length + ' changes');
    } else {
      const e = r.changes[0];
      const got = [e.start, e.deletedLength, e.insertedText];
      const exp = [want.start, want.deletedLength, want.insertedText];
      if (JSON.stringify(got) !== JSON.stringify(exp)) out.push(name + ': edit ' + JSON.stringify(e));
    }
    if (JSON.stringify(r.decorations) !== JSON.stringify(want.decorations)) {
      out.push(name + ': decorations ' + JSON.stringify(r.decorations));
    }
    if (r.selection.start !== want.caret || r.selection.end !== want.caret) {
      out.push(name + ': caret ' + JSON.stringify(r.selection));
    }
    if (!r.canonical) out.push(name + ': not canonical');
    return out;
  },
};
"##;

#[wasm_bindgen(inline_js = r#"
export function js_eval(src) { return (0, eval)(src); }
export function sleep_ms(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }
export function install_flush(f) { window.teFlushPreview = f; }
export function install_bindings(name, f) {
  window.wasmBindings = window.wasmBindings || {};
  window.wasmBindings[name] = f;
}
"#)]
extern "C" {
    pub fn js_eval(src: &str) -> JsValue;
    pub fn sleep_ms(ms: u32) -> Promise;
    pub fn install_flush(f: &Closure<dyn FnMut() -> bool>);
    pub fn install_bindings(name: &str, f: &JsValue);
}

/// Wait on a real browser timer (no fake clocks).
pub async fn sleep(ms: u32) {
    JsFuture::from(sleep_ms(ms))
        .await
        .expect("timer promise should resolve");
}

pub fn now() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}

pub fn js_string(src: &str) -> String {
    js_eval(src)
        .as_string()
        .unwrap_or_else(|| panic!("snippet did not return a string: {src}"))
}

pub fn js_number(src: &str) -> f64 {
    js_eval(src)
        .as_f64()
        .unwrap_or_else(|| panic!("snippet did not return a number: {src}"))
}

pub fn document() -> Document {
    web_sys::window()
        .expect("no global `window` exists")
        .document()
        .expect("should have a document on window")
}

/// Remove any editor left by a previous test and render a fresh one with the
/// Rust entry point. Only the Rust side is initialised.
pub fn fresh_rust_editor() -> Document {
    let document = document();
    while let Some(node) = document
        .query_selector("#app, .command-menu, .te-bench, .te-chrome")
        .unwrap()
    {
        node.remove();
    }
    let app = document.create_element("div").unwrap();
    app.set_id("app");
    document.body().unwrap().append_child(&app).unwrap();
    terraphim_editor::run().expect("Editor should initialise");
    document
}

/// Inject the production JavaScript once per page (top-level `const` and
/// `class` declarations cannot be evaluated twice).
pub fn load_editor_scripts(document: &Document) {
    if document
        .query_selector("script[data-te-test]")
        .unwrap()
        .is_some()
    {
        return;
    }
    // The real stylesheets, so chrome visibility is checked with computed
    // styles exactly as Trunk ships them.
    let style = document.create_element("style").unwrap();
    style.set_attribute("data-te-test", "").unwrap();
    style.set_text_content(Some(&format!("{TOKENS_CSS}\n{WRITE_ON_CSS}")));
    document
        .document_element()
        .unwrap()
        .append_child(&style)
        .unwrap();
    for source in [
        CONFIG_JS,
        CHROME_JS,
        INDICATORS_JS,
        EDITOR_JS,
        TEST_HELPERS_JS,
    ] {
        let script = document.create_element("script").unwrap();
        script.set_attribute("data-te-test", "").unwrap();
        script.set_text_content(Some(source));
        document.body().unwrap().append_child(&script).unwrap();
    }
    // Expose the real exported flush to the JavaScript snippets.
    let flush = Closure::wrap(Box::new(flush_preview) as Box<dyn FnMut() -> bool>);
    install_flush(&flush);
    flush.forget();
    install_document_bindings();
}

/// Expose the real exported document API on `window.wasmBindings`, the object
/// Trunk provides in the production build, so `MarkdownEditor` reaches the
/// same Rust functions it uses in the app.
pub fn install_document_bindings() {
    fn install(name: &str, closure: JsValue) {
        install_bindings(name, &closure);
    }
    install(
        "open_document",
        Closure::<dyn FnMut(String) -> JsValue>::new(|s: String| open_document(&s)).into_js_value(),
    );
    install(
        "save_document",
        Closure::<dyn FnMut() -> String>::new(save_document).into_js_value(),
    );
    install(
        "export_document",
        Closure::<dyn FnMut() -> String>::new(export_document).into_js_value(),
    );
    install(
        "document_counts",
        Closure::<dyn FnMut() -> JsValue>::new(document_counts).into_js_value(),
    );
    install(
        "document_annotations",
        Closure::<dyn FnMut() -> JsValue>::new(document_annotations).into_js_value(),
    );
    install(
        "document_body",
        Closure::<dyn FnMut() -> String>::new(document_body).into_js_value(),
    );
    install(
        "sync_document_body",
        Closure::<dyn FnMut(String) -> JsValue>::new(|s: String| sync_document_body(&s))
            .into_js_value(),
    );
    install(
        "apply_edit",
        Closure::<dyn FnMut(u32, u32, String) -> Result<JsValue, JsValue>>::new(
            |start: u32, deleted: u32, inserted: String| apply_edit(start, deleted, &inserted),
        )
        .into_js_value(),
    );
}

/// Fresh Rust editor plus the real JavaScript controller.
pub fn fresh_full_editor() -> Document {
    let document = fresh_rust_editor();
    load_editor_scripts(&document);
    let ok = js_string(
        r##"(() => {
          if (window.__teEditor) window.__teEditor.destroy();
          // Tests share one page: start every editor in plain mode with no
          // persisted Write_On state.
          teTest.resetWriteOn();
          const ed = new MarkdownEditor(window.EditorConfig);
          ed.initialize();
          window.__teEditor = ed;
          return ed.surface ? 'ok' : 'no surface';
        })()"##,
    );
    assert_eq!(ok, "ok");
    document
}

pub fn surface_element(document: &Document) -> HtmlElement {
    document
        .query_selector(".markdown-input")
        .unwrap()
        .expect("editing surface should be present")
        .dyn_into::<HtmlElement>()
        .unwrap()
}

pub fn dispatch_input(target: &HtmlElement) {
    let event = web_sys::InputEvent::new("input").unwrap();
    target.dispatch_event(&event).unwrap();
}

pub fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[values.len() / 2]
}

/// Encode a Rust string as a JavaScript string literal.
pub fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
