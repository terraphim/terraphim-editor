# Terraphim Editor

A WebAssembly-based Markdown editor built with Rust, supporting **three different UI styles**: Shoelace, Pure HTML/CSS, and Web Awesome.

## Features

- **🎨 Three UI Styles** - Choose between Shoelace, Vanilla HTML/CSS, or Web Awesome
- **⚡ Live Markdown Preview** - Instant rendering powered by Rust/WASM
- **🎯 Zero to Minimal Dependencies** - Vanilla style has no external dependencies
- **♿ Accessible** - Built with web standards and accessibility in mind
- **📦 Multiple Module Formats** - ESM, UMD, and IIFE support
- **🔧 Framework Agnostic** - Works with any JavaScript framework or none at all

## UI Styles

### 1. Shoelace (Default)
Professional web components with comprehensive design system
- Professional UI components
- Built-in accessibility
- Theme customization

### 2. Vanilla HTML/CSS
Pure HTML/CSS with zero dependencies
- No external libraries
- Lightweight (~3KB)
- Maximum control

### 3. Web Awesome
Next-generation web components from Font Awesome
- 11 built-in themes
- Font Awesome integration
- Advanced design system

📖 **[Read the Multi-Style Guide](MULTI_STYLE_GUIDE.md)** for detailed documentation

## Prerequisites

1. Install Rust:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Add the WebAssembly target:
```bash
rustup target add wasm32-unknown-unknown
```

3. Install Trunk:
```bash
cargo install trunk
```

## Development

Run the development server:

```bash
trunk serve
```

Visit `http://127.0.0.1:8080` in your browser.

## Testing

Terraphim Editor has comprehensive test coverage including unit tests, integration tests, and end-to-end tests for all three style variants.

### Quick Start

```bash
# Run all tests (Rust + E2E)
npm test

# Run only Rust unit tests
npm run test:rust

# Run only E2E tests
npm run test:e2e

# Run E2E tests with interactive UI
npm run test:e2e:ui
```

### Test Coverage

- ✅ **49 automated tests** covering all functionality (47 E2E + 2 Rust)
- ✅ **3 browser engines** (Chromium, Firefox, WebKit)
- ✅ **All 3 UI variants** (Shoelace, Vanilla, Web Awesome)
- ✅ **Dynamic script loading** tested and verified
- ✅ **CI/CD pipeline** with GitHub Actions

📖 **[Read the Testing Guide](TESTING.md)** for detailed documentation

## Building for Production

### Option 1: Using build.sh (Recommended)

Build WASM package and distribution files:

```bash
./build.sh
```

This creates:
- `pkg/` - WASM package
- `dist/` - Distribution bundle
- `package/` - NPM package ready for distribution

### Option 2: Manual build

```bash
# Build WASM module (includes all three style templates)
wasm-pack build --target web --out-dir pkg

# Build JavaScript bundles (ESM, UMD, IIFE)
npm run build
```

### Option 3: Development build with Trunk

```bash
trunk build --release
```

The output will be in the `dist` directory.

## Quick Start Examples

### Shoelace Style
```html
<script type="module">
  import init from './js/terraphim_editor.js';
  const wasm = await init('./wasm/terraphim_editor_bg.wasm');
  wasm.run_with_style(wasm.EditorStyle.Shoelace);
</script>
```

### Vanilla Style (No Dependencies!)
```html
<script type="module">
  import init from './js/terraphim_editor.js';
  const wasm = await init('./wasm/terraphim_editor_bg.wasm');
  wasm.run_with_style(wasm.EditorStyle.Vanilla);
</script>
```

### Web Awesome Style
```html
<script type="module">
  import init from './js/terraphim_editor.js';
  const wasm = await init('./wasm/terraphim_editor_bg.wasm');
  wasm.run_with_style(wasm.EditorStyle.WebAwesome);
</script>
```

See `public/example-*.html` for complete working examples.

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 