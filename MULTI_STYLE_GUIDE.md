# Multi-Style Build System Guide

## Overview

Terraphim Editor now supports **three different UI styles** that you can choose from when building your application:

1. **Shoelace** - Beautiful web components with comprehensive design system (default)
2. **Vanilla** - Pure HTML/CSS with zero dependencies
3. **Web Awesome** - Next-generation web components from Font Awesome

## Architecture

### Core Components

```
terraphim-editor/
├── src/lib.rs                          # Rust WASM core with multi-style support
├── templates/
│   ├── editor.html                     # Shoelace template
│   ├── editor-vanilla.html             # Vanilla HTML/CSS template
│   └── editor-webawesome.html          # Web Awesome template
├── public/js/
│   ├── editor.js                       # Shoelace adapter
│   ├── editor-vanilla.js               # Vanilla adapter
│   └── editor-webawesome.js            # Web Awesome adapter
└── public/
    ├── example-shoelace.html           # Shoelace demo
    ├── example-vanilla.html            # Vanilla demo
    ├── example-webawesome.html         # Web Awesome demo
    └── index-multistyle.html           # Interactive style switcher
```

### How It Works

The editor uses a **template-based architecture** where:

1. **Rust WASM Layer** (`src/lib.rs`) generates HTML from Rinja templates
2. **Templates** (`templates/*.html`) define the UI structure for each style
3. **JavaScript Adapters** (`public/js/editor-*.js`) handle framework-specific logic
4. **Style Selection** happens at runtime via `EditorStyle` enum

## Using Different Styles

### 1. Shoelace (Default)

**Use when:** You want a polished, professional UI with minimal effort

**Features:**
- Professional web components
- Built-in accessibility
- Theme customization
- Comprehensive design tokens

**Example:**
```html
<script type="module">
  import init from './js/terraphim_editor.js';

  const wasm = await init('./wasm/terraphim_editor_bg.wasm');
  wasm.run_with_style(wasm.EditorStyle.Shoelace);

  // Load Shoelace adapter
  const script = document.createElement('script');
  script.src = './js/editor.js';
  document.body.appendChild(script);
</script>
```

**Dependencies:**
```html
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/themes/light.css" />
<script type="module" src="https://cdn.jsdelivr.net/npm/@shoelace-style/shoelace@2.12.0/cdn/components/..."></script>
```

### 2. Vanilla HTML/CSS

**Use when:** You want zero dependencies and maximum control

**Features:**
- No external libraries
- Lightweight (~3KB total)
- Full styling control
- Fast loading

**Example:**
```html
<script type="module">
  import init from './js/terraphim_editor.js';

  const wasm = await init('./wasm/terraphim_editor_bg.wasm');
  wasm.run_with_style(wasm.EditorStyle.Vanilla);

  // Load Vanilla adapter
  const script = document.createElement('script');
  script.src = './js/editor-vanilla.js';
  document.body.appendChild(script);
</script>
```

**Dependencies:**
```
None! Just HTML, CSS, and JavaScript
```

### 3. Web Awesome

**Use when:** You want cutting-edge features and Font Awesome integration

**Features:**
- 11 built-in themes (light/dark modes)
- Enhanced icon library
- Advanced design system
- Shoelace-compatible syntax

**Example:**
```html
<script type="module">
  import init from './js/terraphim_editor.js';

  const wasm = await init('./wasm/terraphim_editor_bg.wasm');
  wasm.run_with_style(wasm.EditorStyle.WebAwesome);

  // Load Web Awesome adapter
  const script = document.createElement('script');
  script.src = './js/editor-webawesome.js';
  document.body.appendChild(script);
</script>
```

**Dependencies:**
```html
<!-- Get your project CDN from https://webawesome.com -->
<link rel="stylesheet" href="YOUR_WEBAWESOME_PROJECT_CDN" />
```

## API Reference

### Rust WASM API

