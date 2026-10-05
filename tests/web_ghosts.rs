//! Browser tests for the ghost layer and the selection context menu (issue
//! #11, spec R-5.1 to R-5.3, R-7.3, R-7.4). Run with `wasm-pack test
//! --headless --chrome`. Real scripts, real stylesheets and the real exported
//! document API (`ghost_range`, `revive_range`, ...); nothing is mocked. The
//! one extra menu item registered below goes through the public plug-in API
//! that issues #10, #12 and #13 will use. See `tests/web.rs` for why the
//! browser tests are split across binaries; this one stays small.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;
use terraphim_alternatives::{write, Document, Source, SpanKind};

wasm_bindgen_test_configure!(run_in_browser);

const TEXT: &str = "Keep this one. Ghost this whole sentence please. Last words here.";

/// A document whose middle sentence holds a word ("whole") with two
/// alternatives besides the original, built through the crate.
fn annotated() -> String {
    let mut doc = Document::new(TEXT);
    let start = TEXT.find("whole").unwrap();
    let id = doc
        .add_span(SpanKind::Word, start, start + "whole".len())
        .unwrap();
    doc.add_alternative(&id, "entire", Source::Human, None)
        .unwrap();
    doc.add_alternative(&id, "full", Source::Human, None)
        .unwrap();
    write(&doc)
}

fn install_helpers() {
    let src = format!(
        r##"(() => {{
          window.teGh = {{
            text: {text},
            annotated: {annotated},
            ed() {{ return window.__teEditor; }},
            s() {{ return window.__teEditor.surface; }},
            open(md) {{ teGh.ed().openDocument(md || teGh.text, 'ghosts-test-' + Math.random()); }},
            // Select `needle` (the first occurrence) on a focused surface.
            select(needle, from) {{
              const s = teGh.s();
              const at = s.getText().indexOf(needle, from || 0);
              s.focus();
              s.setSelectionOffsets(at, at + needle.length);
              return [at, at + needle.length];
            }},
            model() {{ return teGh.ed().annotations().ghosts.map((g) => [g.anchor.start, g.anchor.end, g.anchor.text]); }},
            drawn() {{ return teGh.ed().decorations.current('ghosts').map((d) => [d.start, d.end]); }},
            els() {{ return Array.from(teGh.s().root.querySelectorAll('.te-ghost')); }},
            menu() {{ return document.querySelector('.te-selection-menu'); }},
            labels() {{
              const m = teGh.menu();
              return m ? Array.from(m.querySelectorAll('[role="menuitem"] .te-selection-menu-label')).map((x) => x.textContent) : null;
            }},
            active() {{ const a = document.activeElement; return a && a.dataset ? a.dataset.item || a.className : ''; }},
            // Right-click over the middle of the selection (or offset `at`).
            contextmenu(at) {{
              const s = teGh.s();
              const sel = s.getSelectionOffsets();
              const p = at === undefined ? Math.floor((sel.start + sel.end) / 2) : at;
              const r = s.rangeForOffsets(p, p + 1).getBoundingClientRect();
              const ev = new MouseEvent('contextmenu', {{
                bubbles: true, cancelable: true, button: 2,
                clientX: r.left + r.width / 2, clientY: r.top + r.height / 2,
              }});
              s.root.dispatchEvent(ev);
              return ev.defaultPrevented;
            }},
            ctrlSlash(target) {{ return teTest.key(target || teGh.s().root, '/', {{ ctrlKey: true, code: 'Slash' }}); }},
            // Alpha of a computed colour: rgb(), rgba() or color(srgb ... / a).
            alpha(c) {{
              const slash = c.match(/\/\s*([\d.]+)\s*\)$/);
              if (slash) return Number(slash[1]);
              const rgba = c.match(/^rgba\(([^)]+)\)$/);
              if (rgba) return Number(rgba[1].split(',')[3]);
              return 1;
            }},
          }};
          return 'ok';
        }})()"##,
        text = js_string_literal(TEXT),
        annotated = js_string_literal(&annotated()),
    );
    assert_eq!(js_string(&src), "ok");
}

