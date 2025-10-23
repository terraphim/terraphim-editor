# File Summary: public/js/config.js

## File Path
`public/js/config.js`

## Purpose
Configuration file defining editor behavior, keyboard shortcuts, toolbar commands, initial content, and styling options. Exposed as global `window.EditorConfig` for consumption by editor initialization code.

## Key Configuration Objects

###Shortcuts Array
Defines keyboard shortcuts for markdown formatting:
- Bold (`ctrl+b`), Italic (`ctrl+i`), Code (`ctrl+k`), Link (`ctrl+l`), Heading (`ctrl+h`)
- Each shortcut specifies: name, key combo, prefix/suffix strings, description

### Commands Array
Toolbar button configurations for visual formatting:
- Headings (H1-H3), Bold, Italic, Underline
- Each command has: name, icon, prefix/suffix for text wrapping

### Initial Content
Default markdown text displayed on editor load - demonstrates features and provides user onboarding

### Styles Object
Layout configuration: editor/preview dimensions, split position (50%)

## Export Pattern
```javascript
if (typeof exports !== 'undefined') {
  exports.EditorConfig = EditorConfig;
} else {
  window.EditorConfig = EditorConfig;
}
```
Supports both CommonJS and browser globals for maximum compatibility

## Integration
- Loaded before editor.js in index.html
- Used by `MarkdownEditor` class for initialization
- Used by `TeraphimEditor` wrapper for configuration
- Single source of truth for UI behavior
