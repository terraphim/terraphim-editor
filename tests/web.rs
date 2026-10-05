//! Browser tests for the editor: initialisation, Markdown conversion, the
//! debounced preview and the controller lifecycle. Run with
//! `wasm-pack test --headless --chrome`.
//!
//! These tests drive the real Rust entry point and the real JavaScript
//! controller (`public/js/config.js` and `public/js/editor.js`, injected as
//! classic scripts exactly as Trunk inlines them) in a real browser DOM.
//! Interaction is simulated with real DOM events and native editing commands
//! (`document.execCommand`); nothing is mocked.
//!
//! The browser tests are split across several test binaries
//! (`tests/web*.rs`, sharing `tests/support`), because
//! wasm-bindgen-test-runner gives each binary one fixed wall-clock budget
//! (about 20 s) to print its result; a binary that overruns it on a loaded
//! host fails with "Failed to detect test as having been run". Keep each
//! binary's harness `finished in` well inside it.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

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
