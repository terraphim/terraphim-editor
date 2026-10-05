# Diagnostics for spans

The language server publishes diagnostics for every span. Each annotation is re-anchored after an edit, and code actions offer the alternatives stored in the span model. The LSP reads the annotation format from the trailing fenced block.
