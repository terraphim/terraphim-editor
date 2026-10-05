# In-place cycling of alternatives (issue #9)

Spec: `docs/requirements/alternative-control.md` R-2.4 (in-place cycling), R-2.6 (a/an fix-up), R-3.3 (lit dot), R-3.6 (hover target, dot click), R-7.4 (shortcut collisions).

## Controls

All controls work in Write_On mode only, where the indicators are drawn; in plain mode the spans are invisible and every key and click behaves as it does without this feature.

| Control | Effect |
|---|---|
| Pointer over an indicated span's text, then ArrowDown / ArrowUp | Next / previous alternative |
| Caret (or the whole selection) inside an indicated span, or at either edge of it, then Alt+ArrowDown / Alt+ArrowUp | Next / previous alternative (keyboard-only users) |
| Click dot *i* | Alternative *i* (the lit dot does nothing) |

- **Wrap-around.** Cycling wraps at both ends: ArrowDown on the last alternative returns to the original (the first dot), and ArrowUp on the original goes to the last. The dots read as a ring, and the author never hits a dead key while browsing candidates in context.
- **Hover target (R-3.6).** The hover target is the span text, not the dots. `TeIndicatorLayer` listens for `mouseover` and `mousemove` on the surface, keeps the pointer's last client coordinates, and reads the `indicators:<id>` token from the rendered decoration's `data-te-decoration` (walking out through nested decorations such as Lab marks). `mouseleave` on the surface, window `blur`, or moving onto text outside any indicated span ends the hover.
- **Revalidation.** Typing, undo, caret moves, scrolling and re-layout can move the span out from under a still pointer without any pointer event. So before a plain arrow is taken, the layer checks with `document.elementsFromPoint` at the last pointer coordinates that the pointer is still over the hovered span's rendered text; if not, the hover is dropped and the arrow is left to the browser. The one exception is a swap of the hovered span itself: the hover is **held** by span id across that re-render (the new alternative may be shorter and no longer reach the pointer, and no new `mouseover` fires until the pointer moves), so repeated presses keep cycling. Moving the pointer, scrolling, or any other surface change ends the hold.
- **Arrows without hover.** With nothing hovered, plain ArrowUp/ArrowDown are not touched (no `preventDefault`), so caret movement is the browser's. Shift+Arrow (selection), Ctrl/Cmd+Arrow and arrows while the `/` command menu is open are never taken. Alt+Arrow is taken only when the selection lies within an indicated span.
- **Dot click.** The cancelable `te:dot` event (see [indicators.md](indicators.md)) is dispatched first; its default action jumps to that alternative. A listener that calls `preventDefault()` keeps the document unchanged.

### Shortcut collisions (R-7.4)

The editor shortcuts in `public/js/config.js` / `editor_config.toml` are all `ctrl+<letter>` (bold, italic, code, link, heading) and the R-7.3 selection shortcuts are Ctrl+Shift+A/G/X and Ctrl+/, so neither plain arrows nor Alt+ArrowUp/Alt+ArrowDown collide with anything in the app. Alt+Arrow does pre-empt the platform's own binding (on macOS, Option+Arrow moves to the start or end of a paragraph), but only while the caret is inside an indicated span in Write_On mode; everywhere else it is untouched. Alt+Arrow was preferred to Ctrl+Shift+Arrow, which extends the selection by word or paragraph on most platforms and is far more frequently used.

## Model first

`MarkdownEditor.swapAlternative(spanId, index)` (the one call every control uses):

1. Aligns the model body with the surface text (as `moveRange` does).
2. Calls the bridge export `set_active_alternative(span_id, index)` (`src/document.rs`), which runs the crate's `Document::set_active`: it swaps the span text, applies the a/an fix-up to an immediately preceding article and refreshes the anchors' context, atomically. An unknown span, an invalid index or a stale anchor throws and nothing changes, on either side.
3. The export returns the exact text edit it made, `{ start, deletedLength, deletedText, insertedText }` in UTF-16 code units, as **one contiguous edit** covering the article (when it changed) and the span text: the bodies before and after agree up to the first difference (never past the span start) and after the span. It also returns `range`, the span's text afterwards.
4. The surface applies that edit with `replaceRange(..., { source: 'swap', swap: { span, from, to } })` under `suppressModelSync`, so it is not mirrored back through `apply_edit` (which would detach the span). It is recorded as ONE undo step.
5. The indicator layer is flushed at once, so the lit dot (R-3.3) and the accessible description update without waiting for the debounce.

Making the active alternative active again returns `edit: null` and records nothing; the article is left alone in that case.

The selection is kept where it was: offsets before the edit stay, offsets after it move with the text, and an offset inside the edited region is clamped into the span's new text, so a keyboard user's caret stays in the span for the next Alt+Arrow.

## Undo and redo

The history step carries `swap: { span, from, to }`, extended in the same places as the text-move field from #44: `replaceRange` copies it onto the edit, `record` onto the step (and a swap always gets its own history entry), `invertStep` swaps `from` and `to`, and `replaySteps` copies it back onto the replayed edit. When `mirrorEdit` sees a replayed edit with `swap`, it calls `set_active_alternative(span, to)` instead of `apply_edit`, so undo and redo restore the alternative, the article and every annotation (spans, ghosts, context) through the model, not as a plain text edit.

After replaying, the model body is compared with the surface text. If the model refused the swap or produced a different body, the body is re-synced from the surface (`sync_document_body`), so the two never diverge. The case this covers: the fix-up derives the article purely from the alternative's text, so if the author had typed an article against the rule ("a eraser" with "eraser" the original), swapping to "thumbtack" leaves "a", and undoing the swap restores "a eraser" on the surface while `set_active` in the model would write "an eraser"; the re-sync makes the surface authoritative and re-anchors by text.

## Tests

- Native (`src/document.rs`, `swap_*` and `a_refused_swap_changes_nothing`): the exact combined edit and the a/an sequence (`a paperclip` → `an eraser` → `a thumbtack` → back, saving the original file again); context and ghost refreshed and the active index surviving save and reopen; unknown id and invalid index refused with the model unchanged; the no-op swap; UTF-16 offsets with a non-BMP character; set-aside hints shifting and re-attaching.
- Browser (`tests/web_cycling.rs`, real Chrome, real scripts, no mocks): hover plus ArrowDown/ArrowUp with wrap-around, one undo step each, the hover kept across re-renders, unhovered and modified arrows left to the browser; a stale hover (pointer moved onto plain text, left the surface, window blur, or the span shifted away by typing) never takes a plain arrow, while repeated swaps under a still pointer keep working even when the new text no longer reaches it; the a/an demo sequence with undo, redo and typing afterwards, saving the original file again; dot click jumps, the lit dot does nothing, a cancelled `te:dot` does nothing; Alt+Arrow with the caret inside or at the end of the span, not outside it, not over a wider selection, not in plain mode; save and reopen keep the active index; refused swaps change and record nothing.

Synthetic key events cannot move the caret, so "Up/Down without hover keeps normal caret movement" is asserted as "the event is not default-prevented and nothing changes", which is exactly the condition under which the browser moves the caret.
