//! Browser tests for leaving a trim (issue #15, spec R-8.3 to R-8.5): Done
//! persists exact faded coverage as ghosts, and edits invalidate the trim. The levels are in
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
async fn test_done_persists_exact_ghosts_with_save_reopen_and_one_history_step() {
    setup();
    let cuts = plan().active(TrimLevel::Slight, &[]);
    let mut expected: Vec<_> = cuts
        .iter()
        .filter(|cut| {
            !cuts.iter().any(|outer| {
                outer.start <= cut.start
                    && cut.end <= outer.end
                    && (outer.start < cut.start || cut.end < outer.end)
            })
        })
        .map(|cut| (cut.start, cut.end))
        .collect();
    expected.sort_unstable();
    expected.dedup();
    assert!(!expected.is_empty(), "fixture must propose actual cuts");
    let result = run_steps(&[
        &format!(r##"T.expected = {expected};
        T.before = ed.saveDocument();
        T.history = s.historyIndex;
        teTrim.pick('slight');
        teTrim.card().querySelector('.te-trim-done').click();
        if (!teTrim.card().hidden || teTrim.layer().length || s.root.querySelector('.te-trim-faded')) out.push('Done left transient preview');
        if (teTrim.trim().level !== 'original' || teTrim.button('original').getAttribute('aria-checked') !== 'true') out.push('Done did not reset preview controls');
        const ghosts = ed.annotations().ghosts;
        if (JSON.stringify(ghosts.map((g) => [g.anchor.start,g.anchor.end])) !== JSON.stringify(T.expected)) out.push('ghosts differ from exact outermost engine cuts');
        for (const g of ghosts) {{
          if (s.getText().slice(g.anchor.start,g.anchor.end) !== g.anchor.text) out.push('ghost anchor text');
        }}
        if (!s.root.querySelector('.te-ghost')) out.push('persistent fades not rendered');
        if (s.getText() !== teTrim.fixture) out.push('Done rewrote the body');
        if (s.historyIndex !== T.history + 1) out.push('Done is not one history step');
        T.after = ed.saveDocument();
        if (T.after === T.before) out.push('Done did not persist annotations');
        // Clicking a persistent ghost cannot retain a now-retired preview cut.
        teTrim.click(s.root.querySelector('.te-ghost'));
        if (teTrim.trim().kept.size || ed.saveDocument() !== T.after) out.push('click after Done changed annotations');"##,
            expected = serde_json::to_string(&expected).unwrap()),
        r##"if (!s.undo() || ed.saveDocument() !== T.before || s.historyIndex !== T.history) out.push('actual Undo did not restore exact saved source');
        if (!s.redo() || ed.saveDocument() !== T.after || s.historyIndex !== T.history + 1) out.push('actual Redo did not restore exact saved ghosts');
        ed.openDocument(T.after, 'trim-done-reopened');
        if (ed.saveDocument() !== T.after || s.getText() !== teTrim.fixture) out.push('save/reopen changed source');
        if (JSON.stringify(ed.annotations().ghosts.map((g) => [g.anchor.start,g.anchor.end])) !== JSON.stringify(T.expected)) out.push('reopen lost exact ghost ranges');
        if (!s.root.querySelector('.te-ghost')) out.push('reopened ghosts not rendered');"##,
    ]).await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_edits_invalidate_trim_and_leaving_write_on_clears_preview() {
    setup();
    let result = run_steps(&[
        r##"// An edit ends the trim: stale at once, cleared on the next tick.
        const trim = teTrim.pick('slight');
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
