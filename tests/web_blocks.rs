//! Browser tests for the Blocks view (issue #19). Run with `wasm-pack test
//! --headless --chrome`. Real scripts (`public/js/blocks.js` with the real
//! editor and chrome), the real exported document API and real fixtures;
//! nothing is mocked. See `tests/web.rs` for why the browser tests are split
//! across binaries; this one has its own 20 s budget, so every test yields
//! once after setting up and then runs one bounded synchronous snippet.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

const MIXED_MD: &str = include_str!("fixtures/blocks/mixed.md");
const EDGE_MD: &str = include_str!("fixtures/blocks/edge.md");
const FULL_MD: &str = include_str!("fixtures/persistence/full.md");
const PLAIN_MD: &str = include_str!("fixtures/persistence/plain.md");

/// Make the fixtures available to JavaScript as `window.teBlockFixtures`.
fn install_fixtures() {
    let src = format!(
        "window.teBlockFixtures = {{ mixed: {}, edge: {}, full: {}, plain: {} }}; 'ok'",
        js_string_literal(MIXED_MD),
        js_string_literal(EDGE_MD),
        js_string_literal(FULL_MD),
        js_string_literal(PLAIN_MD)
    );
    assert_eq!(js_string(&src), "ok");
}

#[wasm_bindgen_test]
async fn test_body_to_blocks_round_trip_is_byte_identical() {
    let _document = fresh_full_editor();
    // Yield so the webdriver poll is never starved (see tests/web.rs).
    sleep(0).await;
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const out = [];
          const P = BlocksView.parse;
          const S = BlocksView.serialise;
          const check = (name, text) => {
            const m = P(text);
            if (S(m) !== text) { out.push(name + ': round trip differs'); return m; }
            let at = m.lead.length;
            for (const b of m.blocks) {
              if (b.start !== at) out.push(name + ': gap before block at ' + b.start);
              if (text.slice(b.start, b.end) !== b.text) out.push(name + ': text slice at ' + b.start);
              if (/^[ \t\r]*\n|\n[ \t\r]*$/.test(b.text) || b.text.trim() === '') out.push(name + ': blank edge in block at ' + b.start);
              if (b.sep.trim() !== '') out.push(name + ': content in separator at ' + b.end);
              at = b.end + b.sep.length;
            }
            if (at !== text.length) out.push(name + ': does not cover the text');
            return m;
          };
          const types = (m) => m.blocks.map((b) => b.type + (b.level || '')).join(',');
          const mixed = check('mixed', teBlockFixtures.mixed);
          const wantMixed = 'heading1,paragraph,heading2,list,list,code,quote,table,rule,code,paragraph';
          if (types(mixed) !== wantMixed) out.push('mixed types ' + types(mixed));
          const edge = check('edge', teBlockFixtures.edge);
          if (types(edge) !== 'paragraph,heading1,paragraph,code') out.push('edge types ' + types(edge));
          check('plain', teBlockFixtures.plain);
          check('full', teBlockFixtures.full);
          for (const t of ['', '\n', '\n\n  \n', 'x', '# h', '```\n\n```', '> q\n\n\n']) check(JSON.stringify(t), t);

          // Property: random documents built from Markdown fragments, joined
          // by random separators, always round-trip (fixed seed, bounded).
          const fragments = [
            '# Heading', '## Sub *em*', 'Setext\n======', 'Setext two\n---',
            'Plain paragraph.', 'Two line\nparagraph with `code`.',
            '- a\n- b', '* x\n  * nested\n* y', '1. one\n2. two', '- loose\n\n  continued',
            '```js\nlet a = 1;\n\n# not heading\n```', '~~~\nunclosed', '    indented\n    code',
            '> quote\n> more', '> lazy\ncontinued', '| a | b |\n|---|:-:|\n| 1 | 2 |', 'a | b\n--|--\n1 | 2',
            '***', '- - -', 'Café 𝄞 é́ text  ', '<div>html</div>', '\tTabbed',
          ];
          let seed = 0x5eed1234;
          const rand = (n) => {
            seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
            return seed % n;
          };
          const seps = ['\n', '\n\n', '\n\n\n', '\n \n', ''];
          for (let iter = 0; iter < 250 && out.length === 0; iter += 1) {
            let doc = ['', '\n', '\n\n', ' \n'][rand(4)];
            const count = 1 + rand(8);
            for (let k = 0; k < count; k += 1) {
              doc += fragments[rand(fragments.length)];
              if (k + 1 < count) doc += seps[rand(seps.length)];
            }
            doc += ['', '\n', '\n\n', '\n\n\n'][rand(4)];
            check('random #' + iter, doc);
            if (out.length) out.push('document ' + JSON.stringify(doc));
          }
          return out.slice(0, 5).join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_view_toggle_preference_and_destroy() {
    let _document = fresh_full_editor();
    // Yield so the webdriver poll is never starved (see tests/web.rs).
    sleep(0).await;
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          const bv = ed.blocks;
          const s = ed.surface;
          if (!bv) return 'no blocks view';
          const group = document.querySelector('.toolbar .te-view-toggle');
          if (!group) return 'no view toggle in the toolbar';
          const blocksBtn = group.querySelector('[data-view="blocks"]');
          const textBtn = group.querySelector('[data-view="text"]');
          if (!teTest.visible(group)) out.push('toggle not visible in plain mode');
          if (bv.view !== 'text') out.push('starts in ' + bv.view);
          if (teTest.visible(bv.container)) out.push('blocks visible before switching');
          if (bv.list.children.length !== 0) out.push('blocks rendered before use');
          if (group.querySelector('i.fa-solid') === null) out.push('no FontAwesome icon');

          const seen = [];
          const onView = (e) => seen.push(e.detail.view);
          document.addEventListener('te:view-change', onView);
          s.setText('# Title\n\nSome text.\n');
          blocksBtn.click();
          if (bv.view !== 'blocks') out.push('click did not switch');
          if (!teTest.visible(bv.container)) out.push('blocks not visible');
          if (teTest.visible(s.root)) out.push('surface still visible');
          if (blocksBtn.getAttribute('aria-pressed') !== 'true' || textBtn.getAttribute('aria-pressed') !== 'false') out.push('aria-pressed');
          if (bv.cards().length !== 2) out.push('cards ' + bv.cards().length);
          const h = bv.cards()[0].querySelector('.te-block-body h1');
          if (!h || h.textContent !== 'Title') out.push('heading not rendered by the Rust converter');
          if (localStorage.getItem(BLOCKS_VIEW_STORAGE_KEY) !== 'blocks') out.push('preference not stored');
          if (s.getText() !== '# Title\n\nSome text.\n') out.push('switching changed the body');
          textBtn.click();
          if (bv.view !== 'text' || !teTest.visible(s.root) || teTest.visible(bv.container)) out.push('switch back');
          if (localStorage.getItem(BLOCKS_VIEW_STORAGE_KEY) !== 'text') out.push('text preference not stored');
          document.removeEventListener('te:view-change', onView);
          if (seen.join() !== 'blocks,text') out.push('events ' + seen.join());

          // The stored preference is restored by the next editor.
          bv.setView('blocks');
          ed.destroy();
          if (document.querySelector('.te-blocks')) out.push('blocks container left behind');
          if (document.querySelector('.te-view-toggle')) out.push('toggle left behind');
          if (s.root.hidden) out.push('surface left hidden');
          const fresh = new MarkdownEditor(window.EditorConfig);
          fresh.initialize();
          window.__teEditor = fresh;
          if (!fresh.blocks || fresh.blocks.view !== 'blocks') out.push('preference not restored');
          if (document.querySelectorAll('.te-view-toggle').length !== 1) out.push('toggle duplicated');
          // Listeners are gone with the old editor: its button is detached and
          // the old view ignores further calls.
          bv.setView('text');
          if (fresh.blocks.view !== 'blocks') out.push('old view affected the new one');
          teTest.resetBlocks();
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_block_edit_updates_body_preview_and_keeps_annotations() {
    let _document = fresh_full_editor();
    // Yield so the webdriver poll is never starved (see tests/web.rs).
    sleep(0).await;
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          const bv = ed.blocks;
          const s = ed.surface;
          ed.openDocument(teBlockFixtures.full);
          const body0 = s.getText();
          const saved0 = ed.saveDocument();
          const ann0 = ed.annotations();
          bv.setView('blocks');
          // The annotation block is never shown, and switching is lossless.
          if (bv.list.textContent.includes('terraphim-alternatives')) out.push('annotation block shown');
          if (BlocksView.serialise(bv.model) !== body0) out.push('blocks differ from body');
          if (ed.saveDocument() !== saved0) out.push('switching changed the saved document');
          const types = bv.model.blocks.map((b) => b.type).join(',');
          if (types !== 'heading,paragraph,paragraph') out.push('types ' + types);

          // Edit the middle paragraph (it carries s2, s3 and g1) at its end.
          const para = bv.model.blocks[1];
          const ta = bv.editBlock(1);
          if (!ta || ta.value !== para.text) return 'editor did not open with the block text';
          ta.value = para.text + ' More.';
          bv.commitEdit();
          const body1 = s.getText();
          if (body1 !== body0.slice(0, para.end) + ' More.' + body0.slice(para.end)) out.push('body ' + JSON.stringify(body1));
          if (window.wasmBindings.document_body() !== body1) out.push('model body differs from surface');
          if (!teTest.preview().includes('together. More.')) out.push('preview not updated');
          if (!bv.cards()[1].textContent.includes('More.')) out.push('card not re-rendered');
          if (document.activeElement !== bv.cards()[1]) out.push('focus not back on the edited block');
          const ann1 = ed.annotations();
          const shift = (g) => ({ ...g, anchor: { ...g.anchor, start: g.anchor.start + 6, end: g.anchor.end + 6 } });
          if (JSON.stringify(ann1.spans) !== JSON.stringify(ann0.spans)) out.push('spans changed ' + JSON.stringify(ann1.spans));
          const wantGhosts = [ann0.ghosts[0], shift(ann0.ghosts[1])];
          if (JSON.stringify(ann1.ghosts) !== JSON.stringify(wantGhosts)) out.push('ghosts ' + JSON.stringify(ann1.ghosts));
          if (ann1.overflow !== ann0.overflow) out.push('overflow changed');
          if (ed.setAsideCount !== 0) out.push('set aside ' + ed.setAsideCount);

          // One undo step reverts the block edit, from the Blocks view.
          teTest.key(bv.cards()[1], 'z', { ctrlKey: true });
          if (s.getText() !== body0) out.push('undo did not restore the body');
          if (JSON.stringify(ed.annotations()) !== JSON.stringify(ann0)) out.push('undo did not restore annotations');
          if (bv.cards()[1].textContent.includes('More.')) out.push('cards not refreshed after undo');
          teTest.key(document.activeElement, 'y', { ctrlKey: true });
          if (s.getText() !== body1) out.push('redo');

          // Save and reopen: the edit and every annotation persist.
          const saved1 = ed.saveDocument();
          ed.openDocument('other');
          ed.openDocument(saved1);
          if (s.getText() !== body1) out.push('reopened body');
          const ann2 = ed.annotations();
          if (JSON.stringify([ann2.spans, ann2.ghosts, ann2.overflow]) !== JSON.stringify([ann1.spans, ann1.ghosts, ann1.overflow])) out.push('annotations lost on reopen');
          if (bv.view !== 'blocks' || bv.cards().length !== 3) out.push('view not refreshed on open');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_keyboard_navigation_focus_and_caret_across_views() {
    let _document = fresh_full_editor();
    // Yield so the webdriver poll is never starved (see tests/web.rs).
    sleep(0).await;
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          const bv = ed.blocks;
          const s = ed.surface;
          const doc = '# A\n\npara one\n\npara two\n';
          s.setText(doc);
          s.focus();
          s.setSelectionOffsets(15);
          bv.setView('blocks');
          const at = () => document.activeElement && document.activeElement.dataset ? document.activeElement.dataset.index : null;
          if (at() !== '2') out.push('caret block not focused: ' + at());
          const focused = document.activeElement;
          if (getComputedStyle(focused).outlineStyle !== 'solid') out.push('no visible focus outline');
          if (bv.cards().filter((c) => c.tabIndex === 0).length !== 1) out.push('roving tabindex');
          if (bv.list.getAttribute('role') !== 'list' || focused.getAttribute('role') !== 'listitem') out.push('roles');
          if (!/block 3 of 3/.test(focused.getAttribute('aria-label'))) out.push('aria-label ' + focused.getAttribute('aria-label'));
          teTest.key(document.activeElement, 'ArrowUp');
          if (at() !== '1') out.push('ArrowUp ' + at());
          teTest.key(document.activeElement, 'Home');
          if (at() !== '0') out.push('Home ' + at());
          teTest.key(document.activeElement, 'ArrowUp');
          if (at() !== '0') out.push('ArrowUp at top ' + at());
          teTest.key(document.activeElement, 'End');
          if (at() !== '2') out.push('End ' + at());
          teTest.key(document.activeElement, 'ArrowDown');
          if (at() !== '2') out.push('ArrowDown at bottom ' + at());

          // Enter edits, Escape cancels without touching the body.
          teTest.key(document.activeElement, 'Enter');
          let ta = bv.list.querySelector('textarea.te-block-editor');
          if (!ta || document.activeElement !== ta || ta.value !== 'para two') return out.concat('Enter did not open the editor').join('; ');
          ta.value = 'discarded';
          teTest.key(ta, 'Escape');
          if (bv.list.querySelector('textarea')) out.push('Escape left the editor open');
          if (s.getText() !== doc) out.push('Escape changed the body');
          if (at() !== '2') out.push('focus after Escape ' + at());

          // Ctrl+Enter commits.
          teTest.key(document.activeElement, 'Enter');
          ta = bv.list.querySelector('textarea.te-block-editor');
          ta.value = 'para 2';
          teTest.key(ta, 'Enter', { ctrlKey: true });
          if (s.getText() !== '# A\n\npara one\n\npara 2\n') out.push('Ctrl+Enter ' + JSON.stringify(s.getText()));

          // Switching back commits an open edit and maps its selection.
          teTest.key(document.activeElement, 'Enter');
          ta = bv.list.querySelector('textarea.te-block-editor');
          ta.value = 'para TWO!';
          ta.setSelectionRange(5, 8);
          bv.setView('text');
          const text = s.getText();
          if (text !== '# A\n\npara one\n\npara TWO!\n') out.push('commit on switch ' + JSON.stringify(text));
          const sel = s.getSelectionOffsets();
          if (sel.start !== 20 || sel.end !== 23) out.push('selection ' + JSON.stringify(sel));
          if (document.activeElement !== s.root) out.push('surface not focused');
          if (!teTest.canonical()) out.push('surface not canonical');

          // Without an edit, the caret goes to the start of the focused block.
          bv.setView('blocks');
          if (at() !== '2') out.push('selection block ' + at());
          teTest.key(document.activeElement, 'ArrowUp');
          bv.setView('text');
          const sel2 = s.getSelectionOffsets();
          if (sel2.start !== 5 || sel2.end !== 5) out.push('caret at block start ' + JSON.stringify(sel2));

          // Undo in the text view steps back over the block edits.
          s.undo();
          if (s.getText() !== '# A\n\npara one\n\npara 2\n') out.push('undo across views ' + JSON.stringify(s.getText()));
          s.undo();
          if (s.getText() !== doc) out.push('second undo ' + JSON.stringify(s.getText()));
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_insert_delete_and_write_on_mode() {
    let _document = fresh_full_editor();
    // Yield so the webdriver poll is never starved (see tests/web.rs).
    sleep(0).await;
    install_fixtures();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = window.__teEditor;
          const bv = ed.blocks;
          const s = ed.surface;
          s.setText('# A\n\npara one\n\npara two\n');
          bv.setView('blocks');
          const expect = (name, want) => {
            if (s.getText() !== want) out.push(name + ' ' + JSON.stringify(s.getText()));
            if (BlocksView.serialise(bv.model) !== s.getText()) out.push(name + ': stale blocks');
          };
          let ta = bv.insertBlockAfter(0);
          ta.value = 'new';
          bv.commitEdit();
          expect('insert', '# A\n\nnew\n\npara one\n\npara two\n');
          teTest.key(bv.cards()[3], 'Delete');
          expect('delete last', '# A\n\nnew\n\npara one\n');
          if (document.activeElement !== bv.cards()[2]) out.push('focus after delete');
          bv.deleteBlock(0);
          expect('delete first', 'new\n\npara one\n');
          ta = bv.insertBlockAfter(1);
          ta.value = 'end';
          bv.commitEdit();
          expect('append', 'new\n\npara one\n\nend\n');
          s.undo();
          expect('one undo step per operation', 'new\n\npara one\n');
          ta = bv.insertBlockAfter(0);
          ta.value = '   ';
          bv.commitEdit();
          expect('blank insert ignored', 'new\n\npara one\n');
          if (bv.list.querySelector('.te-block-move, [data-action="up"], [data-action="down"]')) out.push('move controls present');

          // A new block between annotated blocks shifts every later anchor
          // and sets nothing aside.
          ed.openDocument(teBlockFixtures.full);
          const before = ed.annotations();
          ta = bv.insertBlockAfter(0);
          ta.value = 'Inserted.';
          bv.commitEdit();
          const after = ed.annotations();
          const d = 'Inserted.\n\n'.length;
          const moved = (a) => a.anchor.start >= 31 ? { ...a, anchor: { ...a.anchor, start: a.anchor.start + d, end: a.anchor.end + d } } : a;
          if (JSON.stringify([after.spans, after.ghosts]) !== JSON.stringify([before.spans.map(moved), before.ghosts.map(moved)])) out.push('anchors after insert ' + JSON.stringify(after.ghosts));
          if (ed.setAsideCount !== 0) out.push('set aside after insert');

          // Write_On: Blocks is not shown; it resumes in plain mode.
          const chrome = ed.chrome;
          chrome.toggle();
          if (document.body.dataset.mode !== 'write-on') out.push('not in write-on');
          if (bv.view !== 'text') out.push('blocks still active in write-on');
          if (!teTest.visible(s.root)) out.push('surface hidden in write-on');
          if (teTest.visible(bv.container) || teTest.visible(document.querySelector('.te-view-toggle'))) out.push('blocks UI visible in write-on');
          bv.setView('blocks');
          if (bv.view !== 'text') out.push('blocks allowed in write-on');
          chrome.toggle();
          if (bv.view !== 'blocks') out.push('blocks not resumed in plain mode');
          return out.join('; ');
        })()"##,
    );
    assert_eq!(result, "");
}
