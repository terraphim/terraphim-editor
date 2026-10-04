# Write_On design tokens

Source of truth: `public/css/tokens.css`. Spec: `docs/requirements/alternative-control.md` section 10 (visual tokens) and R-7.1 (layout). Issue: terraphim/terraphim-editor#5 (epic #1).

All later Write_On UI work (alternatives panel, inline dots, ghosting, overflow panel, context menu, Lab popover, corner chrome) should use these custom properties and not hard-code colours, radii or fonts.

## Provenance

The values come from the spec text. The spec labels them "approximate; sample before finalising", but the source demo frames are not in the repository, so sampling from them is **deferred**. When the frames turn up, change the values in `tokens.css` only. Every consumer reads the tokens, so nothing else needs editing.

## Tokens

| Token | Value | Use |
|---|---|---|
| `--te-color-bg` | `#0a0d1c` | Full-bleed page background |
| `--te-color-text` | `#e8d9c4` | Body text (warm cream) |
| `--te-color-text-dim` | `#8f8781` | Dim UI text: word/char count, shortcut hints in menus |
| `--te-color-accent` | `#8c86e6` | Lavender accent: active tab, lit dot, selected trim border, save icon, caret |
| `--te-color-accent-soft` | `rgba(140, 134, 230, 0.4)` | Underline/dot indicator (accent at 40%), selection |
| `--te-color-panel` | `#11142a` | Side panels (alternatives, overflow) |
| `--te-color-panel-border` | `#262a4a` | 1px lighter panel border |
| `--te-color-popover` | `#171a2e` | Context menu and Lab popover |
| `--te-color-popover-hover` | `#1f2340` | Hovered menu row ("slightly lighter", R-7.3) |
| `--te-radius-popover` | `8px` | Popover/menu corner radius |
| `--te-font-mono` | `"JetBrains Mono", "IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, "Liberation Mono", monospace` | Body and UI |
| `--te-font-display` | `"Caveat", "Segoe Script", "Bradley Hand", "Brush Script MT", cursive` | Panel titles ("Alternatives", "Overflow") |
| `--te-font-size-body` | `1.0625rem` | Body size in Write_On mode |
| `--te-line-height` | `1.75` | Body line height |
| `--te-ghost-opacity` | `0.1` | Opacity of ghosted spans (R-5.1) |
| `--te-measure` | `70ch` | Text column width (R-7.1) |
| `--te-page-margin-top` | `clamp(3rem, 12vh, 8rem)` | Generous top margin (R-7.1) |

### Decisions and deviations

- **Dim text is not ghost text.** Section 10 says "dim/ghost text is about body at 10% opacity". That level is right for ghosting (R-5.1), where text is supposed to fade back, but UI text such as the word count (R-7.2) and menu shortcut hints (R-7.3) has to stay readable. So there are two tokens. `--te-ghost-opacity: 0.1` is used only for ghosted spans. `--te-color-text-dim` is a solid colour, equal to body text at 60% over the page background, chosen to pass AA. Because it is a solid hex and not an opacity, its contrast can be checked and the test is deterministic.
- **Fonts are stacks only.** Loading web fonts (JetBrains Mono, Caveat) would need another `<link>` and a network dependency, so it is deferred. Users who have the fonts installed get them, and everyone else gets the system monospace and cursive fallbacks.
- **Panel border** `#262a4a` is an estimate of "1px lighter border". It is decorative and is not used to convey state.

## Scoping and preview

`:root` holds the tokens and nothing else, so they have no visible effect until something uses them. The Write_On theme and layout apply only under an opt-in scope:

```html
<body data-mode="write-on">   <!-- preferred; issue #7 will toggle this -->
<body class="write-on">       <!-- equivalent class hook -->
```

To preview before #7 lands, add either one to `<body>` by hand or in browser devtools (`document.body.dataset.mode = "write-on"`). The plain editor stays unchanged unless one of them is set.

Inside the scope:

