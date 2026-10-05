# Terraphim Editor: project summary

Refreshed 2026-10-05.

## Overview
A WebAssembly Markdown editor (Rust compiled with Trunk, vanilla JavaScript, Shoelace) that is gaining Write_On-style alternative control: per-span alternatives, ghosting, an Overflow stash and a Lab of deterministic marking actions (epic #1, spec `docs/requirements/alternative-control.md`).

## Architecture
- **Rust (WASM)**: `src/lib.rs` renders the template and runs a debounced Markdown preview; `src/document.rs` bridges the browser to the span model.
- **Span model**: `crates/terraphim_alternatives` holds non-KG state (human alternatives, ghost ranges, overflow, the trailing annotation block, a/an fix-up) and re-anchors through `terraphim_automata` 2.1 from the private `terraphim` registry. KG synonyms as alternatives come later through the `terraphim_lsp` core (#13).
- **JavaScript**: `editor.js` (contenteditable `EditorSurface` with UTF-16 offsets, decorations, undo; `MarkdownEditor` with the document API), `chrome.js` (Write_On mode and corner chrome), `blocks.js` (Blocks view: the body as editable Markdown blocks, edited through the surface; #19), `config.js` (shortcuts and commands).
- **Styling**: `tokens.css` (design tokens), `write-on.css` (Write_On layout), `blocks.css` (Blocks view), FontAwesome with SRI.
- **Build and release**: Trunk builds to `target/trunk-dist`; `scripts/release-dist.sh` regenerates the committed `dist/` release copy.

## Data flow
Load -> `run()` opens the document -> the surface edits text -> each edit is mirrored into the Rust model (anchors follow, ghosts stretch) -> the preview re-renders after a pause -> save writes body plus annotation block; anything that cannot be placed is set aside and still saved.

## Testing
No mocks. Native unit and golden tests (crate and root); browser tests in Chrome split into `web.rs`, `web_chrome.rs`, `web_persistence.rs`, `web_blocks.rs`, `web_bench.rs`, `tokens.rs`; criterion benches (`markdown_bench`, `document_bench`, `alternatives_bench`). Every PR gets a pi-rust structural review (openai-codex/gpt-5.5) before merge.

## Security
Plain-text surface (paste strips formatting); decorations built with DOM APIs, not HTML strings; FontAwesome pinned with SRI; Markdown preview renders HTML as before (pre-existing behaviour). Malformed annotation blocks never lose data.

## Business value
Lets writers compare alternatives in place, fade text to judge a draft without it, and get deterministic, role-aware editing marks without an LLM, all in a lightweight browser editor that can share its document model with LSP-based clients (Zed, Sublime).

## Process
Work is tracked in Gitea issues with `gtr` (PageRank ordering); branches `task/IDX-slug`; PRs through the Gitea flow with pi-rust structural review and fixes before merge.
