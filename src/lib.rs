use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, Window, HtmlTextAreaElement, HtmlDivElement, InputEvent};
use markdown::{to_html_with_options, Options};
use rinja::Template;

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
    
    let window: Window = web_sys::window()
        .ok_or_else(|| JsValue::from_str("No window found"))?;
    let document: Document = window.document()
        .ok_or_else(|| JsValue::from_str("No document found"))?;
    let app: Element = document.get_element_by_id("editor-container")
        .ok_or_else(|| JsValue::from_str("No element with id 'app' found"))?;

    let initial_preview = to_html_with_options(INITIAL_MARKDOWN, &Options::default())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert markdown: {}", e)))?;

    let template = EditorTemplate {
        initial_content: INITIAL_MARKDOWN.to_string(),
        initial_preview,
    };

    app.set_inner_html(&template.render()
        .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))?);
    
    setup_markdown_conversion(&document)?;

    Ok(())
}

fn setup_markdown_conversion(document: &Document) -> Result<(), JsValue> {
    let textarea = document.query_selector(".markdown-input")?
        .ok_or_else(|| JsValue::from_str("No textarea found"))?;
    let preview = document.query_selector(".markdown-preview")?
        .ok_or_else(|| JsValue::from_str("No preview div found"))?;

    let preview_clone = preview.clone();
    let handler = Closure::wrap(Box::new(move |event: InputEvent| {
        let input = event.target()
            .and_then(|t| t.dyn_into::<HtmlTextAreaElement>().ok())
            .map(|t| t.value())
            .expect("Could not get textarea value");
            
        let html = to_html_with_options(&input, &Options::default())
            .expect("Failed to convert markdown to HTML");
            
        preview_clone
            .dyn_ref::<HtmlDivElement>()
            .expect("Preview div not found")
            .set_inner_html(&html);
    }) as Box<dyn FnMut(_)>);

    textarea.add_event_listener_with_callback("input", handler.as_ref().unchecked_ref())?;
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
} 