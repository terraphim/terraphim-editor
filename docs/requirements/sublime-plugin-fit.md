# Can "Alternative Control" be a Sublime Text plugin?

Companion to `alternative-control.md` (the Write_On requirements) and `zed-plugin-fit.md`. Written 2026-10-05.

## Answer

**Mostly yes, and much better than Zed. Ghosting is the one gap.** A Sublime Text 4 package runs Python in-process. It can draw underlines, gutter icons, right-edge annotations and HTML phantoms. It can also add items to the right-click menu that depend on the word under the pointer, bind keys depending on the current view, open extra panes and show popups. Most of §2, §3, §6, §7 and §8 can be built. Some of §3 and §4 is approximate, and the chrome (§7.2) has to be rebuilt from Sublime's own UI.

The gap is **ghosting (R-5.1)**: a highlighted region cannot change the colour of its text. A local spike on build 4215 tested four colour-scheme variants, including bright red, and all of them rendered at full body colour. So ghosting, and the Lab trim preview that depends on it (R-8.4), are **Partial**. They can be shown as a background tint or a stippled underline, not as text faded to about 10%. The only way to get real fading is a syntax-level scope, which needs markers in the buffer (see "Ghosting options").

`terraphim-editor` remains the only target where the full spec can be built as written. Sublime is the best existing editor host for a reduced version: it covers far more of the spec than Zed and keeps the same document format.

## What the plugin host can do (checked)

Sublime Text build 4215, macOS arm64. The API stub is the bundled `Contents/MacOS/Lib/python314/sublime.py` (a Python 3.14 plugin host, plus the legacy 3.3 host).

| Capability | API | Used for |
|---|---|---|
| Underline, outline or fill over a region; gutter icon per region | `View.add_regions(key, regions, scope, icon, flags, annotations, annotation_color, on_navigate, on_close)`; flags `DRAW_SOLID/STIPPLED/SQUIGGLY_UNDERLINE`, `DRAW_NO_FILL`, `DRAW_NO_OUTLINE`, `HIDDEN`, `PERSISTENT`, `NO_UNDO` | R-3.1 underline, R-3.4 gutter, R-8.2 marks |
| HTML annotations aligned to the right-hand edge, with links | `annotations=`, `on_navigate` (since 4050) | R-3.2 sentence dots, clickable (R-3.6) |
| HTML phantoms: `INLINE` at the region start, `BELOW` left-aligned to the region start, `BLOCK` left-aligned to the line start | `View.add_phantom`, `PhantomLayout` | R-3.2 word dot row, R-8.4 status card |
| Hover events over text, gutter and margin | `on_hover(point, HoverZone)` | R-2.4, R-4.4 hover list |
| Popups and native popup menus | `View.show_popup`, `View.show_popup_menu(items, on_done)` | Lab popover, alternatives picker |
| Right-click menu items that depend on what was clicked | `Context.sublime-menu` with commands that use `want_event()`, `is_visible(event)` and `description(event)` | R-7.3, word context menu |
| Key bindings that depend on context | keymap `context` + `on_query_context` | ↑/↓ cycling only on a span |
| Extra panes, HTML sheets, output panels | `Window.set_layout`, `new_html_sheet` (4065), `create_output_panel` | R-4 side panel, R-6 Overflow |
| Folding | `View.fold` | Hide the trailing annotation block |
| Regions that move with edits | regions returned by `get_regions` after edits | Re-anchoring (R-9.2) |
| Focused-centre layout | `draw_centered`, `wrap_width`, `line_padding_top/bottom`, distraction-free mode | R-7.1 |
| Dimming panes that are not focused | `inactive_sheet_dimming` (default `true`) | R-4.1 main text dims when the panel has focus |

**Cannot do:** change the colour or opacity of text in a highlighted region (spike below), hide characters, draw its own window chrome or corner buttons, or style the native context menu.

## Spike evidence (2026-10-05, build 4215, screenshots from Alex)

A throwaway package `Packages/WriteOnSpike` used a custom colour scheme (`bg #0a0d1c`, `fg #e8d9c4`, `accent #8c86e6`) on a scratch Markdown file.

