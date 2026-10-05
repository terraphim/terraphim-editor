//! Browser tests for the decoration registry and the inline indicators
//! (issue #8, spec R-3.1 to R-3.6). Run with `wasm-pack test --headless
//! --chrome`. Real scripts, real stylesheets, the real exported document API
//! and the committed fixture `tests/fixtures/indicators/indicators.md`;
//! nothing is mocked. Geometry is asserted against the client rects of the
//! text itself (dot count, lit index, centring, right alignment, gutter rule
//! extent). See `tests/web.rs` for why the browser tests are split across
//! binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

const FIXTURE: &str = include_str!("fixtures/indicators/indicators.md");

/// Geometry tolerance in CSS pixels.
const TOLERANCE: &str = "1.5";

/// Install the fixture and small geometry helpers as `window.teInd`.
fn install_helpers() {
    let src = format!(
        r##"(() => {{
          window.teInd = {{
            fixture: {fixture},
            tol: {TOLERANCE},
            editor() {{ return window.__teEditor; }},
            layer() {{ return window.__teEditor.indicators; }},
            // Open `md` (the fixture by default) under a fresh document key
            // and switch to Write_On mode.
            open(md, writeOn) {{
              const ed = window.__teEditor;
              ed.openDocument(md || teInd.fixture, 'indicators-test-' + Math.random());
              if (writeOn !== false) ed.chrome.setMode('write-on');
              ed.indicators.flush();
            }},
            span(id) {{ return teInd.editor().annotations().spans.find((s) => s.id === id); }},
            // Client rects of a span's text, from the surface itself.
            rects(id) {{
              const s = teInd.span(id);
              const r = teInd.editor().surface.rangeForOffsets(s.anchor.start, s.anchor.end);
              return Array.from(r.getClientRects()).filter((x) => x.width > 0 && x.height > 0);
            }},
            holder(id) {{ return document.querySelector('.te-indicators [data-span-id="' + id + '"]'); }},
            dots(id) {{ return Array.from(teInd.holder(id).querySelectorAll('.te-ind-dot')); }},
            lit(id) {{ return teInd.dots(id).findIndex((d) => d.classList.contains('te-ind-dot--active')); }},
            // Bounding box of the dots of a span.
            dotBox(id) {{
              const rs = teInd.dots(id).map((d) => d.getBoundingClientRect());
              return {{
                left: Math.min(...rs.map((r) => r.left)), right: Math.max(...rs.map((r) => r.right)),
                top: Math.min(...rs.map((r) => r.top)), bottom: Math.max(...rs.map((r) => r.bottom)),
              }};
            }},
            decorationEl(id) {{
              return teInd.editor().surface.root.querySelector('[data-te-decoration~="indicators:' + id + '"]');
            }},
            near(a, b) {{ return Math.abs(a - b) <= teInd.tol; }},
          }};
          return 'ok';
        }})()"##,
        fixture = js_string_literal(FIXTURE),
    );
    assert_eq!(js_string(&src), "ok");
}

fn setup() {
    let _document = fresh_full_editor();
    install_helpers();
}

