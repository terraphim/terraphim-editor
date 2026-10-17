//! Browser tests for completing a trim: Done persists independent ghosts in
//! one undo step; edits still invalidate a subsequent preview. The levels are in
//! `tests/web_trim.rs`, making the cuts in `tests/web_trim_cuts.rs` and the
//! shared helpers in `tests/support/trim.rs`. Run with `wasm-pack test
//! --headless --chrome --test web_trim_done`. Real scripts, real stylesheets, the real
//! exported document, Lab and trim API on `window.wasmBindings` and the
//! committed fixture `tests/fixtures/lab/lab.md`; nothing is mocked. The
//! expected card numbers, pieces and cut text are computed in the same test
//! by the `terraphim_lab` engine, so the browser must show exactly what the
//! engine returns. Scenarios yield between lifecycle phases through
//! `support::run_steps`; the original browser binary/time budget is unchanged.
#![cfg(target_arch = "wasm32")]

mod support;

use support::trim::*;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Public fixture/role with engine-derived kept and remaining ranges. The
/// heading marker is a disjoint, pre-existing manual ghost, not a trim cut.
fn migration_setup() {
    setup();
    let plan = plan();
    let initial = plan.active(TrimLevel::Sharper, &[]);
    assert!(
        initial.len() > 1,
        "fixture needs multiple independent fades"
    );
    let kept = initial[0].id;
    let remaining = plan.active(TrimLevel::Sharper, &[kept]);
    assert!(!remaining.is_empty() && remaining.len() < initial.len());
    assert!(remaining.iter().all(|c| c.start >= 1));
    // Approved promotion oracle only: retain maximal post-kept coverage,
    // leaving original preview/Make-cuts/export oracle unchanged.
    let mut ordered: Vec<_> = remaining.iter().map(|c| (c.start, c.end)).collect();
    for (i, &(s, e)) in ordered.iter().enumerate() {
        for &(a, b) in &ordered[..i] {
            assert!(
                s >= b || a >= e || (s <= a && b <= e) || (a <= s && e <= b),
                "crossing is not authorized for promotion"
            );
        }
    }
    ordered.sort_by_key(|&(s, e)| (s, std::cmp::Reverse(e)));
    let mut promotion: Vec<(usize, usize)> = Vec::new();
    for (s, e) in ordered {
        if !promotion.last().is_some_and(|&(a, b)| a <= s && e <= b) {
            promotion.push((s, e));
        }
    }
    assert!(promotion.windows(2).all(|w| w[0].1 <= w[1].0));
    // Unit-for-unit coverage proof, not a merged stored-ghost oracle.
    for offset in 0..FIXTURE.encode_utf16().count() {
        assert_eq!(
            remaining
                .iter()
                .any(|c| c.start <= offset && offset < c.end),
            promotion.iter().any(|&(s, e)| s <= offset && offset < e)
        );
    }
    let mut exported = terraphim_alternatives::Document::new(FIXTURE);
    exported.ghost(0, 1).unwrap();
    for cut in &remaining {
        exported.ghost(cut.start, cut.end).unwrap();
    }
    let oracle = serde_json::json!({
        "initial": pieces(&initial),
        "kept": kept.0,
        "remaining": pieces(&remaining),
        "promotion": promotion,
        "exported": exported.export(),
    });
    assert_eq!(
        js_string(&format!(
            r##"(() => {{
        window.doneOracle = {oracle};
        const ed = window.__teEditor;
        const block = {{version:1, spans:[], ghosts:[{{id:'g1', anchor:{{start:0,end:1,text:'#'}}}}], overflow:''}};
        ed.openDocument(teTrim.fixture + '\n\n```terraphim-alternatives\n' + JSON.stringify(block) + '\n```\n', 'trim-done-manual');
        // Opening a new key restores that key's mode preference (plain here).
        ed.chrome.setMode('write-on');
        if (!ed.chrome.isWriteOn()) return 'manual document not in Write_On';
        return ed.annotations().ghosts.length === 1 ? 'ok' : 'manual ghost missing';
    }})()"##
        )),
        "ok"
    );
}

