//! Browser tests for the trim levels, the faded preview and the status card
//! (issue #15, spec R-8.3 to R-8.5). Run with `wasm-pack test --headless
//! --chrome --test web_trim`. Real scripts, real stylesheets, the real
//! exported document, Lab and trim API on `window.wasmBindings` and the
//! committed fixture `tests/fixtures/lab/lab.md`; nothing is mocked. The
//! expected card numbers, pieces and cut text are computed in the same test
//! by the `terraphim_lab` engine, so the browser must show exactly what the
//! engine returns. Scenarios run as short steps with a yield between them
//! (`support::run_steps`), one or two model calls per step.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_lab::{make_cuts, trim_plan, Cut, CutId, LabConfig, TrimLevel, TrimPlan};

wasm_bindgen_test_configure!(run_in_browser);

const FIXTURE: &str = include_str!("fixtures/lab/lab.md");

fn plan() -> TrimPlan {
    trim_plan(
        FIXTURE,
        &LabConfig::with_defaults().expect("default Lab config"),
    )
}

fn level_id(level: TrimLevel) -> String {
    serde_json::to_value(level)
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

fn pieces(cuts: &[Cut]) -> serde_json::Value {
    cuts.iter()
        .map(|c| serde_json::json!([c.id.0, c.start, c.end]))
        .collect::<Vec<_>>()
        .into()
}

/// `{ level: { card, label, pieces } }` from the engine, as JSON.
fn expected_levels() -> String {
    let plan = plan();
    let mut out = serde_json::Map::new();
    for level in &TrimLevel::ALL[1..] {
        out.insert(
            level_id(*level),
            serde_json::json!({
                "card": plan.status(*level, &[]).card_text(),
                "label": level.label(),
                "pieces": pieces(&plan.active(*level, &[])),
            }),
        );
    }
    serde_json::Value::Object(out).to_string()
}

/// The UTF-16 offset of the first `needle` in the fixture.
fn utf16_at(needle: &str) -> usize {
    FIXTURE[..FIXTURE.find(needle).expect("needle in fixture")]
        .encode_utf16()
        .count()
}

/// Fresh editor, fixture open, Write_On on, helpers on `window.teTrim`.
fn setup() {
    let _document = fresh_full_editor();
    let src = format!(
        r##"(() => {{
          window.teTrim = {{
            fixture: {fixture},
            trim() {{ return window.__teEditor.lab.trim; }},
            pill() {{ return teTest.control('lab'); }},
            open() {{ if (!window.__teEditor.lab.isOpen) teTrim.pill().click(); return window.__teEditor.lab.root; }},
            button(level) {{ return teTrim.trim().slot.querySelector('[data-level="' + level + '"]'); }},
            pick(level) {{ teTrim.open(); teTrim.button(level).click(); return teTrim.trim(); }},
            layer() {{ return window.__teEditor.decorations.get('lab-trim'); }},
            pieces() {{ return teTrim.layer().map((i) => [i.data.cut, i.start, i.end]); }},
            card() {{ return teTrim.trim().card; }},
            cardCount() {{ return teTrim.card().querySelector('.te-trim-card-count').textContent; }},
            ghosts() {{ return JSON.stringify(window.__teEditor.annotations().ghosts.map((g) => [g.anchor.start, g.anchor.end])); }},
            segment(cut, start) {{
              return window.__teEditor.surface.root.querySelector('[data-te-decoration~="lab-trim:' + cut + '-' + start + '"]');
            }},
            click(el) {{ el.dispatchEvent(new MouseEvent('click', {{ bubbles: true, cancelable: true }})); }},
          }};
          const ed = window.__teEditor;
          ed.openDocument(teTrim.fixture, 'trim-test-' + Math.random());
          ed.chrome.setMode('write-on');
          return ed.lab && ed.lab.trim ? 'ok' : 'no trim';
        }})()"##,
        fixture = js_string_literal(FIXTURE),
    );
    assert_eq!(js_string(&src), "ok");
}

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
