//! Browser tests for the Blocks view (issue #19): round trip, view toggle,
//! block edits with annotations, keyboard navigation and caret mapping. Run
//! with `wasm-pack test --headless --chrome`. Real scripts
//! (`public/js/blocks.js` with the real editor and chrome), the real exported
//! document API and real fixtures; nothing is mocked.
//!
//! See `tests/web.rs` for why the browser tests are split across binaries.
//! The Blocks tests are split between this binary and `web_blocks_edit.rs`
//! (structural edits, kept drafts, action buttons), and every scenario runs
//! as short steps with a yield between them (`support::run_steps`), so the
//! webdriver poll is never starved on a loaded host.
#![cfg(target_arch = "wasm32")]

mod support;

use support::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn test_body_to_blocks_round_trip_is_byte_identical() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_block_fixtures();
    let result = run_steps(&[
        // Shared checker: every block is a slice, separators hold no content,
        // and the blocks cover the text.
        r##"
          T.check = (problems, name, text) => {
            const m = BlocksView.parse(text);
            if (BlocksView.serialise(m) !== text) { problems.push(name + ': round trip differs'); return m; }
            let at = m.lead.length;
            for (const b of m.blocks) {
              if (b.start !== at) problems.push(name + ': gap before block at ' + b.start);
              if (text.slice(b.start, b.end) !== b.text) problems.push(name + ': text slice at ' + b.start);
              if (/^[ \t\r]*\n|\n[ \t\r]*$/.test(b.text) || b.text.trim() === '') problems.push(name + ': blank edge in block at ' + b.start);
              if (b.sep.trim() !== '') problems.push(name + ': content in separator at ' + b.end);
              at = b.end + b.sep.length;
            }
            if (at !== text.length) problems.push(name + ': does not cover the text');
            return m;
          };
          const types = (m) => m.blocks.map((b) => b.type + (b.level || '')).join(',');
          const mixed = T.check(out, 'mixed', teBlockFixtures.mixed);
          const wantMixed = 'heading1,paragraph,heading2,list,list,code,quote,table,rule,code,paragraph';
          if (types(mixed) !== wantMixed) out.push('mixed types ' + types(mixed));
          const edge = T.check(out, 'edge', teBlockFixtures.edge);
          if (types(edge) !== 'paragraph,heading1,paragraph,code') out.push('edge types ' + types(edge));
          T.check(out, 'plain', teBlockFixtures.plain);
          T.check(out, 'full', teBlockFixtures.full);
          for (const t of ['', '\n', '\n\n  \n', 'x', '# h', '```\n\n```', '> q\n\n\n']) T.check(out, JSON.stringify(t), t);
        "##,
        // Property: random documents built from Markdown fragments, joined
        // by random separators, always round-trip (fixed seed, bounded).
        r##"
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
            T.check(out, 'random #' + iter, doc);
            if (out.length) out.push('document ' + JSON.stringify(doc));
          }
          return out.slice(0, 5).join('; ');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_view_toggle_preference_and_destroy() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let result = run_steps(&[
        r##"
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
        "##,
        // The stored preference is restored by the next editor.
        r##"
          bv.setView('blocks');
          ed.destroy();
          if (document.querySelector('.te-blocks')) out.push('blocks container left behind');
          if (document.querySelector('.te-blocks-drafts')) out.push('drafts notice left behind');
          if (document.querySelector('.te-view-toggle')) out.push('toggle left behind');
          if (s.root.hidden) out.push('surface left hidden');
          const fresh = new MarkdownEditor(window.EditorConfig);
          fresh.initialize();
          window.__teEditor = fresh;
          if (!fresh.blocks || fresh.blocks.view !== 'blocks') out.push('preference not restored');
          if (document.querySelectorAll('.te-view-toggle').length !== 1) out.push('toggle duplicated');
          // The old view is inert after destroy().
          bv.setView('text');
          if (fresh.blocks.view !== 'blocks') out.push('old view affected the new one');
          teTest.resetBlocks();
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_block_edit_updates_body_preview_and_keeps_annotations() {
    let _document = fresh_full_editor();
    sleep(0).await;
    install_block_fixtures();
    let result = run_steps(&[
        r##"
          ed.openDocument(teBlockFixtures.full);
          T.body0 = s.getText();
          T.saved0 = ed.saveDocument();
          T.ann0 = ed.annotations();
          bv.setView('blocks');
          // The annotation block is never shown, and switching is lossless.
          if (bv.list.textContent.includes('terraphim-alternatives')) out.push('annotation block shown');
          if (BlocksView.serialise(bv.model) !== T.body0) out.push('blocks differ from body');
          if (ed.saveDocument() !== T.saved0) out.push('switching changed the saved document');
          const types = bv.model.blocks.map((b) => b.type).join(',');
          if (types !== 'heading,paragraph,paragraph') out.push('types ' + types);
        "##,
        // Edit the middle paragraph (it carries s2, s3 and g1) at its end.
        r##"
          const para = bv.model.blocks[1];
          const ta = bv.editBlock(1);
          if (!ta || ta.value !== para.text) return 'editor did not open with the block text';
          ta.value = para.text + ' More.';
          bv.commitEdit();
          const body0 = T.body0;
          T.body1 = s.getText();
          if (T.body1 !== body0.slice(0, para.end) + ' More.' + body0.slice(para.end)) out.push('body ' + JSON.stringify(T.body1));
          if (window.wasmBindings.document_body() !== T.body1) out.push('model body differs from surface');
          if (!teTest.preview().includes('together. More.')) out.push('preview not updated');
          if (!bv.cards()[1].textContent.includes('More.')) out.push('card not re-rendered');
          if (document.activeElement !== bv.cards()[1]) out.push('focus not back on the edited block');
          const ann0 = T.ann0;
          T.ann1 = ed.annotations();
          const shift = (g) => ({ ...g, anchor: { ...g.anchor, start: g.anchor.start + 6, end: g.anchor.end + 6 } });
          if (JSON.stringify(T.ann1.spans) !== JSON.stringify(ann0.spans)) out.push('spans changed ' + JSON.stringify(T.ann1.spans));
          const wantGhosts = [ann0.ghosts[0], shift(ann0.ghosts[1])];
          if (JSON.stringify(T.ann1.ghosts) !== JSON.stringify(wantGhosts)) out.push('ghosts ' + JSON.stringify(T.ann1.ghosts));
          if (T.ann1.overflow !== ann0.overflow) out.push('overflow changed');
          if (ed.setAsideCount !== 0) out.push('set aside ' + ed.setAsideCount);
        "##,
        // One undo step reverts the block edit, from the Blocks view; then
        // save and reopen keep the edit and every annotation.
        r##"
          teTest.key(bv.cards()[1], 'z', { ctrlKey: true });
          if (s.getText() !== T.body0) out.push('undo did not restore the body');
          if (JSON.stringify(ed.annotations()) !== JSON.stringify(T.ann0)) out.push('undo did not restore annotations');
          if (bv.cards()[1].textContent.includes('More.')) out.push('cards not refreshed after undo');
          teTest.key(document.activeElement, 'y', { ctrlKey: true });
          if (s.getText() !== T.body1) out.push('redo');
          const saved1 = ed.saveDocument();
          ed.openDocument('other');
          ed.openDocument(saved1);
          if (s.getText() !== T.body1) out.push('reopened body');
          const ann2 = ed.annotations();
          const ann1 = T.ann1;
          if (JSON.stringify([ann2.spans, ann2.ghosts, ann2.overflow]) !== JSON.stringify([ann1.spans, ann1.ghosts, ann1.overflow])) out.push('annotations lost on reopen');
          if (bv.view !== 'blocks' || bv.cards().length !== 3) out.push('view not refreshed on open');
        "##,
    ])
    .await;
    assert_eq!(result, "");
}

#[wasm_bindgen_test]
async fn test_keyboard_navigation_focus_and_caret_across_views() {
    let _document = fresh_full_editor();
    sleep(0).await;
    let result = run_steps(&[
        r##"
          T.doc = '# A\n\npara one\n\npara two\n';
          s.setText(T.doc);
          s.focus();
          s.setSelectionOffsets(15);
          bv.setView('blocks');
          const at = () => document.activeElement && document.activeElement.dataset ? document.activeElement.dataset.index : null;
          T.at = at;
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
        "##,
        // Enter edits, Escape cancels, Ctrl+Enter commits.
        r##"
          const at = T.at;
          teTest.key(document.activeElement, 'Enter');
          let ta = bv.list.querySelector('textarea.te-block-editor');
          if (!ta || document.activeElement !== ta || ta.value !== 'para two') return 'Enter did not open the editor';
          ta.value = 'discarded';
          teTest.key(ta, 'Escape');
          if (bv.list.querySelector('textarea')) out.push('Escape left the editor open');
          if (s.getText() !== T.doc) out.push('Escape changed the body');
          if (at() !== '2') out.push('focus after Escape ' + at());
          teTest.key(document.activeElement, 'Enter');
          ta = bv.list.querySelector('textarea.te-block-editor');
          ta.value = 'para 2';
          teTest.key(ta, 'Enter', { ctrlKey: true });
          if (s.getText() !== '# A\n\npara one\n\npara 2\n') out.push('Ctrl+Enter ' + JSON.stringify(s.getText()));
        "##,
        // Switching back commits an open edit and maps its selection; without
        // an edit the caret goes to the start of the focused block; undo in
        // the text view steps back over the block edits.
        r##"
          const at = T.at;
          teTest.key(document.activeElement, 'Enter');
          const ta = bv.list.querySelector('textarea.te-block-editor');
          ta.value = 'para TWO!';
          ta.setSelectionRange(5, 8);
          bv.setView('text');
          const text = s.getText();
          if (text !== '# A\n\npara one\n\npara TWO!\n') out.push('commit on switch ' + JSON.stringify(text));
          const sel = s.getSelectionOffsets();
          if (sel.start !== 20 || sel.end !== 23) out.push('selection ' + JSON.stringify(sel));
          if (document.activeElement !== s.root) out.push('surface not focused');
          if (!teTest.canonical()) out.push('surface not canonical');
          bv.setView('blocks');
          if (at() !== '2') out.push('selection block ' + at());
          teTest.key(document.activeElement, 'ArrowUp');
          bv.setView('text');
          const sel2 = s.getSelectionOffsets();
          if (sel2.start !== 5 || sel2.end !== 5) out.push('caret at block start ' + JSON.stringify(sel2));
          s.undo();
          if (s.getText() !== '# A\n\npara one\n\npara 2\n') out.push('undo across views ' + JSON.stringify(s.getText()));
          s.undo();
          if (s.getText() !== T.doc) out.push('second undo ' + JSON.stringify(s.getText()));
        "##,
    ])
    .await;
    assert_eq!(result, "");
}
