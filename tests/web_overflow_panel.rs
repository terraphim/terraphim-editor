//! Browser tests for the Overflow panel (issue #12, spec R-6.1 to R-6.5):
//! the XYZ toggle, ARIA and Escape, stashing as a move and one undo step
//! (menu and Ctrl+Shift+X), pulling back with Ctrl+Enter and by drag and
//! drop, persistence through save and open, export, the preserved-block
//! refusal and destroy(); Ctrl+Shift+D and the full-height placement are in
//! `web_overflow_layout.rs`. Run with `wasm-pack test --headless --chrome`.
//! Real scripts and the real document API; nothing is mocked. Each test is a
//! short list of `run_steps` steps with one or two model calls each.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_panel_width_matches_the_video() {
    let markup = r#"<aside class="te-overflow"></aside>"#;
    let at_1140 = width_in_viewport(1140.0, markup, ".te-overflow").await;
    assert_video_panel_width("the Overflow panel", at_1140, 1140.0);
    assert_panel_clamp(markup, ".te-overflow").await;

    let _document = fresh_full_editor();
    install_overflow_helpers("Hello world.\n");
    let r = run_steps(&[r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        teTest.control('overflow').click();
        const ov = ed.overflow;
        if (!ov.isOpen()) return 'XYZ did not open the panel';
        window.__teOvWidth = ov.root.getBoundingClientRect().width;
        const header = ov.root.querySelector('.te-overflow-header');
        if (header.scrollWidth > header.clientWidth) out.push('header overflows: ' + header.scrollWidth + ' > ' + header.clientWidth);
        const title = ov.root.querySelector('.te-overflow-title').getBoundingClientRect();
        const close = ov.root.querySelector('.te-overflow-close').getBoundingClientRect();
        if (close.top >= title.bottom) out.push('buttons wrapped below the title');
        ov.close();"#])
    .await;
    assert!(r.is_empty(), "{r}");
    let real = js_number("window.__teOvWidth");
    let window_width = js_number("window.innerWidth");
    let expected = width_in_viewport(window_width, markup, ".te-overflow").await;
    assert!(
        (real - expected).abs() < 0.5,
        "opened panel is {real}px; the stylesheet gives {expected}px at {window_width}px"
    );
}

#[wasm_bindgen_test]
async fn test_panel_drag_accepts_only_its_owned_plain_text_dragover() {
    let _document = fresh_full_editor();
    install_overflow_helpers("Public body.");
    let r = run_steps(&[
        r#"ed.openDocument(window.__teSrc);
        ed.chrome.setMode('write-on');
        const ov=ed.overflow;
        ov.open();
        teOv.type(ov.area,'Public note.');
        ov.area.setSelectionRange(0,12);
        const transfer=new DataTransfer();
        transfer.setData('text/plain','Public note.');
        ov.area.dispatchEvent(new DragEvent('dragstart',{bubbles:true,dataTransfer:transfer}));
        const over=new DragEvent('dragover',{bubbles:true,cancelable:true,dataTransfer:transfer});
        s.root.dispatchEvent(over);
        if (!over.defaultPrevented) out.push('owned plain text drag was not accepted');
        // This constructed event tests acceptance gating; the native driver
        // separately proves a trusted copy and exact unchanged Overflow.
        ov.area.dispatchEvent(new DragEvent('dragend',{bubbles:true,dataTransfer:transfer}));
        const external=new DragEvent('dragover',{bubbles:true,cancelable:true,dataTransfer:transfer});
        s.root.dispatchEvent(external);
        if (external.defaultPrevented) out.push('unowned drag was captured');
        if (s.getText()!=='Public body.') out.push('dragover rewrote body');
        if (ov.text()!=='Public note.') out.push('dragover rewrote Overflow');"#,
    ]).await;
    assert!(r.is_empty(), "{r}");
}
