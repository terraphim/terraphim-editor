//! Browser tests for the editor. Run with `wasm-pack test --headless --chrome`.
//!
//! These tests drive the real Rust entry point and the real JavaScript
//! controller (`public/js/config.js` and `public/js/editor.js`, injected as
//! classic scripts exactly as Trunk inlines them) in a real browser DOM.
//! Interaction is simulated with real DOM events and native editing commands
//! (`document.execCommand`); nothing is mocked.
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::{js_sys::Promise, JsFuture};
use wasm_bindgen_test::*;
use web_sys::{Document, HtmlElement, HtmlTextAreaElement};

use terraphim_editor::{
    flush_preview, preview_delay, preview_pending, preview_render_count, set_preview_delay,
    DEFAULT_PREVIEW_DELAY_MS,
};

wasm_bindgen_test_configure!(run_in_browser);

const CONFIG_JS: &str = include_str!("../public/js/config.js");
const EDITOR_JS: &str = include_str!("../public/js/editor.js");
const CHROME_JS: &str = include_str!("../public/js/chrome.js");
const TOKENS_CSS: &str = include_str!("../public/css/tokens.css");
const WRITE_ON_CSS: &str = include_str!("../public/css/write-on.css");

/// Small helpers shared by the JavaScript snippets below.
const TEST_HELPERS_JS: &str = r##"
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
"#)]
extern "C" {
    fn js_eval(src: &str) -> JsValue;
    fn sleep_ms(ms: u32) -> Promise;
    fn install_flush(f: &Closure<dyn FnMut() -> bool>);
}

/// Wait on a real browser timer (no fake clocks).
async fn sleep(ms: u32) {
    JsFuture::from(sleep_ms(ms))
        .await
        .expect("timer promise should resolve");
}

fn now() -> f64 {
    web_sys::window().unwrap().performance().unwrap().now()
}

fn js_string(src: &str) -> String {
    js_eval(src)
        .as_string()
        .unwrap_or_else(|| panic!("snippet did not return a string: {src}"))
}

fn js_number(src: &str) -> f64 {
    js_eval(src)
        .as_f64()
        .unwrap_or_else(|| panic!("snippet did not return a number: {src}"))
}

fn document() -> Document {
    web_sys::window()
        .expect("no global `window` exists")
        .document()
        .expect("should have a document on window")
}