- The scope element gets the page background, body text colour, monospace face, body size and 1.75 line height, plus a minimum height of `100vh`.
- `.te-surface` (the expected class for the contenteditable writing surface, which another issue is building) and `.markdown-preview` become a centred column `--te-measure` wide, with `--te-page-margin-top` of top padding.
- No toolbar or split-panel rules are included yet. Hiding the toolbar in Write_On mode ("no visible toolbar", R-7.1) belongs to the #7 toggle work.

## Contrast (WCAG 2.x relative luminance)

| Foreground | Background | Ratio | Target | Result |
|---|---|---|---|---|
| text `#e8d9c4` | bg `#0a0d1c` | 13.94:1 | 4.5:1 (AA body) | Pass (AAA) |
| text `#e8d9c4` | panel `#11142a` | 13.09:1 | 4.5:1 | Pass (AAA) |
| text `#e8d9c4` | popover `#171a2e` | 12.38:1 | 4.5:1 | Pass (AAA) |
| text `#e8d9c4` | popover hover `#1f2340` | 11.04:1 | 4.5:1 | Pass (AAA) |
| dim `#8f8781` | bg `#0a0d1c` | 5.47:1 | 4.5:1 | Pass (AA) |
| dim `#8f8781` | panel `#11142a` | 5.14:1 | 4.5:1 | Pass (AA) |
| dim `#8f8781` | popover `#171a2e` | 4.86:1 | 4.5:1 | Pass (AA) |
| dim `#8f8781` | popover hover `#1f2340` | 4.33:1 | 4.5:1 / 3:1 | Below AA for small text, above 3:1 |
| accent `#8c86e6` | bg `#0a0d1c` | 6.13:1 | 4.5:1 | Pass (AA) |
| accent `#8c86e6` | panel `#11142a` | 5.76:1 | 4.5:1 | Pass (AA) |
| accent `#8c86e6` | popover `#171a2e` | 5.45:1 | 4.5:1 | Pass (AA) |
| ghost (text at 10%) | bg `#0a0d1c` | 1.21:1 | n/a | **Flag**: intentionally unreadable-at-a-glance per R-5.1 |
| accent-soft (40%, blended) | bg `#0a0d1c` | 1.93:1 | 3:1 (non-text, 1.4.11) | **Flag**: below 3:1 |
| panel border `#262a4a` | panel `#11142a` | 1.31:1 | n/a | Decorative only |

Flags:

- **Ghost (1.21:1).** This is deliberate. R-5.1 asks for ghosted text to recede. Ghosted text stays selectable and is still exposed to assistive technology. Consumers must not use `--te-ghost-opacity` for any text the user is expected to read.
- **Accent-soft underline/dot (1.93:1).** The spec asks for accent at 40%, which falls below the 3:1 non-text contrast guideline. The fix belongs to the inline-indicator issue: a "has alternatives" span should also be announced to assistive technology (for example via an ARIA description), and the lit or active dot should use the full `--te-color-accent` (6.13:1). If the frames are sampled later and the colours change, recheck this figure.
- **Dim on hover row (4.33:1).** Shortcut hints on a hovered menu row fall slightly below AA for small text. If this matters, have the menu switch hint text to `--te-color-text` on hover.

The ratios were calculated with the WCAG relative-luminance formula. For translucent tokens, the colour was first blended onto the background.

## Tests

`tests/tokens.rs` (browser, `wasm_bindgen_test`, no mocks) loads `public/css/tokens.css` into the live document and uses `getComputedStyle` to check that:

- the key custom properties resolve on `:root` to the values above;
- both `[data-mode="write-on"]` and `.write-on` apply the background `rgb(10, 13, 28)`, text colour, monospace face and a 1.75 line-height ratio;
- `.te-surface` and `.markdown-preview` inside the scope get the same finite max-width, equal left and right margins (centred) and a top padding of at least 48px;
- an element outside the scope is unaffected (transparent background, `max-width: none`).

Run with `wasm-pack test --headless --chrome`.
