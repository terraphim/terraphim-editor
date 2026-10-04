use markdown::{to_html_with_options, Options};
use rinja::Template;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, Event, Window};

const INITIAL_MARKDOWN: &str = r#"# Welcome to Markdown Editor!

This is a simple markdown editor built with:
- Rust
- WebAssembly
- Shoelace components

## Try it out
1. Edit this text on the left
2. See the preview on the right
3. Use **bold**, *italic*, or `code` formatting

---

> Made with ❤️ using Rust and WASM
"#;

#[derive(Template)]
#[template(path = "editor.html")]
struct EditorTemplate {
    initial_content: String,
    initial_preview: String,
}

#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window: Window = web_sys::window().ok_or_else(|| JsValue::from_str("No window found"))?;
    let document: Document = window
        .document()
        .ok_or_else(|| JsValue::from_str("No document found"))?;
    let app: Element = document
        .get_element_by_id("app")
        .ok_or_else(|| JsValue::from_str("No element with id 'app' found"))?;

    let initial_preview = render_markdown(INITIAL_MARKDOWN)?;

    let template = EditorTemplate {
        initial_content: INITIAL_MARKDOWN.to_string(),
        initial_preview,
    };

    app.set_inner_html(
        &template
            .render()
            .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))?,
    );

    setup_markdown_conversion(&document)?;

    Ok(())
}

/// Convert Markdown to HTML with the `markdown` crate.
///
/// Exported so that JavaScript (and the browser benchmarks) use exactly the
/// same conversion as the live preview.
#[wasm_bindgen]
pub fn render_markdown(input: &str) -> Result<String, JsValue> {
    to_html_with_options(input, &Options::default())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert markdown: {}", e)))
}

/// Wire the editing surface's `input` event to the Rust Markdown preview.
///
/// The surface is a `contenteditable` element driven by `EditorSurface` in
/// `public/js/editor.js`. That controller normalises the DOM in a
/// capture-phase `input` listener, so by the time this (target-phase)
/// listener runs the element only contains text nodes, decoration spans and
/// an optional trailing placeholder `<br>`, with newlines stored as literal
/// `\n` characters. Therefore `textContent` is exactly the plain-text
/// document, and that is what is converted here.
fn setup_markdown_conversion(document: &Document) -> Result<(), JsValue> {
    let surface = document
        .query_selector(".markdown-input")?
        .ok_or_else(|| JsValue::from_str("No editing surface found"))?;
    let preview = document
        .query_selector(".markdown-preview")?
        .ok_or_else(|| JsValue::from_str("No preview div found"))?;

    let source = surface.clone();
    let handler = Closure::wrap(Box::new(move |_event: Event| {
        let input = source.text_content().unwrap_or_default();
        match render_markdown(&input) {
            Ok(html) => preview.set_inner_html(&html),
            Err(err) => web_sys::console::error_1(&err),
        }
    }) as Box<dyn FnMut(_)>);

    surface.add_event_listener_with_callback("input", handler.as_ref().unchecked_ref())?;
    handler.forget();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_conversion() {
        let input = "# Hello\n\nThis is a test";
        let result = to_html_with_options(input, &Options::default());
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<p>This is a test</p>"));
    }

    #[test]
    fn test_render_markdown_matches_crate_output() {
        let input = "Para one\n\nPara two with **bold**";
        let html = render_markdown(input).expect("conversion should succeed");
        assert_eq!(
            html,
            to_html_with_options(input, &Options::default()).unwrap()
        );
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn test_template_rendering() {
        let template = EditorTemplate {
            initial_content: "# Test".to_string(),
            initial_preview: "<h1>Test</h1>".to_string(),
        };
        let result = template.render();
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("# Test"));
        assert!(html.contains("<h1>Test</h1>"));
    }

    #[test]
    fn test_template_renders_contenteditable_surface() {
        let template = EditorTemplate {
            initial_content: "a <b>not markup</b>\nline two".to_string(),
            initial_preview: String::new(),
        };
        let html = template.render().unwrap();
        assert!(!html.contains("<textarea"), "textarea must be gone");
        assert!(html.contains("contenteditable=\"plaintext-only\""));
        assert!(html.contains("role=\"textbox\""));
        // Initial content is escaped so it lands in the surface as plain text.
        assert!(html.contains("not markup"));
        assert!(!html.contains("<b>not markup</b>"));
    }
}
