//! Browser tests for the decoration registry (issue #8, spec R-3.1): named
//! layers merged into one render, live offsets across layers, and the
//! attribute allow-list. Run with `wasm-pack test --headless --chrome`. Real
//! scripts, real stylesheets, the real exported document API and the
//! committed fixture `tests/fixtures/indicators/indicators.md`; nothing is
//! mocked. The inline indicators themselves are in `tests/web_indicators.rs`.
//! See `tests/web.rs` for why the browser tests are split across binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::indicators::*;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

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
