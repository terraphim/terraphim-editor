//! Browser tests for the span-aware editing surface (issue #4): canonical
//! DOM, selection offset mapping, native typing, paste, IME composition and
//! decorations (edit-location resolution is in `web_edit_location.rs`). Run
//! with `wasm-pack test --headless --chrome`. Real scripts, real DOM, real
//! native editing commands; nothing is mocked. See `tests/web.rs` for why the
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