#[wasm_bindgen_test]
async fn test_done_persists_ghosts_and_edits_invalidate_the_next_trim() {
    migration_setup();
    let result = run_steps(&[
        r##"const trim = teTrim.pick('sharper');
        if (JSON.stringify(teTrim.pieces()) !== JSON.stringify(doneOracle.initial)) out.push('initial engine ranges');
        trim.keep(doneOracle.kept);
        if (JSON.stringify(teTrim.pieces()) !== JSON.stringify(doneOracle.remaining)) out.push('kept engine ranges');
        T.before = ed.saveDocument();
        T.manual = JSON.stringify(ed.annotations().ghosts);
        T.counts = JSON.stringify(ed.counts());
        T.history = s.history.length;
        T.index = s.historyIndex;
        teTrim.card().querySelector('.te-trim-done').click();
        if (trim.live.textContent !== 'Done: cuts saved as ghosts. Undo restores them.') return 'Done setup failed: ' + trim.live.textContent;
        if (!teTrim.card().hidden) out.push('Done left the card');
        if (teTrim.layer().length || s.root.querySelector('.te-trim-faded')) out.push('Done retained transient fades');
        if (trim.plan || trim.status || trim.active.length || trim.kept.size || trim.walkIndex !== -1) out.push('Done retained plan state');
        if ('teTrimming' in s.root.dataset) out.push('Done retained trimming flag');
        if (teTrim.button('original').getAttribute('aria-checked') !== 'true') out.push('Original not selected after Done');
        if (s.history.length !== T.history + 1 || s.historyIndex !== T.index + 1) out.push('Done not one history step');
        if (s.getText() !== teTrim.fixture || ed.documentApi().document_body() !== teTrim.fixture) out.push('Done mutated raw body');
        if (JSON.stringify(ed.counts()) !== T.counts) out.push('Done excluded ghosts from body counts');
        const ghosts = ed.annotations().ghosts;
        if (JSON.stringify(ghosts.filter(g => g.id === 'g1')) !== T.manual) out.push('Done changed manual ghost');
        const promoted = ghosts.filter(g => g.id !== 'g1');
        const ranges = promoted.map(g => [g.anchor.start, g.anchor.end]);
        if (JSON.stringify(ranges) !== JSON.stringify(doneOracle.promotion)) out.push('independent ghost ranges differ from approved promotion projection');
        if (new Set(ghosts.map(g => g.id)).size !== ghosts.length || ghosts.some(g => typeof g.id !== 'string')) out.push('ghost IDs not independent strings');
        if (promoted.some(g => g.anchor.text !== teTrim.fixture.slice(g.anchor.start, g.anchor.end))) out.push('ghost anchor text');
        T.checkGhostRendering = (annotations, phase, errors) => {
          const expected = annotations.map(g => [g.id, g.anchor.start, g.anchor.end, 'te-ghost']);
          const registered = ed.decorations.get('ghosts').map(d => [d.id, d.start, d.end, d.className]);
          if (JSON.stringify(registered) !== JSON.stringify(expected) || ed.ghosts.count !== annotations.length) errors.push(phase + ': ghost registry/count not refreshed');
          const els = Array.from(s.root.querySelectorAll('.te-ghost'));
          if (!els.length) errors.push(phase + ': no persistent ghost DOM');
          // Match native string IDs as attribute tokens, without numeric coercion
          // or selector interpolation. One ghost may render several segments.
          for (const ghost of annotations) {
            const segments = els.filter(el => (el.getAttribute('data-te-decoration') || '').split(/\s+/).includes('ghosts:' + ghost.id));
            if (!segments.length) errors.push(phase + ': missing rendered ghost ' + ghost.id);
            for (const el of segments) {
              const style = getComputedStyle(el), color = style.color;
              const slash = color.match(/\/\s*([\d.]+)\s*\)$/), rgba = color.match(/^rgba\(([^)]+)\)$/);
              const alpha = slash ? Number(slash[1]) : rgba ? Number(rgba[1].split(',')[3]) : 1;
              if (!el.isConnected || !el.getClientRects().length || style.visibility !== 'visible' || Math.abs(alpha - 0.1) > 0.02) errors.push(phase + ': ghost not visibly faded ' + ghost.id + ' ' + color);
            }
          }
          // Preserve the original check on every ghost-classed segment as well.
          for (const el of els) {
            const color = getComputedStyle(el).color;
            const slash = color.match(/\/\s*([\d.]+)\s*\)$/), rgba = color.match(/^rgba\(([^)]+)\)$/);
            const alpha = slash ? Number(slash[1]) : rgba ? Number(rgba[1].split(',')[3]) : 1;
            if (Math.abs(alpha - 0.1) > 0.02) errors.push(phase + ': ghost alpha ' + color);
          }
          return els.find(el => annotations.some(g => g.id !== 'g1' && (el.getAttribute('data-te-decoration') || '').split(/\s+/).includes('ghosts:' + g.id)));
        };
        const promotedSegment = T.checkGhostRendering(ghosts, 'Done', out);
        if (trim.live.textContent !== 'Done: cuts saved as ghosts. Undo restores them.' || trim.live.getAttribute('role') !== 'status' || trim.live.getAttribute('aria-live') !== 'polite') out.push('Done status');
        T.after = ed.saveDocument();
        T.ghosts = JSON.stringify(ghosts);
        if (!T.after.includes('```terraphim-alternatives\n')) out.push('missing saved annotation block');
        if (ed.exportDocument() !== doneOracle.exported || ed.exportDocument().includes('terraphim-alternatives')) out.push('clean export not engine-range omission');
        // Persistent ghosts cannot keep a retired preview.
        if (promotedSegment) teTrim.click(promotedSegment);
        else out.push('no promoted segment to click');
        if (trim.kept.size) out.push('click kept after Done');
        if (!s.undo()) out.push('Done undo refused');
        if (ed.saveDocument() !== T.before || JSON.stringify(ed.annotations().ghosts) !== T.manual || s.getText() !== teTrim.fixture) out.push('one Undo failed to restore manual-only state');
        if (s.historyIndex !== T.index) out.push('Undo index');
        if (!s.redo() || ed.saveDocument() !== T.after || JSON.stringify(ed.annotations().ghosts) !== T.ghosts) out.push('Redo did not restore exact ghost identities');
        T.checkGhostRendering(ed.annotations().ghosts, 'Redo', out);"##,
        r##"ed.openDocument(T.after, 'trim-done-reopen');
        ed.chrome.setMode('write-on');
        if (!ed.chrome.isWriteOn()) return 'reopened document not in Write_On';
        if (s.getText() !== teTrim.fixture || ed.documentApi().document_body() !== teTrim.fixture || JSON.stringify(ed.counts()) !== T.counts) out.push('reopen body/counts');
        if (JSON.stringify(ed.annotations().ghosts) !== T.ghosts || ed.saveDocument() !== T.after) out.push('reopen annotations/IDs');
        if (ed.exportDocument() !== doneOracle.exported) out.push('reopen clean export');
        T.checkGhostRendering(ed.annotations().ghosts, 'reopen', out);
        // A fresh level reopens the card without removing persisted ghosts.
        teTrim.pick('slight');
        if (teTrim.card().hidden) out.push('card not reopened');
        if (JSON.stringify(ed.annotations().ghosts) !== T.ghosts) out.push('fresh preview changed ghosts');"##,
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
        ed.ghosts.flush();
        const expectedGhosts = JSON.parse(T.ghosts).map(g => [g.id, g.anchor.start + 1, g.anchor.end + 1, g.anchor.text]);
        const editedGhosts = ed.annotations().ghosts.map(g => [g.id, g.anchor.start, g.anchor.end, g.anchor.text]);
        if (JSON.stringify(editedGhosts) !== JSON.stringify(expectedGhosts) || ed.ghosts.count !== expectedGhosts.length) out.push('typing lost persistent ghosts/count');
        T.editedGhosts = JSON.stringify(ed.annotations().ghosts);
        if (teTrim.layer().length || s.root.querySelector('.te-trim-faded')) out.push('preview left after edit');
        if (trim.level !== 'original' || trim.kept.size) out.push('state left after edit');
        if (teTrim.button('original').getAttribute('aria-checked') !== 'true') out.push('Original not selected');
        if (!/text changed/.test(trim.live.textContent)) out.push('no announcement');
        // A fresh level recomputes the plan for the new text.
        teTrim.pick('slight');
        if (!teTrim.layer().length || teTrim.card().hidden) out.push('recompute failed');"##,
        r##"// Leaving Write_On clears the trim; destroy removes everything.
        if (!ed.chrome.isWriteOn()) return 'plain transition requires prior Write_On';
        if (!teTrim.layer().length || teTrim.card().hidden) return 'plain transition requires active preview';
        const api = ed.documentApi();
        const persistentBefore = JSON.stringify(api.document_annotations());
        const ghostIdsBefore = JSON.stringify(api.document_annotations().ghosts.map(g => g.id));
        const manualBefore = JSON.stringify(api.document_annotations().ghosts.filter(g => g.id === 'g1'));
        if (api.document_annotations().ghosts.filter(g => g.id === 'g1').length !== 1) return 'plain transition requires manual ghost';
        const historyBefore = JSON.stringify({entries:s.history,index:s.historyIndex,limit:s.historyLimit});
        const bodyBefore = s.getText();
        const roleBefore = api.kg_role_snapshot_json();
        const modes = [];
        const observeMode = e => { if (e.detail && e.detail.editor === ed) modes.push(e.detail.mode); };
        document.addEventListener('te:mode-change', observeMode);
        try { ed.chrome.setMode('plain'); }
        finally { document.removeEventListener('te:mode-change', observeMode); }
        if (ed.chrome.isWriteOn() || ed.chrome.mode !== 'plain') out.push('plain mode not established');
        if (JSON.stringify(modes) !== JSON.stringify(['plain'])) out.push('plain transition event not exactly once');
        if (teTrim.layer().length || !teTrim.card().hidden) out.push('plain mode left the trim');
        if (JSON.stringify(ed.annotations().ghosts) !== T.editedGhosts) out.push('plain mode removed persistent ghosts');
        if (s.root.querySelector('.te-trim-faded') || 'teTrimming' in s.root.dataset) out.push('plain mode left ephemeral DOM');
        if (JSON.stringify(api.document_annotations()) !== persistentBefore) out.push('plain mode changed persistent annotations');
        if (JSON.stringify(api.document_annotations().ghosts.map(g => g.id)) !== ghostIdsBefore) out.push('plain mode changed ghost IDs');
        if (JSON.stringify(api.document_annotations().ghosts.filter(g => g.id === 'g1')) !== manualBefore) out.push('plain mode changed manual ghost');
        if (JSON.stringify({entries:s.history,index:s.historyIndex,limit:s.historyLimit}) !== historyBefore) out.push('plain mode changed history');
        if (s.getText() !== bodyBefore || api.document_body() !== bodyBefore) out.push('plain mode changed body');
        if (api.kg_role_snapshot_json() !== roleBefore) out.push('plain mode changed role');
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

