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
use wasm_bindgen_test::*;
use web_sys::{Document, HtmlElement, HtmlTextAreaElement};

wasm_bindgen_test_configure!(run_in_browser);

const CONFIG_JS: &str = include_str!("../public/js/config.js");
const EDITOR_JS: &str = include_str!("../public/js/editor.js");

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
  preview() { return document.querySelector('.markdown-preview').innerHTML; },
  menu() { return window.__teEditor.commandMenu; },
  canonical() {
    const s = window.__teEditor.surface;
    return s.isCanonical().ok && s.root.textContent === s.getText();
  },
};
"##;

#[wasm_bindgen(inline_js = "export function js_eval(src) { return (0, eval)(src); }")]
extern "C" {
    fn js_eval(src: &str) -> JsValue;
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
        .query_selector("#app, .command-menu, .te-bench")
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
    for source in [CONFIG_JS, EDITOR_JS, TEST_HELPERS_JS] {
        let script = document.create_element("script").unwrap();
        script.set_attribute("data-te-test", "").unwrap();
        script.set_text_content(Some(source));
        document.body().unwrap().append_child(&script).unwrap();
    }
}

/// Fresh Rust editor plus the real JavaScript controller.
fn fresh_full_editor() -> Document {
    let document = fresh_rust_editor();
    load_editor_scripts(&document);
    let ok = js_string(
        r##"(() => {
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
    }
    let avg_time = (performance.now() - start) / 100.0;

    web_sys::console::log_1(&format!("Average conversion time: {}ms", avg_time).into());
    assert!(avg_time < 50.0, "Conversion took too long: {}ms", avg_time);
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
          if (!teTest.preview().includes('one')) out.push('preview not updated');
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
const ROUNDS: u32 = 5;

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
    let run_surface = |bypass_sync: bool| {
        js_eval(&format!(
            "(() => {{ const s = teTest.surface(); s.focus(); s.setSelectionOffsets({middle}); s.programmatic = {bypass_sync}; }})()"
        ));
        let t = time_keystrokes();
        // Resynchronise the model after a bypassed round.
        js_eval(
            "(() => { const s = teTest.surface(); s.programmatic = false; s.sync('typing'); })()",
        );
        t
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
    run_textarea();
    run_surface(false);
    run_surface(true);
    run_conversion_only();
    let mut textarea_runs = Vec::new();
    let mut surface_runs = Vec::new();
    let mut native_runs = Vec::new();
    let mut conversion_runs = Vec::new();
    for _ in 0..ROUNDS {
        textarea_runs.push(run_textarea());
        surface_runs.push(run_surface(false));
        native_runs.push(run_surface(true));
        conversion_runs.push(run_conversion_only());
    }
    let per_key = |runs: &mut Vec<f64>| median(runs) / f64::from(KEYSTROKES);
    let per_textarea = per_key(&mut textarea_runs);
    let per_surface = per_key(&mut surface_runs);
    let per_native_only = per_key(&mut native_runs);
    let per_conversion = per_key(&mut conversion_runs);

    // Every path really received every keystroke (one warm-up round plus
    // ROUNDS measured rounds; the surface ran two paths).
    let rounds = (ROUNDS + 1) as usize;
    let typed = rounds * KEYSTROKES as usize;
    assert_eq!(textarea.value().len(), doc_text.len() + typed);
    let surface_len = js_number("teTest.surface().getText().length") as usize;
    assert_eq!(surface_len, doc_text.encode_utf16().count() + 2 * typed);
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
             textarea {per_textarea:.3}; contenteditable surface {per_surface:.3}; \
             surface with EditorSurface sync bypassed {per_native_only:.3}; \
             conversion + preview update alone {per_conversion:.3}; surface/textarea ratio {:.3}",
            doc_text.len(),
            per_surface / per_textarea
        )
        .into(),
    );
    assert!(word_count >= 5000, "document has {word_count} words");
    // Headless timing is noisy; allow a margin rather than a strict <=.
    assert!(
        per_surface <= per_textarea * 1.5 + 1.0,
        "surface {per_surface:.3} ms vs textarea {per_textarea:.3} ms per keystroke"
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
