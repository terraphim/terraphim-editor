# Terraphim Editor

A WebAssembly-based Markdown editor built with Rust, [Shoelace](https://shoelace.style/) styles and no other dependencies. 
Trunk is used for the build system

## Features

- Live Markdown preview
- Pure Javascript for front end and WebAssembly implementation for rendering Markdown
- Minimal external dependencies
- Modern web components UI

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

### Rust Tests
Run the Rust unit tests:
```bash
cargo test
```

### Frontend Tests (Wasm)
Run the frontend tests:
```bash
wasm-pack test --chrome
```

## Building for Production

Create a production build:

```bash
trunk build --release
```

The output will be in the `dist` directory.

## Contributing

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details. 