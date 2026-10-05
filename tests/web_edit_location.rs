//! Browser tests for edit-location resolution on the editing surface (issue
//! #4): native edits and `beforeinput` location hints in repeated text, the
//! fallbacks when no hint applies, and stale or cancelled hints. Run with
//! `wasm-pack test --headless --chrome`. Real scripts, real DOM, real native
//! editing commands; nothing is mocked. See `tests/web.rs` for why the
//! browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_native_edits_in_repeated_text_use_the_edit_location() {
    let _document = fresh_full_editor();
    // execCommand fires no beforeinput, so these exercise the location
    // derived from the pre-edit selection checked against the post-edit
    // caret. The plain content diff would attribute every one of these
    // edits to the end of the repeated run.
    let result = js_string(
        r##"(() => {
          const out = [];
          const ins = (t) => () => document.execCommand('insertText', false, t);
          let r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, ins('a'));
          out.push(...teTest.expectEdit('insert a in aaaa', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          r = teTest.editCase('abab', [{ id: 'd', start: 2, end: 4 }], 2, 2, ins('ab'));
          out.push(...teTest.expectEdit('insert ab in abab', r, {
            text: 'ababab', start: 2, deletedLength: 0, insertedText: 'ab', decorations: [['d', 4, 6]], caret: 4,
          }));
          r = teTest.editCase('one one one', [{ id: 'd', start: 8, end: 11 }], 4, 4, ins('one '));
          out.push(...teTest.expectEdit('insert repeated word', r, {
            text: 'one one one one', start: 4, deletedLength: 0, insertedText: 'one ', decorations: [['d', 12, 15]], caret: 8,
          }));
          // Backward delete of one of several identical characters.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, () => document.execCommand('delete'));
          out.push(...teTest.expectEdit('backward delete in aaaa', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // Forward delete of one of several identical characters.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 1, 1, () => document.execCommand('forwardDelete'));
          out.push(...teTest.expectEdit('forward delete in aaaa', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // Backward delete of a repeated word.
          r = teTest.editCase('ab ab ab', [{ id: 'd', start: 6, end: 8 }], 3, 6, () => document.execCommand('delete'));
          out.push(...teTest.expectEdit('delete repeated word', r, {
            text: 'ab ab', start: 3, deletedLength: 3, insertedText: '', decorations: [['d', 3, 5]], caret: 3,
          }));
          // Replacement over a selection: the replaced character's decoration
          // is dropped, the later one shifts.
          r = teTest.editCase('aaaa', [{ id: 'x', start: 1, end: 2 }, { id: 'd', start: 3, end: 4 }], 1, 2, ins('aa'));
          out.push(...teTest.expectEdit('replace selection in aaaa', r, {
            text: 'aaaaa', start: 1, deletedLength: 1, insertedText: 'aa', decorations: [['d', 4, 5]], caret: 3,
          }));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_beforeinput_location_hints_in_repeated_text() {
    let _document = fresh_full_editor();
    // Real `beforeinput` events (as the browser fires for keyboard input)
    // followed by the native edit itself. The surface records the pre-edit
    // range from the event and uses it when the edit's `input` arrives.
    let result = js_string(
        r##"(() => {
          const out = [];
          const before = (s, inputType, data, range) => {
            const init = { inputType, data, bubbles: true, cancelable: true };
            if (range) {
              const a = s.offsetToPoint(range[0]);
              const b = s.offsetToPoint(range[1]);
              init.targetRanges = [new StaticRange({ startContainer: a.node, startOffset: a.offset, endContainer: b.node, endOffset: b.offset })];
            }
            const ev = new InputEvent('beforeinput', init);
            s.root.dispatchEvent(ev);
            if (ev.defaultPrevented) out.push(inputType + ' unexpectedly cancelled');
          };
          // Exact target range [1, 2): a replacement in repeated text.
          let r = teTest.editCase('aaaa', [{ id: 'x', start: 1, end: 2 }, { id: 'd', start: 3, end: 4 }], 1, 2, (s) => {
            before(s, 'insertText', 'aa', [1, 2]);
            document.execCommand('insertText', false, 'aa');
          });
          out.push(...teTest.expectEdit('target range replacement', r, {
            text: 'aaaaa', start: 1, deletedLength: 1, insertedText: 'aa', decorations: [['d', 4, 5]], caret: 3,
          }));
          // Exact target range for a backward delete in repeated text.
          r = teTest.editCase('abab', [{ id: 'd', start: 2, end: 4 }], 2, 2, (s) => {
            before(s, 'deleteContentBackward', null, [1, 2]);
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('target range delete', r, {
            text: 'aab', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 1, 3]], caret: 1,
          }));
          // No target range: the selection is the fallback. For a backward
          // delete from a caret it does not cover the deleted character, so
          // it is widened backwards by the deletion length.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, (s) => {
            before(s, 'deleteContentBackward', null, null);
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('selection hint backward delete', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // No target range, insertion: the selection hint is consistent.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, (s) => {
            before(s, 'insertText', 'a', null);
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('selection hint insert', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          // A hint is only used by the edit it was recorded for: a (wrong)
          // target range recorded before a programmatic change is ignored by
          // the next native edit.
          r = teTest.editCase('aaaa', [], 1, 1, (s) => {
            before(s, 'insertText', 'a', [3, 3]);
            s.replaceRange(4, 4, 'a');
            s.setSelectionOffsets(1);
            document.execCommand('insertText', false, 'a');
          });
          if (r.text !== 'aaaaaa') out.push('stale hint text ' + JSON.stringify(r.text));
          const last = r.changes[r.changes.length - 1];
          if (r.changes.length !== 2 || last.start !== 1) out.push('stale hint used ' + JSON.stringify(r.changes));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_edit_location_fallbacks() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const s = teTest.surface();
          // External mutation with no selection in the surface and a newer DOM
          // selection pending: no location is known, so the plain diff is used
          // and the model still matches the DOM.
          s.setText('aaaa');
          s.focus();
          getSelection().setBaseAndExtent(s.root.firstChild, 3, s.root.firstChild, 3);
          document.dispatchEvent(new Event('selectionchange'));
          if (!s.pendingSelection) out.push('selectionchange not recorded');
          getSelection().removeAllRanges();
          const changes = [];
          const off = s.onChange((c) => changes.push(c.edit));
          s.root.firstChild.insertData(1, 'a');
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
          off();
          if (s.getText() !== 'aaaaa') out.push('fallback text ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('fallback not canonical');
          if (changes.length !== 1) out.push('fallback changes ' + changes.length);
          else if (changes[0].insertedText !== 'a' || changes[0].deletedLength !== 0) out.push('fallback edit ' + JSON.stringify(changes[0]));
          // A newer DOM selection is pending, so the last known selection is
          // not trusted: the post-edit caret read from the DOM decides.
          let r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, () => {
            document.dispatchEvent(new Event('selectionchange'));
            if (!s.pendingSelection) out.push('caret case: selectionchange not recorded');
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('caret only insert', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, () => {
            document.dispatchEvent(new Event('selectionchange'));
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('caret only delete', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // The hinted diff always reproduces the new text, even for a hint
          // that does not match the change.
          const cases = [
            ['aaaa', 'aaaaa', { start: 3, end: 3 }],
            ['abab', 'aabb', { start: 0, end: 4 }],
            ['aaaa', 'aaa', { caret: 0 }],
            ['x\u{1F600}\u{1F600}y', 'x\u{1F600}y', { caret: 2 }],
            ['hello', 'help', { start: 9, end: 12 }],
          ];
          for (const [a, b, hint] of cases) {
            const e = EditorSurface.diff(a, b, hint);
            const applied = a.slice(0, e.start) + e.insertedText + a.slice(e.start + e.deletedLength);
            if (applied !== b) out.push('diff ' + JSON.stringify([a, b, hint]) + ' gave ' + JSON.stringify(e));
            if (e.deletedText !== a.slice(e.start, e.start + e.deletedLength)) out.push('deletedText ' + JSON.stringify(e));
          }
          // A surrogate pair is never split by a hinted diff.
          const e = EditorSurface.diff('x\u{1F600}\u{1F600}y', 'x\u{1F600}y', { caret: 2 });
          if (e.start !== 1 && e.start !== 3) out.push('surrogate split ' + JSON.stringify(e));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_cancelled_or_stale_beforeinput_hint_is_not_reused() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const out = [];
          const before = (s, inputType, range) => {
            const a = s.offsetToPoint(range[0]);
            const b = s.offsetToPoint(range[1]);
            const ev = new InputEvent('beforeinput', {
              inputType, bubbles: true, cancelable: true,
              targetRanges: [new StaticRange({ startContainer: a.node, startOffset: a.offset, endContainer: b.node, endOffset: b.offset })],
            });
            s.root.dispatchEvent(ev);
            return ev;
          };
          // A later listener cancels the beforeinput, so no input follows it.
          const cancel = (e) => e.preventDefault();
          // 1. Cancelled insertText with a target range at 3; the real edit
          // is an execCommand insertion at 1 (no beforeinput of its own).
          let r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, (s) => {
            s.root.addEventListener('beforeinput', cancel);
            const ev = before(s, 'insertText', [3, 3]);
            s.root.removeEventListener('beforeinput', cancel);
            if (!ev.defaultPrevented) out.push('beforeinput was not cancelled');
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('cancelled insert hint', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          // 2. Cancelled backward delete at [0, 1); the real delete is at [1, 2).
          r = teTest.editCase('aaaa', [{ id: 'd', start: 3, end: 4 }], 2, 2, (s) => {
            s.root.addEventListener('beforeinput', cancel);
            before(s, 'deleteContentBackward', [0, 1]);
            s.root.removeEventListener('beforeinput', cancel);
            document.execCommand('delete');
          });
          out.push(...teTest.expectEdit('cancelled delete hint', r, {
            text: 'aaa', start: 1, deletedLength: 1, insertedText: '', decorations: [['d', 2, 3]], caret: 1,
          }));
          // 3. A selectionchange after the beforeinput makes its hint stale.
          r = teTest.editCase('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1, (s) => {
            before(s, 'insertText', [3, 3]);
            document.dispatchEvent(new Event('selectionchange'));
            if (s.pendingHint !== null) out.push('selectionchange kept the hint');
            document.execCommand('insertText', false, 'a');
          });
          out.push(...teTest.expectEdit('stale hint after selectionchange', r, {
            text: 'aaaaa', start: 1, deletedLength: 0, insertedText: 'a', decorations: [['d', 3, 5]], caret: 2,
          }));
          // 4. A hint is consumed by its input: it cannot serve a second edit.
          r = teTest.editCase('aaaa', [], 1, 1, (s) => {
            before(s, 'insertText', [1, 1]);
            document.execCommand('insertText', false, 'a');
            if (s.pendingHint !== null) out.push('hint not consumed');
          });
          if (r.text !== 'aaaaa' || r.changes.length !== 1 || r.changes[0].start !== 1) out.push('consumed hint edit ' + JSON.stringify(r.changes));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