#[wasm_bindgen_test]
async fn test_done_capacity_refusal_preserves_preview_selection_and_redo() {
    migration_setup();
    let result = run_steps(&[
        r##"// Build a real redo branch, then preview/keep/walk without history edits.
        const end = s.getText().length;
        s.replaceRange(end, end, 'X', {source:'typing'});
        if (!s.undo() || !s.canRedo()) out.push('no redo branch');
        const trim = teTrim.pick('sharper');
        trim.keep(doneOracle.kept);
        trim.startWalk();
        s.setSelectionOffsets(2, 5);
        T.limit = s.historyLimit;
        s.historyLimit = 1;
        T.refusal = {
          saved:ed.saveDocument(), annotations:JSON.stringify(ed.annotations()), body:s.getText(),
          history:JSON.stringify(s.history), index:s.historyIndex, selection:JSON.stringify(s.getSelectionOffsets()),
          plan:JSON.stringify(trim.plan), status:JSON.stringify(trim.status), pieces:JSON.stringify(teTrim.pieces()),
          kept:JSON.stringify(Array.from(trim.kept)), walk:trim.walkIndex, ghosts:JSON.stringify(ed.decorations.get('ghosts'))
        };
        teTrim.card().querySelector('.te-trim-done').click();
        const old = T.refusal;
        if (ed.saveDocument() !== old.saved || JSON.stringify(ed.annotations()) !== old.annotations || s.getText() !== old.body || ed.documentApi().document_body() !== old.body) out.push('refusal mutated model/JSON');
        if (JSON.stringify(s.history) !== old.history || s.historyIndex !== old.index || !s.canRedo() || s.historyLimit !== 1) out.push('refusal changed history/redo');
        if (JSON.stringify(s.getSelectionOffsets()) !== old.selection) out.push('refusal changed selection');
        if (JSON.stringify(trim.plan) !== old.plan || JSON.stringify(trim.status) !== old.status || JSON.stringify(teTrim.pieces()) !== old.pieces || JSON.stringify(Array.from(trim.kept)) !== old.kept || trim.walkIndex !== old.walk) out.push('refusal changed preview/keeps/walk');
        if (trim.level !== 'sharper' || !trim.cardOpen || trim.card.hidden || !s.root.querySelector('.te-trim-faded')) out.push('refusal retired valid preview');
        if (JSON.stringify(ed.decorations.get('ghosts')) !== old.ghosts || ed.annotations().ghosts.length !== 1 || ed.annotations().ghosts[0].id !== 'g1') out.push('refusal changed manual ghosts');
        if (trim.live.textContent !== 'Done refused: History capacity must retain a base state and one ghost operation' || trim.live.getAttribute('role') !== 'status' || trim.live.getAttribute('aria-live') !== 'polite') out.push('capacity refusal status');
        // Restore normal capacity; the preserved redo operation must remain usable.
        s.historyLimit = T.limit;
        if (!s.redo() || s.getText() !== teTrim.fixture + 'X') out.push('refusal lost redo operation');"##,
    ]).await;
    assert_eq!(result, "");
}
