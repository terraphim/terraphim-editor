# Inline indicators and the decoration registry

Issue: terraphim/terraphim-editor#8 (epic #1). Spec: `docs/requirements/alternative-control.md` R-3.1 to R-3.6 and section 10. Code: `public/js/indicators.js` (`TeDecorationRegistry`, `TeIndicatorLayer`), the indicator section of `public/css/write-on.css`, and a small hook in `public/js/editor.js`. Tokens: `docs/design/tokens.md`.

## What is drawn (Write_On mode only)

| Span | Underline (R-3.1) | Dots (R-3.2, R-3.3) | Other |
|---|---|---|---|
| word | 1px, `--te-color-accent-soft` | row centred under the word | |
| sentence | as word | row ending at the right end of the underline (the last line, if it wraps) | |
| headline (a sentence span on a line starting with `#`) | as word | as sentence | bold, `1.15em`, same face and colour (R-3.5) |
| paragraph | none | vertical column immediately left of the rule | 1px rule in the left gutter over the paragraph's height (R-3.4) |

- One dot per alternative, the original first. Exactly one dot, the active alternative, is lit in the full `--te-color-accent`; the others use `--te-color-accent-soft`. Dots are 3px with 3px spacing (about one diameter).
- Spans with only their original alternative (R-2.7) and set-aside spans get no indicator.
- Nothing hovers: no rule changes on hover (R-3.1).
- Plain mode renders nothing: the layer clears its decorations, removes its overlay and stops observing the surface. Every CSS rule is also scoped to `body[data-mode="write-on"]`.
- Line height: the dots, rule and column never change it (they are out of flow). The one intended exception is R-3.5: the headline's `1.15em` inline span makes that one line's box slightly taller, as the spec's appearance requires.
- Headline styling applies to the span only. Styling every Markdown heading line in Write_On mode, with or without alternatives, is a separate decision.

## How it is drawn