| Test | Result |
|---|---|
| Solid underline on a phrase and a sentence, `DRAW_NO_FILL\|DRAW_NO_OUTLINE\|DRAW_SOLID_UNDERLINE`, scope coloured at 40% of the accent | **Works.** Thin, muted lavender underline, close to R-3.1. |
| Right-edge annotation `○●○` per sentence | **Works.** Drawn at the right-hand edge with a coloured left border (default red; set it with `annotation_color`). |
| `BELOW` phantom of 7 dots on a mid-line word | **Works.** The dots sit directly under the word's first column. Each phantom adds a row of vertical space: `text_to_layout` measured 48 px with default minihtml styling at a line height of 18 px. CSS can reduce it but not remove it. Centring it under the word needs CSS padding calculated from the word's width. |
| `INLINE` phantom after a word | Works, but pushes the following text to the right. Not suitable for dots. |
| Gutter `dot` icon on a paragraph line | **Works.** Small dot in the gutter. A vertical rule across the paragraph needs a custom icon on every line. |
| Ghost: rule with `foreground` (blended to 10%), `background` slightly off the page, `DRAW_NO_OUTLINE` | **Fails.** Text renders at full body colour. |
| Same rule with `foreground: #ff0000`, flags `0` | **Fails.** Text is not red. |
| `foreground` only, no `background` | **Fails.** |
| `foreground_adjust: l(10%)` | **Fails.** |

`View.style_for_scope` does resolve the ghost rule (`foreground #20212d`). The colour scheme accepts the rule, but the renderer does not apply it to text in a highlighted region.

## Requirement-by-requirement fit

Key: **Full** = implementable as specified; **Partial** = an approximation; **None** = not possible.

