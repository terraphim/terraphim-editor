# File Summary: templates/editor.html

## File Path
`templates/editor.html`

## Purpose
Rinja (Jinja2-like) template rendered by Rust code to generate the editor's HTML structure. Creates split-pane interface with markdown input, live preview, toolbar, and keyboard shortcut dialog.

## Key Components
- **Split Panel**: Uses Shoelace `<sl-split-panel>` for resizable panes
- **Toolbar**: Button group with formatting controls and help button
- **Markdown Input**: Textarea with monospace font for editing
- **Preview Pane**: Div for rendered HTML output
- **Shortcuts Dialog**: Modal showing keyboard shortcuts

## Template Variables
- `{{initial_content}}` - Pre-populated markdown text
- `{{initial_preview|safe}}` - Pre-rendered HTML (unescaped)

## Shoelace Components Used
- `sl-split-panel`, `sl-button-group`, `sl-button`, `sl-icon`, `sl-dialog`, `sl-divider`

## Styling
- Inline `<style>` block with component-specific CSS
- Monospace font for input, custom border/padding
- Responsive split panel with adjustable divider

## Integration
- Rendered in Rust via `EditorTemplate` struct (src/lib.rs)
- JavaScript in `editor.js` initializes interactive features
- Template placeholders populated at WASM initialization
