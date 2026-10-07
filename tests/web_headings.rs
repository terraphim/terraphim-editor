//! Browser tests for R-3.5 (issue #84): every ATX `#` line renders as the
//! headline in Write_On mode (bold, same face and colour, larger) with its
//! `#` marker hidden, while the document text, caret, selection and export
//! keep the marker. Run with
//! `wasm-pack test --headless --chrome --test web_headings`. Real scripts,
//! real DOM and real stylesheets; nothing is mocked. See `tests/web.rs` for
//! why the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::indicators::*;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

const DOC: &str = "# Plain heading\n\nA paragraph of body text.\n\n## Second level\n\n```\n# not a heading\n```\n\nLast line.\n";

/// Fresh editor with `DOC` open in Write_On mode and `window.teHead` helpers.
fn setup_doc(write_on: bool) {
    let _document = fresh_full_editor();
    let script = format!(
        r##"(() => {{
          const ed = window.__teEditor;
          ed.openDocument({doc}, 'headings-test-' + Math.random());
          ed.chrome.setMode({mode});
          ed.headings.flush();
          window.teHead = {{
            ed,
            s: ed.surface,
            // Rendered spans of heading line `n` and of its marker.
            line(n) {{ return Array.from(ed.surface.root.querySelectorAll('[data-te-decoration~="headings:h' + n + '"]')); }},
            marker(n) {{ return Array.from(ed.surface.root.querySelectorAll('[data-te-decoration~="headings:m' + n + '"]')); }},
            width(els) {{ return els.reduce((w, el) => w + el.getBoundingClientRect().width, 0); }},
            // After the queued selectionchange task, two animation frames.
            frame() {{ return new Promise((r) => setTimeout(() => requestAnimationFrame(() => requestAnimationFrame(r)), 0)); }},
            shown(n) {{
              const els = teHead.marker(n);
              return els.length > 0 && els.every((el) => el.classList.contains('te-heading-marker--shown'));
            }},
            // Wait (real frames, at most about a second) until marker `n` is
            // shown or hidden as `want` says.
            async until(n, want) {{
              for (let i = 0; i < 60 && teHead.shown(n) !== want; i += 1) await teHead.frame();
            }},
          }};
          return 'ok';
        }})()"##,
        doc = js_string_literal(DOC),
        mode = if write_on { "'write-on'" } else { "'plain'" },
    );
    assert_eq!(js_string(&script), "ok");
}

async fn until_shown(n: u32, want: bool) {
    let p = js_eval(&format!("teHead.until({n}, {want})"));
    JsFuture::from(p.dyn_into::<Promise>().unwrap())
        .await
        .unwrap();
}

async fn frames() {
    let p = js_eval("teHead.frame()");
    JsFuture::from(p.dyn_into::<Promise>().unwrap())
        .await
        .unwrap();
}

