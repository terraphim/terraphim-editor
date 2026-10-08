//! Browser test for the alternatives panel's width (issue #96, R-4.1): the
//! stylesheet gives 18% of the window as measured in the video, within the
//! clamp, and the panel opened from the chrome control is that wide. Split
//! from `web_alt_panel.rs` to keep each binary inside its time budget. Run
//! with `wasm-pack test --headless --chrome --test web_alt_panel_width`.
//! Real stylesheets in a real iframe; nothing is mocked.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// R-4.1: the panel is 18% of the window, as measured in the demo video, so
/// within 2% of 205px at a 1140px window, clamped to 12rem-20rem; and the
/// panel the editor opens takes the width the same stylesheet gives at the
/// test window's width.
#[wasm_bindgen_test]
async fn test_panel_width_matches_the_video() {
    let markup = r#"<aside class="te-alt-panel"></aside>"#;
    let at_1140 = width_in_viewport(1140.0, markup, ".te-alt-panel").await;
    assert_video_panel_width("the alternatives panel", at_1140, 1140.0);
    assert_panel_clamp(markup, ".te-alt-panel").await;

    let _document = fresh_full_editor();
    let opened = js_string(
        r##"(() => {
          const ed = window.__teEditor;
          ed.openDocument('Hello world.\n');
          ed.chrome.setMode('write-on');
          teTest.control('alternatives').click();
          if (!ed.altPanel.isOpen()) return 'chrome control did not open the panel';
          window.__teAltWidth = ed.altPanel.root.getBoundingClientRect().width;
          ed.altPanel.close({ restoreFocus: false });
          return 'ok';
        })()"##,
    );
    assert_eq!(opened, "ok");
    let real = js_number("window.__teAltWidth");
    let window_width = js_number("window.innerWidth");
    let expected = width_in_viewport(window_width, markup, ".te-alt-panel").await;
    assert!(
        (real - expected).abs() < 0.5,
        "opened panel is {real}px; the stylesheet gives {expected}px at {window_width}px"
    );
}
