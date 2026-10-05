//! Browser tests for the span-aware editing surface (issue #4): canonical
//! DOM, selection offset mapping, native typing, paste, IME composition,
//! decorations and edit-location resolution in repeated text. Run with
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
fn test_surface_initial_state_is_canonical() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          if (!s.getText().startsWith('# Welcome to Markdown Editor!')) out.push('initial text');
          if (!s.getText().endsWith('\n')) out.push('initial text should end with a newline');
          if (!teTest.canonical()) out.push('not canonical');
          const last = s.root.lastChild;
          if (!(last && last.tagName === 'BR' && last.classList.contains('te-placeholder'))) out.push('placeholder br missing');
          if (s.root.getAttribute('role') !== 'textbox') out.push('role');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_selection_offset_mapping_unicode_and_paragraphs() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const text = 'First paragraph line\nsecond line\n\nEmoji \u{1F600} and family \u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467} here\n' +
                       'Café and naïve and क्ष\n\nLast ❤️\n';
          s.setText(text);
          if (s.getText() !== text) out.push('setText round trip');
          const isHigh = (c) => c >= 0xd800 && c <= 0xdbff;
          const snap = (o) => (o > 0 && o < text.length && isHigh(text.charCodeAt(o - 1)) && !isHigh(text.charCodeAt(o))) ? o - 1 : o;
          const check = () => {
            for (let o = 0; o <= text.length; o++) {
              s.setSelectionOffsets(o);
              const got = s.getSelectionOffsets();
              const want = snap(o);
              if (got.start !== want || got.end !== want) { out.push('offset ' + o + ' -> ' + JSON.stringify(got)); continue; }
              // Independent check against the DOM: text before the DOM caret.
              const sel = getSelection();
              const r = document.createRange();
              r.setStart(s.root, 0);
              r.setEnd(sel.focusNode, sel.focusOffset);
              if (r.toString() !== text.slice(0, want)) out.push('dom prefix mismatch at ' + o);
            }
            // Ranges spanning paragraphs, emoji and combining marks, both directions.
            const e = text.indexOf('\u{1F600}');
            const c = text.indexOf('é');
            const pairs = [[0, 21], [10, 40], [e, e + 2], [c, c + 2], [5, text.length], [e - 3, c + 2]];
            for (const [a, b] of pairs) {
              s.setSelectionOffsets(a, b);
              let got = s.getSelectionOffsets();
              if (got.start !== a || got.end !== b || got.direction !== 'forward') out.push('range ' + a + '-' + b + ' ' + JSON.stringify(got));
              if (getSelection().toString() !== text.slice(a, b)) out.push('selected text ' + a + '-' + b);
              s.setSelectionOffsets(b, a);
              got = s.getSelectionOffsets();
              if (got.start !== a || got.end !== b || got.direction !== 'backward') out.push('backward ' + a + '-' + b + ' ' + JSON.stringify(got));
            }
            // An offset inside a surrogate pair snaps to the pair start.
            s.setSelectionOffsets(e + 1, e + 1);
            if (s.getSelectionOffsets().start !== e) out.push('surrogate snap');
          };
          check();
          // The same mapping must hold when decoration spans split the text.
          const e = text.indexOf('\u{1F600}');
          const c = text.indexOf('Café');
          const decorations = s.setDecorations([
            { start: e, end: e + 2, className: 'te-test-a' },
            { start: c, end: c + 5, className: 'te-test-b' },
            { start: c + 3, end: c + 12, className: 'te-test-c' },
            { start: 15, end: 30 },
          ]);
          if (decorations.length !== 4) out.push('decoration count ' + decorations.length);
          if (s.getText() !== text || !teTest.canonical()) out.push('decorated text not canonical');
          const spans = Array.from(s.root.querySelectorAll('span.te-decoration'));
          if (spans.length < 5) out.push('expected split spans, got ' + spans.length);
          const emojiSpan = spans.find((sp) => sp.classList.contains('te-test-a'));
          if (!emojiSpan || emojiSpan.textContent !== '\u{1F600}') out.push('emoji span text');
          const cafe = spans.filter((sp) => sp.classList.contains('te-test-b')).map((sp) => sp.textContent).join('');
          if (cafe !== 'Café') out.push('combining span text ' + JSON.stringify(cafe));
          const overlap = spans.find((sp) => sp.classList.contains('te-test-b') && sp.classList.contains('te-test-c'));
          if (!overlap || overlap.getAttribute('data-te-decoration').split(' ').length !== 2) out.push('overlap span');
          check();
          return out.slice(0, 20).join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_browser_dom_shapes_read_back_as_plain_text() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          // Block-per-line shape, as produced by contenteditable="true" engines.
          s.root.innerHTML = '<div>one</div><div>two \u{1F600}</div><div><br></div><div>é</div>';
          let r = s.serialise();
          if (r.text !== 'one\ntwo \u{1F600}\n\né') out.push('div shape ' + JSON.stringify(r.text));
          const second = s.root.childNodes[1].firstChild;
          if (s.pointToOffset(second, 4) !== 8) out.push('div point ' + s.pointToOffset(second, 4));
          if (s.pointToOffset(s.root.childNodes[3].firstChild, 2) !== 14) out.push('combining point');
          if (s.pointToOffset(s.root.childNodes[3].firstChild, 1) !== 13) out.push('between base and mark');
          // An input event normalises it to the canonical shape and updates the preview.
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
          if (s.getText() !== 'one\ntwo \u{1F600}\n\né') out.push('normalised text ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('not canonical after input');
          // The Rust preview must see the normalised text: two paragraphs only
          // appear if it read "para one\n\npara two" after the capture-phase
          // sync, not the raw textContent "para onepara two".
          s.root.innerHTML = '<div>para one</div><div><br></div><div>para two</div>';
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText' }));
          if (!teTest.preview().includes('<p>para one</p>') || !teTest.preview().includes('<p>para two</p>')) {
            out.push('preview read un-normalised DOM: ' + teTest.preview());
          }
          // <br> shape.
          s.root.innerHTML = 'a<br>b<br>';
          r = s.serialise();
          if (r.text !== 'a\nb') out.push('br shape ' + JSON.stringify(r.text));
          s.root.innerHTML = 'a<br><br>';
          if (s.serialise().text !== 'a\n') out.push('double br shape ' + JSON.stringify(s.serialise().text));
          if (s.pointToOffset(s.root, 2) !== 2) out.push('element point');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_native_typing_newlines_and_paste() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          s.setText('Hello');
          s.focus();
          s.setSelectionOffsets(5);
          // Native typing pipeline (beforeinput/input fired by the browser).
          const renders = s.renderCount;
          if (!document.execCommand('insertText', false, ' world')) out.push('execCommand failed');
          if (s.renderCount !== renders) out.push('native typing rebuilt the DOM');
          if (s.getText() !== 'Hello world') out.push('typed ' + JSON.stringify(s.getText()));
          if (!teTest.preview().includes('Hello world')) out.push('preview after typing');
          if (!teTest.canonical()) out.push('not canonical after typing');
          // Enter is handled as a literal newline.
          const enter = new InputEvent('beforeinput', { inputType: 'insertParagraph', bubbles: true, cancelable: true });
          s.root.dispatchEvent(enter);
          if (!enter.defaultPrevented) out.push('enter not handled');
          if (s.getText() !== 'Hello world\n') out.push('enter text ' + JSON.stringify(s.getText()));
          if (!(s.root.lastChild && s.root.lastChild.tagName === 'BR')) out.push('placeholder after trailing newline');
          if (s.getSelectionOffsets().start !== 12) out.push('caret after enter');
          document.execCommand('insertText', false, 'next');
          if (s.getText() !== 'Hello world\nnext') out.push('typed after enter ' + JSON.stringify(s.getText()));
          if (!teTest.canonical()) out.push('not canonical after enter');
          // Paste strips formatting and normalises line endings.
          const dt = new DataTransfer();
          dt.setData('text/html', '<b>rich</b><img src=x>');
          dt.setData('text/plain', 'p1\r\np2\rp3');
          s.setSelectionOffsets(0);
          const paste = new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true });
          s.root.dispatchEvent(paste);
          if (!paste.defaultPrevented) out.push('paste not cancelled');
          if (s.getText() !== 'p1\np2\np3Hello world\nnext') out.push('paste text ' + JSON.stringify(s.getText()));
          if (s.root.querySelector('b, img')) out.push('rich content inserted');
          if (s.getSelectionOffsets().start !== 8) out.push('caret after paste');
          // Copy produces plain text with newlines.
          const copyData = new DataTransfer();
          s.setSelectionOffsets(0, 5);
          s.root.dispatchEvent(new ClipboardEvent('copy', { clipboardData: copyData, bubbles: true, cancelable: true }));
          if (copyData.getData('text/plain') !== 'p1\np2') out.push('copy ' + JSON.stringify(copyData.getData('text/plain')));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_ime_composition_is_not_corrupted() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          s.setText('ab');
          s.focus();
          s.setSelectionOffsets(1);
          const historyBefore = s.history.length;
          const node = s.root.firstChild;
          s.root.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true, data: '' }));
          // Interim composition text, written into the DOM as an IME would.
          node.insertData(1, 'ni');
          s.root.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true, inputType: 'insertCompositionText', data: 'ni' }));
          if (s.root.firstChild !== node) out.push('DOM re-rendered during composition');
          if (s.getText() !== 'ab') out.push('model changed during composition');
          if (s.history.length !== historyBefore) out.push('history recorded during composition');
          // Shortcuts are ignored while composing.
          if (teTest.key(s.root, 'b', { ctrlKey: true, isComposing: true })) out.push('shortcut fired while composing');
          if (teTest.key(s.root, '/', { isComposing: true })) out.push('palette fired while composing');
          // Commit the final text.
          node.replaceData(1, 2, '你');
          s.root.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true, data: '你' }));
          if (s.getText() !== 'a你b') out.push('committed ' + JSON.stringify(s.getText()));
          if (s.history.length !== historyBefore + 1) out.push('expected one history entry');
          if (!teTest.canonical()) out.push('not canonical after composition');
          s.undo();
          if (s.getText() !== 'ab') out.push('undo composition ' + JSON.stringify(s.getText()));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_decorations_follow_edits() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const changes = [];
          const off = s.onChange((c) => changes.push(c));
          s.setText('alpha beta gamma');
          s.setDecorations([
            { id: 'b', start: 6, end: 10, className: 'te-test-alt' },
            { id: 'g', start: 11, end: 16, data: { note: 'g' } },
          ]);
          const texts = () => Array.from(s.root.querySelectorAll('span.te-decoration')).map((sp) => sp.getAttribute('data-te-decoration') + ':' + sp.textContent).join(',');
          if (texts() !== 'b:beta,g:gamma') out.push('initial spans ' + texts());
          if (s.decorationsAt(7).map((d) => d.id).join() !== 'b') out.push('decorationsAt');
          // An edit before the decorations shifts them.
          s.replaceRange(0, 0, 'X ');
          let d = s.getDecorations();
          if (d.length !== 2 || d[0].start !== 8 || d[0].end !== 12 || d[1].start !== 13) out.push('shifted ' + JSON.stringify(d));
          if (texts() !== 'b:beta,g:gamma') out.push('spans after shift ' + texts());
          // An edit inside a decoration drops it; later ones still shift.
          s.replaceRange(10, 10, 'ZZ');
          d = s.getDecorations();
          if (d.length !== 1 || d[0].id !== 'g' || d[0].start !== 15) out.push('dropped ' + JSON.stringify(d));
          // Native typing straight after a decoration does not extend it.
          s.focus();
          s.setSelectionOffsets(20);
          document.execCommand('insertText', false, '!');
          if (s.getText() !== 'X alpha beZZta gamma!') out.push('typed ' + JSON.stringify(s.getText()));
          if (texts() !== 'g:gamma') out.push('spans after typing ' + texts());
          if (!teTest.canonical()) out.push('not canonical');
          if (changes.length < 4) out.push('change hook calls ' + changes.length);
          const lastEdit = changes[changes.length - 1].edit;
          if (lastEdit.start !== 20 || lastEdit.insertedText !== '!' || lastEdit.deletedLength !== 0) out.push('edit ' + JSON.stringify(lastEdit));
          off();
          s.clearDecorations();
          if (s.root.querySelector('span')) out.push('clear');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

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
