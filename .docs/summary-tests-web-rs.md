# File Summary: tests/web.rs

## File Path
`tests/web.rs`

## Purpose
Browser-based integration tests for the WebAssembly editor using `wasm-bindgen-test`. Tests run in a real browser environment to validate DOM manipulation, event handling, and markdown conversion functionality.

## Key Components

### Test Configuration
- **`wasm_bindgen_test_configure!(run_in_browser)`** - Configures tests to execute in browser context
  - Enables access to full web APIs
  - Allows DOM manipulation testing
  - Requires headless browser or actual browser for test execution

### Test Constants
- **`BENCHMARK_TEXT`** - Comprehensive markdown sample covering multiple syntax elements
  - Headings (H1, H2, H3)
  - Text formatting (bold, italic)
  - Lists (unordered, nested, ordered)
  - Blockquotes
  - Code blocks with syntax highlighting
  - Links
  - Tables
  - Used for both functional tests and performance benchmarking

## Test Cases

### 1. Editor Initialization Test (`test_editor_initialization`)
**Purpose**: Validates that the editor properly initializes and renders required DOM elements

**Test Steps**:
1. Gets window and document objects
2. Creates test container div with id `editor-container`
3. Appends container to document body
4. Calls `terraphim_editor::run()` to initialize editor
5. Queries for `.markdown-input` textarea
6. Queries for `.markdown-preview` div
7. Asserts both elements exist

**What It Validates**:
- WASM module can initialize without errors
- Template rendering creates expected DOM structure
- Textarea and preview elements have correct CSS classes

### 2. Markdown Conversion Test (`test_markdown_conversion`)
**Purpose**: Validates live markdown-to-HTML conversion functionality

**Test Steps**:
1. Initializes editor in test container
2. Gets textarea and preview elements
3. Sets test markdown input: `"# Test Heading"`
4. Dispatches synthetic `InputEvent` to textarea
5. Reads preview div's inner HTML
6. Asserts preview contains `<h1>Test Heading</h1>`

**What It Validates**:
- Event listener properly attached to textarea
- Input events trigger markdown conversion
- Conversion produces correct HTML output
- Preview div updates with converted content

### 3. Browser Performance Benchmark (`bench_markdown_conversion_in_browser`)
**Purpose**: Measures real-world markdown conversion performance in browser environment

**Test Steps**:
1. Initializes editor
2. Gets Performance API for timing
3. Records start time
4. Runs 100 iterations of:
   - Set benchmark text in textarea
   - Dispatch input event
   - Trigger markdown conversion
5. Records end time
6. Calculates average conversion time
7. Logs performance results to console
8. Asserts average time < 50ms

**What It Validates**:
- Conversion performance meets acceptable thresholds
- WASM rendering speed in browser context
- No memory leaks or performance degradation over iterations
- Realistic workload testing (complex markdown document)

## Dependencies
- `wasm-bindgen-test` - Test framework for WASM browser tests
- `wasm-bindgen` - FFI bindings for test utilities
- `web-sys` - Browser API bindings
  - Performance API for benchmarking
  - DOM manipulation APIs
  - Event system

## Integration Points

### Test Execution
- **Development**: Run with `wasm-pack test --chrome` (or other browsers)
- **CI/CD**: Can integrate with headless browser testing
- Requires WASM compilation before running tests

### Test Environment Requirements
- Browser with WASM support (Chrome, Firefox, Safari, Edge)
- Document body available for DOM manipulation
- Performance API available for benchmarking

## Notable Patterns

### DOM Setup Pattern
```rust
let window = web_sys::window().expect("no global `window` exists");
let document = window.document().expect("should have a document on window");
let app = document.create_element("div").unwrap();
app.set_id("editor-container");
document.body().unwrap().append_child(&app).unwrap();
```
- Creates isolated test container for each test
- Prevents test interference
- Mirrors real-world initialization

### Type Casting Pattern
```rust
.dyn_into::<web_sys::HtmlTextAreaElement>()
```
- Safely downcasts generic Element to specific HTML element types
- Enables type-specific operations (e.g., `.set_value()`)

### Synthetic Event Testing
- Uses `InputEvent::new("input")` to programmatically trigger events
- Simulates user interaction without actual user input
- Enables automated UI testing

## Performance Characteristics

### Benchmark Results Interpretation
- **Target**: < 50ms average conversion time
- **Workload**: Complex markdown with multiple element types
- **Iterations**: 100 runs for statistical significance
- **Environment**: Real browser with WASM runtime overhead

### What Performance Test Reveals
- WASM overhead in browser context
- Markdown parsing efficiency
- DOM manipulation speed
- Event handler overhead

## Technical Considerations

### Test Isolation
- Each test creates its own container element
- Tests don't clean up DOM (potential memory accumulation in test suite)
- No explicit teardown logic

### Browser Requirements
- Tests assume browser environment (not Node.js)
- Require full web-sys features enabled
- Performance API may vary across browsers

### Limitations
- Tests don't validate visual rendering (only DOM structure)
- No accessibility testing
- No cross-browser compatibility validation in single test run
- Benchmark test has hard-coded performance threshold (may need adjustment for different hardware)

## Improvement Opportunities
- Add cleanup/teardown logic to remove test containers
- Test edge cases (empty input, malformed markdown, very large documents)
- Validate error handling paths
- Test keyboard shortcuts and toolbar functionality
- Add screenshot/visual regression testing
- Test memory usage over extended use
- Validate accessibility attributes (ARIA labels, keyboard navigation)
