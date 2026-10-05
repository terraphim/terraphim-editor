//! Browser tests for making the trim cuts (issue #15, spec R-8.3 to R-8.5):
//! one undo step that restores annotations, and walking through the cuts
//! keeping some. The levels and the faded preview are in
//! `tests/web_trim.rs`, Done in `tests/web_trim_done.rs` and the shared
//! helpers in `tests/support/trim.rs`. Run with `wasm-pack test --headless
//! --chrome --test web_trim_cuts`. Real scripts, real stylesheets, the real
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
async fn test_make_the_cuts_is_one_undo_step_and_undo_restores_annotations() {
    setup();
    let plan = plan();
    let probably = plan
        .faded(TrimLevel::Slight)
        .find(|c| c.reason.contains("probably"))
        .expect("probably cut");
    let kept = [probably.id];
    let made = make_cuts(FIXTURE, &plan.active(TrimLevel::Slight, &kept));
    let card = plan.status(TrimLevel::Slight, &kept).card_text();
    let really = utf16_at(" really just") + 1;
    let garden = utf16_at("garden needs");
    let step1 = format!(
        r##"// A user ghost inside a cut (" really") and one outside every cut.
        if (!ed.ghosts.ghost({really}, {really_end}).ok) return 'ghost 1';
        if (!ed.ghosts.ghost({garden}, {garden_end}).ok) return 'ghost 2';
        T.export = ed.exportDocument();
        T.index = s.historyIndex;
        const trim = teTrim.pick('slight');
        trim.keep({probably_id});
        if (teTrim.cardCount() !== {card:?}) out.push('card ' + teTrim.cardCount());
        T.changes = 0;
        T.off = s.onChange(() => {{ T.changes += 1; }});
        teTrim.card().querySelector('.te-trim-cut').click();
        T.off();
        if (s.getText() !== {made_text}) out.push('text after cuts ' + JSON.stringify(s.getText()));
        if (s.historyIndex !== T.index + 1) out.push('not one undo step: ' + (s.historyIndex - T.index));
        if (T.changes !== {edits}) out.push('changes ' + T.changes + ' != {edits}');
        if (ed.documentApi().document_body() !== s.getText()) out.push('model body out of step');
        if (teTrim.layer().length || !teTrim.card().hidden) out.push('trim not ended');
        if (trim.level !== 'original') out.push('level ' + trim.level);
        if (!teTest.canonical()) out.push('surface not canonical');
        // The ghost outside every cut survives, re-anchored on its text.
        const g = JSON.parse(teTrim.ghosts());
        if (!g.some(([a, b]) => s.getText().slice(a, b) === 'garden')) out.push('outside ghost lost ' + teTrim.ghosts());
        // The kept "probably" is still there.
        if (s.getText().indexOf('probably') < 0) out.push('kept text was cut');"##,
        really_end = really + 6,
        garden_end = garden + 6,
        probably_id = probably.id.0,
        made_text = js_string_literal(&made.text),
        edits = made.edits.len(),
    );
    let result = run_steps(&[
        &step1,
        r##"s.undo();
        if (s.getText() !== teTrim.fixture) out.push('undo did not restore the text');
        if (s.historyIndex !== T.index) out.push('undo index ' + s.historyIndex);
        if (ed.exportDocument() !== T.export) out.push('undo did not restore the annotations');
        if (!teTest.canonical()) out.push('surface not canonical after undo');"##,
        &format!(
            r##"s.redo();
            if (s.getText() !== {made_text}) out.push('redo text');
            s.undo();
            if (ed.exportDocument() !== T.export) out.push('second undo annotations');"##,
            made_text = js_string_literal(&made.text),
        ),
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_walk_through_steps_and_keeps_without_cutting() {
    setup();
    let plan = plan();
    let active = plan.active(TrimLevel::Tighten, &[]);
    let n = active.len();
    let first = &active[0];
    // Keeping the first piece keeps its outermost faded cut.
    let outer = active
        .iter()
        .filter(|c| c.start <= first.start && first.end <= c.end)
        .max_by_key(|c| c.end - c.start)
        .unwrap();
    let card = plan
        .status(TrimLevel::Tighten, &[CutId(outer.id.0)])
        .card_text();
    let result = run_steps(&[
        &format!(
            r##"T.history = s.history.length + ':' + s.historyIndex;
            const trim = teTrim.pick('tighten');
            const card = teTrim.card();
            card.querySelector('.te-trim-walk-start').click();
            const walk = card.querySelector('.te-trim-walk');
            if (walk.hidden) return 'walk hidden';
            const text = card.querySelector('.te-trim-walk-text').textContent;
            if (!text.startsWith('1 of {n}: ')) out.push('walk text ' + text);
            const cur = s.root.querySelectorAll('.te-trim-current');
            if (!cur.length || !cur[0].getAttribute('data-te-decoration').includes('lab-trim:{first_id}-{first_start}')) out.push('current not marked');
            if (!card.contains(document.activeElement)) out.push('focus left the card');
            card.querySelector('.te-trim-next').click();
            if (trim.walkIndex !== 1) out.push('next ' + trim.walkIndex);
            card.querySelector('.te-trim-prev').click();
            card.querySelector('.te-trim-prev').click();
            if (trim.walkIndex !== {last}) out.push('previous should wrap: ' + trim.walkIndex);
            teTest.key(document.activeElement, 'ArrowRight');
            if (trim.walkIndex !== 0) out.push('ArrowRight ' + trim.walkIndex);"##,
            first_id = first.id.0,
            first_start = first.start,
            last = n - 1,
        ),
        &format!(
            r##"const trim = teTrim.trim();
            const card = teTrim.card();
            teTest.key(document.activeElement, 'k');
            if (teTrim.cardCount() !== {card:?}) out.push('card after keep ' + teTrim.cardCount());
            if (!trim.kept.has({outer_id})) out.push('kept ' + JSON.stringify(Array.from(trim.kept)));
            if (s.getText() !== teTrim.fixture) out.push('walk through cut text');
            if (s.history.length + ':' + s.historyIndex !== T.history) out.push('walk touched the undo history');
            if (card.querySelector('.te-trim-walk').hidden) out.push('walk ended after keep');
            teTest.key(document.activeElement, 'Escape');
            if (!card.querySelector('.te-trim-walk').hidden) out.push('Escape did not stop the walk');
            if (card.hidden) out.push('stopping the walk closed the card');
            if (s.root.querySelector('.te-trim-current')) out.push('current mark left');"##,
            outer_id = outer.id.0,
        ),
    ])
    .await;
    assert_eq!(result, "");
}