```rust
#[wasm_bindgen]
pub enum EditorStyle {
    Shoelace,    // Default
    Vanilla,     // Pure HTML/CSS
    WebAwesome,  // Web Awesome components
}

// Initialize with specific style
#[wasm_bindgen]
pub fn run_with_style(style: EditorStyle) -> Result<(), JsValue>

// Render editor HTML for a specific style
#[wasm_bindgen]
pub fn render_editor_html(style: EditorStyle, content: &str) -> Result<String, JsValue>
```

### JavaScript Usage

```javascript
// Import WASM module
import init from './js/terraphim_editor.js';

// Initialize
const wasm = await init('./wasm/terraphim_editor_bg.wasm');

// Use specific style
wasm.run_with_style(wasm.EditorStyle.Shoelace);
wasm.run_with_style(wasm.EditorStyle.Vanilla);
wasm.run_with_style(wasm.EditorStyle.WebAwesome);

// Generate HTML programmatically
const html = wasm.render_editor_html(
  wasm.EditorStyle.Vanilla,
  "# My Content"
);
```

## Building & Development

### Build for All Styles

```bash
# Build WASM module (includes all templates)
wasm-pack build --target web --out-dir pkg

# Build JavaScript bundles
npm run build

# Serve locally for testing
trunk serve  # Development mode
```

### File Structure

Each style variant requires:

1. **Template file** in `templates/editor-{style}.html`
2. **JavaScript adapter** in `public/js/editor-{style}.js`
3. **Example page** in `public/example-{style}.html`

### Adding a New Style

1. Create template: `templates/editor-newstyle.html`
2. Add to Rust enum in `src/lib.rs`:
   ```rust
   #[derive(Template)]
   #[template(path = "editor-newstyle.html")]
   struct EditorTemplateNewStyle {
       initial_content: String,
       initial_preview: String,
   }
   ```
3. Update `run_with_style()` match statement
4. Create adapter: `public/js/editor-newstyle.js`
5. Create example: `public/example-newstyle.html`

## Performance Comparison

| Style       | Bundle Size | Dependencies | Initial Load | Runtime Performance |
|-------------|-------------|--------------|--------------|---------------------|
| Shoelace    | ~250KB      | Shoelace CDN | ~400ms       | Excellent           |
| Vanilla     | ~3KB        | None         | ~100ms       | Excellent           |
| Web Awesome | ~300KB      | Web Awesome  | ~450ms       | Excellent           |

*Note: WASM bundle (~140KB) is shared across all styles*

## Browser Support

All three styles support:
- Chrome/Edge 88+
- Firefox 87+
- Safari 14+
- Mobile browsers (iOS 14+, Android 5+)

## Migration Guide

### From Shoelace to Vanilla

Replace component imports with vanilla adapter:
```diff
- <script src="./js/editor.js"></script>
+ <script src="./js/editor-vanilla.js"></script>
```

No CDN dependencies needed!

### From Shoelace to Web Awesome

Update component prefix and get Web Awesome CDN:
```diff
- wasm.run_with_style(wasm.EditorStyle.Shoelace);
+ wasm.run_with_style(wasm.EditorStyle.WebAwesome);
- <script src="./js/editor.js"></script>
+ <script src="./js/editor-webawesome.js"></script>
```

## Examples

See working examples:
- `public/example-shoelace.html` - Shoelace implementation
- `public/example-vanilla.html` - Vanilla implementation
- `public/example-webawesome.html` - Web Awesome implementation
- `public/index-multistyle.html` - Interactive style switcher

## FAQ

**Q: Can I switch styles at runtime?**
A: Yes! Call `wasm.run_with_style()` with a different style and load the corresponding adapter.

**Q: Which style should I use?**
A:
- **Shoelace** for most projects (best balance)
- **Vanilla** for minimal size/no dependencies
- **Web Awesome** for advanced theming and Font Awesome

**Q: Can I customize the templates?**
A: Yes! Edit templates in `templates/` and rebuild with `wasm-pack build`.

**Q: What about Web Awesome's CDN?**
A: Create a free project at https://webawesome.com to get your CDN link.

## Contributing

To add support for a new UI framework:

1. Create a template following existing patterns
2. Implement a JavaScript adapter
3. Add to the `EditorStyle` enum
4. Update this documentation
5. Add example page

## License

Same as terraphim-editor main license.