#[wasm_bindgen_test]
fn test_word_headline_sentence_and_paragraph_geometry() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          teInd.open();
          const near = teInd.near;
          // Fixture spans: s1 headline (3, original active), s2 word (7, fifth
          // active), s3 sentence (2), s4 paragraph (3, second active).
          const want = { s1: [3, 0, 'headline'], s2: [7, 4, 'word'], s3: [2, 0, 'sentence'], s4: [3, 1, 'paragraph'] };
          for (const [id, [count, lit, kind]] of Object.entries(want)) {
            const h = teInd.holder(id);
            if (!h) { out.push(id + ': no indicator'); continue; }
            if (h.hidden) out.push(id + ': hidden');
            if (h.dataset.kind !== kind) out.push(id + ': kind ' + h.dataset.kind);
            if (teInd.dots(id).length !== count) out.push(id + ': ' + teInd.dots(id).length + ' dots');
            if (teInd.lit(id) !== lit) out.push(id + ': lit ' + teInd.lit(id));
            if (h.querySelectorAll('.te-ind-dot--active').length !== 1) out.push(id + ': not exactly one lit dot');
          }
          if (document.querySelectorAll('.te-indicators [data-span-id]').length !== 4) out.push('indicator count');

          // R-3.3 colours: the lit dot is the full accent, the rest the 40% accent.
          const dots = teInd.dots('s2');
          const bg = (el) => getComputedStyle(el).backgroundColor;
          if (bg(dots[4]) !== 'rgb(140, 134, 230)') out.push('lit colour ' + bg(dots[4]));
          if (bg(dots[0]) !== 'rgba(140, 134, 230, 0.4)') out.push('dim colour ' + bg(dots[0]));
          const d0 = dots[0].getBoundingClientRect();
          if (!near(d0.width, d0.height) || d0.width < 2 || d0.width > 6) out.push('dot size ' + d0.width + 'x' + d0.height);
          // Spacing about one diameter, all on one row.
          const d1 = dots[1].getBoundingClientRect();
          if (!near(d1.left - d0.right, d0.width)) out.push('dot spacing ' + (d1.left - d0.right));
          if (!dots.every((d) => near(d.getBoundingClientRect().top, d0.top))) out.push('word dots not in a row');

          // R-3.2 word: the row is centred under the word, just below it.
          const w = teInd.rects('s2');
          const wl = Math.min(...w.map((r) => r.left)), wr = Math.max(...w.map((r) => r.right));
          const wb = Math.max(...w.map((r) => r.bottom));
          const box = teInd.dotBox('s2');
          if (!near((box.left + box.right) / 2, (wl + wr) / 2)) out.push('word row not centred: ' + JSON.stringify([box, wl, wr]));
          if (!(box.top >= wb - 0.5 && box.top - wb < 6)) out.push('word row vertical ' + (box.top - wb));

          // R-3.2 headline and sentence: the row ends at the underline's end.
          for (const id of ['s1', 's3']) {
            const rs = teInd.rects(id);
            const last = rs[rs.length - 1];
            const b = teInd.dotBox(id);
            if (!near(b.right, last.right)) out.push(id + ' row not right-aligned: ' + b.right + ' vs ' + last.right);
            if (!(b.top >= last.bottom - 0.5 && b.top - last.bottom < 6)) out.push(id + ' row vertical ' + (b.top - last.bottom));
          }

          // R-3.1 underline under words and sentences; none under paragraphs.
          const cs = (id) => getComputedStyle(teInd.decorationEl(id));
          for (const id of ['s1', 's2', 's3']) {
            const s = cs(id);
            if (!s.textDecorationLine.includes('underline')) out.push(id + ' underline ' + s.textDecorationLine);
            if (s.textDecorationColor !== 'rgba(140, 134, 230, 0.4)') out.push(id + ' underline colour ' + s.textDecorationColor);
            if (s.textDecorationThickness !== '1px') out.push(id + ' underline thickness ' + s.textDecorationThickness);
          }
          if (cs('s4').textDecorationLine !== 'none') out.push('paragraph underline ' + cs('s4').textDecorationLine);

          // R-3.5 headline bold, same face, slightly larger.
          const root = teInd.editor().surface.root;
          const hs = cs('s1'), body = getComputedStyle(root);
          if (Number(hs.fontWeight) < 700) out.push('headline weight ' + hs.fontWeight);
          if (hs.fontFamily !== body.fontFamily) out.push('headline face ' + hs.fontFamily);
          const ratio = parseFloat(hs.fontSize) / parseFloat(body.fontSize);
          if (!(ratio > 1.05 && ratio < 1.35)) out.push('headline size ratio ' + ratio);
          if (Number(cs('s2').fontWeight) >= 700) out.push('word is bold');

          // R-3.4 paragraph: rule in the left gutter over the paragraph's
          // height, with a vertical dot column immediately to its left.
          const p = teInd.rects('s4');
          const rule = teInd.holder('s4').querySelector('.te-ind-rule').getBoundingClientRect();
          const contentLeft = root.getBoundingClientRect().left + root.clientLeft + parseFloat(body.paddingLeft);
          if (!near(rule.top, p[0].top)) out.push('rule top ' + rule.top + ' vs ' + p[0].top);
          if (!near(rule.bottom, p[p.length - 1].bottom)) out.push('rule bottom ' + rule.bottom + ' vs ' + p[p.length - 1].bottom);
          if (p.length < 2) out.push('paragraph should wrap for this test');
          if (!(rule.right < contentLeft && rule.left > contentLeft - 24)) out.push('rule x ' + rule.left + ' content ' + contentLeft);
          if (!near(rule.width, 1)) out.push('rule width ' + rule.width);
          const pd = teInd.dots('s4').map((d) => d.getBoundingClientRect());
          if (!pd.every((r) => near(r.left, pd[0].left))) out.push('paragraph dots not in a column');
          if (!(pd[1].top > pd[0].bottom)) out.push('paragraph dots not stacked');
          if (!(pd[0].right < rule.left && rule.left - pd[0].right < 8)) out.push('dot column not just left of the rule');
          if (!near(pd[0].top, rule.top)) out.push('dot column top ' + pd[0].top);

          // Overlay is out of flow: hiding it changes no layout.
          const overlay = document.querySelector('.te-indicators');
          if (overlay.getAttribute('aria-hidden') !== 'true') out.push('overlay not aria-hidden');
          const h1 = root.getBoundingClientRect().height, top1 = teInd.rects('s4')[0].top;
          overlay.style.display = 'none';
          const h2 = root.getBoundingClientRect().height, top2 = teInd.rects('s4')[0].top;
          overlay.style.display = '';
          if (h1 !== h2 || top1 !== top2) out.push('overlay affects layout');

          // Accessibility: every indicated span is described.
          const descs = { s1: 'Headline: 3 alternatives, 1 of 3 active', s2: 'Word: 7 alternatives, 5 of 7 active',
            s3: 'Sentence: 2 alternatives, 1 of 2 active', s4: 'Paragraph: 3 alternatives, 2 of 3 active' };
          for (const [id, text] of Object.entries(descs)) {
            const ref = teInd.decorationEl(id).getAttribute('aria-describedby');
            const el = ref && document.getElementById(ref);
            if (!el || el.textContent !== text) out.push(id + ' description ' + (el && el.textContent));
            else if (el.closest('[aria-hidden="true"]')) out.push(id + ' description hidden from AT');
          }
          if (!teInd.editor().surface.isCanonical().ok) out.push('surface not canonical');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_dot_click_dispatches_te_dot_without_changing_the_document() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          teInd.open();
          const ed = teInd.editor();
          const before = ed.surface.getText();
          const events = [];
          const on = (e) => events.push(e.detail);
          document.addEventListener('te:dot', on);
          const dot = teInd.dots('s2')[2];
          const down = new MouseEvent('mousedown', { bubbles: true, cancelable: true });
          dot.dispatchEvent(down);
          if (!down.defaultPrevented) out.push('mousedown not prevented');
          dot.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          teInd.dots('s4')[1].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          document.removeEventListener('te:dot', on);
          if (events.length !== 2) out.push(events.length + ' events');
          const [a, b] = events;
          if (a && (a.spanId !== 's2' || a.index !== 2 || a.kind !== 'word' || a.active !== false || a.editor !== ed)) out.push('word detail ' + JSON.stringify({ ...a, editor: !!a.editor }));
          if (b && (b.spanId !== 's4' || b.index !== 1 || b.kind !== 'paragraph' || b.active !== true)) out.push('paragraph detail ' + JSON.stringify({ ...b, editor: !!b.editor }));
          // R-3.6: dots are passive; jumping is left to issue #9.
          if (ed.surface.getText() !== before) out.push('text changed');
          if (teInd.span('s2').active !== 4) out.push('active changed');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_indicators_follow_reanchored_spans_while_typing() {
    setup();
    let prep = js_string(
        r##"(() => {
          teInd.open();
          const ed = teInd.editor();
          const s = ed.surface;
          const layer = teInd.layer();
          // A wide debounce window, so the per-frame check below cannot race
          // the refresh on a loaded host.
          layer.delay = 600;
          window.__teIndRun = {
            startWord: teInd.span('s2').anchor.start,
            startPara: teInd.span('s4').anchor.start,
            renders: s.renderCount,
            refreshes: layer.refreshCount,
            registryRenders: ed.decorations.renderCount,
          };
          // Type, keystroke by keystroke, at the start of the word's
          // paragraph (before the word, inside the same paragraph).
          const at = s.getText().indexOf('There is a');
          s.focus();
          s.setSelectionOffsets(at, at);
          for (const ch of 'Indeed, ') document.execCommand('insertText', false, ch);
          __teIndRun.typedAt = performance.now();
          const out = [];
          // The surface moved the decoration at once; the model read waits.
          const live = ed.decorations.current('indicators').find((d) => d.id === 's2');
          if (!live || live.start !== __teIndRun.startWord + 8) out.push('live decoration ' + JSON.stringify(live));
          if (layer.refreshCount !== __teIndRun.refreshes) out.push('refreshed per keystroke');
          if (!layer.pending()) out.push('no debounced refresh pending');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(prep, "");
    // Two real animation frames, well inside the debounce: the dots have
    // already followed the moved decoration without any model read.
    let frames =
        js_eval("new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)))");
    JsFuture::from(frames.dyn_into::<Promise>().unwrap())
        .await
        .unwrap();
    let early = js_string(
        r##"(() => {
          const out = [];
          const layer = teInd.layer();
          if (layer.refreshCount !== __teIndRun.refreshes) out.push('refreshed before the debounce');
          if (!layer.pending()) out.push('refresh no longer pending after ' + (performance.now() - __teIndRun.typedAt) + ' ms');
          const live = teInd.editor().decorations.current('indicators').find((d) => d.id === 's2');
          const r = teInd.editor().surface.rangeForOffsets(live.start, live.end).getBoundingClientRect();
          const box = teInd.dotBox('s2');
          if (!teInd.near((box.left + box.right) / 2, (r.left + r.right) / 2)) out.push('dots lag the word: ' + JSON.stringify([box.left, box.right, r.left, r.right]));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(early, "");
    // Real timers: the rest of the trailing debounce plus a few frames.
    sleep(600 + 150).await;
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teInd.editor();
          const s = ed.surface;
          const layer = teInd.layer();
          const run = window.__teIndRun;
          if (layer.pending()) out.push('refresh still pending');
          if (layer.refreshCount !== run.refreshes + 1) out.push('refreshes ' + (layer.refreshCount - run.refreshes));
          // No DOM rebuild beyond what the surface already did for typing.
          if (s.renderCount !== run.renders) out.push('surface re-rendered ' + (s.renderCount - run.renders));
          if (ed.decorations.renderCount !== run.registryRenders) out.push('registry re-rendered');
          // The model re-anchored the spans after the typed text.
          if (teInd.span('s2').anchor.start !== run.startWord + 8) out.push('word anchor ' + teInd.span('s2').anchor.start);
          if (teInd.span('s4').anchor.start !== run.startPara + 8) out.push('paragraph anchor');
          if (!s.getText().includes('Indeed, There is a struggle')) out.push('text ' + JSON.stringify(s.getText().slice(0, 80)));
          // The dots followed the word (laid out on a real animation frame).
          const w = teInd.rects('s2');
          const wl = Math.min(...w.map((r) => r.left)), wr = Math.max(...w.map((r) => r.right));
          const box = teInd.dotBox('s2');
          if (!teInd.near((box.left + box.right) / 2, (wl + wr) / 2)) out.push('word row not centred after typing ' + JSON.stringify([box.left, box.right, wl, wr]));
          if (teInd.lit('s2') !== 4 || teInd.dots('s2').length !== 7) out.push('word dots changed');
          const rs = teInd.rects('s3');
          if (!teInd.near(teInd.dotBox('s3').right, rs[rs.length - 1].right)) out.push('sentence row not right-aligned after typing');
          const p = teInd.rects('s4');
          const rule = teInd.holder('s4').querySelector('.te-ind-rule').getBoundingClientRect();
          if (!teInd.near(rule.top, p[0].top) || !teInd.near(rule.bottom, p[p.length - 1].bottom)) out.push('rule did not follow');
          if (!s.isCanonical().ok) out.push('not canonical');
          layer.delay = 120;
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_nothing_renders_in_plain_mode() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teInd.editor();
          teInd.open(null, false);
          const s = ed.surface;
          const layer = teInd.layer();
          const check = (label) => {
            if (ed.chrome.isWriteOn()) out.push(label + ': in write-on');
            if (document.querySelector('.te-indicators, .te-ind-descriptions')) out.push(label + ': overlay present');
            if (s.getDecorations().some((d) => d.id.startsWith('indicators:'))) out.push(label + ': decorations set');
            if (s.root.querySelector('.te-ind')) out.push(label + ': decorated spans');
            if (document.querySelector('.te-indicator-host')) out.push(label + ': host class');
            const struggle = s.getText().indexOf('struggle');
            if (struggle < 0) out.push(label + ': fixture not open');
          };
          check('plain');
          const renders = s.renderCount;
          // Typing in plain mode schedules no indicator work.
          s.focus();
          s.setSelectionOffsets(0, 0);
          document.execCommand('insertText', false, 'x');
          if (layer.pending()) out.push('refresh scheduled in plain mode');
          if (s.renderCount !== renders) out.push('plain typing re-rendered');
          // Write_On shows them; back to plain removes everything again.
          ed.chrome.toggle();
          if (document.querySelectorAll('.te-indicators [data-span-id]').length !== 4) out.push('write-on indicators missing');
          ed.chrome.toggle();
          check('toggled back');
          // Unowned decorations survive the layer's activation cycle.
          s.setDecorations([{ id: 'mine', start: 0, end: 3, className: 'te-test' }]);
          ed.chrome.toggle();
          ed.chrome.toggle();
          if (!s.getDecorations().some((d) => d.id === 'mine')) out.push('unowned decoration dropped');
          // destroy() removes the overlay and the layer's decorations.
          ed.chrome.toggle();
          ed.destroy();
          if (document.querySelector('.te-indicators, .te-ind-descriptions, .te-indicator-host')) out.push('destroy left indicator DOM');
          if (!layer.destroyed) out.push('layer not destroyed');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_spans_with_only_the_original_get_no_indicator() {
    setup();
    // One word span with only its original (R-2.7) and one with an
    // alternative, built through the crate.
    let mut doc = Document::new("plain word and chosen word");
    doc.add_span(SpanKind::Word, 6, 10).unwrap();
    let id = doc.add_span(SpanKind::Word, 22, 26).unwrap();
    doc.add_alternative(&id, "term", Source::Human, None)
        .unwrap();
    let source = write(&doc);
    let result = js_string(&format!(
        r##"(() => {{
          const out = [];
          teInd.open({source});
          const holders = document.querySelectorAll('.te-indicators [data-span-id]');
          if (holders.length !== 1 || holders[0].dataset.spanId !== {id}) out.push('indicators ' + holders.length);
          if (teInd.dots({id}).length !== 2 || teInd.lit({id}) !== 0) out.push('dots');
          const decos = teInd.editor().surface.getDecorations().filter((d) => d.id.startsWith('indicators:'));
          if (decos.length !== 1) out.push(decos.length + ' decorations');
          return out.join('; ');
        }})()"##,
        source = js_string_literal(&source),
        id = js_string_literal(&id),
    ));
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_registry_merges_named_layers_into_one_render() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teInd.editor();
          const s = ed.surface;
          const reg = ed.decorations;
          s.setText('alpha beta gamma delta');
          s.setDecorations([{ id: 'direct', start: 0, end: 5, className: 'te-direct' }]);
          let renders = s.renderCount;
          if (!reg.set('ghosts', [{ id: 'g1', start: 3, end: 10, className: 'te-ghost' }])) out.push('set did not render');
          if (s.renderCount !== renders + 1) out.push('set renders ' + (s.renderCount - renders));
          const ids = s.getDecorations().map((d) => d.id).sort().join(',');
          if (ids !== 'direct,ghosts:g1') out.push('ids ' + ids);
          // Overlap renders one split span carrying both classes.
          const both = s.root.querySelector('.te-direct.te-ghost');
          if (!both || both.textContent !== 'ha') out.push('overlap ' + (both && both.textContent));
          // The same items again: no render.
          renders = s.renderCount;
          if (reg.set('ghosts', [{ id: 'g1', start: 3, end: 10, className: 'te-ghost' }])) out.push('identical set rendered');
          if (s.renderCount !== renders) out.push('identical set re-rendered');
          // A batch over two layers renders once.
          reg.batch(() => {
            reg.set('ghosts', [{ id: 'g1', start: 11, end: 16, className: 'te-ghost' }]);
            reg.set('menu', [{ start: 17, end: 22, className: 'te-menu', attributes: { 'data-x': '1' } }]);
          });
          if (s.renderCount !== renders + 1) out.push('batch renders ' + (s.renderCount - renders));
          if (reg.names().join(',') !== 'ghosts,menu') out.push('names ' + reg.names().join(','));
          const menu = s.root.querySelector('.te-menu');
          if (!menu || menu.getAttribute('data-x') !== '1' || menu.getAttribute('data-te-decoration') !== 'menu:0') out.push('menu span');
          // Live offsets follow edits; clear() removes only that layer.
          s.replaceRange(0, 0, 'xx ');
          const g = reg.current('ghosts')[0];
          if (!g || g.id !== 'g1' || g.start !== 14) out.push('current ' + JSON.stringify(g));
          reg.clear('ghosts');
          const left = s.getDecorations().map((d) => d.id).sort().join(',');
          if (left !== 'direct,menu:0') out.push('after clear ' + left);
          let threw = false;
          try { reg.set('Bad Name', []); } catch (e) { threw = true; }
          if (!threw) out.push('invalid name accepted');
          reg.clear('menu');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_registry_layers_keep_live_offsets_when_another_layer_is_set() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teInd.editor();
          const s = ed.surface;
          const reg = ed.decorations;
          s.setText('alpha beta gamma delta');
          reg.set('a', [{ id: 'w', start: 6, end: 10, className: 'te-a' }, { id: 'x', start: 11, end: 16, className: 'te-a' }]);
          // Real typing before both items, then inside the second one.
          s.focus();
          s.setSelectionOffsets(0, 0);
          document.execCommand('insertText', false, 'zz ');
          const gamma = s.getText().indexOf('gamma');
          s.setSelectionOffsets(gamma + 2, gamma + 2);
          document.execCommand('insertText', false, 'Q');
          if (s.getText() !== 'zz alpha beta gaQmma delta') out.push('text ' + JSON.stringify(s.getText()));
          const textOf = (id) => {
            const el = s.root.querySelector('[data-te-decoration~="' + id + '"]');
            return el ? el.textContent : null;
          };
          const check = (label) => {
            if (textOf('a:w') !== 'beta') out.push(label + ': a:w on ' + JSON.stringify(textOf('a:w')));
            if (textOf('a:x') !== null) out.push(label + ': dropped a:x came back on ' + JSON.stringify(textOf('a:x')));
            const cur = JSON.stringify(reg.current('a').map((d) => [d.id, d.start, d.end]));
            const got = JSON.stringify(reg.get('a').map((d) => [d.id, d.start, d.end]));
            if (cur !== '[["w",9,13]]') out.push(label + ': current ' + cur);
            if (got !== cur) out.push(label + ': stored ' + got + ' vs live ' + cur);
          };
          check('after typing');
          // Setting and clearing another layer re-merges; layer a must stay live.
          if (!reg.set('b', [{ id: '1', start: 0, end: 2, className: 'te-b' }])) out.push('set b did not render');
          check('after set b');
          reg.clear('b');
          check('after clear b');
          // The same holds through undo, which replays edits on the surface.
          s.undo();
          const live = reg.current('a').map((d) => [d.id, d.start, d.end]);
          reg.set('b', []);
          const after = reg.current('a').map((d) => [d.id, d.start, d.end]);
          if (JSON.stringify(live) !== JSON.stringify(after)) out.push('undo then set: ' + JSON.stringify([live, after]));
          if (JSON.stringify(reg.get('a').map((d) => [d.id, d.start, d.end])) !== JSON.stringify(after)) out.push('stored differs after undo');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_decoration_attributes_are_limited_to_a_safe_allow_list() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teInd.editor();
          const s = ed.surface;
          s.setText('alpha beta gamma');
          const attrs = {
            'aria-label': 'ok', 'data-ok': '1', 'data-x.y_z': '2', role: 'note',
            onclick: 'window.__teBad = 1', onmouseover: 'x', style: 'color: red', href: '#',
            'xlink:href': '#', 'data-te-decoration': 'evil', 'ARIA-label': 'x', 'data-': 'x',
            'aria-': 'x', class: 'evil', id: 'evil',
          };
          let threw = false;
          try {
            s.setDecorations([{ id: 'q', start: 0, end: 5, attributes: attrs }]);
          } catch (e) { threw = true; }
          if (threw) out.push('setDecorations threw');
          const span = s.root.querySelector('[data-te-decoration~="q"]');
          if (!span) return 'no span';
          const names = span.getAttributeNames().sort().join(',');
          if (names !== 'aria-label,class,data-ok,data-te-decoration,data-x.y_z,role') out.push('attributes ' + names);
          if (span.getAttribute('data-te-decoration') !== 'q') out.push('decoration id overwritten');
          if (span.className !== 'te-decoration') out.push('class overwritten ' + span.className);
          const kept = JSON.stringify(s.getDecorations()[0].attributes);
          if (kept !== '{"aria-label":"ok","data-ok":"1","data-x.y_z":"2","role":"note"}') out.push('stored ' + kept);
          // Through the registry: unsafe names dropped, and an identical set
          // does not re-render.
          const reg = ed.decorations;
          reg.set('c', [{ id: '1', start: 6, end: 10, attributes: { onclick: 'x', 'aria-hidden': 'true' } }]);
          const c = s.root.querySelector('[data-te-decoration~="c:1"]');
          if (!c || c.hasAttribute('onclick') || c.getAttribute('aria-hidden') !== 'true') out.push('registry attributes');
          if (reg.set('c', [{ id: '1', start: 6, end: 10, attributes: { onclick: 'x', 'aria-hidden': 'true' } }])) out.push('identical set re-rendered');
          if (window.__teBad) out.push('handler ran');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