/// Remove any editor left by a previous test and render a fresh one with the
/// Rust entry point. Only the Rust side is initialised.
fn fresh_rust_editor() -> Document {
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
fn load_editor_scripts(document: &Document) {
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
    for source in [CONFIG_JS, CHROME_JS, EDITOR_JS, TEST_HELPERS_JS] {
        let script = document.create_element("script").unwrap();
        script.set_attribute("data-te-test", "").unwrap();
        script.set_text_content(Some(source));
        document.body().unwrap().append_child(&script).unwrap();
    }
    // Expose the real exported flush to the JavaScript snippets.
    let flush = Closure::wrap(Box::new(flush_preview) as Box<dyn FnMut() -> bool>);
    install_flush(&flush);
    flush.forget();
}

/// Fresh Rust editor plus the real JavaScript controller.
fn fresh_full_editor() -> Document {
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

fn surface_element(document: &Document) -> HtmlElement {
    document
        .query_selector(".markdown-input")
        .unwrap()
        .expect("editing surface should be present")
        .dyn_into::<HtmlElement>()
        .unwrap()
}

fn dispatch_input(target: &HtmlElement) {
    let event = web_sys::InputEvent::new("input").unwrap();
    target.dispatch_event(&event).unwrap();
}

const BENCHMARK_TEXT: &str = r#"# Heading 1
## Heading 2
### Heading 3

This is a paragraph with **bold** and *italic* text.

- List item 1
- List item 2
  - Nested item 1
  - Nested item 2

1. Ordered item 1
2. Ordered item 2

> This is a blockquote

```rust
fn hello_world() {
    println!("Hello, World!");
}
```

[Link](https://example.com)

| Table | Header |
|-------|--------|
| Cell 1 | Cell 2 |
"#;

#[wasm_bindgen_test]
fn test_editor_initialization() {
    let document = fresh_rust_editor();

    let surface = surface_element(&document);
    assert_eq!(surface.tag_name(), "DIV");
    assert_eq!(
        surface.get_attribute("contenteditable").as_deref(),
        Some("plaintext-only")
    );
    assert!(
        document.query_selector("textarea").unwrap().is_none(),
        "the textarea must be replaced"
    );
    let text = surface.text_content().unwrap_or_default();
    assert!(text.starts_with("# Welcome to Markdown Editor!"));

    let preview = document.query_selector(".markdown-preview").unwrap();
    assert!(preview.is_some(), "Preview div should be present");
}

#[wasm_bindgen_test]
fn test_markdown_conversion() {
    let document = fresh_rust_editor();
    let surface = surface_element(&document);
    let preview = document
        .query_selector(".markdown-preview")
        .unwrap()
        .expect("Preview div should be present");

    surface.set_text_content(Some("# Test Heading"));
    dispatch_input(&surface);
    assert!(
        preview_pending(),
        "input should schedule a debounced render"
    );
    assert!(flush_preview(), "flush should run the pending render");

    let preview_html = preview.inner_html();
    assert!(
        preview_html.contains("<h1>Test Heading</h1>"),
        "Preview should contain converted markdown"
    );
}

#[wasm_bindgen_test]
fn bench_markdown_conversion_in_browser() {
    let window = web_sys::window().expect("no global `window` exists");
    let performance = window
        .performance()
        .expect("performance should be available");
    let document = fresh_rust_editor();
    let surface = surface_element(&document);

    let start = performance.now();
    for _ in 0..100 {
        surface.set_text_content(Some(BENCHMARK_TEXT));
        dispatch_input(&surface);
        // Render now so the timing still bounds the conversion itself.
        assert!(flush_preview());
    }
    let avg_time = (performance.now() - start) / 100.0;

    web_sys::console::log_1(&format!("Average conversion time: {}ms", avg_time).into());
    assert!(avg_time < 50.0, "Conversion took too long: {}ms", avg_time);
}

fn preview_html(document: &Document) -> String {
    document
        .query_selector(".markdown-preview")
        .unwrap()
        .expect("Preview div should be present")
        .inner_html()
}

#[wasm_bindgen_test]
fn test_initial_render_is_immediate() {
    let document = fresh_rust_editor();
    // No timer has had a chance to run: the initial preview is synchronous.
    assert!(preview_html(&document).contains("<h1>Welcome to Markdown Editor!</h1>"));
    assert!(!preview_pending(), "nothing should be pending after load");
    assert_eq!(preview_render_count(), 0);
    assert_eq!(preview_delay(), DEFAULT_PREVIEW_DELAY_MS);
    assert!(!flush_preview(), "flush with nothing pending is a no-op");
}

#[wasm_bindgen_test]
fn test_flush_renders_immediately() {
    let document = fresh_rust_editor();
    let surface = surface_element(&document);
    surface.set_text_content(Some("Flushed **now**"));
    dispatch_input(&surface);
    assert!(preview_pending());
    assert_eq!(preview_render_count(), 0);
    assert!(!preview_html(&document).contains("Flushed"));

    assert!(flush_preview());
    assert!(!preview_pending());
    assert_eq!(preview_render_count(), 1);
    assert!(preview_html(&document).contains("<p>Flushed <strong>now</strong></p>"));
    // A second flush has nothing to do.
    assert!(!flush_preview());
    assert_eq!(preview_render_count(), 1);
}

#[wasm_bindgen_test]
async fn test_rapid_input_renders_once_after_window() {
    let document = fresh_rust_editor();
    let surface = surface_element(&document);
    let delay = preview_delay();

    let mut text = String::new();
    for i in 0..25 {
        text.push_str(&format!("word{i} "));
        surface.set_text_content(Some(&text));
        dispatch_input(&surface);
    }
    text.push_str("\n\n# Final heading");
    surface.set_text_content(Some(&text));
    dispatch_input(&surface);
    let last_input = now();

    assert_eq!(preview_render_count(), 0, "rapid input must not render");
    assert!(preview_pending());
    assert!(!preview_html(&document).contains("Final heading"));

    // Still inside the window: nothing rendered yet. Timers never fire early,
    // so this only asserts while the elapsed time really is below the delay.
    sleep(delay / 3).await;
    if now() - last_input < f64::from(delay) {
        assert_eq!(
            preview_render_count(),
            0,
            "rendered before the window closed"
        );
    }

    sleep(delay + 100).await;
    assert_eq!(preview_render_count(), 1, "exactly one trailing render");
    assert!(!preview_pending());
    let html = preview_html(&document);
    assert!(
        html.contains("<h1>Final heading</h1>"),
        "final text: {html}"
    );
    assert!(html.contains("word24"));
}

#[wasm_bindgen_test]
async fn test_each_input_restarts_the_window() {
    let document = fresh_rust_editor();
    let surface = surface_element(&document);
    set_preview_delay(80);

    // Inputs spaced closer than the delay keep pushing the render back.
    // Timers never fire early, but a loaded runner can oversleep, so the
    // "not yet rendered" checks only apply while the gap really was short.
    let mut within_window = true;
    for i in 0..4 {
        surface.set_text_content(Some(&format!("step {i}")));
        dispatch_input(&surface);
        let last_input = now();
        sleep(30).await;
        if now() - last_input < 80.0 {
            assert_eq!(preview_render_count(), 0, "rendered mid-burst at step {i}");
        } else {
            within_window = false;
        }
    }
    sleep(80 + 100).await;
    if within_window {
        assert_eq!(preview_render_count(), 1, "one render for the whole burst");
    }
    assert!(!preview_pending());
    assert!(preview_html(&document).contains("<p>step 3</p>"));
}

#[wasm_bindgen_test]
async fn test_preview_delay_is_configurable() {
    let document = fresh_rust_editor();
    let surface = surface_element(&document);

    // Zero means synchronous rendering inside the input handler.
    set_preview_delay(0);
    assert_eq!(preview_delay(), 0);
    surface.set_text_content(Some("# Sync"));
    dispatch_input(&surface);
    assert!(!preview_pending());
    assert_eq!(preview_render_count(), 1);
    assert!(preview_html(&document).contains("<h1>Sync</h1>"));

    // A short delay renders after that delay, on a real timer.
    set_preview_delay(20);
    surface.set_text_content(Some("# Short"));
    dispatch_input(&surface);
    assert!(preview_pending());
    sleep(20 + 100).await;
    assert_eq!(preview_render_count(), 2);
    assert!(preview_html(&document).contains("<h1>Short</h1>"));

    // run() restores the default delay.
    fresh_rust_editor();
    assert_eq!(preview_delay(), DEFAULT_PREVIEW_DELAY_MS);
}

#[wasm_bindgen_test]
async fn test_rerun_cancels_stale_pending_render() {
    let document = fresh_rust_editor();
    let surface = surface_element(&document);
    surface.set_text_content(Some("# Stale"));
    dispatch_input(&surface);
    assert!(preview_pending());

    let document = fresh_rust_editor();
    assert!(!preview_pending(), "run() must cancel the old timer");
    sleep(DEFAULT_PREVIEW_DELAY_MS + 100).await;
    assert_eq!(preview_render_count(), 0, "stale timer fired");
    assert!(preview_html(&document).contains("<h1>Welcome to Markdown Editor!</h1>"));
}

#[wasm_bindgen_test]
async fn test_native_typing_on_surface_renders_once_after_debounce() {
    let document = fresh_full_editor();
    // EditorSurface normalises in a capture-phase listener; the debounced
    // Rust render later reads the normalised text.
    let before = js_string(
        r##"(() => {
          const s = teTest.surface();
          s.setText('');
          s.focus();
          s.setSelectionOffsets(0);
          for (const ch of '# Typed title') document.execCommand('insertText', false, ch);
          return s.getText();
        })()"##,
    );
    assert_eq!(before, "# Typed title");
    assert_eq!(
        preview_render_count(),
        0,
        "typing must not render synchronously"
    );
    assert!(preview_pending());
    sleep(preview_delay() + 100).await;
    assert_eq!(preview_render_count(), 1);
    assert!(preview_html(&document).contains("<h1>Typed title</h1>"));
}

#[wasm_bindgen_test]
fn test_surface_initial_state_is_canonical() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          if (!s.getText().startsWith('# Welcome to Markdown Editor!')) out.push('initial text');
          if (!s.getText().endsWith('\n')) out.push('initial text should end with a newline');
          if (!teTest.canonical()) out.push('not canonical');
          const last = s.root.lastChild;
          if (!(last && last.tagName === 'BR' && last.classList.contains('te-placeholder'))) out.push('placeholder br missing');
          if (s.root.getAttribute('role') !== 'textbox') out.push('role');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_every_shortcut_wraps_selection_on_surface() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const shortcuts = window.EditorConfig.shortcuts;
          if (shortcuts.length < 5) out.push('expected the five configured shortcuts');
          for (const sc of shortcuts) {
            const key = sc.key.split('+').pop();
            // Non-empty selection: wrap it and keep it selected.
            s.setText('hello world');
            s.setSelectionOffsets(6, 11);
            const prevented = teTest.key(s.root, key, { ctrlKey: true });
            const want = 'hello ' + sc.prefix + 'world' + sc.suffix;
            if (!prevented) out.push(sc.key + ': default not prevented');
            if (s.getText() !== want) out.push(sc.key + ': got ' + JSON.stringify(s.getText()));
            const sel = s.getSelectionOffsets();
            if (sel.start !== 6 + sc.prefix.length || sel.end !== 11 + sc.prefix.length) {
              out.push(sc.key + ': selection ' + JSON.stringify(sel));
            }
            if (!teTest.canonical()) out.push(sc.key + ': not canonical');
            // Empty selection: insert a selected placeholder word.
            s.setText('ab');
            s.setSelectionOffsets(1);
            teTest.key(s.root, key, { ctrlKey: true });
            const want2 = 'a' + sc.prefix + 'text' + sc.suffix + 'b';
            if (s.getText() !== want2) out.push(sc.key + ' (empty): got ' + JSON.stringify(s.getText()));
            const sel2 = s.getSelectionOffsets();
            if (sel2.start !== 1 + sc.prefix.length || sel2.end !== 5 + sc.prefix.length) {
              out.push(sc.key + ' (empty): selection ' + JSON.stringify(sel2));
            }
          }
          // The Rust preview must follow shortcut edits.
          s.setText('hello world');
          s.setSelectionOffsets(6, 11);
          teTest.key(s.root, 'b', { ctrlKey: true });
          if (!teTest.preview().includes('<strong>world</strong>')) out.push('preview not updated: ' + teTest.preview());
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_toolbar_buttons_use_last_selection() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const buttons = document.querySelectorAll('#formatting-toolbar sl-button');
          const shortcuts = window.EditorConfig.shortcuts;
          if (buttons.length !== shortcuts.length) out.push('button count ' + buttons.length);
          buttons.forEach((button, i) => {
            const sc = shortcuts[i];
            s.setText('one two');
            s.setSelectionOffsets(4, 7);
            // Move the live selection out of the surface, as a toolbar click can.
            getSelection().removeAllRanges();
            button.focus();
            button.click();
            const want = 'one ' + sc.prefix + 'two' + sc.suffix;
            if (s.getText() !== want) out.push('button ' + i + ': ' + JSON.stringify(s.getText()));
          });
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_slash_command_palette_on_surface() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const menu = teTest.menu();
          const out = [];
          // '/' inserts a slash and opens the palette at the caret.
          s.setText('Intro\n');
          s.setSelectionOffsets(6);
          if (!teTest.key(s.root, '/')) out.push('slash default not prevented');
          if (s.getText() !== 'Intro\n/') out.push('slash text ' + JSON.stringify(s.getText()));
          if (menu.style.display !== 'block') out.push('menu not shown');
          if (menu.style.position !== 'fixed') out.push('menu not positioned');
          // Keyboard: ArrowDown selects the second command, Enter applies it.
          teTest.key(menu, 'ArrowDown');
          const selected = menu.querySelectorAll('.command-item.selected');
          if (selected.length !== 1 || selected[0] !== menu.querySelectorAll('.command-item')[1]) out.push('selection highlight');
          teTest.key(menu, 'Enter');
          const cmd = window.EditorConfig.commands[1];
          const want = 'Intro\n' + cmd.prefix + 'text' + cmd.suffix;
          if (s.getText() !== want) out.push('enter applied ' + JSON.stringify(s.getText()));
          if (menu.style.display !== 'none') out.push('menu not hidden after Enter');
          if (!teTest.preview().includes('<h2>text</h2>')) out.push('preview ' + teTest.preview());
          // Escape closes the palette and keeps the literal slash.
          s.setText('a');
          s.setSelectionOffsets(1);
          teTest.key(s.root, '/');
          teTest.key(menu, 'Escape');
          if (s.getText() !== 'a/') out.push('escape text ' + JSON.stringify(s.getText()));
          if (menu.style.display !== 'none') out.push('menu not hidden after Escape');
          // Every command, applied by click.
          const items = menu.querySelectorAll('.command-item');
          window.EditorConfig.commands.forEach((c, i) => {
            s.setText('x ');
            s.setSelectionOffsets(2);
            teTest.key(s.root, '/');
            items[i].click();
            const w = 'x ' + c.prefix + 'text' + c.suffix;
            if (s.getText() !== w) out.push(c.name + ': ' + JSON.stringify(s.getText()));
            if (!teTest.canonical()) out.push(c.name + ': not canonical');
          });
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_selection_offset_mapping_unicode_and_paragraphs() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const text = 'First paragraph line\nsecond line\n\nEmoji \u{1F600} and family \u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467} here\n' +
                       'Café and naïve and क्ष\n\nLast ❤️\n';
          s.setText(text);
          if (s.getText() !== text) out.push('setText round trip');
          const isHigh = (c) => c >= 0xd800 && c <= 0xdbff;
          const snap = (o) => (o > 0 && o < text.length && isHigh(text.charCodeAt(o - 1)) && !isHigh(text.charCodeAt(o))) ? o - 1 : o;
          const check = () => {
            for (let o = 0; o <= text.length; o++) {
              s.setSelectionOffsets(o);
              const got = s.getSelectionOffsets();
              const want = snap(o);
              if (got.start !== want || got.end !== want) { out.push('offset ' + o + ' -> ' + JSON.stringify(got)); continue; }
              // Independent check against the DOM: text before the DOM caret.
              const sel = getSelection();
              const r = document.createRange();
              r.setStart(s.root, 0);
              r.setEnd(sel.focusNode, sel.focusOffset);
              if (r.toString() !== text.slice(0, want)) out.push('dom prefix mismatch at ' + o);
            }
            // Ranges spanning paragraphs, emoji and combining marks, both directions.
            const e = text.indexOf('\u{1F600}');
            const c = text.indexOf('é');
            const pairs = [[0, 21], [10, 40], [e, e + 2], [c, c + 2], [5, text.length], [e - 3, c + 2]];
            for (const [a, b] of pairs) {
              s.setSelectionOffsets(a, b);
              let got = s.getSelectionOffsets();
              if (got.start !== a || got.end !== b || got.direction !== 'forward') out.push('range ' + a + '-' + b + ' ' + JSON.stringify(got));
              if (getSelection().toString() !== text.slice(a, b)) out.push('selected text ' + a + '-' + b);
              s.setSelectionOffsets(b, a);
              got = s.getSelectionOffsets();
              if (got.start !== a || got.end !== b || got.direction !== 'backward') out.push('backward ' + a + '-' + b + ' ' + JSON.stringify(got));
            }
            // An offset inside a surrogate pair snaps to the pair start.
            s.setSelectionOffsets(e + 1, e + 1);
            if (s.getSelectionOffsets().start !== e) out.push('surrogate snap');
          };
          check();
          // The same mapping must hold when decoration spans split the text.
          const e = text.indexOf('\u{1F600}');
          const c = text.indexOf('Café');
          const decorations = s.setDecorations([
            { start: e, end: e + 2, className: 'te-test-a' },
            { start: c, end: c + 5, className: 'te-test-b' },
            { start: c + 3, end: c + 12, className: 'te-test-c' },
            { start: 15, end: 30 },
          ]);
          if (decorations.length !== 4) out.push('decoration count ' + decorations.length);
          if (s.getText() !== text || !teTest.canonical()) out.push('decorated text not canonical');
          const spans = Array.from(s.root.querySelectorAll('span.te-decoration'));
          if (spans.length < 5) out.push('expected split spans, got ' + spans.length);
          const emojiSpan = spans.find((sp) => sp.classList.contains('te-test-a'));
          if (!emojiSpan || emojiSpan.textContent !== '\u{1F600}') out.push('emoji span text');
          const cafe = spans.filter((sp) => sp.classList.contains('te-test-b')).map((sp) => sp.textContent).join('');
          if (cafe !== 'Café') out.push('combining span text ' + JSON.stringify(cafe));
          const overlap = spans.find((sp) => sp.classList.contains('te-test-b') && sp.classList.contains('te-test-c'));
          if (!overlap || overlap.getAttribute('data-te-decoration').split(' ').length !== 2) out.push('overlap span');
          check();
          return out.slice(0, 20).join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_browser_dom_shapes_read_back_as_plain_text() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          // Block-per-line shape, as produced by contenteditable="true" engines.
          s.root.innerHTML = '<div>one</div><div>two \u{1F600}</div><div><br></div><div>é</div>';
          let r = s.serialise();
          if (r.text !== 'one\ntwo \u{1F600}\n\né') out.push('div shape ' + JSON.stringify(r.text));
          const second = s.root.childNodes[1].firstChild;
          if (s.pointToOffset(second, 4) !== 8) out.push('div point ' + s.pointToOffset(second, 4));
          if (s.pointToOffset(s.root.childNodes[3].firstChild, 2) !== 14) out.push('combining point');
          if (s.pointToOffset(s.root.childNodes[3].firstChild, 1) !== 13) out.push('between base and mark');
          // An input event normalises it to the canonical shape and updates the preview.
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
          if (s.getText() !== 'one\ntwo \u{1F600}\n\né') out.push('normalised text ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('not canonical after input');
          // The Rust preview must see the normalised text: two paragraphs only
          // appear if it read "para one\n\npara two" after the capture-phase
          // sync, not the raw textContent "para onepara two".
          s.root.innerHTML = '<div>para one</div><div><br></div><div>para two</div>';
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
          if (!teTest.preview().includes('<p>para one</p>') || !teTest.preview().includes('<p>para two</p>')) {
            out.push('preview read un-normalised DOM: ' + teTest.preview());
          }
          // <br> shape.
          s.root.innerHTML = 'a<br>b<br>';
          r = s.serialise();
          if (r.text !== 'a\nb') out.push('br shape ' + JSON.stringify(r.text));
          s.root.innerHTML = 'a<br><br>';
          if (s.serialise().text !== 'a\n') out.push('double br shape ' + JSON.stringify(s.serialise().text));
          if (s.pointToOffset(s.root, 2) !== 2) out.push('element point');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_native_typing_newlines_and_paste() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          s.setText('Hello');
          s.focus();
          s.setSelectionOffsets(5);
          // Native typing pipeline (beforeinput/input fired by the browser).
          const renders = s.renderCount;
          if (!document.execCommand('insertText', false, ' world')) out.push('execCommand failed');
          if (s.renderCount !== renders) out.push('native typing rebuilt the DOM');
          if (s.getText() !== 'Hello world') out.push('typed ' + JSON.stringify(s.getText()));
          if (!teTest.preview().includes('Hello world')) out.push('preview after typing');
          if (!teTest.canonical()) out.push('not canonical after typing');
          // Enter is handled as a literal newline.
          const enter = new InputEvent('beforeinput', { inputType: 'insertParagraph', bubbles: true, cancelable: true });
          s.root.dispatchEvent(enter);
          if (!enter.defaultPrevented) out.push('enter not handled');
          if (s.getText() !== 'Hello world\n') out.push('enter text ' + JSON.stringify(s.getText()));
          if (!(s.root.lastChild && s.root.lastChild.tagName === 'BR')) out.push('placeholder after trailing newline');
          if (s.getSelectionOffsets().start !== 12) out.push('caret after enter');
          document.execCommand('insertText', false, 'next');
          if (s.getText() !== 'Hello world\nnext') out.push('typed after enter ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('not canonical after enter');
          // Paste strips formatting and normalises line endings.
          const dt = new DataTransfer();
          dt.setData('text/html', '<b>rich</b><img src=x>');
          dt.setData('text/plain', 'p1\r\np2\rp3');
          s.setSelectionOffsets(0);
          const paste = new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true });
          s.root.dispatchEvent(paste);
          if (!paste.defaultPrevented) out.push('paste not cancelled');
          if (s.getText() !== 'p1\np2\np3Hello world\nnext') out.push('paste text ' + JSON.stringify(s.getText()));
          if (s.root.querySelector('b, img')) out.push('rich content inserted');
          if (s.getSelectionOffsets().start !== 8) out.push('caret after paste');
          // Copy produces plain text with newlines.
          const copyData = new DataTransfer();
          s.setSelectionOffsets(0, 5);
          s.root.dispatchEvent(new ClipboardEvent('copy', { clipboardData: copyData, bubbles: true, cancelable: true }));
          if (copyData.getData('text/plain') !== 'p1\np2') out.push('copy ' + JSON.stringify(copyData.getData('text/plain')));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_undo_redo_stack() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          s.setText('base');
          s.focus();
          s.setSelectionOffsets(4);
          // A burst of typing coalesces into one undo step.
          document.execCommand('insertText', false, ' a');
          document.execCommand('insertText', false, 'b');
          document.execCommand('insertText', false, 'c');
          if (s.getText() !== 'base abc') out.push('typed ' + JSON.stringify(s.getText()));
          s.setSelectionOffsets(5, 8);
          teTest.key(s.root, 'b', { ctrlKey: true });
          if (s.getText() !== 'base **abc**') out.push('bold ' + JSON.stringify(s.getText()));
          if (!teTest.key(s.root, 'z', { ctrlKey: true })) out.push('ctrl+z not prevented');
          if (s.getText() !== 'base abc') out.push('undo 1 ' + JSON.stringify(s.getText()));
          if (teTest.preview().includes('<strong>')) out.push('preview did not follow undo');
          teTest.key(s.root, 'z', { ctrlKey: true });
          if (s.getText() !== 'base') out.push('undo 2 ' + JSON.stringify(s.getText()));
          teTest.key(s.root, 'z', { ctrlKey: true, shiftKey: true });
          if (s.getText() !== 'base abc') out.push('redo 1 ' + JSON.stringify(s.getText()));
          teTest.key(s.root, 'y', { ctrlKey: true });
          if (s.getText() !== 'base **abc**') out.push('redo 2 ' + JSON.stringify(s.getText()));
          if (!teTest.preview().includes('<strong>abc</strong>')) out.push('preview did not follow redo');
          // Undo from the browser's edit menu arrives as beforeinput historyUndo.
          const ev = new InputEvent('beforeinput', { inputType: 'historyUndo', bubbles: true, cancelable: true });
          s.root.dispatchEvent(ev);
          if (!ev.defaultPrevented || s.getText() !== 'base abc') out.push('historyUndo ' + JSON.stringify(s.getText()));
          // A new edit discards the redo branch.
          s.replaceRange(0, 0, 'X');
          if (s.redo()) out.push('redo branch not discarded');
          if (!s.undo() || s.getText() !== 'base abc') out.push('undo after new edit');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_ime_composition_is_not_corrupted() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          s.setText('ab');
          s.focus();
          s.setSelectionOffsets(1);
          const historyBefore = s.history.length;
          const node = s.root.firstChild;
          s.root.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
          // Interim composition text, written into the DOM as an IME would.
          node.insertData(1, 'ni');
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true, inputType: 'insertCompositionText', data: 'ni' }));
          if (s.root.firstChild !== node) out.push('DOM re-rendered during composition');
          if (s.getText() !== 'ab') out.push('model changed during composition');
          if (s.history.length !== historyBefore) out.push('history recorded during composition');
          // Shortcuts are ignored while composing.
          if (teTest.key(s.root, 'b', { ctrlKey: true, isComposing: true })) out.push('shortcut fired while composing');
          if (teTest.key(s.root, '/', { isComposing: true })) out.push('palette fired while composing');
          // Commit the final text.
          node.replaceData(1, 2, '你');
          s.root.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '你' }));
          if (s.getText() !== 'a你b') out.push('committed ' + JSON.stringify(s.getText()));
          if (s.history.length !== historyBefore + 1) out.push('expected one history entry');
          if (!teTest.canonical()) out.push('not canonical after composition');
          s.undo();
          if (s.getText() !== 'ab') out.push('undo composition ' + JSON.stringify(s.getText()));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_decorations_follow_edits() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const changes = [];
          const off = s.onChange((c) => changes.push(c));
          s.setText('alpha beta gamma');
          s.setDecorations([
            { id: 'b', start: 6, end: 10, className: 'te-test-alt' },
            { id: 'g', start: 11, end: 16, data: { note: 'g' } },
          ]);
          const texts = () => Array.from(s.root.querySelectorAll('span.te-decoration')).map((sp) => sp.getAttribute('data-te-decoration') + ':' + sp.textContent).join(',');
          if (texts() !== 'b:beta,g:gamma') out.push('initial spans ' + texts());
          if (s.decorationsAt(7).map((d) => d.id).join() !== 'b') out.push('decorationsAt');
          // An edit before the decorations shifts them.
          s.replaceRange(0, 0, 'X ');
          let d = s.getDecorations();
          if (d.length !== 2 || d[0].start !== 8 || d[0].end !== 12 || d[1].start !== 13) out.push('shifted ' + JSON.stringify(d));
          if (texts() !== 'b:beta,g:gamma') out.push('spans after shift ' + texts());
          // An edit inside a decoration drops it; later ones still shift.
          s.replaceRange(10, 10, 'ZZ');
          d = s.getDecorations();
          if (d.length !== 1 || d[0].id !== 'g' || d[0].start !== 15) out.push('dropped ' + JSON.stringify(d));
          // Native typing straight after a decoration does not extend it.
          s.focus();
          s.setSelectionOffsets(20);
          document.execCommand('insertText', false, '!');
          if (s.getText() !== 'X alpha beZZta gamma!') out.push('typed ' + JSON.stringify(s.getText()));
          if (texts() !== 'g:gamma') out.push('spans after typing ' + texts());
          if (!teTest.canonical()) out.push('not canonical');
          if (changes.length < 4) out.push('change hook calls ' + changes.length);
          const lastEdit = changes[changes.length - 1].edit;
          if (lastEdit.start !== 20 || lastEdit.insertedText !== '!' || lastEdit.deletedLength !== 0) out.push('edit ' + JSON.stringify(lastEdit));
          off();
          s.clearDecorations();
          if (s.root.querySelector('span')) out.push('clear');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_native_edits_in_repeated_text_use_the_edit_location() {
    let _document = fresh_full_editor();
    // execCommand fires no beforeinput, so these exercise the location
    // derived from the pre-edit selection checked against the post-edit
    // caret. The plain content diff would attribute every one of these
    // edits to the end of the repeated run.
    let result = js_string(
        r##"(() => {
          const out = [];
          const ins = (t) => () => document.execCommand('insertText', false, t);
          let r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, ins('a'));
          out.push(...teTest.expectEdit('insert a in aaaa', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          r = teTest.editCase('abab', [{ id: 'd', start: 2, end: 4 }], 2, 2, ins('ab'));
          out.push(...teTest.expectEdit('insert ab in abab', r, {
            text: 'ababab', start: 2, deletedLength: 0, insertedText: 'ab', decorations: [['d', 4, 6]], caret: 4,
          }));
          r = teTest.editCase('one one one', [{ id: 'd', start: 8, end: 11 }], 4, 4, ins('one '));
          out.push(...teTest.expectEdit('insert repeated word', r, {
            text: 'one one one one', start: 4, deletedLength: 0, insertedText: 'one ', decorations: [['d', 12, 15]], caret: 8,
          }));
          // Backward delete of one of several identical characters.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, () => document.execCommand('delete'));
          out.push(...teTest.expectEdit('backward delete in aaaa', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // Forward delete of one of several identical characters.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 1, 1, () => document.execCommand('forwardDelete'));
          out.push(...teTest.expectEdit('forward delete in aaaa', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // Backward delete of a repeated word.
          r = teTest.editCase('ab ab ab', [{ id: 'd', start: 6, end: 8 }], 3, 6, () => document.execCommand('delete'));
          out.push(...teTest.expectEdit('delete repeated word', r, {
            text: 'ab ab', start: 3, deletedLength: 3, insertedText: '', decorations: [['d', 3, 5]], caret: 3,
          }));
          // Replacement over a selection: the replaced character's decoration
          // is dropped, the later one shifts.
          r = teTest.editCase('aaaa', [{ id: 'x', start: 1, end: 2 }, { id: 'd', start: 3, end: 4 }], 1, 2, ins('aa'));
          out.push(...teTest.expectEdit('replace selection in aaaa', r, {
            text: 'aaaaa', start: 1, deletedLength: 1, insertedText: 'aa', decorations: [['d', 4, 5]], caret: 3,
          }));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_beforeinput_location_hints_in_repeated_text() {
    let _document = fresh_full_editor();
    // Real `beforeinput` events (as the browser fires for keyboard input)
    // followed by the native edit itself. The surface records the pre-edit
    // range from the event and uses it when the edit's `input` arrives.
    let result = js_string(
        r##"(() => {
          const out = [];
          const before = (s, inputType, data, range) => {
            const init = { inputType, data, bubbles: true, cancelable: true };
            if (range) {
              const a = s.offsetToPoint(range[0]);
              const b = s.offsetToPoint(range[1]);
              init.targetRanges = [new StaticRange({ startContainer: a.node, startOffset: a.offset, endContainer: b.node, endOffset: b.offset })];
            }
            const ev = new InputEvent('beforeinput', init);
            s.root.dispatchEvent(ev);
            if (ev.defaultPrevented) out.push(inputType + ' unexpectedly cancelled');
          };
          // Exact target range [1, 2): a replacement in repeated text.
          let r = teTest.editCase('aaaa', [{ id: 'x', start: 1, end: 2 }, { id: 'd', start: 3, end: 4 }], 1, 2, (s) => {
            before(s, 'insertText', 'aa', [1, 2]);
            document.execCommand('insertText', false, 'aa');
          });
          out.push(...teTest.expectEdit('target range replacement', r, {
            text: 'aaaaa', start: 1, deletedLength: 1, insertedText: 'aa', decorations: [['d', 4, 5]], caret: 3,
          }));
          // Exact target range for a backward delete in repeated text.
          r = teTest.editCase('abab', [{ id: 'd', start: 2, end: 4 }], 2, 2, (s) => {
            before(s, 'deleteContentBackward', null, [1, 2]);
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('target range delete', r, {
            text: 'aab', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 1, 3]], caret: 1,
          }));
          // No target range: the selection is the fallback. For a backward
          // delete from a caret it does not cover the deleted character, so
          // it is widened backwards by the deletion length.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, (s) => {
            before(s, 'deleteContentBackward', null, null);
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('selection hint backward delete', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // No target range, insertion: the selection hint is consistent.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, (s) => {
            before(s, 'insertText', 'a', null);
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('selection hint insert', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          // A hint is only used by the edit it was recorded for: a (wrong)
          // target range recorded before a programmatic change is ignored by
          // the next native edit.
          r = teTest.editCase('aaaa', [], 1, 1, (s) => {
            before(s, 'insertText', 'a', [3, 3]);
            s.replaceRange(4, 4, 'a');
            s.setSelectionOffsets(1);
            document.execCommand('insertText', false, 'a');
          });
          if (r.text !== 'aaaaaa') out.push('stale hint text ' + JSON.stringify(r.text));
          const last = r.changes[r.changes.length - 1];
          if (r.changes.length !== 2 || last.start !== 1) out.push('stale hint used ' + JSON.stringify(r.changes));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_edit_location_fallbacks() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const s = teTest.surface();
          // External mutation with no selection in the surface and a newer DOM
          // selection pending: no location is known, so the plain diff is used
          // and the model still matches the DOM.
          s.setText('aaaa');
          s.focus();
          getSelection().setBaseAndExtent(s.root.firstChild, 3, s.root.firstChild, 3);
          document.dispatchEvent(new Event('selectionchange'));
          if (!s.pendingSelection) out.push('selectionchange not recorded');
          getSelection().removeAllRanges();
          const changes = [];
          const off = s.onChange((c) => changes.push(c.edit));
          s.root.firstChild.insertData(1, 'a');
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
          off();
          if (s.getText() !== 'aaaaa') out.push('fallback text ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('fallback not canonical');
          if (changes.length !== 1) out.push('fallback changes ' + changes.length);
          else if (changes[0].insertedText !== 'a' || changes[0].deletedLength !== 0) out.push('fallback edit ' + JSON.stringify(changes[0]));
          // A newer DOM selection is pending, so the last known selection is
          // not trusted: the post-edit caret read from the DOM decides.
          let r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, () => {
            document.dispatchEvent(new Event('selectionchange'));
            if (!s.pendingSelection) out.push('caret case: selectionchange not recorded');
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('caret only insert', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, () => {
            document.dispatchEvent(new Event('selectionchange'));
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('caret only delete', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // The hinted diff always reproduces the new text, even for a hint
          // that does not match the change.
          const cases = [
            ['aaaa', 'aaaaa', { start: 3, end: 3 }],
            ['abab', 'aabb', { start: 0, end: 4 }],
            ['aaaa', 'aaa', { caret: 0 }],
            ['x\u{1F600}\u{1F600}y', 'x\u{1F600}y', { caret: 2 }],
            ['hello', 'help', { start: 9, end: 12 }],
          ];
          for (const [a, b, hint] of cases) {
            const e = EditorSurface.diff(a, b, hint);
            const applied = a.slice(0, e.start) + e.insertedText + a.slice(e.start + e.deletedLength);
            if (applied !== b) out.push('diff ' + JSON.stringify([a, b, hint]) + ' gave ' + JSON.stringify(e));
            if (e.deletedText !== a.slice(e.start, e.start + e.deletedLength)) out.push('deletedText ' + JSON.stringify(e));
          }
          // A surrogate pair is never split by a hinted diff.
          const e = EditorSurface.diff('x\u{1F600}\u{1F600}y', 'x\u{1F600}y', { caret: 2 });
          if (e.start !== 1 && e.start !== 3) out.push('surrogate split ' + JSON.stringify(e));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_destroy_removes_listeners_and_palette_dom() {
    let document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const a = window.__teEditor;
          const shortcuts = window.EditorConfig.shortcuts;
          a.destroy();
          a.destroy(); // idempotent
          if (!a.destroyed || !a.surface.destroyed) out.push('not marked destroyed');
          if (document.querySelectorAll('.command-menu').length !== 0) out.push('palette DOM left behind');
          if (document.querySelectorAll('#formatting-toolbar sl-button').length !== 0) out.push('toolbar buttons left behind');
          if (document.querySelectorAll('#shortcuts-list .shortcut-item').length !== 0) out.push('help items left behind');
          const b = new MarkdownEditor(window.EditorConfig);
          b.initialize();
          window.__teEditor = b;
          const s = b.surface;
          const root = s.root;
          if (document.querySelectorAll('.command-menu').length !== 1) out.push('palette count ' + document.querySelectorAll('.command-menu').length);
          if (document.querySelectorAll('#formatting-toolbar sl-button').length !== shortcuts.length) out.push('toolbar duplicated');
          if (document.querySelectorAll('#shortcuts-list .shortcut-item').length !== shortcuts.length) out.push('help items duplicated');
          // The document-level selectionchange listener only reaches B.
          s.setText('hello');
          getSelection().setBaseAndExtent(root.firstChild, 1, root.firstChild, 3);
          a.surface.pendingSelection = null;
          s.pendingSelection = null;
          document.dispatchEvent(new Event('selectionchange'));
          if (a.surface.pendingSelection !== null) out.push('destroyed surface saw selectionchange');
          if (s.pendingSelection === null) out.push('live surface missed selectionchange');
          // Palette key: exactly one slash and only B's menu opens.
          s.setText('x');
          s.setSelectionOffsets(1);
          teTest.key(root, '/');
          if (s.getText() !== 'x/') out.push('slash text ' + JSON.stringify(s.getText()));
          if (b.commandMenu.style.display !== 'block') out.push('live menu not shown');
          if (a.commandMenu.style.display !== 'none') out.push('destroyed menu shown');
          teTest.key(b.commandMenu, 'Escape');
          // Shortcut: wrapped once, not once per editor ever created.
          s.setText('word');
          s.setSelectionOffsets(0, 4);
          teTest.key(root, 'b', { ctrlKey: true });
          if (s.getText() !== '**word**') out.push('shortcut ' + JSON.stringify(s.getText()));
          // Native input only reaches the live model.
          const before = a.surface.getText();
          s.focus();
          s.setSelectionOffsets(s.getText().length);
          document.execCommand('insertText', false, '!');
          if (s.getText() !== '**word**!') out.push('live text ' + JSON.stringify(s.getText()));
          if (a.surface.getText() !== before) out.push('destroyed surface followed input');
          // A document click no longer reaches A's palette handler.
          a.commandMenu.style.display = 'block';
          document.body.click();
          if (a.commandMenu.style.display !== 'block') out.push('destroyed palette saw document click');
          if (!teTest.canonical()) out.push('not canonical');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
    // Only the live editor's palette remains in the document.
    let _menu = document
        .query_selector(".command-menu")
        .unwrap()
        .expect("live palette should be present");
    assert_eq!(
        js_number("document.querySelectorAll('.command-menu').length"),
        1.0
    );
}

#[wasm_bindgen_test]
fn test_init_editor_replaces_previous_instance() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          window.__teEditor.destroy();
          window.terraphimEditor = undefined;
          initEditor();
          const first = window.terraphimEditor;
          if (!first || !first.surface) out.push('first init');
          initEditor();
          const second = window.terraphimEditor;
          if (!second || second === first) out.push('second init did not replace the editor');
          if (!first.destroyed || !first.surface.destroyed) out.push('previous editor not destroyed');
          if (second.destroyed) out.push('new editor destroyed');
          if (document.querySelectorAll('.command-menu').length !== 1) out.push('palette count ' + document.querySelectorAll('.command-menu').length);
          const n = window.EditorConfig.shortcuts.length;
          if (document.querySelectorAll('#formatting-toolbar sl-button').length !== n) out.push('toolbar duplicated');
          window.__teEditor = second;
          const s = second.surface;
          s.setText('x');
          s.setSelectionOffsets(1);
          teTest.key(s.root, '/');
          if (s.getText() !== 'x/') out.push('slash text ' + JSON.stringify(s.getText()));
          teTest.key(second.commandMenu, 'Escape');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_undo_redo_replays_exact_edits_in_repeated_text() {
    let _document = fresh_full_editor();
    // Undo and redo must reverse/replay the recorded edit, not a re-diff of
    // the text, so decorations after the edit and the caret land correctly.
    let result = js_string(
        r##"(() => {
          const out = [];
          const s = teTest.surface();
          const state = () => JSON.stringify({
            text: s.getText(),
            decorations: s.getDecorations().map((d) => [d.id, d.start, d.end]),
            caret: [s.getSelectionOffsets().start, s.getSelectionOffsets().end],
          });
          const want = (text, decorations, caret) => JSON.stringify({ text, decorations, caret: [caret, caret] });
          const check = (name, expected) => {
            const got = state();
            if (got !== expected) out.push(name + ': ' + got);
            if (!teTest.canonical()) out.push(name + ': not canonical');
          };
          // Fresh history so coalescing cannot join a previous case.
          const setup = (text, decos, start, end) => {
            s.setText(text);
            s.history = [];
            s.historyIndex = -1;
            s.record('init', { start: 0, end: 0 });
            s.setDecorations(decos);
            s.focus();
            s.setSelectionOffsets(start, end);
          };
          const ins = (t) => document.execCommand('insertText', false, t);
          const undo = () => { if (!teTest.key(s.root, 'z', { ctrlKey: true })) out.push('ctrl+z not handled'); };
          const redo = () => { if (!teTest.key(s.root, 'z', { ctrlKey: true, shiftKey: true })) out.push('ctrl+shift+z not handled'); };

          // 1. Single insertion in "aaaa"; the undo change is reported exactly.
          setup('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1);
          ins('a');
          check('insert', want('aaaaa', [['d', 3, 5]], 2));
          const changes = [];
          const off = s.onChange((c) => changes.push(c.edit));
          undo();
          off();
          check('insert undo', want('aaaa', [['d', 2, 4]], 1));
          if (changes.length !== 1 || changes[0].start !== 1 || changes[0].deletedLength !== 1 || changes[0].insertedText !== '') {
            out.push('undo change ' + JSON.stringify(changes));
          }
          redo();
          check('insert redo', want('aaaaa', [['d', 3, 5]], 2));
          undo();
          check('insert undo again', want('aaaa', [['d', 2, 4]], 1));

          // 2. Repeated word.
          setup('one one one', [{ id: 'd', start: 8, end: 11 }], 4, 4);
          ins('one ');
          check('word', want('one one one one', [['d', 12, 15]], 8));
          undo();
          check('word undo', want('one one one', [['d', 8, 11]], 4));
          redo();
          check('word redo', want('one one one one', [['d', 12, 15]], 8));

          // 3. Merged typing burst: three keystrokes, one undo step.
          setup('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1);
          const depth = s.history.length;
          ins('a'); ins('a'); ins('a');
          if (s.history.length !== depth + 1) out.push('burst not coalesced: ' + (s.history.length - depth));
          const top = s.history[s.historyIndex];
          if (!top.edits || top.edits.length !== 1) out.push('burst steps ' + JSON.stringify(top.edits));
          check('burst', want('aaaaaaa', [['d', 5, 7]], 4));
          undo();
          check('burst undo', want('aaaa', [['d', 2, 4]], 1));
          redo();
          check('burst redo', want('aaaaaaa', [['d', 5, 7]], 4));

          // 4. Merged burst at two separate places stays exact.
          setup('aaaa', [{ id: 'd', start: 3, end: 4 }], 1, 1);
          ins('a');
          s.setSelectionOffsets(5);
          ins('a');
          check('split burst', want('aaaaaa', [['d', 4, 5]], 6));
          if (s.history[s.historyIndex].edits.length !== 2) out.push('split burst steps ' + JSON.stringify(s.history[s.historyIndex].edits));
          undo();
          check('split burst undo', want('aaaa', [['d', 3, 4]], 1));
          redo();
          check('split burst redo', want('aaaaaa', [['d', 4, 5]], 6));

          // 5. Merged backspace burst.
          setup('aaaa', [{ id: 'd', start: 3, end: 4 }], 3, 3);
          document.execCommand('delete');
          document.execCommand('delete');
          check('backspace burst', want('aa', [['d', 1, 2]], 1));
          undo();
          check('backspace burst undo', want('aaaa', [['d', 3, 4]], 3));
          redo();
          check('backspace burst redo', want('aa', [['d', 1, 2]], 1));

          // 6. Replacement over a selection: the replaced character's
          // decoration is gone, the later one maps both ways.
          setup('aaaa', [{ id: 'x', start: 1, end: 2 }, { id: 'd', start: 3, end: 4 }], 1, 2);
          ins('aa');
          check('replace', want('aaaaa', [['d', 4, 5]], 3));
          undo();
          check('replace undo', want('aaaa', [['d', 3, 4]], 2));
          redo();
          check('replace redo', want('aaaaa', [['d', 4, 5]], 3));

          // 7. Programmatic edits (shortcuts) are replayed exactly too.
          setup('ab ab ab', [{ id: 'd', start: 6, end: 8 }], 3, 5);
          teTest.key(s.root, 'b', { ctrlKey: true });
          if (s.getText() !== 'ab **ab** ab') out.push('bold ' + JSON.stringify(s.getText()));
          undo();
          check('bold undo', want('ab ab ab', [['d', 6, 8]], 5));

          // 8. Guarded fallback: an entry without edit data still restores
          // the text through the minimal diff.
          setup('abc', [], 3, 3);
          ins('d');
          s.history[s.historyIndex].edits = null;
          undo();
          if (s.getText() !== 'abc') out.push('fallback undo ' + JSON.stringify(s.getText()));
          redo();
          if (s.getText() !== 'abcd') out.push('fallback redo ' + JSON.stringify(s.getText()));
          // Steps that do not match the text are rejected, not applied.
          if (EditorSurface.replaySteps('abc', [{ start: 0, deletedText: 'x', insertedText: '' }], 'bc') !== null) out.push('mismatched step applied');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_cancelled_or_stale_beforeinput_hint_is_not_reused() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const before = (s, inputType, range) => {
            const a = s.offsetToPoint(range[0]);
            const b = s.offsetToPoint(range[1]);
            const ev = new InputEvent('beforeinput', {
              inputType, bubbles: true, cancelable: true,
              targetRanges: [new StaticRange({ startContainer: a.node, startOffset: a.offset, endContainer: b.node, endOffset: b.offset })],
            });
            s.root.dispatchEvent(ev);
            return ev;
          };
          // A later listener cancels the beforeinput, so no input follows it.
          const cancel = (e) => e.preventDefault();
          // 1. Cancelled insertText with a target range at 3; the real edit
          // is an execCommand insertion at 1 (no beforeinput of its own).
          let r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, (s) => {
            s.root.addEventListener('beforeinput', cancel);
            const ev = before(s, 'insertText', [3, 3]);
            s.root.removeEventListener('beforeinput', cancel);
            if (!ev.defaultPrevented) out.push('beforeinput was not cancelled');
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('cancelled insert hint', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          // 2. Cancelled backward delete at [0, 1); the real delete is at [1, 2).
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, (s) => {
            s.root.addEventListener('beforeinput', cancel);
            before(s, 'deleteContentBackward', [0, 1]);
            s.root.removeEventListener('beforeinput', cancel);
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('cancelled delete hint', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // 3. A selectionchange after the beforeinput makes its hint stale.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, (s) => {
            before(s, 'insertText', [3, 3]);
            document.dispatchEvent(new Event('selectionchange'));
            if (s.pendingHint !== null) out.push('selectionchange kept the hint');
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('stale hint after selectionchange', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          // 4. A hint is consumed by its input: it cannot serve a second edit.
          r = teTest.editCase('aaaa', [], 1, 1, (s) => {
            before(s, 'insertText', [1, 1]);
            document.execCommand('insertText', false, 'a');
            if (s.pendingHint !== null) out.push('hint not consumed');
          });
          if (r.text !== 'aaaaa' || r.changes.length !== 1 || r.changes[0].start !== 1) out.push('consumed hint edit ' + JSON.stringify(r.changes));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_custom_dialog_is_removed_by_destroy() {
    let _document = fresh_full_editor();
    // Shoelace is not loaded on the test page, so `sl-dialog` is an undefined
    // element: the test drives its buttons and events directly.
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          const s = ed.surface;
          const parts = (d) => {
            const [apply, cancel] = d.querySelectorAll('sl-button');
            return { apply, cancel, prefix: d.querySelector('#prefix-input'), suffix: d.querySelector('#suffix-input') };
          };
          // Apply on a live editor wraps the selection and removes the dialog.
          s.setText('word');
          s.setSelectionOffsets(0, 4);
          const d1 = ed.showCustomDialog();
          if (!d1.isConnected || !ed.createdNodes.includes(d1)) out.push('dialog not shown/tracked');
          const p1 = parts(d1);
          p1.prefix.value = '~~';
          p1.suffix.value = '~~';
          p1.apply.click();
          if (s.getText() !== '~~word~~') out.push('apply ' + JSON.stringify(s.getText()));
          if (d1.isConnected || ed.createdNodes.includes(d1)) out.push('applied dialog left behind');
          // Normal hide (sl-after-hide) removes and untracks it.
          const d2 = ed.showCustomDialog();
          d2.dispatchEvent(new Event('sl-after-hide'));
          if (d2.isConnected || ed.createdNodes.includes(d2)) out.push('hidden dialog left behind');
          // Cancel closes without editing.
          const d3 = ed.showCustomDialog();
          parts(d3).cancel.click();
          if (d3.isConnected || s.getText() !== '~~word~~') out.push('cancel');
          // An open dialog does not survive destroy(), and its handlers detach.
          s.setText('word');
          s.setSelectionOffsets(0, 4);
          const d4 = ed.showCustomDialog();
          d4.open = true;
          const p4 = parts(d4);
          p4.prefix.value = '**';
          p4.suffix.value = '**';
          ed.destroy();
          if (d4.isConnected) out.push('open dialog survived destroy');
          if (d4.open !== false) out.push('open dialog not closed');
          if (document.querySelectorAll('sl-dialog:not(.shortcuts-dialog)').length !== 0) out.push('custom dialogs in DOM');
          const textBefore = s.getText();
          p4.apply.click();
          d4.dispatchEvent(new Event('sl-after-hide'));
          if (s.getText() !== textBefore) out.push('destroyed dialog handler ran: ' + JSON.stringify(s.getText()));
          if (ed.createdNodes.length !== 0) out.push('nodes still tracked');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

// ---------------------------------------------------------------------------
// Write_On mode toggle and corner chrome (issue #7)
// ---------------------------------------------------------------------------

/// The corner controls of R-7.2 (the counter is separate: always visible).
const CHROME_CONTROLS_JS: &str =
    "['alternatives', 'markdown', 'shortcuts', 'save', 'open', 'lab', 'overflow']";

#[wasm_bindgen_test]
fn test_plain_mode_shows_only_the_counter() {
    let _document = fresh_full_editor();
    let result = js_string(&format!(
        r##"(() => {{
          const out = [];
          const counter = document.querySelector('.te-chrome-counter');
          if (!teTest.visible(counter)) out.push('counter not visible');
          if (counter.getAttribute('aria-pressed') !== 'false') out.push('aria-pressed ' + counter.getAttribute('aria-pressed'));
          if (document.body.dataset.mode !== undefined) out.push('body mode ' + document.body.dataset.mode);
          for (const name of {CHROME_CONTROLS_JS}) {{
            const el = teTest.control(name);
            if (!el) out.push('missing control ' + name);
            else if (teTest.visible(el)) out.push('control visible in plain mode: ' + name);
          }}
          if (teTest.visible(document.querySelector('.te-chrome-shortcuts'))) out.push('reference visible');
          // Hidden corner controls must not act in plain mode, even when
          // clicked programmatically.
          const types = ['te:open-panel', 'te:markdown', 'te:shortcuts', 'te:save', 'te:open', 'te:lab', 'te:overflow'];
          const fired = [];
          const onAny = (e) => fired.push(e.type);
          for (const t of types) document.addEventListener(t, onAny);
          for (const name of {CHROME_CONTROLS_JS}) {{
            const el = teTest.control(name);
            if (el) el.click();
          }}
          for (const t of types) document.removeEventListener(t, onAny);
          if (fired.length) out.push('plain-mode clicks dispatched ' + fired.join(','));
          if (document.body.dataset.mode !== undefined) out.push('plain-mode click changed mode');
          if (document.querySelector('.te-chrome-shortcuts')?.open) out.push('plain-mode click opened reference');
          // The plain editor itself is unchanged.
          if (!teTest.visible(document.querySelector('.toolbar'))) out.push('toolbar hidden in plain mode');
          // Counter text follows the surface: whitespace-separated words,
          // Unicode code points for chars.
          const s = teTest.surface();
          s.setText('one two  three\nfour');
          const t1 = counter.textContent.trim();
          if (!t1.startsWith('4 words 19 chars')) out.push('count ' + JSON.stringify(t1));
          s.setText('a \u{{1F600}}');
          const t2 = counter.textContent.trim();
          if (!t2.startsWith('2 words 3 chars')) out.push('unicode count ' + JSON.stringify(t2));
          s.setText('');
          const t3 = counter.textContent.trim();
          if (!t3.startsWith('0 words 0 chars')) out.push('empty count ' + JSON.stringify(t3));
          // Native typing updates it too.
          s.focus();
          document.execCommand('insertText', false, 'hi');
          if (!counter.textContent.trim().startsWith('1 word 2 chars')) out.push('typed count ' + JSON.stringify(counter.textContent));
          return out.join('; ');
        }})()"##
    ));
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_counter_click_toggles_write_on_and_every_corner_control() {
    let _document = fresh_full_editor();
    let result = js_string(&format!(
        r##"(() => {{
          const out = [];
          const modes = [];
          const onMode = (e) => modes.push(e.detail.mode);
          document.addEventListener('te:mode-change', onMode);
          const counter = document.querySelector('.te-chrome-counter');
          counter.click();
          if (document.body.dataset.mode !== 'write-on') out.push('body mode ' + document.body.dataset.mode);
          if (counter.getAttribute('aria-pressed') !== 'true') out.push('aria-pressed not true');
          if (!teTest.chrome().isWriteOn()) out.push('chrome not in write-on');
          if (!teTest.visible(counter)) out.push('counter hidden in write-on');
          const want = {{
            alternatives: ['te-chrome-top-centre', '●●●'],
            markdown: ['te-chrome-top-centre', 'M↓'],
            shortcuts: ['te-chrome-top-right', ''],
            save: ['te-chrome-bottom-left', ''],
            open: ['te-chrome-bottom-left', ''],
            lab: ['te-chrome-bottom-centre', 'LAB'],
            overflow: ['te-chrome-bottom-right', 'XYZ'],
          }};
          for (const name of {CHROME_CONTROLS_JS}) {{
            const el = teTest.control(name);
            if (!teTest.visible(el)) {{ out.push('control not visible: ' + name); continue; }}
            if (!el.parentElement.classList.contains(want[name][0])) out.push(name + ' in ' + el.parentElement.className);
            if (el.textContent.trim() !== want[name][1]) out.push(name + ' text ' + JSON.stringify(el.textContent));
            if (!el.getAttribute('aria-label')) out.push(name + ' has no label');
          }}
          // Icon controls use FontAwesome classes, never emoji.
          if (!teTest.control('save').querySelector('i.fa-floppy-disk')) out.push('save icon');
          if (!teTest.control('open').querySelector('i.fa-folder-open')) out.push('open icon');
          if (!teTest.control('shortcuts').querySelector('i.fa-keyboard')) out.push('keyboard icon');
          // Corners are placed where R-7.2 says.
          const r = (n) => teTest.control(n).getBoundingClientRect();
          // Fixed positioning is relative to the viewport without scrollbars.
          const w = document.documentElement.clientWidth, h = document.documentElement.clientHeight;
          const mid = (a, b) => (a.left + b.right) / 2;
          if (!(r('alternatives').top < h / 2 && Math.abs(mid(r('alternatives'), r('markdown')) - w / 2) < 4)) out.push('top-centre placement');
          if (!(r('shortcuts').left > w / 2 && r('shortcuts').top < h / 2)) out.push('top-right placement');
          if (!(r('save').right < w / 2 && r('save').top > h / 2)) out.push('bottom-left placement');
          if (!(r('lab').top > h / 2 && Math.abs(mid(r('lab'), r('lab')) - w / 2) < 4)) out.push('bottom-centre placement');
          if (!(r('overflow').left > w / 2 && r('overflow').top > h / 2)) out.push('bottom-right placement');
          // R-7.1: no toolbar, no preview pane, centred dark column.
          if (teTest.visible(document.querySelector('.toolbar'))) out.push('toolbar visible in write-on');
          if (teTest.visible(document.querySelector('.markdown-preview'))) out.push('preview visible in write-on');
          if (getComputedStyle(document.body).backgroundColor !== 'rgb(10, 13, 28)') out.push('bg ' + getComputedStyle(document.body).backgroundColor);
          const sr = teTest.root().getBoundingClientRect();
          const gutter = document.documentElement.clientWidth;
          if (Math.abs(sr.left - (gutter - sr.right)) > 2) out.push('surface not centred ' + sr.left + ' ' + (gutter - sr.right));
          if (getComputedStyle(teTest.root()).maxWidth === 'none') out.push('surface has no measure');
          // Each control dispatches its documented event from the chrome.
          const seen = [];
          const types = ['te:open-panel', 'te:markdown', 'te:shortcuts', 'te:save', 'te:open', 'te:lab', 'te:overflow'];
          const rec = (e) => seen.push(e.type + (e.detail.panel ? ':' + e.detail.panel : '') + (e.detail.editor === window.__teEditor ? '' : '!editor'));
          types.forEach((t) => document.addEventListener(t, rec));
          for (const name of {CHROME_CONTROLS_JS}) teTest.control(name).click();
          document.querySelector('.te-chrome-shortcuts').close();
          types.forEach((t) => document.removeEventListener(t, rec));
          const wantSeen = 'te:open-panel:alternatives,te:markdown,te:shortcuts,te:save,te:open,te:lab,te:overflow';
          if (seen.join(',') !== wantSeen) out.push('events ' + seen.join(','));
          // Clicking the counter again returns to plain mode.
          counter.click();
          if (document.body.dataset.mode !== undefined) out.push('body mode after toggle back ' + document.body.dataset.mode);
          if (counter.getAttribute('aria-pressed') !== 'false') out.push('aria-pressed not false');
          for (const name of {CHROME_CONTROLS_JS}) if (teTest.visible(teTest.control(name))) out.push('still visible: ' + name);
          if (!teTest.visible(document.querySelector('.toolbar'))) out.push('toolbar not restored');
          if (!teTest.visible(document.querySelector('.markdown-preview'))) out.push('preview not restored');
          document.removeEventListener('te:mode-change', onMode);
          if (modes.join(',') !== 'write-on,plain') out.push('mode events ' + modes.join(','));
          return out.join('; ');
        }})()"##
    ));
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_write_on_mode_persists_per_document() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const reinit = (config) => {
            window.__teEditor.destroy();
            const ed = new MarkdownEditor(config || window.EditorConfig);
            ed.initialize();
            window.__teEditor = ed;
            return ed;
          };
          const input = () => document.querySelector('.markdown-input');
          const counter = () => document.querySelector('.te-chrome-counter');
          const docA = 'Document A\n\nsome words';
          const docB = 'Document B\n\nother words';
          input().textContent = docA;
          let ed = reinit();
          if (ed.chrome.isWriteOn()) out.push('A started in write-on');
          const keyA = ed.chrome.storageKey();
          if (!keyA.startsWith(window.WRITE_ON_STORAGE_PREFIX + 'h:')) out.push('hash key ' + keyA);
          counter().click();
          if (localStorage.getItem(keyA) !== '1') out.push('not stored ' + localStorage.getItem(keyA));
          // Same document after a re-init: Write_On is restored.
          input().textContent = docA;
          ed = reinit();
          if (!ed.chrome.isWriteOn()) out.push('A not restored after re-init');
          if (document.body.dataset.mode !== 'write-on') out.push('body mode not restored');
          if (ed.chrome.storageKey() !== keyA) out.push('A key changed');
          // Another document is not affected.
          input().textContent = docB;
          ed = reinit();
          if (ed.chrome.isWriteOn()) out.push('B inherited write-on');
          if (document.body.dataset.mode !== undefined) out.push('B body mode ' + document.body.dataset.mode);
          if (ed.chrome.storageKey() === keyA) out.push('B shares the A key');
          // Loading a different document re-keys via documentChanged().
          ed.surface.setText(docA);
          ed.chrome.documentChanged();
          if (!ed.chrome.isWriteOn()) out.push('documentChanged did not apply A');
          // Toggling A back to plain is stored too.
          counter().click();
          if (localStorage.getItem(keyA) !== '0') out.push('plain not stored');
          input().textContent = docA;
          ed = reinit();
          if (ed.chrome.isWriteOn()) out.push('A plain not restored');
          // A save key or file name takes precedence over the content hash.
          const named = Object.assign({}, window.EditorConfig, { documentKey: 'notes.md' });
          ed = reinit(named);
          if (ed.chrome.storageKey() !== window.WRITE_ON_STORAGE_PREFIX + 'notes.md') out.push('named key ' + ed.chrome.storageKey());
          counter().click();
          ed.surface.setText('completely different text');
          ed = reinit(named);
          if (!ed.chrome.isWriteOn()) out.push('named document not restored despite new text');
          ed.documentKey = 'saved-key';
          ed.chrome.documentChanged();
          if (ed.chrome.storageKey() !== window.WRITE_ON_STORAGE_PREFIX + 'saved-key') out.push('editor.documentKey precedence');
          if (ed.chrome.isWriteOn()) out.push('saved-key inherited write-on');
          // Leave a clean plain editor for later tests.
          teTest.resetWriteOn();
          reinit();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_shortcut_reference_lists_every_shortcut() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          document.querySelector('.te-chrome-counter').click();
          teTest.control('shortcuts').click();
          const dialog = document.querySelector('.te-chrome-shortcuts');
          if (!dialog.open) out.push('reference not open');
          if (!teTest.visible(dialog)) out.push('reference not visible');
          const rows = Array.from(dialog.querySelectorAll('.te-chrome-shortcut')).map((r) => ({
            desc: r.querySelector('dt').textContent.trim(),
            kbd: r.querySelector('kbd').textContent.trim().toLowerCase(),
          }));
          const has = (kbd, desc) => rows.some((r) => r.kbd === kbd.toLowerCase() && r.desc === desc);
          // Every shortcut from public/js/config.js.
          for (const s of window.EditorConfig.shortcuts) {
            if (!has(s.key, s.desc)) out.push('missing config shortcut ' + s.key + ' ' + s.desc);
          }
          // Every selection shortcut from R-7.3 (fixed expectations).
          const r73 = [
            ['Ctrl+Shift+A', 'Alternatives for selection'],
            ['Ctrl+Shift+G', 'AI alternatives for selection'],
            ['Ctrl+/', 'Ghost it / Revive'],
            ['Ctrl+Shift+X', 'Stash this in Overflow'],
          ];
          for (const [kbd, desc] of r73) {
            if (!has(kbd, desc)) out.push('missing R-7.3 shortcut ' + kbd);
            if (!Array.from(dialog.querySelectorAll('kbd')).some((k) => k.textContent.trim() === kbd)) out.push('not displayed as ' + kbd);
          }
          // Close button and leaving Write_On both close it.
          dialog.querySelector('.te-chrome-shortcuts-close').click();
          if (dialog.open) out.push('close button did not close');
          teTest.control('shortcuts').click();
          document.querySelector('.te-chrome-counter').click();
          if (dialog.open) out.push('reference open after leaving write-on');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_counts_adapter_falls_back_to_surface_text() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          // Without editor.counts() (issue #6 adds it, and tests it against
          // the real counts there) countsFor reads the surface text.
          if (typeof ed.counts === 'function') out.push('unexpected editor.counts');
          ed.surface.setText('three short words');
          const c = countsFor(ed);
          if (c.words !== 3 || c.chars !== 17) out.push('counts ' + JSON.stringify(c));
          const text = document.querySelector('.te-chrome-counter').textContent.trim();
          if (!text.startsWith('3 words 17 chars')) out.push('counter ' + JSON.stringify(text));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_destroy_removes_chrome_dom_and_listeners() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const a = window.__teEditor;
          const counter = document.querySelector('.te-chrome-counter');
          const lab = teTest.control('lab');
          counter.click();
          teTest.control('shortcuts').click();
          const dialog = document.querySelector('.te-chrome-shortcuts');
          a.destroy();
          a.destroy(); // idempotent
          if (!a.chrome.destroyed) out.push('chrome not marked destroyed');
          if (document.querySelectorAll('.te-chrome').length !== 0) out.push('chrome DOM left behind');
          if (dialog.open) out.push('reference left open');
          if (document.body.dataset.mode !== undefined) out.push('body mode left ' + document.body.dataset.mode);
          // Click listeners on the removed nodes were aborted with the editor.
          let events = 0;
          const count = () => { events += 1; };
          counter.addEventListener('te:mode-change', count);
          lab.addEventListener('te:lab', count);
          counter.click();
          lab.click();
          if (events !== 0) out.push('destroyed chrome still handles clicks: ' + events);
          if (document.body.dataset.mode !== undefined) out.push('destroyed counter toggled mode');
          // A new editor gets exactly one chrome.
          const b = new MarkdownEditor(window.EditorConfig);
          b.initialize();
          window.__teEditor = b;
          if (document.querySelectorAll('.te-chrome').length !== 1) out.push('chrome count ' + document.querySelectorAll('.te-chrome').length);
          if (document.querySelectorAll('.te-chrome-shortcuts').length !== 1) out.push('reference count');
          // chrome.destroy() itself drops its change subscription.
          const n = b.surface.changeListeners.size;
          const extra = new WriteOnChrome(b, { signal: b.abortController.signal });
          if (b.surface.changeListeners.size !== n + 1) out.push('chrome did not subscribe');
          extra.destroy();
          if (b.surface.changeListeners.size !== n) out.push('chrome left its change listener');
          if (document.querySelectorAll('.te-chrome').length !== 1) out.push('extra chrome DOM left');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

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

const KEYSTROKES: u32 = 30;
// Kept small so the whole suite stays well inside the runner's 20 s budget.
const ROUNDS: u32 = 3;

fn time_keystrokes() -> f64 {
    js_number(&format!(
        "(() => {{ const t0 = performance.now(); for (let i = 0; i < {KEYSTROKES}; i++) document.execCommand('insertText', false, 'a'); return performance.now() - t0; }})()"
    ))
}

#[wasm_bindgen_test]
fn bench_typing_latency_textarea_vs_surface() {
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

    let run_textarea = || {
        textarea.focus().unwrap();
        textarea
            .set_selection_range(middle as u32, middle as u32)
            .unwrap();
        time_keystrokes()
    };
    // `delay` 0 reproduces the previous synchronous preview; anything else
    // is the debounced preview.
    let run_surface = |bypass_sync: bool, delay: u32| {
        set_preview_delay(delay);
        js_eval(&format!(
            "(() => {{ const s = teTest.surface(); s.focus(); s.setSelectionOffsets({middle}); s.programmatic = {bypass_sync}; }})()"
        ));
        let t = time_keystrokes();
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

    let renders_before = js_number("teTest.surface().renderCount");

    // Warm up every path, then alternate measured rounds and take medians.
    let renders_rust_before = preview_render_count();
    run_textarea();
    run_surface(false, 0);
    run_surface(true, 0);
    run_surface(false, DEFAULT_PREVIEW_DELAY_MS);
    run_conversion_only();
    let mut textarea_runs = Vec::new();
    let mut surface_runs = Vec::new();
    let mut native_runs = Vec::new();
    let mut debounced_runs = Vec::new();
    let mut flush_runs = Vec::new();
    let mut conversion_runs = Vec::new();
    for _ in 0..ROUNDS {
        textarea_runs.push(run_textarea());
        surface_runs.push(run_surface(false, 0).0);
        native_runs.push(run_surface(true, 0).0);
        let (debounced, flush_ms) = run_surface(false, DEFAULT_PREVIEW_DELAY_MS);
        debounced_runs.push(debounced);
        flush_runs.push(flush_ms);
        conversion_runs.push(run_conversion_only());
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
    let typed = rounds * KEYSTROKES as usize;
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

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    values[values.len() / 2]
}

/// Encode a Rust string as a JavaScript string literal.
fn js_string_literal(s: &str) -> String {
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
