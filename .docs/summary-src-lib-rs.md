# File Summary: src/lib.rs

## File Path
`src/lib.rs`

## Purpose
Core Rust WebAssembly library that implements the terraphim-editor's markdown rendering engine and editor initialization logic. This is the main entry point for the WASM module that bridges Rust functionality with JavaScript/web environment.

## Key Components

### Entry Point Function
- **`run()`** - WASM-bindgen start function that auto-executes on WASM initialization
  - Configures panic hook for better error reporting in browser console
  - Locates the `#editor-container` DOM element
  - Renders the initial editor template using Rinja
  - Sets up markdown conversion event handlers
  - Returns `Result<(), JsValue>` for error handling

### Template System
- **`EditorTemplate`** struct - Rinja template data structure
  - `initial_content: String` - Pre-populated markdown content
  - `initial_preview: String` - Pre-rendered HTML preview
  - Uses `templates/editor.html` as the template source

### Markdown Conversion
- **`setup_markdown_conversion()`** - Configures live preview functionality
  - Attaches input event listener to `.markdown-input` textarea
  - Converts markdown to HTML on every keystroke
  - Updates `.markdown-preview` div with rendered content
  - Uses closure with `forget()` for persistent event handling

- **`render_markdown()`** - Public WASM-exposed function
  - Takes markdown string as input
  - Returns rendered HTML or error
  - Can be called directly from JavaScript

### Constants
- **`INITIAL_MARKDOWN`** - Welcome text demonstrating editor features
  - Showcases various markdown elements (headings, lists, quotes, code blocks)
  - Serves as user onboarding content

## Dependencies
- `wasm-bindgen` - Core FFI bindings between Rust and JavaScript
- `web-sys` - Web API bindings (DOM manipulation, events)
  - Document, Element, HtmlElement, HtmlTextAreaElement, HtmlDivElement
  - InputEvent, Node, Window, Text, Performance
- `console_error_panic_hook` - Better panic messages in browser console
- `markdown` crate (1.0.0-alpha.21) - Markdown to HTML conversion with Options API
- `rinja` (0.3.5) - Template rendering engine

## Integration Points

### JavaScript Integration
- Exports `render_markdown()` function for direct JS calls
- Auto-runs `run()` function via `#[wasm_bindgen(start)]` attribute
- Expects `#editor-container` element to exist in DOM before initialization

### Template Integration
- Requires `templates/editor.html` file for UI structure
- Template receives initial content and preview data
- Shoelace web components rendered via template

### Event Flow
1. WASM module loads → `run()` executes automatically
2. Editor template rendered into `#editor-container`
3. Event listener attached to textarea
4. User types → Input event fires → Markdown converted → Preview updated

## Notable Patterns

### Error Handling
- Consistent use of `Result<T, JsValue>` for WASM-compatible errors
- `.ok_or_else()` with descriptive error messages for Option unwrapping
- `.map_err()` to convert Rust errors to JsValue strings

### Memory Management
- Event handler closure uses `.forget()` to prevent deallocation
- Necessary for persistent event listeners in WASM context
- Clone of preview element to move into closure

### Performance Considerations
- Live markdown conversion on every input event (no debouncing)
- Synchronous rendering (blocking)
- Direct DOM manipulation via web-sys

## Testing
- Unit tests included for markdown conversion logic
- Template rendering validation
- Tests verify HTML output contains expected tags

## Technical Debt & Improvement Opportunities
- No input sanitization before setting innerHTML (XSS risk)
- No debouncing for markdown conversion (performance concern for large documents)
- Handler closure leaked with `forget()` - no cleanup mechanism
- No error recovery UI when conversion fails
- Hard-coded selector strings could be constants
