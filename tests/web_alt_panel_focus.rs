//! Browser tests for where Escape sends focus when the alternatives panel
//! closes (issue #106 follow-up, found in acceptance testing): opened from
//! the text (Ctrl+Shift+A, or Ctrl+Shift+G showing a span, even while the
//! panel was already open from the `●●●` control), Escape returns focus to
//! the text with its selection, so a Ctrl+Z straight afterwards undoes;
//! a dot clicked while focus is in the panel counts as the text; opened
//! from `●●●`, focus goes back to `●●●`. Run with `wasm-pack test
//! --headless --chrome --test web_alt_panel_focus`. Real scripts, a real
//! host provider and the real document API; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_escape_returns_focus_to_what_opened_the_panel() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_alt_panel_helpers();
    let result = run_steps(&[
        // Ctrl+Shift+A on a selection, an alternative typed, Escape: focus
        // and the selection are back in the text, and Ctrl+Z undoes at once.
        r##"
          s.setText('Pass me a paperclip. Drop this.');
          ed.chrome.setMode('write-on');
          s.focus();
          s.setSelectionOffsets(10, 19);
          teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          teAlt.typeKeys(teAlt.newInput(), 'pin');
          teTest.key(document.activeElement, 'Enter');
          if (s.getText() !== 'Pass me a pin. Drop this.') return 'setup ' + JSON.stringify(s.getText());
          teTest.key(document.activeElement, 'Escape');
          if (document.activeElement !== s.root) out.push('focus on ' + (document.activeElement && document.activeElement.className));
          const sel = s.getSelectionOffsets();
          if (sel.start !== 10 || sel.end !== 19) out.push('selection ' + JSON.stringify(sel));
          if (!teTest.key(document.activeElement, 'z', { ctrlKey: true })) out.push('Ctrl+Z not handled');
          if (s.getText() !== 'Pass me a paperclip. Drop this.') out.push('undo did not apply: ' + JSON.stringify(s.getText()));
        "##,
        // Opened from the ●●● control: Escape gives focus back to the control.
        r##"
          const dots = document.querySelector('.te-chrome-dots');
          if (!dots) return 'no ●●● control';
          dots.focus();
          dots.click();
          if (!teAlt.P().isOpen()) return 'not opened by ●●●';
          teTest.key(document.activeElement, 'Escape');
          if (document.activeElement !== dots) out.push('focus not back on ●●●: ' + (document.activeElement && document.activeElement.className));
        "##,
        // Opened from ●●●, then the writer goes back to the text and asks a
        // host provider (Ctrl+Shift+G), which shows the span in the open panel.
        r##"
          T.off = ed.registerAlternativesProvider({ id: 'focus-test', model: 'm', alternatives: async () => ['leave', 'skip'] });
          const dots = document.querySelector('.te-chrome-dots');
          dots.focus();
          dots.click();
          s.focus();
          s.setSelectionOffsets(21, 25);
          teTest.key(s.root, 'G', { ctrlKey: true, shiftKey: true });
        "##,
        r##"
          const P = teAlt.P();
          if (!P.isOpen() || !P.span || P.span.anchor.text !== 'Drop') return 'panel not on the span: ' + JSON.stringify(P.span && P.span.anchor);
          P.focusLine(P.activeLineIndex());
          teTest.key(document.activeElement, 'Escape');
          if (document.activeElement !== s.root) out.push('focus on ' + (document.activeElement && document.activeElement.className));
          const sel = s.getSelectionOffsets();
          if (sel.start !== 21 || sel.end !== 25) out.push('selection ' + JSON.stringify(sel));
          teTest.key(document.activeElement, 'z', { ctrlKey: true });
          if (teAlt.spans().some((x) => x.anchor.text === 'Drop')) out.push('undo did not remove the provider alternatives');
          if (!teAlt.inSync()) out.push('model and surface differ');
          T.off();
        "##,
        // Opened from ●●● with focus in a panel line, then a dot clicked in
        // the text: Escape goes back to the text and Ctrl+Z undoes there.
        r##"
          s.setText('Pass me a paperclip. Drop this.');
          s.focus();
          s.setSelectionOffsets(10, 19);
          teTest.key(s.root, 'A', { ctrlKey: true, shiftKey: true });
          teAlt.typeKeys(teAlt.newInput(), 'pin');
          teTest.key(document.activeElement, 'Enter');
          teTest.key(document.activeElement, 'Escape');
          const id = teAlt.spans()[0].id;
          const dots = document.querySelector('.te-chrome-dots');
          dots.focus();
          dots.click();
          const P = teAlt.P();
          P.focusLine(1);
          if (!P.root.contains(document.activeElement)) return 'focus not in the panel';
          ed.indicators.flush();
          const dot = document.querySelector('.te-indicators [data-span-id="' + id + '"] .te-ind-dot');
          if (!dot) return 'no dot';
          dot.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          teTest.key(document.activeElement, 'Escape');
          if (document.activeElement !== s.root) out.push('focus on ' + (document.activeElement && document.activeElement.className));
          const before = s.getText();
          if (!teTest.key(document.activeElement, 'z', { ctrlKey: true })) out.push('Ctrl+Z not handled by the text');
          if (s.getText() === before) out.push('undo did not apply');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
