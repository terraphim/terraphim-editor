//! Browser tests for the Overflow panel's layout and toggle key (R-6.1,
//! issue #101): Ctrl+Shift+D, Shift-exact and Write_On only, and the
//! full-height, flush-right placement with the corner controls on top. Split
//! from `web_overflow.rs` to keep each binary inside its time budget (see
//! `tests/web.rs`). Run with `wasm-pack test --headless --chrome`. Real
//! scripts, real stylesheets and the real document API; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// R-6.1: Ctrl+Shift+D toggles the panel in Write_On mode, as the XYZ
/// tooltip shows at 3:52: exactly Ctrl and Shift (not Ctrl+D, not with Alt or
/// Cmd added), from the page and from inside the panel, with focus returned
/// on close. Plain mode leaves the chord to the browser. The XYZ tooltip and
/// the shortcut reference show it, and the XYZ tag is lit while open.
#[wasm_bindgen_test]
async fn test_ctrl_shift_d_toggles_the_panel_shift_exact() {
    let _document = fresh_full_editor();
    install_overflow_helpers("Hello world.\n");
    let r = run_steps(&[
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        const ov = ed.overflow;
        const xyz = teTest.control('overflow');
        if (!/\(Ctrl\+Shift\+D\)$/.test(xyz.title)) out.push('XYZ tooltip ' + JSON.stringify(xyz.title));
        if (xyz.getAttribute('aria-keyshortcuts') !== 'Control+Shift+D') out.push('aria-keyshortcuts ' + xyz.getAttribute('aria-keyshortcuts'));
        const listed = ed.chrome.shortcutEntries().flatMap((sec) => sec.items).some((it) => it.key === 'ctrl+shift+d' && /Overflow/.test(it.desc));
        if (!listed) out.push('not in the shortcut reference');
        s.focus();
        // Near misses do nothing and leave the browser default alone.
        for (const opts of [{ ctrlKey: true, code: 'KeyD' }, { ctrlKey: true, shiftKey: true, altKey: true, code: 'KeyD' },
                            { ctrlKey: true, shiftKey: true, metaKey: true, code: 'KeyD' }, { metaKey: true, shiftKey: true, code: 'KeyD' }]) {
          if (teTest.key(s.root, opts.shiftKey ? 'D' : 'd', opts) || ov.isOpen()) out.push('fired on ' + JSON.stringify(opts));
        }"#,
        r#"const ov = ed.overflow;
        const xyz = teTest.control('overflow');
        if (!teTest.key(s.root, 'D', { ctrlKey: true, shiftKey: true, code: 'KeyD' })) out.push('open: default not prevented');
        if (!ov.isOpen()) return 'Ctrl+Shift+D did not open the panel';
        if (document.activeElement !== ov.area) out.push('focus not in the text area');
        if (xyz.getAttribute('aria-expanded') !== 'true') out.push('aria-expanded');
        // The border (the text colour eases in over 120ms).
        if (getComputedStyle(xyz).borderTopColor !== 'rgb(140, 134, 230)') out.push('XYZ not lit: ' + getComputedStyle(xyz).borderTopColor);
        // From inside the panel, in lower case too, it closes again.
        if (!teTest.key(ov.area, 'd', { ctrlKey: true, shiftKey: true, code: 'KeyD' })) out.push('close: default not prevented');
        if (ov.isOpen()) return 'Ctrl+Shift+D in the panel did not close it';
        if (document.activeElement !== s.root && !s.root.contains(document.activeElement)) out.push('focus did not return to the page');
        if (getComputedStyle(xyz).borderTopColor === 'rgb(140, 134, 230)') out.push('XYZ still lit');"#,
        r#"const ov = ed.overflow;
        ed.chrome.setMode('plain');
        if (teTest.key(s.root, 'D', { ctrlKey: true, shiftKey: true, code: 'KeyD' })) out.push('plain mode took the chord');
        if (ov.isOpen()) out.push('opened in plain mode');"#,
    ])
    .await;
    assert!(r.is_empty(), "{r}");
}

/// R-6.1 placement: full height and flush with the right edge, as in the demo
/// (x=2536 to 3095, top to bottom): right edge at the viewport's, top 0 and
/// the viewport's height, in a 1140px window and in the editor. The corner
/// controls stay usable over it: the keyboard glyph and the XYZ tag are the
/// topmost elements at their centres, and the writing column stays clear.
#[wasm_bindgen_test]
async fn test_panel_is_full_height_and_flush_right() {
    let markup = r#"<aside class="te-overflow"></aside>"#;
    let rect = rect_in_viewport(1140.0, 800.0, markup, ".te-overflow").await;
    // Layout engines can return fractional CSS pixels (Firefox reports
    // 1139.999984741211 here). Preserve the geometry check within 0.01px.
    assert!(
        (rect.right - 1140.0).abs() < 0.01
            && rect.top.abs() < 0.01
            && (rect.height - 800.0).abs() < 0.01,
        "the panel should fill the right edge of a 1140x800 window: {:?}",
        (rect.right, rect.top, rect.height)
    );

    let _document = fresh_full_editor();
    install_overflow_helpers("Hello world.\n");
    let r = run_steps(&[r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        teTest.control('overflow').click();
        const ov = ed.overflow;
        if (!ov.isOpen()) return 'XYZ did not open the panel';
        const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
        const r = ov.root.getBoundingClientRect();
        if (Math.abs(r.right - vw) > 0.5 || Math.abs(r.top) > 0.5 || Math.abs(r.height - vh) > 0.5) {
          out.push('panel ' + JSON.stringify([r.left, r.top, r.right, r.bottom]) + ' in ' + vw + 'x' + vh);
        }
        for (const name of ['shortcuts', 'overflow']) {
          const c = teTest.control(name);
          const b = c.getBoundingClientRect();
          if (b.left < r.left || b.right > r.right) out.push(name + ' is not over the panel');
          const hit = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2);
          if (!hit || !c.contains(hit)) out.push(name + ' is covered by ' + (hit && hit.className));
        }
        // The panel's content clears both corner controls.
        const area = ov.area.getBoundingClientRect();
        const title = ov.root.querySelector('.te-overflow-title').getBoundingClientRect();
        const hint = ov.root.querySelector('.te-overflow-hint').getBoundingClientRect();
        if (title.top < teTest.control('shortcuts').getBoundingClientRect().bottom) out.push('title under the keyboard glyph');
        if (hint.bottom > teTest.control('overflow').getBoundingClientRect().top) out.push('hint under the XYZ tag');
        if (area.height <= 0) out.push('no room for the text area');
        // The writing column stays left of the panel on a wide window.
        if (vw >= 960 && s.root.getBoundingClientRect().right > r.left) out.push('writing column runs under the panel');
        // The keyboard glyph still opens the reference while the panel is open.
        teTest.control('shortcuts').click();
        const dialog = document.querySelector('.te-chrome-shortcuts');
        if (!dialog || !dialog.open) out.push('keyboard glyph did not open the reference');
        else dialog.close();
        teTest.control('overflow').click();
        if (ov.isOpen()) out.push('XYZ over the panel did not close it');"#])
    .await;
    assert!(r.is_empty(), "{r}");
}