| Req | Feature | Sublime | How / why not |
|---|---|---|---|
| R-2.1 | Plain mode / Write_On toggle | Partial | Plain mode = distraction-free mode with a centred 70-column wrap. The toggle is a command or key, not a click on the word count (the status bar cannot be clicked). |
| R-2.2, R-2.3 | Span-anchored alternatives, 3 granularities | Full | The span model is not reimplemented in the plugin: KG spans come from the `terraphim_lsp` core and non-KG spans from `terraphim_alternatives` (see "Provider path"); the plugin only renders them, with one region key per granularity. |
| R-2.4 | Hover + ↑/↓ cycling in place | Partial | `on_hover` records the span under the pointer, and a keymap context enables ↑/↓ only while that span is active. Sublime does not report when the pointer leaves a span, so cycling the span under the caret is more reliable. |
| R-2.5 | Persistence | Full | Trailing fenced block (decision §12.2), written on `on_pre_save` and folded on load. |
| R-2.6 | a/an fix-up on swap | Full | The same `TextCommand` edits the swap and the article, so one undo reverts both. |
| R-2.7 | Emptying removes indicator | Full | `erase_regions` / `erase_phantoms`. |
| R-3.1 | Underline | Full | Spike-verified. Sublime sets the thickness, not exactly 1 px. |
| R-3.2 | Dot row, word: centred | Partial | `BELOW` phantom, spike-verified. It adds vertical space under the line, so line spacing changes where spans have alternatives. |
| R-3.2 | Dot row, sentence/headline: right-aligned | Partial | Right-edge annotation, spike-verified. It sits at the view's right edge, not at the end of the underline. A `BELOW` phantom at the sentence end is the alternative, and also adds a row. |
| R-3.3 | Active-dot highlight | Full | HTML `●`/`○` with CSS colours. |
| R-3.4 | Paragraph gutter rule + dot column | Partial | Gutter icons on every line of the paragraph. One icon per line, so the dot column becomes an "i/n" icon or a `BLOCK` phantom. |
| R-3.5 | Headline rendering | Partial | Bold comes from the colour scheme. Font size is per view, so headlines cannot be "slightly larger". |
| R-3.6 | Click dot to jump | Full | Phantom and annotation links via `on_navigate`. |
| R-4.1 | Left alternatives panel, main text dims | Partial | A left pane via `set_layout` holding a scratch view. `inactive_sheet_dimming` dims the document while the pane has focus. The pane is part of the window layout and cannot be toggled as an overlay. |
| R-4.2 | Word · Sentence · Paragraph tabs | Partial | An HTML header (phantom or `new_html_sheet`) with links. Font is per view, so the cursive face covers the whole pane or nothing. |
| R-4.3 | Editable list, Enter adds live | Full | `on_modified` on the pane view syncs lines to the span's list. |
| R-4.4 | Provenance markers (human dot / bot glyph) | Full | Gutter icons per line in the pane: a dot for human, a custom bot icon for AI. Tinted when active. |
| R-4.5 | ↑/↓ in panel moves active alternative | Full | Keymap context scoped to the pane view. |
| R-4.6 | Undo removes AI alternatives | Partial | Buffer undo works for document text. The plugin holds a working copy of the list (obtained from the engine, see "Provider path") until save, so its undo needs a small history of its own. |
| R-5.1–R-5.3 | Ghost it / Revive | **Partial** | Fading text is not possible (spike). Options below. Revive and persistence are Full. |
| R-6.1–R-6.2 | Overflow panel | Full | A right pane holding a scratch view, with a cursive per-view font. |
| R-6.3 | Stash (move selection) | Full | One command: erase in the document, append to the Overflow view. |
| R-6.4 | Ctrl+⏎ / drag to pull back | Full | `ctrl+enter` is free on macOS. Dragging text between views is native. |
| R-6.5 | Persistence | Full | In the trailing block (`overflow`). |
| R-7.1 | Dark full-bleed, ~70 cols, centred, line height ~1.75 | Full | Colour scheme + `draw_centered` + `wrap_width: 70` + `line_padding_top/bottom`. |
| R-7.2 | Corner controls | None / Partial | No custom chrome. The word count goes in the status bar (`set_status`). The other controls become command palette entries and keys. |
| R-7.3 | Selection context menu with shortcuts | Full | `Context.sublime-menu`. Captions and visibility depend on the selection. The native menu shows the shortcuts. |
| R-7.3+ | **Word context menu** (right-click a word) | Full | `want_event()` gives the clicked point and therefore the word. The menu lists that word's alternatives in up to N declared slots (labels from `description(event)`, unused slots hidden) plus Ghost/Revive, Stash and AI alternatives. `show_popup_menu` covers lists of any length. |
| R-7.3 look | Dark rounded menu | None | Native OS menu. |
| R-7.4 | Shortcut collisions | Full | Checked against `Default (OSX).sublime-keymap` and the user keymap: `ctrl+forward_slash`, `ctrl+shift+a/g/x` and `ctrl+enter` are unbound. `toggle_comment` is on `super+/`, so there is no clash. |
| R-8.1 | Lab popover | Full | `show_popup` with minihtml links, anchored at the caret (not at a bottom-centre pill). |
| R-8.2 | Mark actions | Full | Squiggly or stippled regions, one key per mark type, with a right-edge annotation explaining each. Deterministic heuristics (decision §12.5). |
| R-8.3 | Trim level buttons | Full | Links in the Lab popover. |
| R-8.4 | Trim preview (ghosted cuts) + status card | **Partial** | Candidate cuts get a tint or strikethrough-like underline instead of fading. The status card is a phantom or popup. Spike verdict as for R-5.1. |
| R-8.5 | Click to keep, Make the cuts, Walk through, Done | Full | Click → `on_text_command("drag_select")` with an event, or a phantom link. Walk through = step the selection through the regions. |
| R-8.6 | AI alternatives for selection | Full | Bot-tagged entries appended from the thesaurus provider. |
| R-8.7 | Provider abstraction | Full | See "Provider path". |
| R-9.1–R-9.3 | Embedded block, schema, export | Full | Parsing and writing the block, and export, belong to `terraphim_alternatives` (non-KG state and the annotation block); KG-derived spans are recomputed by the `terraphim_lsp` core (see "Provider path"). |
| R-10 | Visual tokens | Partial | Colours and monospace font: Full. Cursive titles: per-view only. ~10% ghost: not possible. |

Summary: §2, §6, §8.2/8.3/8.5–8.7 and §9 carry over in full. The context menus carry over in substance (native styling). §3 and §4 carry over approximately, with phantom rows changing line spacing. **§5 ghosting and §8.4 trim preview carry over only partly, because Sublime cannot fade text in a region.** §7.2 chrome does not carry over.

## Ghosting options

1. **Background tint (recommended for v1).** Fill ghosted spans with a background close to the page colour (`DRAW_NO_OUTLINE`), plus a stippled underline. The text stays bright. It marks the span, but Jason's "judge the paragraph without it" effect is lost.
2. **Syntax scope.** A Markdown syntax extension that scopes text between invisible-looking markers (for example `⟪…⟫`) as `markup.ghost`. Syntax scopes *do* get foreground colours, so the text fades properly. The cost: markers in the body text, which breaks "body stays plain Markdown" (R-9.1). Only worth it if real fading is essential in Sublime.
3. **Fold.** `View.fold` hides a span behind an ellipsis. That deletes it visually rather than dimming it: the opposite of R-5.1. Not recommended.

