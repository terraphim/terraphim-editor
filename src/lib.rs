use markdown::{to_html_with_options, Options};
use rinja::Template;
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, Event, Window};

mod document;
pub use document::{
    apply_edit, counts_json, document_annotations, document_body, document_counts, export_document,
    open_document, save_document, sync_document_body, with_session, DocumentSession, Opened,
    Synced,
};

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

    // The welcome text is the open document until the user opens a file, so
    // edits to it are tracked by the span model like any other document.
    with_session(|session| session.open(INITIAL_MARKDOWN));

    // The initial preview is rendered immediately, as part of the template.
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

/// Default trailing debounce, in milliseconds, between the last `input`
/// event and the preview render.
pub const DEFAULT_PREVIEW_DELAY_MS: u32 = 120;

/// Live preview state for the editor rendered by the most recent [`run`].
struct PreviewState {
    surface: Element,
    preview: Element,
    /// Handle of the scheduled render, if one is pending.
    pending: Option<i32>,
    /// Single persistent timer callback, reused for every scheduled render
    /// so that typing does not allocate (or leak) a closure per keystroke.
    timer: Closure<dyn FnMut()>,
    delay_ms: u32,
    render_count: u32,
}

thread_local! {
    static PREVIEW: RefCell<Option<PreviewState>> = const { RefCell::new(None) };
}

/// Wire the editing surface's `input` event to the Rust Markdown preview.
///
/// The surface is a `contenteditable` element driven by `EditorSurface` in
/// `public/js/editor.js`. That controller normalises the DOM in a
/// capture-phase `input` listener, so by the time this (target-phase)
/// listener runs the element only contains text nodes, decoration spans and
/// an optional trailing placeholder `<br>`, with newlines stored as literal
/// `\n` characters. Therefore `textContent` is exactly the plain-text
/// document, and that is what is converted.
///
/// Conversion is debounced: each `input` cancels any pending render and
/// schedules a new one [`preview_delay`] milliseconds later (trailing edge).
/// `textContent` is read when the render fires, so the preview always shows
/// the latest text. A delay of `0` renders synchronously inside the handler.
fn setup_markdown_conversion(document: &Document) -> Result<(), JsValue> {
    let surface = document
        .query_selector(".markdown-input")?
        .ok_or_else(|| JsValue::from_str("No editing surface found"))?;
    let preview = document
        .query_selector(".markdown-preview")?
        .ok_or_else(|| JsValue::from_str("No preview div found"))?;

    // Replace the state of any editor from a previous `run()`, cancelling its
    // pending render so a stale timer can never fire against removed nodes.
    cancel_pending();
    let timer = Closure::wrap(Box::new(render_preview_now) as Box<dyn FnMut()>);
    PREVIEW.with(|cell| {
        *cell.borrow_mut() = Some(PreviewState {
            surface: surface.clone(),
            preview,
            pending: None,
            timer,
            delay_ms: DEFAULT_PREVIEW_DELAY_MS,
            render_count: 0,
        });
    });

    // The listener captures nothing; it works on the global state, which
    // always describes the current editor.
    let handler = Closure::wrap(Box::new(|_event: Event| schedule_preview()) as Box<dyn FnMut(_)>);
    surface.add_event_listener_with_callback("input", handler.as_ref().unchecked_ref())?;
    handler.forget();

    Ok(())
}

/// Cancel any pending render, returning whether one was pending.
fn cancel_pending() -> bool {
    let handle = PREVIEW.with(|cell| cell.borrow_mut().as_mut().and_then(|s| s.pending.take()));
    match handle {
        Some(handle) => {
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(handle);
            }
            true
        }
        None => false,
    }
}

/// Debounce entry point, called on every `input` event.
fn schedule_preview() {
    cancel_pending();
    let delay = preview_delay();
    if delay == 0 {
        render_preview_now();
        return;
    }
    let Some(window) = web_sys::window() else {
        return;
    };
    PREVIEW.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            match window.set_timeout_with_callback_and_timeout_and_arguments_0(
                state.timer.as_ref().unchecked_ref(),
                i32::try_from(delay).unwrap_or(i32::MAX),
            ) {
                Ok(handle) => state.pending = Some(handle),
                Err(err) => web_sys::console::error_1(&err),
            }
        }
    });
}

/// Convert the surface text and update the preview now.
///
/// The `RefCell` borrow is released before touching the DOM.
fn render_preview_now() {
    let elements = PREVIEW.with(|cell| {
        cell.borrow_mut().as_mut().map(|state| {
            state.pending = None;
            state.render_count = state.render_count.wrapping_add(1);
            (state.surface.clone(), state.preview.clone())
        })
    });
    let Some((surface, preview)) = elements else {
        return;
    };
    let input = surface.text_content().unwrap_or_default();
    match render_markdown(&input) {
        Ok(html) => preview.set_inner_html(&html),
        Err(err) => web_sys::console::error_1(&err),
    }
}

/// Render a pending (debounced) preview immediately.
///
/// Call this before reading the preview for save, export or tests. Returns
/// `true` if a render was pending and has now run, `false` if the preview was
/// already up to date (or no editor is initialised). From JavaScript in the
/// Trunk build this is `window.wasmBindings.flush_preview()`.
#[wasm_bindgen]
pub fn flush_preview() -> bool {
    if cancel_pending() {
        render_preview_now();
        true
    } else {
        false
    }
}

/// Set the preview debounce delay in milliseconds; `0` renders synchronously
/// on every `input`. Applies to renders scheduled after the call; a render
/// already pending keeps its original deadline. [`run`] resets the delay to
/// [`DEFAULT_PREVIEW_DELAY_MS`]. Does nothing if no editor is initialised.
#[wasm_bindgen]
pub fn set_preview_delay(ms: u32) {
    PREVIEW.with(|cell| {
        if let Some(state) = cell.borrow_mut().as_mut() {
            state.delay_ms = ms;
        }
    });
}

/// Current preview debounce delay in milliseconds.
#[wasm_bindgen]
pub fn preview_delay() -> u32 {
    PREVIEW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(DEFAULT_PREVIEW_DELAY_MS, |s| s.delay_ms)
    })
}

/// Whether a debounced preview render is scheduled but has not yet run.
#[wasm_bindgen]
pub fn preview_pending() -> bool {
    PREVIEW.with(|cell| cell.borrow().as_ref().is_some_and(|s| s.pending.is_some()))
}

/// Number of preview renders triggered by edits since the last [`run`].
/// The initial render on load is part of the template and is not counted.
/// Intended for diagnostics and tests.
#[wasm_bindgen]
pub fn preview_render_count() -> u32 {
    PREVIEW.with(|cell| cell.borrow().as_ref().map_or(0, |s| s.render_count))
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
    fn test_preview_api_without_editor_is_inert() {
        // No editor initialised natively: the API must be safe no-ops.
        assert!(!flush_preview());
        assert!(!preview_pending());
        assert_eq!(preview_render_count(), 0);
        set_preview_delay(5);
        assert_eq!(preview_delay(), DEFAULT_PREVIEW_DELAY_MS);
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