fn setup() {
    let _document = fresh_full_editor();
    install_helpers();
    // These tests cover the ghost item (and plug-in items) on their own; the
    // "Alternatives for selection" item (#10) is covered in web_alt_panel.rs
    // and "AI alternatives for selection" (#13) in web_kg_menu.rs.
    assert_eq!(
        js_string(
            "const m = window.__teEditor.selectionMenu; \
             m.unregister('alternatives'); m.unregister('ai-alternatives'); 'ok'"
        ),
        "ok"
    );
}

fn problems(src: &str) {
    let result = js_string(src);
    assert_eq!(result, "", "{result}");
}

#[wasm_bindgen_test]
fn test_ghost_and_revive_via_menu_and_shortcut() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open();
          const [a, b] = teGh.select('Ghost this whole sentence please.');
          // Right-click over the selection opens the menu instead of the native one.
          if (!teGh.contextmenu()) out.push('contextmenu not prevented');
          const m = teGh.menu();
          if (!m) return 'no menu';
          if (m.getAttribute('role') !== 'menu') out.push('menu role');
          // Only Ghost it is implemented; the other R-7.3 items are hidden, not disabled.
          if (JSON.stringify(teGh.labels()) !== '["Ghost it"]') out.push('labels ' + JSON.stringify(teGh.labels()));
          const row = m.querySelector('[role="menuitem"]');
          if (row.getAttribute('aria-keyshortcuts') !== 'Control+/') out.push('aria-keyshortcuts ' + row.getAttribute('aria-keyshortcuts'));
          if (row.querySelector('.te-selection-menu-key').textContent !== 'Ctrl+/') out.push('shortcut text');
          // Tokens: dark popover, rounded, monospace, dim shortcut.
          const cs = getComputedStyle(m);
          if (cs.backgroundColor !== 'rgb(23, 26, 46)') out.push('popover colour ' + cs.backgroundColor);
          if (cs.borderTopLeftRadius !== '8px') out.push('radius ' + cs.borderTopLeftRadius);
          if (!cs.fontFamily.includes('monospace')) out.push('font ' + cs.fontFamily);
          if (getComputedStyle(row.querySelector('.te-selection-menu-key')).color !== 'rgb(143, 135, 129)') out.push('dim shortcut');
          if (document.activeElement !== row) out.push('first item not focused');
          // Near the pointer, inside the viewport.
          const mr = m.getBoundingClientRect();
          const sr = teGh.s().rangeForOffsets(a, b).getBoundingClientRect();
          if (Math.abs(mr.top - sr.bottom) > 120) out.push('menu far from the selection');
          row.click();
          if (teGh.menu()) out.push('menu still open after click');
          if (JSON.stringify(teGh.model()) !== JSON.stringify([[a, b, 'Ghost this whole sentence please.']])) out.push('model ' + JSON.stringify(teGh.model()));
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify([[a, b]])) out.push('drawn ' + JSON.stringify(teGh.drawn()));
          // Faded via --te-ghost-opacity, still in the text, the counts and the selection.
          const el = teGh.els()[0];
          const alpha = el ? teGh.alpha(getComputedStyle(el).color) : 1;
          if (Math.abs(alpha - 0.1) > 0.02) out.push('ghost alpha ' + alpha + ' ' + (el && getComputedStyle(el).color));
          if (teGh.s().getText() !== teGh.text) out.push('text changed');
          if (teGh.ed().counts().words !== 11) out.push('counts ' + JSON.stringify(teGh.ed().counts()));
          const sel = teGh.s().getSelectionOffsets();
          if (sel.start !== a || sel.end !== b) out.push('selection lost ' + JSON.stringify(sel));
          if (document.activeElement !== teGh.s().root) out.push('focus not back on the surface');
          // Ctrl+/ on an entirely ghosted selection revives it ...
          if (!teGh.ctrlSlash()) out.push('Ctrl+/ not handled');
          if (teGh.model().length !== 0 || teGh.els().length !== 0) out.push('not revived ' + JSON.stringify(teGh.model()));
          // ... and ghosts it again.
          teGh.ctrlSlash();
          if (teGh.model().length !== 1) out.push('Ctrl+/ did not ghost');
          // The menu now offers Revive for text inside the ghost.
          teGh.select('whole');
          teGh.contextmenu();
          if (JSON.stringify(teGh.labels()) !== '["Revive"]') out.push('revive label ' + JSON.stringify(teGh.labels()));
          teGh.ed().selectionMenu.close();
          // Collapsed caret outside any ghost: the native menu, and Ctrl+/ does nothing.
          teGh.s().setSelectionOffsets(2);
          if (teGh.contextmenu(2)) out.push('native menu suppressed');
          if (teGh.menu()) out.push('menu opened with nothing to offer');
          if (teGh.ctrlSlash()) out.push('Ctrl+/ prevented with nothing to do');
          // Collapsed caret inside a ghost: Revive the whole ghost (R-5.2).
          teGh.s().setSelectionOffsets(a + 3);
          teGh.ctrlSlash();
          if (teGh.model().length !== 0) out.push('caret revive ' + JSON.stringify(teGh.model()));
          // Both ends count (a caret placed just before the first or just
          // after the last ghosted character); one unit outside does not.
          for (const [at, revives] of [[a, true], [b, true], [a - 1, false], [b + 1, false]]) {
            teGh.ed().ghosts.ghost(a, b);
            teGh.s().setSelectionOffsets(at);
            teGh.ctrlSlash();
            if ((teGh.model().length === 0) !== revives) out.push('caret at ' + (at - a) + ' revives ' + !revives);
            teGh.ed().ghosts.revive(a, b);
          }
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
fn test_ghost_over_sentence_with_alternatives_keeps_the_indicator() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open(teGh.annotated);
          teGh.ed().chrome.setMode('write-on');
          teGh.ed().indicators.flush();
          const [a, b] = teGh.select('Ghost this whole sentence please.');
          const r = teGh.ed().ghosts.toggle();
          if (!r.ok) return 'ghost failed ' + JSON.stringify(r);
          // The span is untouched and the ghost covers it (independent layers).
          const ann = teGh.ed().annotations();
          if (ann.spans.length !== 1 || ann.spans[0].alts.length !== 3) out.push('span changed');
          if (JSON.stringify(teGh.model()) !== JSON.stringify([[a, b, 'Ghost this whole sentence please.']])) out.push('model');
          // One rendered span carries both decorations.
          const both = teGh.s().root.querySelector('.te-ghost.te-ind');
          if (!both || both.textContent !== 'whole') out.push('no shared span ' + (both && both.textContent));
          else {
            const cs = getComputedStyle(both);
            if (Math.abs(teGh.alpha(cs.color) - 0.1) > 0.02) out.push('word alpha ' + cs.color);
            // The underline keeps its own colour and the description its id.
            if (cs.textDecorationColor !== 'rgba(140, 134, 230, 0.4)') out.push('underline ' + cs.textDecorationColor);
            if (!both.getAttribute('aria-describedby')) out.push('aria-describedby lost');
          }
          // The dots are still shown, at full strength (outside the ghost).
          const holder = document.querySelector('.te-indicators [data-span-id]');
          if (!holder || !teTest.visible(holder)) out.push('indicator hidden');
          else if (holder.querySelectorAll('.te-ind-dot').length !== 3) out.push('dot count');
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
fn test_partial_revive_splits_and_ghosts_merge() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open();
          const g = teGh.ed().ghosts;
          const [a, b] = teGh.select('Ghost this whole sentence please.');
          g.toggle();
          // Revive one word in the middle: the ghost is split in two.
          const [w0, w1] = teGh.select('whole');
          teGh.ctrlSlash();
          const want = [[a, w0, 'Ghost this '], [w1, b, ' sentence please.']];
          if (JSON.stringify(teGh.model()) !== JSON.stringify(want)) out.push('split ' + JSON.stringify(teGh.model()));
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify(want.map((x) => [x[0], x[1]]))) out.push('drawn ' + JSON.stringify(teGh.drawn()));
          if (teGh.els().map((e) => e.textContent).join('|') !== 'Ghost this | sentence please.') out.push('elements');
          // Ghosting across the gap merges everything back into one ghost.
          teGh.s().setSelectionOffsets(w0 - 2, w1 + 2);
          teGh.ctrlSlash();
          if (JSON.stringify(teGh.model()) !== JSON.stringify([[a, b, 'Ghost this whole sentence please.']])) out.push('merge ' + JSON.stringify(teGh.model()));
          // The event reports each change.
          let seen = null;
          document.addEventListener('te:ghosts-change', (e) => { seen = e.detail; }, { once: true });
          g.revive(a, a + 6);
          if (!seen || seen.action !== 'revive' || seen.ghosts.length !== 1 || seen.ghosts[0].start !== a + 6) out.push('event ' + JSON.stringify(seen && seen.ghosts));
          // Invalid ranges come back as error objects; nothing changes.
          const bad = g.ghost(5, 5);
          if (bad.ok !== false || bad.kind !== 'invalid-range') out.push('error object ' + JSON.stringify(bad));
          if (teGh.model().length !== 1) out.push('changed by an invalid range');
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
fn test_ghosts_persist_through_save_and_reopen_and_skip_export() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open();
          const [a, b] = teGh.select('Ghost this whole sentence please.');
          teGh.ctrlSlash();
          const exported = teGh.ed().exportDocument();
          if (exported.includes('Ghost') || !exported.includes('Keep this one.') || !exported.includes('Last words here.')) out.push('export ' + JSON.stringify(exported));
          const saved = teGh.ed().saveDocument();
          if (!saved.includes('terraphim-alternatives') || !saved.includes('"ghosts"')) out.push('no block');
          // Reopen in a fresh editor state: the ghost is drawn again.
          teGh.open('Something else entirely.');
          if (teGh.els().length !== 0) out.push('stale ghost after opening another file');
          teGh.open(saved);
          if (teGh.s().getText() !== teGh.text) out.push('body ' + JSON.stringify(teGh.s().getText()));
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify([[a, b]])) out.push('not redrawn ' + JSON.stringify(teGh.drawn()));
          if (teGh.els().map((e) => e.textContent).join('') !== 'Ghost this whole sentence please.') out.push('elements');
          if (teGh.ed().saveDocument() !== saved) out.push('save/open not lossless');
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
async fn test_ghost_follows_typing_elsewhere_and_inside() {
    setup();
    let before = js_string(
        r##"(() => {
          teGh.open();
          teGh.select('Ghost this whole sentence please.');
          teGh.ctrlSlash();
          const g = teGh.ed().ghosts;
          const reads = g.refreshCount;
          // Real typing before the ghost.
          teGh.s().setSelectionOffsets(0);
          document.execCommand('insertText', false, 'Oh. ');
          const out = [];
          // The decoration moved with the text at once, without a model read.
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify([[19, 52]])) out.push('drawn ' + JSON.stringify(teGh.drawn()));
          if (g.refreshCount !== reads) out.push('read the model per keystroke');
          if (!g.pending()) out.push('no debounced confirmation pending');
          window.__ghReads = reads;
          return out.join('\n');
        })()"##,
    );
    assert_eq!(before, "", "{before}");
    // The trailing debounce confirms against the model (one read, no change).
    sleep(200).await;
    problems(
        r##"(() => {
          const out = [];
          const g = teGh.ed().ghosts;
          if (g.refreshCount !== window.__ghReads + 1) out.push('debounced reads ' + (g.refreshCount - window.__ghReads));
          if (JSON.stringify(teGh.model()) !== JSON.stringify([[19, 52, 'Ghost this whole sentence please.']])) out.push('model ' + JSON.stringify(teGh.model()));
          // Typing inside the ghost: the model grows it; the surface dropped
          // the decoration and gets it back on the next frame.
          teGh.s().setSelectionOffsets(25);
          document.execCommand('insertText', false, 'XY');
          window.__ghFrame = g.pending();
          return out.join('\n');
        })()"##,
    );
    sleep(50).await;
    problems(
        r##"(() => {
          const out = [];
          if (!window.__ghFrame) out.push('no frame refresh scheduled');
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify([[19, 54]])) out.push('drawn after typing inside ' + JSON.stringify(teGh.drawn()));
          if (teGh.els().map((e) => e.textContent).join('') !== 'Ghost XYthis whole sentence please.') out.push('elements ' + teGh.els().map((e) => e.textContent).join('|'));
          if (!teTest.canonical()) out.push('surface not canonical');
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
fn test_menu_keyboard_navigation_and_plugins() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open();
          const menu = teGh.ed().selectionMenu;
          const runs = [];
          // A later issue's item, registered through the public API (slot order
          // puts it after Ghost it), plus one that is never available.
          menu.register({ id: 'stash', label: 'Stash this in Overflow', key: 'ctrl+shift+x', run: (ctx) => runs.push(ctx.text) });
          menu.register({ id: 'alternatives', label: 'Alternatives for selection', key: 'ctrl+shift+a', available: () => false, run: () => runs.push('alt') });
          if (JSON.stringify(menu.ids()) !== '["alternatives","ghost","stash"]') out.push('order ' + JSON.stringify(menu.ids()));
          const [a, b] = teGh.select('Last words');
          // Shift+F10 opens it under the selection.
          if (!teTest.key(teGh.s().root, 'F10', { shiftKey: true })) out.push('Shift+F10 not handled');
          if (JSON.stringify(teGh.labels()) !== '["Ghost it","Stash this in Overflow"]') out.push('labels ' + JSON.stringify(teGh.labels()));
          const nav = (key, want) => {
            teTest.key(document.activeElement, key);
            if (teGh.active() !== want) out.push(key + ' -> ' + teGh.active());
            const act = teGh.menu().querySelector('.te-selection-menu-item--active');
            if (!act || act.dataset.item !== want) out.push(key + ' highlight');
          };
          if (teGh.active() !== 'ghost') out.push('first focus ' + teGh.active());
          // Keyboard-opened: just below the selection's line, at its left edge.
          const lineRects = Array.from(teGh.s().rangeForOffsets(a, b).getClientRects()).filter((r) => r.width > 0);
          const mr = teGh.menu().getBoundingClientRect();
          const lineBottom = Math.max(...lineRects.map((r) => r.bottom));
          const lineLeft = Math.min(...lineRects.map((r) => r.left));
          if (mr.top < lineBottom || mr.top - lineBottom > 24) out.push('menu top ' + mr.top + ' vs line bottom ' + lineBottom);
          if (Math.abs(mr.left - lineLeft) > 2) out.push('menu left ' + mr.left + ' vs ' + lineLeft);
          nav('ArrowDown', 'stash');
          nav('ArrowDown', 'ghost');
          nav('ArrowUp', 'stash');
          nav('Home', 'ghost');
          nav('End', 'stash');
          // The Menu key's trailing contextmenu event keeps the menu open.
          if (!teGh.contextmenu()) out.push('second contextmenu not prevented');
          if (!teGh.menu()) out.push('menu closed by the second contextmenu');
          // Escape closes and gives the surface back its focus and selection.
          teTest.key(document.activeElement, 'Escape');
          if (teGh.menu()) out.push('Escape did not close');
          const sel = teGh.s().getSelectionOffsets();
          if (document.activeElement !== teGh.s().root || sel.start !== a || sel.end !== b) out.push('focus/selection after Escape ' + JSON.stringify(sel));
          // The Menu key, then Enter on the second item runs the plug-in.
          teTest.key(teGh.s().root, 'ContextMenu');
          teTest.key(document.activeElement, 'ArrowDown');
          teTest.key(document.activeElement, 'Enter');
          if (JSON.stringify(runs) !== '["Last words"]') out.push('runs ' + JSON.stringify(runs));
          if (teGh.menu()) out.push('menu open after Enter');
          // Its shortcut works on the surface too; the unavailable one does not.
          teTest.key(teGh.s().root, 'X', { ctrlKey: true, shiftKey: true, code: 'KeyX' });
          teTest.key(teGh.s().root, 'A', { ctrlKey: true, shiftKey: true, code: 'KeyA' });
          if (runs.length !== 2) out.push('shortcut runs ' + JSON.stringify(runs));
          // Enter on Ghost it ghosts the selection.
          teTest.key(teGh.s().root, 'ContextMenu');
          teTest.key(document.activeElement, 'Enter');
          if (JSON.stringify(teGh.model()) !== JSON.stringify([[a, b, 'Last words']])) out.push('ghost via Enter ' + JSON.stringify(teGh.model()));
          // A click outside and a scroll both close it.
          teTest.key(teGh.s().root, 'ContextMenu');
          document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
          if (teGh.menu()) out.push('outside click did not close');
          teTest.key(teGh.s().root, 'ContextMenu');
          window.dispatchEvent(new Event('scroll'));
          if (teGh.menu()) out.push('scroll did not close');
          // Ctrl only: Cmd+/ is left to the browser (Safari binds it).
          teGh.select('Last words');
          if (teTest.key(teGh.s().root, '/', { metaKey: true, code: 'Slash' })) out.push('Cmd+/ taken');
          if (teGh.model().length !== 1) out.push('Cmd+/ changed the ghosts');
          // Config shortcuts still work (no collision): Ctrl+B bolds.
          teGh.select('Keep');
          teTest.key(teGh.s().root, 'b', { ctrlKey: true });
          if (!teGh.s().getText().startsWith('**Keep**')) out.push('Ctrl+B broken');
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
fn test_destroy_cleans_up() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open();
          teGh.select('Ghost this whole sentence please.');
          teGh.ctrlSlash();
          const ed = teGh.ed();
          const root = ed.surface.root;
          teGh.select('Keep this');
          teGh.contextmenu();
          if (!teGh.menu()) return 'menu did not open';
          ed.destroy();
          if (document.querySelector('.te-selection-menu')) out.push('menu left behind');
          if (!ed.ghosts.destroyed || !ed.selectionMenu.destroyed) out.push('not destroyed');
          if (ed.decorations.names().includes('ghosts') && ed.decorations.get('ghosts').length) out.push('ghost layer left');
          // Nothing reacts any more.
          const ev = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2, clientX: 5, clientY: 5 });
          root.dispatchEvent(ev);
          if (ev.defaultPrevented || document.querySelector('.te-selection-menu')) out.push('contextmenu still handled');
          if (teTest.key(root, '/', { ctrlKey: true })) out.push('Ctrl+/ still handled');
          window.__teEditor = null;
          return out.join('\n');
        })()"##,
    );
}