## Provider path

Unlike Zed, Sublime has no extension sandbox, so the engine can run in-process. The `terraphim-automata` 1.0.0 wheel on PyPI is `cp39-abi3-macosx_11_0_arm64`. It uses Python's stable ABI, so it loads in the 3.14 plugin host. It provides `load_thesaurus`, `build_index`, `find_all_matches` and the `AutocompleteIndex` class, which is enough for thesaurus alternatives (R-8.6) and the KG-list marks (R-8.2). This repeats the approach in `editor_autocomplete_integration_options.md` (in the sibling `editors_research` directory, outside this repository).

Caveats: the 1.0.0 wheel is evidence that a Python binding works, not the current engine: the Rust crate is now `terraphim_automata` 2.1 (overlap mode, allocation-free positions), so a rebuilt 2.x binding would be needed for the re-anchoring features. Only a macOS arm64 wheel was confirmed. Other platforms need their own wheels, vendored into the package because Package Control does not install arbitrary PyPI wheels.

The **span model must not be rewritten in Python.** Under the current plan (`zed-plugin-fit.md`, "Shared model"), KG spans and their alternatives come from the `terraphim_lsp` core (`terraphim/terraphim-ai#3409`, KG synonyms from `terraphim/terraphim-core#75`), and `terraphim_alternatives` (`terraphim/terraphim-editor#2`) owns only the non-KG state: human-written alternatives, ghost ranges, overflow, the a/an rule, re-anchoring of those spans and ghosts, and the annotation block format. The Sublime package should reach them in one of two ways:

- **(a) via `terraphim_lsp` and the `LSP` package (sublimelsp).** Marks become diagnostics and alternatives become code actions, the same server work as `terraphim/terraphim-ai#3409`. The Python package then only does the UI that LSP cannot (dots, panes, context menu).
- **(b) via a small Python binding** of `terraphim_alternatives`, built as an abi3 wheel like `terraphim_automata_py`.

(a) shares the most with Zed and follows the current plan, where the `terraphim_lsp` core (`terraphim/terraphim-ai#3409`) is the shared engine and KG synonyms come from `terraphim/terraphim-core#75` (see `zed-plugin-fit.md`, "Shared model"). (b) avoids running a server. The earlier extraction plan (`terraphim/terraphim-editor#16`) is superseded. Until a route is chosen, a Sublime prototype can call the crate through a CLI.

## Things Sublime gets for free

- **Re-anchoring.** Sublime moves regions as the text around them is edited. While the file is open, spans never need re-anchoring by search. R-9.2's text search is needed only on load: on save, region offsets are written back to the block's `start/end`.
- **Hiding the annotation block.** `View.fold` collapses the trailing block on load. Zed could not promise this.
- **Panel dimming.** `inactive_sheet_dimming` is on by default.

## Effort and recommendation

- A **Sublime prototype is a good, cheap test of the editing model.** About a week to get alternatives, cycling, the word and selection context menus, Overflow, Lab marks and thesaurus alternatives. Jason's interaction can then be tried in a real editor before the `contenteditable` rewrite of `terraphim-editor` (§11) lands.
- It **should not replace** `terraphim-editor` as the primary target. The ghosting gap, the extra line spacing from phantoms and the native chrome all undermine what makes Write_On feel the way it does.

## Evidence

- Bundled API stub: `/Applications/Sublime Text.app/Contents/MacOS/Lib/python314/sublime.py` (`RegionFlags` l.271, `PhantomLayout` l.481, `add_regions` l.3410, `add_phantom` l.3496, `show_popup_menu` l.3644), build 4215.
- `Default.sublime-package` → `Default (OSX).sublime-keymap` and `Preferences.sublime-settings` (`inactive_sheet_dimming: true`, `draw_centered`, `wrap_width`, `line_padding_top`).
- Spike screenshots, 2026-10-05 00:11 and 00:13 (Alex), package `WriteOnSpike` (removed after the test).
- PyPI `terraphim-automata` 1.0.0 wheel tag and `__init__.pyi`.
- Background: [sublimehq/sublime_text#6381](https://github.com/sublimehq/sublime_text/issues/6381) (region colours only through scopes), [Sublime forum: arbitrarily color text](https://forum.sublimetext.com/t/arbitrarily-color-text/18742).
