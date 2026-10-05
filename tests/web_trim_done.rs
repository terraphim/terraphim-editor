//! Browser tests for leaving a trim (issue #15, spec R-8.3 to R-8.5): Done
//! leaves the preview, and edits invalidate the trim. The levels are in
//! `tests/web_trim.rs`, making the cuts in `tests/web_trim_cuts.rs` and the
//! shared helpers in `tests/support/trim.rs`. Run with `wasm-pack test
//! --headless --chrome --test web_trim_done`. Real scripts, real stylesheets, the real
//! exported document, Lab and trim API on `window.wasmBindings` and the
//! committed fixture `tests/fixtures/lab/lab.md`; nothing is mocked. The
//! expected card numbers, pieces and cut text are computed in the same test
//! by the `terraphim_lab` engine, so the browser must show exactly what the
//! engine returns. Scenarios run as short steps with a yield between them
//! (`support::run_steps`), one or two model calls per step.
#![cfg(target_arch = "wasm32")]

mod support;

use support::trim::*;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_done_leaves_the_preview_and_edits_invalidate_the_trim() {
    setup();
    let result = run_steps(&[
        r##"T.history = s.history.length + ':' + s.historyIndex;
        const trim = teTrim.pick('slight');
        T.pieces = JSON.stringify(teTrim.pieces());
        teTrim.card().querySelector('.te-trim-done').click();
        if (!teTrim.card().hidden) out.push('Done left the card');
        if (JSON.stringify(teTrim.pieces()) !== T.pieces) out.push('Done changed the preview');
        if (!s.root.querySelector('.te-trim-faded')) out.push('Done removed the faded text');
        if (teTrim.ghosts() !== '[]') out.push('Done wrote ghosts ' + teTrim.ghosts());
        if (teTrim.button('slight').getAttribute('aria-checked') !== 'true') out.push('level not kept');
        if (s.history.length + ':' + s.historyIndex !== T.history) out.push('Done touched the undo history');
        // Clicks no longer keep once the card is dismissed.
        teTrim.click(s.root.querySelector('.te-trim-faded'));
        if (trim.kept.size) out.push('click kept after Done');
        // Picking the level again reopens the card.
        teTrim.pick('slight');
        if (teTrim.card().hidden) out.push('card not reopened');"##,
        r##"// An edit ends the trim: stale at once, cleared on the next tick.
        const trim = teTrim.trim();
        s.replaceRange(0, 0, 'X', { source: 'typing' });
        if (!trim.stale || !teTrim.card().hidden) out.push('edit did not invalidate');
        if (trim.makeCuts() !== null) out.push('cut with a stale plan');
        if (s.getText() !== 'X' + teTrim.fixture) out.push('stale cuts applied');
        // The bridge refuses the stale plan too.
        const api = ed.documentApi();
        ed.alignDocumentModel(api);
        let refused = false;
        try { api.trim_make_cuts('slight', '[]'); } catch (e) { refused = /stale/.test(String(e)); }
        if (!refused) out.push('bridge accepted a stale plan');"##,
        r##"const trim = teTrim.trim();
        if (teTrim.layer().length || s.root.querySelector('.te-trim-faded')) out.push('preview left after edit');
        if (trim.level !== 'original' || trim.kept.size) out.push('state left after edit');
        if (teTrim.button('original').getAttribute('aria-checked') !== 'true') out.push('Original not selected');
        if (!/text changed/.test(trim.live.textContent)) out.push('no announcement');
        // A fresh level recomputes the plan for the new text.
        teTrim.pick('slight');
        if (!teTrim.layer().length || teTrim.card().hidden) out.push('recompute failed');"##,
        r##"// Leaving Write_On clears the trim; destroy removes everything.
        ed.chrome.setMode('plain');
        if (teTrim.layer().length || !teTrim.card().hidden) out.push('plain mode left the trim');
        ed.chrome.setMode('write-on');
        teTrim.pick('slight');
        const trim = teTrim.trim();
        const surface = ed.surface;
        ed.destroy();
        if (document.querySelector('.te-trim-card, .te-trim-live')) out.push('DOM left after destroy');
        if (surface.getDecorations().some((d) => d.id.startsWith('lab-trim:'))) out.push('preview left after destroy');
        if ('teTrimming' in surface.root.dataset) out.push('trimming flag left');
        if (!trim.destroyed) out.push('not destroyed');"##,
    ])
    .await;
    assert_eq!(result, "");
}
