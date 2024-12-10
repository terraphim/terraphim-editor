use wasm_bindgen_test::*;
use wasm_bindgen::JsCast;
use web_sys::Performance;

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
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    
    // Create test container
    let app = document.create_element("div").unwrap();
    app.set_id("app");
    document.body().unwrap().append_child(&app).unwrap();
    
    // Initialize the editor
    terraphim_editor::run().expect("Editor should initialize");
    
    // Check if editor elements are present
    let textarea = document.query_selector(".markdown-input").unwrap();
    assert!(textarea.is_some(), "Textarea should be present");
    
    let preview = document.query_selector(".markdown-preview").unwrap();
    assert!(preview.is_some(), "Preview div should be present");
}

#[wasm_bindgen_test]
fn test_markdown_conversion() {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    
    // Initialize editor
    let app = document.create_element("div").unwrap();
    app.set_id("app");
    document.body().unwrap().append_child(&app).unwrap();
    terraphim_editor::run().expect("Editor should initialize");
    
    // Get editor elements
    let textarea = document.query_selector(".markdown-input").unwrap()
        .expect("Textarea should be present")
        .dyn_into::<web_sys::HtmlTextAreaElement>()
        .unwrap();
    
    let preview = document.query_selector(".markdown-preview").unwrap()
        .expect("Preview div should be present");
    
    // Test markdown conversion
    let test_input = "# Test Heading";
    textarea.set_value(test_input);
    
    // Trigger input event
    let event = web_sys::InputEvent::new("input").unwrap();
    textarea.dispatch_event(&event).unwrap();
    
    // Check preview content
    let preview_html = preview.inner_html();
    assert!(preview_html.contains("<h1>Test Heading</h1>"), 
           "Preview should contain converted markdown");
}

#[wasm_bindgen_test]
fn bench_markdown_conversion_in_browser() {
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let performance = window.performance().expect("performance should be available");
    
    // Initialize editor
    let app = document.create_element("div").unwrap();
    app.set_id("app");
    document.body().unwrap().append_child(&app).unwrap();
    terraphim_editor::run().expect("Editor should initialize");
    
    // Get editor elements
    let textarea = document.query_selector(".markdown-input").unwrap()
        .expect("Textarea should be present")
        .dyn_into::<web_sys::HtmlTextAreaElement>()
        .unwrap();
    
    let preview = document.query_selector(".markdown-preview").unwrap()
        .expect("Preview div should be present");
    
    // Measure performance
    let start = performance.now();
    
    // Run conversion multiple times
    for _ in 0..100 {
        textarea.set_value(BENCHMARK_TEXT);
        let event = web_sys::InputEvent::new("input").unwrap();
        textarea.dispatch_event(&event).unwrap();
    }
    
    let end = performance.now();
    let avg_time = (end - start) / 100.0;
    
    // Log performance results
    web_sys::console::log_1(&format!("Average conversion time: {}ms", avg_time).into());
    
    // Basic assertion to ensure reasonable performance
    assert!(avg_time < 50.0, "Conversion took too long: {}ms", avg_time);
} 