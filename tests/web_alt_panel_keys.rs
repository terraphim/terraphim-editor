//! Browser tests for the alternatives side panel (issue #10), part two:
//! Ctrl+Shift+A on a caret and inside the panel, the selection menu item, the
//! chrome control with Escape returning focus, leaving Write_On mode, the
//! text helpers, typed text committed on blur, tab change and close (never
//! lost), and `destroy()`. Split from `web_alt_panel.rs` to keep each
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
        // Typed text is never silently lost: text left in the empty line is
        // committed like Enter when focus leaves it (here to the document),
        // creating the pending span; one undo step removes it again.
        r##"
          ed.chrome.setMode('write-on');
          s.setText('Red green blue.');
          s.focus();
          s.setSelectionOffsets(5, 5);
          const P = teAlt.P();
          P.open();
          if (!P.target.pending) return 'not pending ' + JSON.stringify(P.target);
          teAlt.newInput().focus();
          teAlt.newInput().value = 'pair';
          s.focus();
          const spans = teAlt.spans();
          if (spans.length !== 1 || JSON.stringify(spans[0].alts.map((a) => a.text)) !== '["green","pair"]') return 'blur did not add ' + JSON.stringify(spans);
          if (document.activeElement !== s.root) out.push('focus did not stay on the document');
          s.undo();
          P.flush();
          if (teAlt.spans().length !== 0) out.push('undo did not remove it');
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        // A tab click (which need not move focus), closing with the chrome
        // control and Escape all commit the empty line; whitespace-only
        // text is ignored.
        r##"
          const P = teAlt.P();
          P.open({ start: 10, end: 14, kind: 'word' }, { focus: false });
          P.addAlternative('pair');
          T.id = P.target.spanId;
          teAlt.newInput().focus();
          teAlt.newInput().value = 'quill';
          P.tabs.sentence.click();
          const texts = () => (teAlt.spans()[0] || { alts: [] }).alts.map((a) => a.text);
          if (JSON.stringify(texts()) !== '["blue","pair","quill"]') return 'tab click ' + JSON.stringify(texts());
          s.undo();
          if (JSON.stringify(texts()) !== '["blue","pair"]') out.push('undo tab-click add ' + JSON.stringify(texts()));
        "##,
        r##"
          const P = teAlt.P();
          const texts = () => (teAlt.spans()[0] || { alts: [] }).alts.map((a) => a.text);
          P.open({ spanId: T.id });
          teAlt.newInput().focus();
          teAlt.newInput().value = 'pen';
          teTest.control('alternatives').click();
          if (P.isOpen()) out.push('control did not close');
          if (JSON.stringify(texts()) !== '["blue","pair","pen"]') return 'close ' + JSON.stringify(texts());
          P.open({ spanId: T.id });
          teAlt.newInput().focus();
          teAlt.newInput().value = 'ink';
          teTest.key(teAlt.newInput(), 'Escape');
          if (P.isOpen()) out.push('Escape did not close');
          if (JSON.stringify(texts()) !== '["blue","pair","pen","ink"]') out.push('Escape ' + JSON.stringify(texts()));
          s.undo();
          s.undo();
          if (JSON.stringify(texts()) !== '["blue","pair"]') out.push('undo ' + JSON.stringify(texts()));
          // Whitespace only: Enter, blur and close add nothing.
          P.open({ spanId: T.id });
          teAlt.newInput().focus();
          teAlt.newInput().value = '   ';
          teTest.key(teAlt.newInput(), 'Enter');
          teAlt.newInput().value = '  ';
          s.focus();
          P.open({ spanId: T.id });
          teAlt.newInput().focus();
          teAlt.newInput().value = ' ';
          P.close({ restoreFocus: false });
          if (JSON.stringify(texts()) !== '["blue","pair"]') out.push('whitespace added ' + JSON.stringify(texts()));
          if (!teAlt.inSync()) out.push('model out of step');
        "##,
        r##"
          // destroy() removes the panel and its listeners, never edits the
          // document, and reports text left in a line.
          const P = teAlt.P();
          P.open({ spanId: T.id });
          teAlt.newInput().focus();
          teAlt.newInput().value = 'draft';
          const before = JSON.stringify(ed.documentApi().document_annotations().spans);
          let reported = null;
          const listener = (e) => { reported = e.detail.drafts; };
          document.addEventListener('te:alt-drafts', listener);
          const ret = ed.destroy();
          document.removeEventListener('te:alt-drafts', listener);
          // The editor-level return value carries the same draft, tagged.
          if (!Array.isArray(ret) || ret.length !== 1 || ret[0].kind !== 'alternative' || ret[0].value !== 'draft' || ret[0].spanId !== T.id) out.push('destroy returned ' + JSON.stringify(ret));
          if (JSON.stringify(ed.destroy()) !== '[]') out.push('second destroy');
          if (!reported || reported.length !== 1 || reported[0].value !== 'draft' || !reported[0].isNew || reported[0].spanId !== T.id) out.push('drafts ' + JSON.stringify(reported));
          if (JSON.stringify(ed.documentApi().document_annotations().spans) !== before) out.push('destroy edited the document');
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
