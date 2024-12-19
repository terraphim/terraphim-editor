const EditorConfig = {
  shortcuts: [
    {
      name: "type-bold",
      key: "ctrl+b",
      prefix: "**",
      suffix: "**",
      desc: "Bold text"
    },
    {
      name: "type-italic",
      key: "ctrl+i",
      prefix: "_",
      suffix: "_",
      desc: "Italic text"
    },
    {
      name: "code",
      key: "ctrl+k",
      prefix: "`",
      suffix: "`",
      desc: "Inline code"
    },
    {
      name: "link",
      key: "ctrl+l",
      prefix: "[",
      suffix: "](url)",
      desc: "Create link"
    },
    {
      name: "type-h1",
      key: "ctrl+h",
      prefix: "# ",
      suffix: "",
      desc: "Heading"
    }
  ],
  
  commands: [
    { name: 'Heading 1', icon: 'type-h1', prefix: '# ', suffix: '' },
    { name: 'Heading 2', icon: 'type-h2', prefix: '## ', suffix: '' },
    { name: 'Heading 3', icon: 'type-h3', prefix: '### ', suffix: '' },
    { name: 'Bold', icon: 'type-bold', prefix: '**', suffix: '**' },
    { name: 'Italic', icon: 'type-italic', prefix: '_', suffix: '_' },
    { name: 'Underline', icon: 'type-underline', prefix: '<u>', suffix: '</u>' }
  ],

  initialContent: `# Welcome to Markdown Editor!

This is a simple markdown editor built with:
- Rust
- WebAssembly
- Shoelace components

## Try it out
1. Edit this text on the left
2. See the preview on the right
3. Use **bold**, *italic*, or \`code\` formatting

---

> Made with ❤️ using Rust and WASM`,

  styles: {
    editorWidth: '100%',
    editorHeight: '100%',
    previewWidth: '100%',
    previewHeight: '100%',
    defaultSplitPosition: 50
  }
};

if (typeof exports !== 'undefined') {
  exports.EditorConfig = EditorConfig;
} else {
  window.EditorConfig = EditorConfig;
} 