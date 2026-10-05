//! Browser tests for the trim levels, the faded preview and the status card
//! (issue #15, spec R-8.3 to R-8.5): each level against the engine, and
//! clicking a cut to keep it. Making the cuts and walking through them are
//! in `tests/web_trim_cuts.rs`, Done in `tests/web_trim_done.rs` and the
//! shared helpers in `tests/support/trim.rs`. Run with `wasm-pack test --headless
//! --chrome --test web_trim`. Real scripts, real stylesheets, the real
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
async fn test_each_level_card_and_preview_equal_the_engine() {
    setup();
    let expected = expected_levels();
    let check_level = |level: &str| {
        format!(
            r##"const want = {expected}['{level}'];
            const trim = teTrim.pick('{level}');
            if (ed.lab.isOpen) out.push('popover still open');
            const card = teTrim.card();
            if (card.hidden) return 'card hidden';
            if (teTrim.cardCount() !== want.card) out.push('card ' + teTrim.cardCount() + ' != ' + want.card);
            // One word count (issue #59): the Write_On counter shows the card's words_before.
            const counter = document.querySelector('.te-chrome-counter-text').textContent;
            if (!counter.startsWith(want.wordsBefore + ' words ')) out.push('counter ' + counter + ' != ' + want.wordsBefore + ' words');
            const name = card.querySelector('.te-trim-card-level');
            if (name.textContent !== want.label) out.push('level ' + name.textContent);
            if (getComputedStyle(name).fontWeight < 600) out.push('level name not bold');
            if (JSON.stringify(teTrim.pieces()) !== JSON.stringify(want.pieces)) out.push('pieces ' + JSON.stringify(teTrim.pieces()) + ' != ' + JSON.stringify(want.pieces));
            if (teTrim.button('{level}').getAttribute('aria-checked') !== 'true') out.push('level not checked');
            for (const p of teTrim.layer()) {{
              const el = teTrim.segment(p.data.cut, p.start);
              if (!el) {{ out.push('piece not rendered ' + p.id); continue; }}
              if (!el.classList.contains('te-trim-faded') || el.classList.contains('te-ghost')) out.push('classes ' + el.className);
            }}
            if (s.getText() !== teTrim.fixture) out.push('text changed');
            if (teTrim.ghosts() !== '[]') out.push('preview wrote ghosts ' + teTrim.ghosts());
            if (s.history.length + ':' + s.historyIndex !== T.history) out.push('preview touched the undo history');"##
        )
    };
    let slight = check_level("slight");
    let tighten = check_level("tighten");
    let sharper = check_level("sharper");
    let half = check_level("half");
    let result = run_steps(&[
        r##"T.history = s.history.length + ':' + s.historyIndex;
        const pop = teTrim.open();
        const slot = pop.querySelector('[data-slot="trim"]');
        if (slot.hidden) return 'slot hidden';
        const labels = Array.from(slot.querySelectorAll('[role="radio"]')).map((b) => b.textContent);
        const want = ['Original', 'Slight trim~10%', 'Tighten more~20%', 'Even sharper~30%', 'Cut in half~50%'];
        if (JSON.stringify(labels) !== JSON.stringify(want)) out.push('labels ' + JSON.stringify(labels));
        if (teTrim.button('original').getAttribute('aria-checked') !== 'true') out.push('Original not selected');
        if (getComputedStyle(teTrim.button('original')).borderTopColor !== 'rgb(140, 134, 230)') out.push('Original border not lavender');
        // Arrow keys move along the radio group.
        teTrim.button('original').focus();
        teTest.key(document.activeElement, 'ArrowRight');
        if (document.activeElement !== teTrim.button('slight')) out.push('ArrowRight');"##,
        &slight,
        // The card: hint, buttons, bottom-centre, live count.
        r##"const card = teTrim.card();
        if (card.querySelector('.te-trim-card-hint').textContent !== 'Faded words would go. Click one to keep it.') out.push('hint');
        const buttons = Array.from(card.querySelectorAll('.te-trim-actions button')).map((b) => b.textContent);
        if (JSON.stringify(buttons) !== JSON.stringify(['Make the cuts', 'Walk through', 'Done'])) out.push('buttons ' + JSON.stringify(buttons));
        const r = card.getBoundingClientRect();
        const pr = teTrim.pill().getBoundingClientRect();
        if (Math.abs((r.left + r.right) / 2 - (pr.left + pr.right) / 2) > 1.5) out.push('card not centred on the pill: ' + [r.left, r.right, pr.left, pr.right, window.innerWidth, document.documentElement.clientWidth]);
        if (!(r.bottom <= pr.top)) out.push('card overlaps the pill');
        if (r.bottom > window.innerHeight || r.top < window.innerHeight / 2) out.push('card not at the bottom');
        const count = card.querySelector('.te-trim-card-count');
        if (count.getAttribute('aria-live') !== 'polite') out.push('count not live');
        if (document.activeElement !== card) out.push('focus not on the card');"##,
        &tighten,
        &sharper,
        &half,
        r##"teTrim.pick('original');
        if (teTrim.layer().length) out.push('Original left faded pieces');
        if (s.root.querySelector('.te-trim-faded')) out.push('Original left rendered pieces');
        if (!teTrim.card().hidden) out.push('Original left the card');
        if (teTrim.trim().kept.size) out.push('kept set not cleared');
        if (s.getText() !== teTrim.fixture) out.push('text changed');
        if (s.history.length + ':' + s.historyIndex !== T.history) out.push('undo history touched');"##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_click_keeps_the_outermost_cut_and_user_ghosts_are_untouched() {
    setup();
    let plan = plan();
    let really = utf16_at(" really just");
    // The weak sentence that holds the filler "really" (both faded at Tighten).
    let outer = plan
        .faded(TrimLevel::Tighten)
        .filter(|c| c.start <= really && really + 7 <= c.end)
        .max_by_key(|c| c.end - c.start)
        .unwrap();
    let inner = plan
        .faded(TrimLevel::Tighten)
        .find(|c| c.start == really && c.end == really + 7)
        .expect("filler cut");
    assert_ne!(outer.id, inner.id);
    let kept = plan.status(TrimLevel::Tighten, &[outer.id]).card_text();
    let kept_pieces = pieces(&plan.active(TrimLevel::Tighten, &[outer.id]));
    let garden = utf16_at("garden needs");
    let setup_ghosts = format!(
        r##"// One user ghost inside the faded sentence, one outside any cut.
        if (!ed.ghosts.ghost({really}, {really_end}).ok) return 'ghost 1';
        if (!ed.ghosts.ghost({garden}, {garden_end}).ok) return 'ghost 2';
        T.ghosts = teTrim.ghosts();
        if (T.ghosts !== JSON.stringify([[{really}, {really_end}], [{garden}, {garden_end}]])) return 'ghosts ' + T.ghosts;
        teTrim.pick('tighten');
        if (teTrim.ghosts() !== T.ghosts) out.push('preview changed ghosts ' + teTrim.ghosts());
        if (s.root.querySelectorAll('.te-ghost').length < 2) out.push('user ghosts not drawn');
        const both = s.root.querySelector('.te-ghost.te-trim-faded');
        if (!both) out.push('ghosted faded text should carry both classes');"##,
        really_end = really + 7,
        garden_end = garden + 6,
    );
    let click = format!(
        r##"// Click on the filler: both the sentence and the filler cover it.
        const el = teTrim.segment({inner_id}, {really});
        if (!el) return 'filler segment not rendered';
        const ids = el.getAttribute('data-te-decoration');
        if (ids.indexOf('lab-trim:{outer_id}-') < 0) return 'segment not covered by the sentence: ' + ids;
        teTrim.click(el);
        const trim = teTrim.trim();
        if (JSON.stringify(Array.from(trim.kept)) !== '[{outer_id}]') out.push('kept ' + JSON.stringify(Array.from(trim.kept)));
        if (teTrim.cardCount() !== {kept:?}) out.push('card ' + teTrim.cardCount());
        if (JSON.stringify(teTrim.pieces()) !== JSON.stringify({kept_pieces})) out.push('pieces ' + JSON.stringify(teTrim.pieces()));
        if (teTrim.ghosts() !== T.ghosts) out.push('keep changed ghosts ' + teTrim.ghosts());
        if (s.getText() !== teTrim.fixture) out.push('keep changed text');
        // A click on unfaded text keeps nothing.
        teTrim.click(s.root);
        if (trim.kept.size !== 1) out.push('a click outside kept something');"##,
        inner_id = inner.id.0,
        outer_id = outer.id.0,
    );
    let result = run_steps(&[
        &setup_ghosts,
        &click,
        r##"teTrim.pick('original');
        if (teTrim.ghosts() !== T.ghosts) out.push('Original changed ghosts');
        if (s.root.querySelectorAll('.te-ghost').length < 2) out.push('user ghosts lost');
        if (s.root.querySelector('.te-trim-faded')) out.push('preview left');"##,
    ])
    .await;
    assert_eq!(result, "");
}
