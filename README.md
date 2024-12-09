# Terraphim Editor

A WebAssembly project using Rust and Trunk.

## Prerequisites

1. Install Rust (if you haven't already):
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

To run the development server:

```bash
trunk serve
```

This will start a local server at `http://127.0.0.1:8080` by default.

## Building for Production

To create a production build:

```bash
trunk build --release
```

The output will be in the `dist` directory. 