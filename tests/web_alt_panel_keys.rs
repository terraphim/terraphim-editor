//! Browser tests for the alternatives side panel (issue #10), part two:
//! Ctrl+Shift+A on a caret and inside the panel, the selection menu item, the
//! chrome control with Escape returning focus, leaving Write_On mode, the
//! text helpers and `destroy()`. Split from `web_alt_panel.rs` to keep each
//! binary inside its time budget. Run with `wasm-pack test --headless
//! --chrome`. Real scripts and the real document API; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_keyboard_toggle_mode_and_destroy() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        r##"
          s.setText('One two three.\n\nAnother paragraph here.');
          ed.chrome.setMode('write-on');
          s.focus();
          s.setSelectionOffsets(5, 5);
          // A collapsed caret: the word there, pending.
          if (!teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true })) out.push('not cancelled');
          const P = teAlt.P();
          if (!P.isOpen() || !P.target.pending || P.target.pending.text !== 'two') return 'caret word ' + JSON.stringify(P.target);
          // Ctrl+Shift+A inside the panel closes it again.
          if (!teTest.key(document.activeElement, 'A', { ctrlKey: true, shiftKey: true })) out.push('panel shortcut not cancelled');
          if (P.isOpen() || document.activeElement !== s.root) out.push('toggle close');
        "##,
        // The menu offers the item for a selection, first in R-7.3 order;
        // a bare caret keeps the browser's own context menu.
        r##"
          const menu = ed.selectionMenu;
          s.focus();
          s.setSelectionOffsets(0, 3);
          if (!menu.open(s.getSelectionOffsets(), null)) return 'menu did not open on a selection';
          const labels = Array.from(document.querySelectorAll('.te-selection-menu [role="menuitem"] .te-selection-menu-label')).map((x) => x.textContent);
          if (JSON.stringify(labels) !== '["Alternatives for selection","Ghost it"]') out.push('labels ' + JSON.stringify(labels));
          const row = document.querySelector('.te-selection-menu [role="menuitem"]');
          if (row.getAttribute('aria-keyshortcuts') !== 'Control+Shift+A') out.push('aria-keyshortcuts');
          row.click();
          const P = teAlt.P();
          if (!P.isOpen() || !P.target.pending || P.target.pending.text !== 'One') out.push('menu item target ' + JSON.stringify(P.target));
          P.close({ restoreFocus: true });
          s.setSelectionOffsets(5, 5);
          if (menu.open(s.getSelectionOffsets(), null)) out.push('menu opened on a bare caret');
          menu.close({ restoreFocus: false });
        "##,
        r##"
          // The chrome control toggles; Escape returns focus to it.
          const control = teTest.control('alternatives');
          control.focus();
          control.click();
          const P = teAlt.P();
          if (!P.isOpen()) return 'not open';
          P.setTab('paragraph');
          if (!P.target.pending || P.target.pending.text !== 'One two three.') out.push('paragraph ' + JSON.stringify(P.target));
          teTest.key(document.activeElement, 'Escape');
          if (P.isOpen() || document.activeElement !== control) out.push('focus not back on the control');
          if (control.getAttribute('aria-expanded') !== 'false') out.push('aria-expanded');
          control.click();
          control.click();
          if (P.isOpen()) out.push('second click did not close');
        "##,
        r##"
          // Leaving Write_On mode closes the panel.
          const P = teAlt.P();
          P.open();
          ed.chrome.setMode('plain');
          if (P.isOpen()) out.push('open in plain mode');
          // Static text helpers.
          const t = TeAlternativesPanel.text;
          const doc = '# Head line\n\nFirst one. Second "two"! Third\nline';
          const show = (r) => r ? doc.slice(r.start, r.end) : null;
          if (show(t.sentenceAt(doc, 3)) !== 'Head line') out.push('headline ' + show(t.sentenceAt(doc, 3)));
          if (show(t.sentenceAt(doc, 26)) !== 'Second "two"!') out.push('sentence ' + show(t.sentenceAt(doc, 26)));
          if (show(t.paragraphAt(doc, 20)) !== 'First one. Second "two"! Third\nline') out.push('paragraph');
          if (show(t.wordAt(doc, 17)) !== 'First') out.push('word end');
          if (t.inferKind('tension') !== 'word' || t.inferKind('A whole sentence here.') !== 'sentence' || t.inferKind('One. Two.') !== 'paragraph') out.push('inferKind');
        "##,
        // Document -> panel: typing inside a span while the panel shows it
        // detaches the span (the panel follows), and undo brings it back.
        r##"
          ed.chrome.setMode('write-on');
          s.setText('One two three.');
          const P = teAlt.P();
          P.open({ start: 4, end: 7, kind: 'word' }, { focus: false });
          P.addAlternative('pair');
          const id = P.target.spanId;
          if (!id) return 'no span ' + JSON.stringify(P.target);
          T.id = id;
          s.replaceRange(5, 5, 'x');
          P.flush();
          if (P.target.spanId === id) out.push('panel still shows the detached span');
          if (teAlt.spans().length !== 0) out.push('span not detached');
          if (!P.target.pending || P.target.pending.text !== 'txwo') out.push('fallback ' + JSON.stringify(P.target));
          if (!teAlt.inSync()) out.push('model out of step after typing');
        "##,
        r##"
          const P = teAlt.P();
          s.undo();
          P.flush();
          if (teAlt.spans().length !== 1) return 'span not re-attached';
          if (P.target.spanId !== T.id) out.push('panel did not show the span again ' + JSON.stringify(P.target));
          if (JSON.stringify(teAlt.texts()) !== '["two","pair"]') out.push('lines ' + teAlt.texts());
          if (!teAlt.inSync()) out.push('model out of step after undo');
          P.close({ restoreFocus: false });
        "##,
        r##"
          // destroy() removes the panel and its listeners.
          const P = teAlt.P();
          P.open();
          ed.destroy();
          if (document.querySelector('.te-alt-panel')) out.push('panel DOM left');
          if (!P.destroyed || P.isOpen()) out.push('not destroyed');
          document.dispatchEvent(new CustomEvent('te:open-panel', { detail: { panel: 'alternatives', editor: ed } }));
          if (P.isOpen()) out.push('listener survived destroy');
          if (document.querySelector('.te-alt-open')) out.push('dim class left');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
