# Terraphim Editor

A Markdown editor built with Rust compiled to WebAssembly and plain JavaScript. It renders Markdown with a live preview and adds a distraction-free **Write_On** mode for working with alternative wordings: keep several versions of a word, sentence or paragraph, cycle between them in place, ghost text you are unsure about, stash ideas in an overflow panel, and let the Lab mark weak, long or hedged sentences or trim the draft by a chosen amount. Nothing is rewritten without you asking; every change is one undo step.

Trunk is the build system. Styles come from [Shoelace](https://shoelace.style/) and design tokens; there are no JavaScript build dependencies.

![Write_On mode](docs/images/write-on.png)

## Features

- **Plain mode.** A Markdown source pane with a live preview (debounced), a formatting toolbar and a `/` command palette.

  ![Plain mode](docs/images/plain.png)

- **Write_On mode.** A full-window writing surface with corner controls: the word and character counter (click it to switch modes), `●●●` alternatives panel, `M↓` Markdown view, the keyboard reference, save and open, the **LAB** pill and the **XYZ** Overflow panel.

- **Alternatives and inline indicators.** A word, sentence or paragraph can hold several alternatives. Text with alternatives shows a row of dots under it (one per alternative, the active one lit); hover it and press Up or Down to cycle in place. "a" and "an" before the word are corrected as you cycle. Alternatives are stored in a trailing `terraphim-alternatives` block in the Markdown file, so the file stays plain Markdown.

- **Alternatives panel.** Word, Sentence and Paragraph tabs list every alternative for the span at the caret, one editable line each. A dot marks your own lines and a robot glyph marks AI or knowledge-graph lines.

  ![Alternatives panel](docs/images/alternatives-panel.png)

- **Knowledge-graph alternatives.** Load a thesaurus and every term it knows gets its synonyms as alternatives, with capitalisation kept and "a"/"an" fixed. The matching comes from [`terraphim_lsp_core`](https://git.terraphim.cloud/terraphim/terraphim-ai), the same engine the Terraphim language server uses. Knowledge-graph spans are worked out from the text each time and are not saved into the file.

  ![Knowledge-graph alternatives](docs/images/kg-alternatives.png)

- **Ghosting and the selection menu.** Right-click a selection (or press the Menu key or Shift+F10) for "Alternatives for selection", "AI alternatives for selection", "Ghost it" and "Stash this in Overflow". Ghosted text fades but stays editable, counted and saved; it is left out of exports.

  ![Ghost and the selection menu](docs/images/ghosts-and-menu.png)

- **Overflow panel.** A free-form scratch area on the right. Stashing moves the selection into it as one undo step; Ctrl+Enter or dragging copies text back into the page. The overflow is saved with the document and left out of exports.

  ![Overflow panel](docs/images/overflow-panel.png)

- **The Lab.** Six actions that mark text and never rewrite it: fix punctuation and typos (proposed fixes you can accept), mark the weakest sentences, sentences that run long, convoluted sentences, words that don't fit the tone, and hedges and filler.

  ![The Lab](docs/images/lab-marks.png)

- **Trim.** Slight trim (about 10%), Tighten more (20%), Even sharper (30%) or Cut in half (50%) fades the candidate cuts and shows the word count before and after. Click a faded span to keep it, walk through the cuts one by one, then make the cuts as one undo step.

  ![Trim preview](docs/images/lab-trim.png)

- **Blocks view.** The document as a list of block cards (heading, paragraph, list, quote and so on) over the same Markdown text. Edit a block in place, add or delete blocks, or move a block up or down; spans and ghosts move with it. A draft you are typing is never lost: if the text changes underneath it, it is kept in a notice with Replace or Insert and Discard.

  ![Blocks view](docs/images/blocks-view.png)

## Keyboard shortcuts

The keyboard glyph in Write_On mode opens the same reference. Ctrl is not aliased to Cmd.

### Writing

| Keys | Action |
|---|---|
| Ctrl+B | Bold |
| Ctrl+I | Italic |
| Ctrl+K | Inline code |
| Ctrl+L | Link |
| Ctrl+H | Heading |
| `/` | Command palette (headings, bold, italic, underline) |
| Ctrl+Z / Ctrl+Y | Undo / redo |

### Selection (Write_On mode)

| Keys | Action |
|---|---|
| Right-click, Menu key or Shift+F10 | Open the selection menu |
| Ctrl+Shift+A | Alternatives for selection (opens the panel) |
| Ctrl+Shift+G | AI alternatives for selection (knowledge-graph synonyms) |
| Ctrl+/ | Ghost it / Revive (a caret inside a ghost revives it) |
| Ctrl+Shift+X | Stash this in Overflow |

### Alternatives

| Keys | Action |
|---|---|
| Hover a span, then Up / Down | Cycle its alternatives in place (wraps round) |
| Alt+Up / Alt+Down with the caret in a span | Cycle without the mouse |
| Click a dot | Jump to that alternative |
| In the panel: Up / Down | Make a line the active alternative |
| In the panel: Alt+Up / Alt+Down | Reorder alternatives |
| In the panel: Enter on the last line | Add an alternative |
| Escape | Close the panel |

### Overflow panel

| Keys | Action |
|---|---|
| XYZ (bottom right) | Open or close the panel |
| Ctrl+Enter | Insert the panel selection (or the current line) at the caret |
| Drag | Panel text into the page copies it; page text onto the panel stashes it |
| Escape | Close the panel |

### Lab and trim

| Keys | Action |
|---|---|
| LAB (bottom centre) | Open the Lab |
| Up / Down, Home / End | Move between actions |
| Escape | Close the Lab |
| Walk through: Left / Right | Previous / next cut |
| Walk through: K | Keep the current cut |
| Walk through: Escape | Stop |

### Blocks view

| Keys | Action |
|---|---|
| Up / Down, Home / End | Move between cards |
| Enter or F2 | Edit the block |
| Shift+Enter | Add a paragraph below |
| Delete | Delete the block |
| Alt+Up / Alt+Down | Move the block up or down |
| Ctrl+Enter (while editing) | Commit the edit |
| Escape (while editing) | Close; typed text is kept in the drafts notice |

## Prerequisites

1. Install Rust:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. Add the WebAssembly target:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```
3. Install Trunk and wasm-pack:
   ```bash
   cargo install trunk wasm-pack
   ```

Some crates come from the private `terraphim` Cargo registry on git.terraphim.cloud (`terraphim_automata`, `terraphim_types`, `terraphim_lsp_core`, and the editor's own `terraphim_alternatives` and `terraphim_lab`).

## Development

```bash
trunk serve
```

Visit `http://127.0.0.1:8080`. Trunk rebuilds on change.

## Testing

| Command | What it runs |
|---|---|
| `cargo test --workspace` | Rust unit and integration tests for the editor and the `terraphim_alternatives` and `terraphim_lab` crates |
| `scripts/browser-tests.sh` | Every browser test binary in headless Chrome, one at a time, with a per-binary time and load table; waits for the machine to settle and skips the benchmarks under heavy load (exit code 3) |
| `wasm-pack test --headless --chrome --test web_kg` | One browser test binary |
| `cargo bench` | Criterion benchmarks |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lints (also run with `--target wasm32-unknown-unknown --lib --tests`) |

Browser tests use the real DOM and real WebAssembly bindings; there are no mocks.

## Building for production

`trunk build`, `trunk build --release` and `trunk serve` write to `target/trunk-dist`, which is not tracked.

## Release build

The repository keeps a deliberate release copy of the build in `dist/`. Development builds never touch it. Regenerate it on purpose with:

```bash
scripts/release-dist.sh
```

This runs `trunk build --release --dist dist` and adds the embeddable bundle (`terraphim-editor.min.js`, `terraphim-editor.min.css`, `example.html`; needs `terser` and `cleancss`). Review `git status dist` and commit the result.

## Project layout

| Path | Contents |
|---|---|
| `src/` | The WebAssembly entry point, Markdown rendering, the document bridge (`document.rs`), the Lab bridge (`lab.rs`) and knowledge-graph alternatives (`kg.rs`) |
| `crates/terraphim_alternatives` | The span model: alternatives, ghosts, overflow, the annotation block, re-anchoring, moves and the shared word count |
| `crates/terraphim_lab` | The Lab engine: mark actions and the trim engine |
| `public/js/` | The editor surface and its features: chrome, indicators, selection menu, Lab, trim, Blocks view, alternatives panel, Overflow panel |
| `public/css/` | Design tokens and feature styles |
| `docs/requirements/`, `docs/design/` | Specification and per-feature design notes |
| `tests/` | Native tests and the browser test binaries (`web_*.rs`) |

## Contributing

Work is tracked in Gitea issues at git.terraphim.cloud. Create a branch per issue (`task/<number>-<short-title>`), open a pull request against `main`, and reference the issue in commits.

## Licence

This project is licensed under the MIT Licence; see the [LICENSE](LICENSE) file for details.