#[wasm_bindgen_test]
fn test_deleting_ghosted_text_then_undo_restores_the_ghost() {
    setup();
    problems(
        r##"(() => {
          const out = [];
          teGh.open();
          const [a, b] = teGh.select('Ghost this whole sentence please.');
          teGh.ctrlSlash();
          // Delete exactly the ghosted text: the live ghost goes (R-5.3) but
          // the model keeps it set aside, and a save keeps it.
          teGh.select('Ghost this whole sentence please.');
          document.execCommand('delete');
          if (teGh.s().getText().includes('Ghost this')) return 'text not deleted';
          const ann = teGh.ed().annotations();
          if (ann.ghosts.length !== 0 || ann.setAside.ghosts.length !== 1) out.push('after delete ' + JSON.stringify([ann.ghosts.length, ann.setAside.ghosts.length]));
          if (!teGh.ed().saveDocument().includes('Ghost this whole sentence please.')) out.push('save lost the ghost');
          // Undo brings the text back and the ghost with it.
          teGh.s().undo();
          if (teGh.s().getText() !== teGh.text) out.push('undo text ' + JSON.stringify(teGh.s().getText()));
          teGh.ed().ghosts.flush();
          if (JSON.stringify(teGh.model()) !== JSON.stringify([[a, b, 'Ghost this whole sentence please.']])) out.push('model after undo ' + JSON.stringify(teGh.model()));
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify([[a, b]])) out.push('drawn after undo ' + JSON.stringify(teGh.drawn()));
          if (teGh.ed().annotations().setAside.ghosts.length !== 0) out.push('still set aside');
          // Save and reopen after the undo: one live ghost, nothing set aside.
          const saved = teGh.ed().saveDocument();
          teGh.open(saved);
          if (JSON.stringify(teGh.drawn()) !== JSON.stringify([[a, b]])) out.push('drawn after reopen ' + JSON.stringify(teGh.drawn()));
          const re = teGh.ed().annotations();
          if (re.ghosts.length !== 1 || re.setAside.ghosts.length !== 0) out.push('reopen ' + JSON.stringify([re.ghosts.length, re.setAside.ghosts.length]));
          return out.join('\n');
        })()"##,
    );
}