- **Underline**: CSS `text-decoration` on the decoration span classes (`te-ind-word`, `te-ind-sentence`) the layer puts on the surface, so it moves with the text at no cost while typing.
- **Dots and gutter rule**: elements in `.te-indicators`, an `aria-hidden`, absolutely positioned overlay appended beside the surface (in the surface's parent, which gets the `te-indicator-host` class for `position: relative`). The overlay is out of flow, so line height never changes (a browser test checks that hiding it changes no layout). It cannot live inside the contenteditable: that would break the surface's canonical DOM (`root.textContent === getText()`).
- **Geometry**: from `range.getClientRects()` of each span's *current* surface decoration. The surface moves decorations with edits, so after a keystroke the dots follow at the next animation frame without reading the model. Layout runs at most once per frame, after a surface change, a registry render, a window resize, a surface scroll or a surface resize (`ResizeObserver`, only while active), and after web fonts load.
- **Model reads**: `editor.annotations()` re-syncs the whole body into the WASM model, so it runs only on a trailing debounce after typing (120 ms, the preview's default; `new TeIndicatorLayer(editor, { delay })`). Typing elsewhere in a paragraph moves the spans in both the surface and the model by the same amount, so the refresh produces the decorations the surface already has, and the registry skips the render. A browser test types with real `execCommand` and checks that the surface does not re-render, that the refresh runs once (not per keystroke) and that the dots stay centred or right-aligned.
- `layer.flush()` runs a pending refresh and the layout now. `MarkdownEditor.openDocument()` calls it, and so does switching to Write_On. Code that changes alternatives without editing the text (issue #9) should call `editor.indicators.flush()`.

## Accessibility

The 40% underline and dim dots are below 3:1 contrast against the page (see the contrast note in `docs/design/tokens.md`), so they are not the only signal:

- the active dot uses the full accent;
- every indicated span carries `aria-describedby`, pointing at a visually hidden description in `.te-ind-descriptions` (not `aria-hidden`), for example `Word: 7 alternatives, 5 of 7 active` or `Headline: 3 alternatives, 1 of 3 active`.

The overlay itself is `aria-hidden`, and the dots are not focusable: R-3.6 calls them passive indicators, not buttons.

## Dot clicks (R-3.6)

The spec only infers that clicking a dot should jump to that alternative. A click dispatches a bubbling, cancelable `te:dot` CustomEvent from the overlay; unless a listener calls `preventDefault()`, the default action (issue #9) makes that alternative active through `editor.swapAlternative` (see [cycling.md](cycling.md)). Clicking the lit dot does nothing. Listen for it on `document`:

```js
document.addEventListener('te:dot', (e) => {
  const { editor, spanId, index, kind, active } = e.detail; // kind: word | sentence | headline | paragraph
});
```

`mousedown` on a dot is prevented so the caret and focus stay in the surface.

## Decoration registry

`EditorSurface.setDecorations()` holds one list and re-renders the surface DOM on every call. Several features decorate text (these indicators, then ghosts and the selection menu in issue #11), so none of them should call it directly. `MarkdownEditor.initialize()` creates `editor.decorations = new TeDecorationRegistry(editor.surface)`. Each feature owns a named layer:

```js
const reg = editor.decorations;
reg.set('ghosts', [{ id: 'g1', start, end, className: 'te-ghost', attributes: { 'data-ghost': 'g1' } }]);
reg.get('ghosts');      // stored items, moved through edits like the surface's
reg.current('ghosts');  // live offsets, moved through edits since the last render (an edit inside drops one)
reg.clear('ghosts');    // same as set('ghosts', []); the layer stays registered
reg.batch(() => { reg.set('ghosts', g); reg.set('menu', m); }); // one render
reg.names();            // ['indicators', 'ghosts', ...] in merge order
const off = reg.onApply(() => { /* after each real re-render */ });
```

Rules:

1. Layer names match `/^[a-z][a-z0-9-]*$/`. Item ids appear on the surface as `<layer>:<id>` (an item without an id gets its index), so layers never collide. Select a layer's rendered spans with `[data-te-decoration~="ghosts:g1"]`.
2. Layers merge in registration order into one `setDecorations()` call. Overlaps between layers are fine: the surface splits the text and each rendered span carries the union of the classes and every covering id. For `attributes`, the later decoration (by start, then end) wins.
3. Decorations the registry does not own (set directly on the surface, as older code and tests do) are kept.
4. Stored items follow every surface edit through the same `EditorSurface.mapRanges()` the surface uses: an edit before an item moves it, and an edit inside it drops it. The registry subscribes to `surface.onChange`, and every surface path that maps decorations reports the same edit there. So when one layer is set or cleared, the other layers are re-emitted at their live offsets, never at the offsets they were registered with, and a dropped item does not come back. `get(name)` and `current(name)` agree.
5. If the merged list equals what the surface already has (ids, offsets, classes and attributes), nothing is rendered. Recomputing a layer after typing is therefore free unless something really changed.
6. `set()` and `clear()` return `true` when the surface re-rendered.

`EditorSurface` got one small extension for this: a decoration may carry an `attributes` map, set on each of its rendered spans (used here for `aria-describedby`). Only `role`, `aria-*` and `data-*` names in plain lowercase attribute-name syntax are accepted, and `data-te-decoration` is reserved for the surface. Anything else (event handlers such as `onclick`, `style`, `href`, `class`, `id`, namespaced names) is dropped with a console warning and never throws (`EditorSurface.sanitiseAttributes()`). The registry sanitises the same way when a layer is set, so its comparison with the surface is like for like.

The ghost layer for issue #11 should call `reg.set('ghosts', ...)` from its own debounced model read, the same way `TeIndicatorLayer.refresh()` does, and style `.te-ghost` in Write_On scope.

## Script order

`index.html` and `scripts/build-dist.sh` load `indicators.js` after `chrome.js` and before `editor.js`, following the existing rule that dependencies come before the code that instantiates them. `MarkdownEditor.initialize()` creates the registry and the layer only when `window.TeDecorationRegistry` and `window.TeIndicatorLayer` exist, so the editor still works without the script.

## Lifecycle

The layer registers every listener with its own `AbortController`, which is aborted when the editor's signal aborts. `MarkdownEditor.destroy()` calls `indicators.destroy()` before destroying the surface. That clears the layer's decorations, disconnects the `ResizeObserver`, cancels pending timers and frames, and removes the overlay, the descriptions and the host class.

## Tests

- `tests/indicators_fixture.rs` (native): builds `tests/fixtures/indicators/indicators.md` through `terraphim_alternatives` and checks it against the committed file (`UPDATE_GOLDENS=1` to regenerate). The fixture has a 3-alternative headline (original active), a 7-alternative word (fifth active), a 2-alternative sentence and a 3-alternative paragraph (second active).
- `tests/web_indicators.rs` (real Chrome, real scripts and stylesheets, no mocks):
  - geometry: dot counts, lit index and colours, dot size and spacing; word row centred, headline and sentence rows right-aligned, all within 1.5px; underline style; headline weight, face and size; paragraph rule extent and dot column; the overlay does not affect layout; descriptions;
  - `te:dot` detail; a cancelled `te:dot` leaves the document unchanged (the default jump is tested in `tests/web_cycling.rs`);
  - indicators follow re-anchored spans while typing with `execCommand`, both within a frame (before the debounce) and after the model refresh, with no surface re-render;
  - nothing renders in plain mode, including the toggle cycle, unowned decorations surviving it, and `destroy()`;
  - spans with only their original get no indicator;
  - registry: merging, overlap, skipped identical renders, `batch`, `current`, `clear` and name validation.
- `tests/fixtures/visual/`: reference screenshots of the word, headline and paragraph cases from a release Trunk build (see the README there). They are reference images for review. The geometry tests are the assertions of record, because headless fonts differ between hosts.