#[wasm_bindgen_test]
async fn test_plain_heading_is_styled_as_headline_with_marker_hidden() {
    setup_doc(true);
    // Park the caret in the body paragraph, away from both headings.
    js_eval(
        "teHead.s.focus(); teHead.s.setSelectionOffsets(teHead.s.getText().indexOf('paragraph'));",
    );
    frames().await;
    let result = js_string(
        r##"(() => {
          const { ed, s } = teHead;
          const out = [];
          const body = getComputedStyle(s.root);
          for (const [n, title] of [[0, 'Plain heading'], [1, 'Second level']]) {
            const els = teHead.line(n);
            const text = els.map((el) => el.textContent).join('');
            if (!text.endsWith(title)) out.push('h' + n + ' covers ' + JSON.stringify(text));
            const titleEl = els.find((el) => el.textContent === title);
            if (!titleEl) { out.push('h' + n + ' title span missing'); continue; }
            const cs = getComputedStyle(titleEl);
            if (Number(cs.fontWeight) < 700) out.push('h' + n + ' weight ' + cs.fontWeight);
            if (cs.fontFamily !== body.fontFamily) out.push('h' + n + ' face ' + cs.fontFamily);
            if (cs.color !== body.color) out.push('h' + n + ' colour ' + cs.color);
            const ratio = parseFloat(cs.fontSize) / parseFloat(body.fontSize);
            if (Math.abs(ratio - 1.15) > 0.01) out.push('h' + n + ' size ratio ' + ratio);
            // The marker is in the DOM text but takes no space.
            const marker = teHead.marker(n);
            const markerText = marker.map((el) => el.textContent).join('');
            if (markerText !== (n === 0 ? '# ' : '## ')) out.push('m' + n + ' text ' + JSON.stringify(markerText));
            if (teHead.width(marker) !== 0) out.push('m' + n + ' visible, width ' + teHead.width(marker));
            if (marker.some((el) => el.classList.contains('te-heading-marker--shown'))) out.push('m' + n + ' shown without the caret');
          }
          // The heading starts flush with the body text.
          const para = s.rangeForOffsets(s.getText().indexOf('A paragraph'), s.getText().indexOf('A paragraph') + 1).getBoundingClientRect();
          const title = s.rangeForOffsets(2, 3).getBoundingClientRect();
          if (Math.abs(title.left - para.left) > 1.5) out.push('heading indented ' + (title.left - para.left));
          // A `#` line inside a fenced code block is not a heading.
          const fenced = s.getText().indexOf('# not a heading');
          if (s.decorationsAt(fenced + 2).some((d) => d.id.startsWith('headings:'))) out.push('fenced line decorated');
          const ranges = ed.headings.ranges();
          if (ranges.map((r) => s.getText().slice(r.start, r.markerEnd)).join('|') !== '# |## ') out.push('ranges ' + JSON.stringify(ranges));
          // Text, export and counts keep the marker; the surface stays canonical.
          if (!s.getText().startsWith('# Plain heading\n')) out.push('text ' + JSON.stringify(s.getText().slice(0, 20)));
          if (s.root.textContent !== s.getText()) out.push('textContent differs');
          const exported = window.wasmBindings.export_document();
          if (!exported.startsWith('# Plain heading\n') || !exported.includes('\n## Second level\n')) out.push('export ' + JSON.stringify(exported.slice(0, 40)));
          const saved = window.wasmBindings.save_document();
          if (!saved.startsWith('# Plain heading\n')) out.push('save ' + JSON.stringify(saved.slice(0, 20)));
          if (!teTest.canonical()) out.push('not canonical');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_marker_is_shown_while_the_caret_is_on_the_heading_line() {
    setup_doc(true);
    js_eval(
        "teHead.s.focus(); teHead.s.setSelectionOffsets(teHead.s.getText().indexOf('paragraph'));",
    );
    frames().await;
    // Caret onto the first heading (inside the title): its marker shows.
    js_eval("teHead.s.setSelectionOffsets(5);");
    until_shown(0, true).await;
    let on_line = js_string(
        r##"(() => {
          const out = [];
          const m0 = teHead.marker(0), m1 = teHead.marker(1);
          if (!m0.every((el) => el.classList.contains('te-heading-marker--shown'))) out.push('m0 not shown');
          if (!(teHead.width(m0) > 0)) out.push('m0 width ' + teHead.width(m0));
          if (m1.some((el) => el.classList.contains('te-heading-marker--shown'))) out.push('m1 shown');
          const sel = teHead.s.getSelectionOffsets();
          if (sel.start !== 5 || sel.end !== 5) out.push('caret moved ' + JSON.stringify(sel));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(on_line, "");
    // Home: the caret before the `#` is visible and typing there works.
    let typed = js_string(
        r##"(() => {
          const s = teHead.s;
          const out = [];
          s.setSelectionOffsets(0);
          document.execCommand('insertText', false, 'Intro');
          if (!s.getText().startsWith('Intro# Plain heading\n')) out.push('text ' + JSON.stringify(s.getText().slice(0, 20)));
          let sel = s.getSelectionOffsets();
          if (sel.start !== 5 || sel.end !== 5) out.push('caret ' + JSON.stringify(sel));
          // A split before the marker makes it a heading again on the next line.
          s.replaceRange(5, 5, '\n');
          if (!s.getText().startsWith('Intro\n# Plain heading\n')) out.push('split ' + JSON.stringify(s.getText().slice(0, 20)));
          s.setSelectionOffsets(6);
          sel = s.getSelectionOffsets();
          if (sel.start !== 6 || sel.end !== 6) out.push('caret after split ' + JSON.stringify(sel));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(typed, "");
    until_shown(0, true).await;
    let after = js_string(
        r##"(() => {
          const s = teHead.s;
          const out = [];
          // The heading is back on its own line and the caret is at its
          // start, so its marker shows.
          const m0 = teHead.marker(0);
          if (m0.map((el) => el.textContent).join('') !== '# ') out.push('m0 text');
          if (!m0.every((el) => el.classList.contains('te-heading-marker--shown'))) out.push('m0 not shown at line start');
          // The `#` the caret sits before has a real box to draw it against.
          const r = s.rangeForOffsets(6, 7).getBoundingClientRect();
          if (!(r.height > 0 && r.width > 0)) out.push('marker box ' + JSON.stringify([r.width, r.height]));
          // Selection across the heading keeps exact offsets.
          s.setSelectionOffsets(6, 21, 'forward');
          const sel = s.getSelectionOffsets();
          if (sel.start !== 6 || sel.end !== 21) out.push('selection ' + JSON.stringify(sel));
          if (document.getSelection().toString() !== '# Plain heading') out.push('selected ' + JSON.stringify(document.getSelection().toString()));
          // Off the heading again: hidden.
          s.setSelectionOffsets(s.getText().indexOf('Last line'));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(after, "");
    until_shown(0, false).await;
    let off = js_string(
        r##"(() => {
          const m0 = teHead.marker(0);
          if (m0.some((el) => el.classList.contains('te-heading-marker--shown'))) return 'm0 still shown';
          return teHead.width(m0) === 0 ? '' : 'm0 width ' + teHead.width(m0);
        })()"##,
    );
    assert_eq!(off, "");
}

#[wasm_bindgen_test]
async fn test_typing_keeps_headings_styled_without_extra_renders() {
    setup_doc(true);
    let typed = js_string(
        r##"(() => {
          const s = teHead.s;
          const out = [];
          // Typing at the end of a heading: the new text joins the headline.
          const end = s.getText().indexOf('\n');
          s.focus();
          s.setSelectionOffsets(end);
          for (const ch of ' too') document.execCommand('insertText', false, ch);
          // Typing `# ` at the start of a body line makes it a heading.
          const para = s.getText().indexOf('Last line');
          s.setSelectionOffsets(para);
          for (const ch of '# ') document.execCommand('insertText', false, ch);
          window.__teHeadRenders = s.renderCount;
          return out.join('; ');
        })()"##,
    );
    assert_eq!(typed, "");
    frames().await;
    let result = js_string(
        r##"(() => {
          const s = teHead.s;
          const out = [];
          const h0 = teHead.line(0).map((el) => el.textContent).join('');
          if (h0 !== '# Plain heading too') out.push('h0 ' + JSON.stringify(h0));
          const last = teHead.line(2).map((el) => el.textContent).join('');
          if (last !== '# Last line.') out.push('new heading ' + JSON.stringify(last));
          // Typing in a body paragraph re-renders nothing.
          const renders = s.renderCount;
          s.setSelectionOffsets(s.getText().indexOf('body text'));
          for (const ch of 'more ') document.execCommand('insertText', false, ch);
          window.__teHeadRenders = renders;
          if (!teTest.canonical()) out.push('not canonical');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
    frames().await;
    let renders = js_string(
        r##"(() => {
          const s = teHead.s;
          const out = [];
          if (s.renderCount !== window.__teHeadRenders) out.push('body typing re-rendered ' + (s.renderCount - window.__teHeadRenders));
          // Word counts and the export see the plain text, marker included.
          if (!window.wasmBindings.export_document().includes('# Plain heading too\n')) out.push('export');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(renders, "");
}

#[wasm_bindgen_test]
fn test_headings_are_not_styled_in_plain_mode() {
    setup_doc(false);
    let result = js_string(
        r##"(() => {
          const { ed, s } = teHead;
          const out = [];
          const check = (label) => {
            if (s.getDecorations().some((d) => d.id.startsWith('headings:'))) out.push(label + ': decorations set');
            if (s.root.querySelector('.te-heading, .te-heading-marker')) out.push(label + ': styled spans');
          };
          check('plain');
          const renders = s.renderCount;
          s.focus();
          s.setSelectionOffsets(5);
          document.execCommand('insertText', false, 'x');
          if (s.renderCount !== renders) out.push('plain typing re-rendered');
          ed.chrome.setMode('write-on');
          if (teHead.line(0).length === 0) out.push('write-on headings missing');
          ed.chrome.setMode('plain');
          check('toggled back');
          ed.destroy();
          if (!ed.headings.destroyed) out.push('layer not destroyed');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
fn test_headline_span_with_alternatives_keeps_one_size() {
    let _document = fresh_full_editor();
    install_helpers();
    let result = js_string(
        r##"(() => {
          teInd.open();
          const ed = teInd.editor();
          const s = ed.surface;
          ed.headings.flush();
          const out = [];
          // s1 is the fixture's headline sentence with alternatives.
          const el = teInd.decorationEl('s1');
          if (!el || !el.classList.contains('te-ind-headline')) out.push('te-ind-headline missing');
          if (!el.classList.contains('te-heading')) out.push('te-heading missing on the headline span');
          const ratio = parseFloat(getComputedStyle(el).fontSize) / parseFloat(getComputedStyle(s.root).fontSize);
          if (Math.abs(ratio - 1.15) > 0.01) out.push('size ratio ' + ratio);
          const marker = Array.from(s.root.querySelectorAll('[data-te-decoration~="headings:m0"]'));
          if (marker.map((m) => m.textContent).join('') !== '# ') out.push('marker ' + JSON.stringify(marker.map((m) => m.textContent)));
          if (document.querySelectorAll('.te-indicators [data-span-id]').length !== 4) out.push('indicators');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
