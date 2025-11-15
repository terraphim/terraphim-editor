use markdown::{to_html_with_options, Options};
use rinja::Template;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, HtmlDivElement, HtmlTextAreaElement, InputEvent, Window};

const INITIAL_MARKDOWN: &str = r#"# Welcome to Markdown Editor!

This is a simple markdown editor built with:
- Rust
- WebAssembly
- Multiple UI frameworks

## Try it out
1. Edit this text on the left
2. See the preview on the right
3. Use **bold**, *italic*, or `code` formatting

---

> Made with ❤️ using Rust and WASM
"#;

// Shoelace template (default)
#[derive(Template)]
#[template(path = "editor.html")]
struct EditorTemplate {
    initial_content: String,
    initial_preview: String,
}

// Vanilla HTML/CSS template
#[derive(Template)]
#[template(path = "editor-vanilla.html")]
struct EditorTemplateVanilla {
    initial_content: String,
    initial_preview: String,
}

// Web Awesome template
#[derive(Template)]
#[template(path = "editor-webawesome.html")]
struct EditorTemplateWebAwesome {
    initial_content: String,
    initial_preview: String,
}

#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EditorStyle {
    #[default]
    Shoelace,
    Vanilla,
    WebAwesome,
}

#[wasm_bindgen(start)]
pub fn run() -> Result<(), JsValue> {
    run_with_style(EditorStyle::Shoelace)
}

#[wasm_bindgen]
pub fn run_with_style(style: EditorStyle) -> Result<(), JsValue> {
    console_error_panic_hook::set_once();

    let window: Window = web_sys::window().ok_or_else(|| JsValue::from_str("No window found"))?;
    let document: Document = window
        .document()
        .ok_or_else(|| JsValue::from_str("No document found"))?;
    let app: Element = document
        .get_element_by_id("editor-container")
        .ok_or_else(|| JsValue::from_str("No element with id 'editor-container' found"))?;

    let initial_preview = to_html_with_options(INITIAL_MARKDOWN, &Options::default())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert markdown: {}", e)))?;

    let html = match style {
        EditorStyle::Shoelace => {
            let template = EditorTemplate {
                initial_content: INITIAL_MARKDOWN.to_string(),
                initial_preview,
            };
            template
                .render()
                .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))?
        }
        EditorStyle::Vanilla => {
            let template = EditorTemplateVanilla {
                initial_content: INITIAL_MARKDOWN.to_string(),
                initial_preview,
            };
            template
                .render()
                .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))?
        }
        EditorStyle::WebAwesome => {
            let template = EditorTemplateWebAwesome {
                initial_content: INITIAL_MARKDOWN.to_string(),
                initial_preview,
            };
            template
                .render()
                .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))?
        }
    };

    app.set_inner_html(&html);
    setup_markdown_conversion(&document)?;

    Ok(())
}

#[wasm_bindgen]
pub fn render_editor_html(style: EditorStyle, content: &str) -> Result<String, JsValue> {
    let initial_preview = to_html_with_options(content, &Options::default())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert markdown: {}", e)))?;

    match style {
        EditorStyle::Shoelace => {
            let template = EditorTemplate {
                initial_content: content.to_string(),
                initial_preview,
            };
            template
                .render()
                .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))
        }
        EditorStyle::Vanilla => {
            let template = EditorTemplateVanilla {
                initial_content: content.to_string(),
                initial_preview,
            };
            template
                .render()
                .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))
        }
        EditorStyle::WebAwesome => {
            let template = EditorTemplateWebAwesome {
                initial_content: content.to_string(),
                initial_preview,
            };
            template
                .render()
                .map_err(|e| JsValue::from_str(&format!("Failed to render template: {}", e)))
        }
    }
}

fn setup_markdown_conversion(document: &Document) -> Result<(), JsValue> {
    let textarea = document
        .query_selector(".markdown-input")?
        .ok_or_else(|| JsValue::from_str("No textarea found"))?;
    let preview = document
        .query_selector(".markdown-preview")?
        .ok_or_else(|| JsValue::from_str("No preview div found"))?;

    let preview_clone = preview.clone();
    let handler = Closure::wrap(Box::new(move |event: InputEvent| {
        let input = event
            .target()
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

#[wasm_bindgen]
pub fn render_markdown(input: &str) -> Result<String, JsValue> {
    to_html_with_options(input, &Options::default())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert markdown: {}", e)))
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
