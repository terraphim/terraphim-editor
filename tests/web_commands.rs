//! Browser tests for editor commands on the editing surface: keyboard
//! shortcuts, toolbar buttons, the slash command palette and undo/redo. Run
//! with `wasm-pack test --headless --chrome`. Real scripts, real DOM, real
//! native editing commands; nothing is mocked. See `tests/web.rs` for why
//! the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

// The shared module re-exports wasm-bindgen, wasm-bindgen-test, web-sys
// types and the editor API used here.
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn test_every_shortcut_wraps_selection_on_surface() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const shortcuts = window.EditorConfig.shortcuts;
          if (shortcuts.length < 5) out.push('expected the five configured shortcuts');
          for (const sc of shortcuts) {
            const key = sc.key.split('+').pop();
            // Non-empty selection: wrap it and keep it selected.
            s.setText('hello world');
            s.setSelectionOffsets(6, 11);
            const prevented = teTest.key(s.root, key, { ctrlKey: true });
            const want = 'hello ' + sc.prefix + 'world' + sc.suffix;
            if (!prevented) out.push(sc.key + ': default not prevented');
            if (s.getText() !== want) out.push(sc.key + ': got ' + JSON.stringify(s.getText()));
            const sel = s.getSelectionOffsets();
            if (sel.start !== 6 + sc.prefix.length || sel.end !== 11 + sc.prefix.length) {
              out.push(sc.key + ': selection ' + JSON.stringify(sel));
            }
            if (!teTest.canonical()) out.push(sc.key + ': not canonical');
            // Empty selection: insert a selected placeholder word.
            s.setText('ab');
            s.setSelectionOffsets(1);
            teTest.key(s.root, key, { ctrlKey: true });
            const want2 = 'a' + sc.prefix + 'text' + sc.suffix + 'b';
            if (s.getText() !== want2) out.push(sc.key + ' (empty): got ' + JSON.stringify(s.getText()));
            const sel2 = s.getSelectionOffsets();
            if (sel2.start !== 1 + sc.prefix.length || sel2.end !== 5 + sc.prefix.length) {
              out.push(sc.key + ' (empty): selection ' + JSON.stringify(sel2));
            }
          }
          // The Rust preview must follow shortcut edits.
          s.setText('hello world');
          s.setSelectionOffsets(6, 11);
          teTest.key(s.root, 'b', { ctrlKey: true });
          if (!teTest.preview().includes('<strong>world</strong>')) out.push('preview not updated: ' + teTest.preview());
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_toolbar_buttons_use_last_selection() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          const buttons = document.querySelectorAll('#formatting-toolbar sl-button');
          const shortcuts = window.EditorConfig.shortcuts;
          if (buttons.length !== shortcuts.length) out.push('button count ' + buttons.length);
          buttons.forEach((button, i) => {
            const sc = shortcuts[i];
            s.setText('one two');
            s.setSelectionOffsets(4, 7);
            // Move the live selection out of the surface, as a toolbar click can.
            getSelection().removeAllRanges();
            button.focus();
            button.click();
            const want = 'one ' + sc.prefix + 'two' + sc.suffix;
            if (s.getText() !== want) out.push('button ' + i + ': ' + JSON.stringify(s.getText()));
          });
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_slash_command_palette_on_surface() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const menu = teTest.menu();
          const out = [];
          // '/' inserts a slash and opens the palette at the caret.
          s.setText('Intro\n');
          s.setSelectionOffsets(6);
          if (!teTest.key(s.root, '/')) out.push('slash default not prevented');
          if (s.getText() !== 'Intro\n/') out.push('slash text ' + JSON.stringify(s.getText()));
          if (menu.style.display !== 'block') out.push('menu not shown');
          if (menu.style.position !== 'fixed') out.push('menu not positioned');
          // Keyboard: ArrowDown selects the second command, Enter applies it.
          teTest.key(menu, 'ArrowDown');
          const selected = menu.querySelectorAll('.command-item.selected');
          if (selected.length !== 1 || selected[0] !== menu.querySelectorAll('.command-item')[1]) out.push('selection highlight');
          teTest.key(menu, 'Enter');
          const cmd = window.EditorConfig.commands[1];
          const want = 'Intro\n' + cmd.prefix + 'text' + cmd.suffix;
          if (s.getText() !== want) out.push('enter applied ' + JSON.stringify(s.getText()));
          if (menu.style.display !== 'none') out.push('menu not hidden after Enter');
          if (!teTest.preview().includes('<h2>text</h2>')) out.push('preview ' + teTest.preview());
          // Escape closes the palette and keeps the literal slash.
          s.setText('a');
          s.setSelectionOffsets(1);
          teTest.key(s.root, '/');
          teTest.key(menu, 'Escape');
          if (s.getText() !== 'a/') out.push('escape text ' + JSON.stringify(s.getText()));
          if (menu.style.display !== 'none') out.push('menu not hidden after Escape');
          // Every command, applied by click.
          const items = menu.querySelectorAll('.command-item');
          window.EditorConfig.commands.forEach((c, i) => {
            s.setText('x ');
            s.setSelectionOffsets(2);
            teTest.key(s.root, '/');
            items[i].click();
            const w = 'x ' + c.prefix + 'text' + c.suffix;
            if (s.getText() !== w) out.push(c.name + ': ' + JSON.stringify(s.getText()));
            if (!teTest.canonical()) out.push(c.name + ': not canonical');
          });
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_undo_redo_stack() {
    let _document = fresh_full_editor();
    let result = js_string(
        r##"(() => {
          const s = teTest.surface();
          const out = [];
          s.setText('base');
          s.focus();
          s.setSelectionOffsets(4);
          // A burst of typing coalesces into one undo step.
          document.execCommand('insertText', false, ' a');
          document.execCommand('insertText', false, 'b');
          document.execCommand('insertText', false, 'c');
          if (s.getText() !== 'base abc') out.push('typed ' + JSON.stringify(s.getText()));
          s.setSelectionOffsets(5, 8);
          teTest.key(s.root, 'b', { ctrlKey: true });
          if (s.getText() !== 'base **abc**') out.push('bold ' + JSON.stringify(s.getText()));
          if (!teTest.key(s.root, 'z', { ctrlKey: true })) out.push('ctrl+z not prevented');
          if (s.getText() !== 'base abc') out.push('undo 1 ' + JSON.stringify(s.getText()));
          if (teTest.preview().includes('<strong>')) out.push('preview did not follow undo');
          teTest.key(s.root, 'z', { ctrlKey: true });
          if (s.getText() !== 'base') out.push('undo 2 ' + JSON.stringify(s.getText()));
          teTest.key(s.root, 'z', { ctrlKey: true, shiftKey: true });
          if (s.getText() !== 'base abc') out.push('redo 1 ' + JSON.stringify(s.getText()));
          teTest.key(s.root, 'y', { ctrlKey: true });
          if (s.getText() !== 'base **abc**') out.push('redo 2 ' + JSON.stringify(s.getText()));
          if (!teTest.preview().includes('<strong>abc</strong>')) out.push('preview did not follow redo');
          // Undo from the browser's edit menu arrives as beforeinput historyUndo.
          const ev = new InputEvent('beforeinput', { inputType: 'historyUndo', bubbles: true, cancelable: true });
          s.root.dispatchEvent(ev);
          if (!ev.defaultPrevented || s.getText() !== 'base abc') out.push('historyUndo ' + JSON.stringify(s.getText()));
          // A new edit discards the redo branch.
          s.replaceRange(0, 0, 'X');
          if (s.redo()) out.push('redo branch not discarded');
          if (!s.undo() || s.getText() !== 'base abc') out.push('undo after new edit');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_undo_redo_replays_exact_edits_in_repeated_text() {
    let _document = fresh_full_editor();
    // Undo and redo must reverse/replay the recorded edit, not a re-diff of
    // the text, so decorations after the edit and the caret land correctly.
    let result = js_string(
        r##"(() => {
          const out = [];
          const s = teTest.surface();
          const state = () => JSON.stringify({
            text: s.getText(),
            decorations: s.getDecorations().map((d) => [d.id, d.start, d.end]),
            caret: [s.getSelectionOffsets().start, s.getSelectionOffsets().end],
          });
          const want = (text, decorations, caret) => JSON.stringify({ text, decorations, caret: [caret, caret] });
          const check = (name, expected) => {
            const got = state();
            if (got !== expected) out.push(name + ': ' + got);
            if (!teTest.canonical()) out.push(name + ': not canonical');
          };
          // Fresh history so coalescing cannot join a previous case.
          const setup = (text, decos, start, end) => {
            s.setText(text);
            s.history = [];
            s.historyIndex = -1;
            s.record('init', { start: 0, end: 0 });
            s.setDecorations(decos);
            s.focus();
            s.setSelectionOffsets(start, end);
          };
          const ins = (t) => document.execCommand('insertText', false, t);
          const undo = () => { if (!teTest.key(s.root, 'z', { ctrlKey: true })) out.push('ctrl+z not handled'); };
          const redo = () => { if (!teTest.key(s.root, 'z', { ctrlKey: true, shiftKey: true })) out.push('ctrl+shift+z not handled'); };

          // 1. Single insertion in "aaaa"; the undo change is reported exactly.
          setup('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1);
          ins('a');
          check('insert', want('aaaaa', [['d', 3, 5]], 2));
          const changes = [];
          const off = s.onChange((c) => changes.push(c.edit));
          undo();
          off();
          check('insert undo', want('aaaa', [['d', 2, 4]], 1));
          if (changes.length !== 1 || changes[0].start !== 1 || changes[0].deletedLength !== 1 || changes[0].insertedText !== '') {
            out.push('undo change ' + JSON.stringify(changes));
          }
          redo();
          check('insert redo', want('aaaaa', [['d', 3, 5]], 2));
          undo();
          check('insert undo again', want('aaaa', [['d', 2, 4]], 1));

          // 2. Repeated word.
          setup('one one one', [{ id: 'd', start: 8, end: 11 }], 4, 4);
          ins('one ');
          check('word', want('one one one one', [['d', 12, 15]], 8));
          undo();
          check('word undo', want('one one one', [['d', 8, 11]], 4));
          redo();
          check('word redo', want('one one one one', [['d', 12, 15]], 8));

          // 3. Merged typing burst: three keystrokes, one undo step.
          setup('aaaa', [{ id: 'd', start: 2, end: 4 }], 1, 1);
          const depth = s.history.length;
          ins('a'); ins('a'); ins('a');
          if (s.history.length !== depth + 1) out.push('burst not coalesced: ' + (s.history.length - depth));
          const top = s.history[s.historyIndex];
          if (!top.edits || top.edits.length !== 1) out.push('burst steps ' + JSON.stringify(top.edits));
          check('burst', want('aaaaaaa', [['d', 5, 7]], 4));
          undo();
          check('burst undo', want('aaaa', [['d', 2, 4]], 1));
          redo();
          check('burst redo', want('aaaaaaa', [['d', 5, 7]], 4));

          // 4. Merged burst at two separate places stays exact.
          setup('aaaa', [{ id: 'd', start: 3, end: 4 }], 1, 1);
          ins('a');
          s.setSelectionOffsets(5);
          ins('a');
          check('split burst', want('aaaaaa', [['d', 4, 5]], 6));
          if (s.history[s.historyIndex].edits.length !== 2) out.push('split burst steps ' + JSON.stringify(s.history[s.historyIndex].edits));
          undo();
          check('split burst undo', want('aaaa', [['d', 3, 4]], 1));
          redo();
          check('split burst redo', want('aaaaaa', [['d', 4, 5]], 6));

          // 5. Merged backspace burst.
          setup('aaaa', [{ id: 'd', start: 3, end: 4 }], 3, 3);
          document.execCommand('delete');
          document.execCommand('delete');
          check('backspace burst', want('aa', [['d', 1, 2]], 1));
          undo();
          check('backspace burst undo', want('aaaa', [['d', 3, 4]], 3));
          redo();
          check('backspace burst redo', want('aa', [['d', 1, 2]], 1));

          // 6. Replacement over a selection: the replaced character's
          // decoration is gone, the later one maps both ways.
          setup('aaaa', [{ id: 'x', start: 1, end: 2 }, { id: 'd', start: 3, end: 4 }], 1, 2);
          ins('aa');
          check('replace', want('aaaaa', [['d', 4, 5]], 3));
          undo();
          check('replace undo', want('aaaa', [['d', 3, 4]], 2));
          redo();
          check('replace redo', want('aaaaa', [['d', 4, 5]], 3));

          // 7. Programmatic edits (shortcuts) are replayed exactly too.
          setup('ab ab ab', [{ id: 'd', start: 6, end: 8 }], 3, 5);
          teTest.key(s.root, 'b', { ctrlKey: true });
          if (s.getText() !== 'ab **ab** ab') out.push('bold ' + JSON.stringify(s.getText()));
          undo();
          check('bold undo', want('ab ab ab', [['d', 6, 8]], 5));

          // 8. Guarded fallback: an entry without edit data still restores
          // the text through the minimal diff.
          setup('abc', [], 3, 3);
          ins('d');
          s.history[s.historyIndex].edits = null;
          undo();
          if (s.getText() !== 'abc') out.push('fallback undo ' + JSON.stringify(s.getText()));
          redo();
          if (s.getText() !== 'abcd') out.push('fallback redo ' + JSON.stringify(s.getText()));
          // Steps that do not match the text are rejected, not applied.
          if (EditorSurface.replaySteps('abc', [{ start: 0, deletedText: 'x', insertedText: '' }], 'bc') !== null) out.push('mismatched step applied');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
