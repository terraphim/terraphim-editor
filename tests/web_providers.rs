//! Browser tests for the alternatives provider seam (issue #83, R-8.7): a
//! host page registers a real asynchronous provider (a deterministic
//! function behind a real browser timer, standing in for a future LLM call)
//! and "AI alternatives for selection" appends its results after the
//! thesaurus's, as source `ai` with the provider's model, bot-marked, as ONE
//! undo step. Real scripts and the real exported document API; nothing is
//! mocked. See `tests/web.rs` for why the browser tests are split across
//! binaries.
#![cfg(target_arch = "wasm32")]

mod support;

use support::kg::setup;
use support::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Await a script that returns a promise of a string.
async fn js_await_string(src: &str) -> String {
    let promise: Promise = js_eval(src)
        .dyn_into()
        .expect("snippet should return a promise");
    JsFuture::from(promise)
        .await
        .expect("promise should resolve")
        .as_string()
        .expect("promise should resolve to a string")
}

/// Run an async `body` (statements using `out`, `teKg`, `ed`) after
/// [`setup`] and return the problems found, joined.
async fn run_async(body: &str) -> String {
    setup(KG_DOC);
    js_await_string(&format!(
        r##"(async () => {{
          const out = [];
          const ed = teKg.ed;
          const later = (ms, value) => new Promise((resolve) => setTimeout(() => resolve(value), ms));
          {body}
          return out.join('; ');
        }})()"##
    ))
    .await
}

#[wasm_bindgen_test]
async fn test_the_thesaurus_is_the_default_provider_and_the_sync_path_is_unchanged() {
    let body = r##"
          if (JSON.stringify(ed.alternativesProviderIds()) !== '["thesaurus"]') out.push('ids ' + JSON.stringify(ed.alternativesProviderIds()));
          const choice = teKg.at('choice');
          ed.surface.setSelectionOffsets(choice, choice + 6);
          // No host provider: alternativesForSelection acts before its first
          // await, so the change is there when the call returns.
          const promise = ed.alternativesForSelection();
          const span = ed.annotations().spans.find((x) => x.anchor.text === 'choice');
          const lines = span ? span.alts.map((a) => a.text + ':' + a.source + (a.model ? '/' + a.model : '')).join(' ') : 'none';
          if (lines !== 'choice:original decision:ai/kg judgment:ai/kg option:ai/kg') out.push('lines ' + lines);
          const change = await promise;
          if (!change || change.span !== span.id) out.push('change ' + JSON.stringify(change));
    "##;
    assert_eq!(run_async(body).await, "");
}

#[wasm_bindgen_test]
async fn test_a_host_provider_is_appended_after_the_thesaurus_as_one_undo_step() {
    let body = r##"
          let asked = null;
          const off = ed.registerAlternativesProvider({
            id: 'llm',
            model: 'llama-3',
            alternatives: async (ctx) => {
              asked = ctx;
              // Real asynchronous wait; "decision" repeats the thesaurus.
              return later(20, ['decision', 'pick', { text: 'selection', model: 'llama-3-70b' }]);
            },
          });
          if (JSON.stringify(ed.alternativesProviderIds()) !== '["thesaurus","llm"]') out.push('ids ' + JSON.stringify(ed.alternativesProviderIds()));
          const choice = teKg.at('choice');
          const s = ed.surface;
          s.focus();
          s.setSelectionOffsets(choice, choice + 6);
          const depth = s.historyIndex;
          // Ctrl+Shift+G takes the key and appends once the provider answers.
          if (!teKg.key('G', { ctrlKey: true, shiftKey: true })) out.push('Ctrl+Shift+G not taken');
          if (ed.annotations().spans.length !== 0) out.push('appended before the provider answered');
          await later(120);
          if (!asked || asked.text !== 'choice' || asked.start !== choice || asked.end !== choice + 6) out.push('asked ' + JSON.stringify(asked));
          const span = ed.annotations().spans.find((x) => x.anchor.text === 'choice');
          if (!span) {
            out.push('no span');
          } else {
            const lines = span.alts.map((a) => a.text + ':' + a.source + (a.model ? '/' + a.model : '')).join(' ');
            const want = 'choice:original decision:ai/kg judgment:ai/kg option:ai/kg pick:ai/llama-3 selection:ai/llama-3-70b';
            if (lines !== want) out.push('lines ' + lines);
          }
          if (s.historyIndex !== depth + 1) out.push('history ' + (s.historyIndex - depth));
          if (s.getText().indexOf('choice') !== choice) out.push('text changed');
          // The panel lists the bot-marked lines.
          const robots = document.querySelectorAll('.te-alt-glyph--ai.fa-robot').length;
          if (robots !== 5) out.push('robot glyphs ' + robots);
          // One undo removes them all; redo restores them.
          s.undo();
          if (ed.annotations().spans.length !== 0) out.push('span after undo');
          s.redo();
          if (ed.annotations().spans.length !== 1) out.push('span after redo');
          // Asking again adds nothing and records nothing.
          s.setSelectionOffsets(choice, choice + 6);
          const depth2 = s.historyIndex;
          if ((await ed.alternativesForSelection()) !== null) out.push('second ask changed something');
          if (s.historyIndex !== depth2) out.push('second ask recorded');
          if (!off() || off()) out.push('unregister result');
          if (JSON.stringify(ed.alternativesProviderIds()) !== '["thesaurus"]') out.push('ids after ' + JSON.stringify(ed.alternativesProviderIds()));
    "##;
    assert_eq!(run_async(body).await, "");
}

#[wasm_bindgen_test]
async fn test_a_host_provider_offers_the_menu_item_on_any_selection() {
    let body = r##"
          const menu = ed.selectionMenu;
          const avail = (start, end) => menu.availableItems(menu.contextFor({ start, end })).map((i) => i.id);
          const every = teKg.at('Every');
          if (avail(every, every + 5).includes('ai-alternatives')) out.push('offered on Every without a host provider');
          ed.registerAlternativesProvider({ id: 'llm', model: 'm', alternatives: async () => ['every one'] });
          if (!avail(every, every + 5).includes('ai-alternatives')) out.push('not offered on Every');
          if (avail(every, every).includes('ai-alternatives')) out.push('offered on a bare caret away from a term');
          ed.surface.setSelectionOffsets(every, every + 5);
          const change = await ed.alternativesForSelection();
          const span = ed.annotations().spans.find((x) => x.anchor.text === 'Every');
          if (!change || !span) {
            out.push('nothing appended ' + JSON.stringify(change));
          } else {
            const lines = span.alts.map((a) => a.text + ':' + a.source + (a.model ? '/' + a.model : '')).join(' ');
            if (lines !== 'Every:original every one:ai/m') out.push('lines ' + lines);
          }
    "##;
    assert_eq!(run_async(body).await, "");
}

#[wasm_bindgen_test]
async fn test_a_failing_or_stale_provider_never_corrupts_the_document() {
    let body = r##"
          const choice = teKg.at('choice');
          const s = ed.surface;
          ed.registerAlternativesProvider({ id: 'bad', alternatives: async () => { throw new Error('offline'); } });
          ed.registerAlternativesProvider({ id: 'worse', alternatives: () => 'not an array' });
          ed.registerAlternativesProvider({ id: 'ok', model: 'm', alternatives: async () => ['fine'] });
          s.setSelectionOffsets(choice, choice + 6);
          // The failing providers are skipped; the thesaurus and the good one answer.
          await ed.alternativesForSelection();
          const span = ed.annotations().spans.find((x) => x.anchor.text === 'choice');
          const lines = span ? span.alts.map((a) => a.text + ':' + (a.model || '')).join(' ') : 'none';
          if (lines !== 'choice: decision:kg judgment:kg option:kg fine:m') out.push('lines ' + lines);
          s.undo();

          // The text changes while the provider is still thinking: nothing is appended.
          ed.unregisterAlternativesProvider('bad');
          ed.unregisterAlternativesProvider('worse');
          ed.unregisterAlternativesProvider('ok');
          ed.registerAlternativesProvider({ id: 'slow', alternatives: () => later(40, ['late']) });
          s.setSelectionOffsets(choice, choice + 6);
          const pending = ed.alternativesForSelection();
          s.replaceRange(0, 0, 'X', { source: 'test' });
          const result = await pending;
          if (result !== null) out.push('appended into changed text');
          if (ed.annotations().spans.length !== 0) out.push('span created in changed text');
          if (ed.surface.getText() === '' || ed.surface.getText()[0] !== 'X') out.push('text lost');

          // The reserved id, and a stale handle that must not remove a later registration.
          try { ed.registerAlternativesProvider({ id: 'thesaurus', alternatives: async () => [] }); out.push('thesaurus id accepted'); } catch (e) { /* refused */ }
          const offA = ed.registerAlternativesProvider({ id: 'dup', alternatives: async () => ['a'] });
          ed.registerAlternativesProvider({ id: 'dup', alternatives: async () => ['b'] });
          if (offA()) out.push('stale handle removed the later provider');
          if (!ed.alternativesProviderIds().includes('dup')) out.push('dup gone');
          ed.unregisterAlternativesProvider('dup');
          // A provider replaced while its answer is in flight is not used.
          ed.registerAlternativesProvider({ id: 'swap', model: 'old', alternatives: () => later(40, ['stale']) });
          const c2 = teKg.at('choice');
          s.setSelectionOffsets(c2, c2 + 6);
          const inflight = ed.alternativesForSelection();
          ed.registerAlternativesProvider({ id: 'swap', model: 'new', alternatives: async () => ['fresh'] });
          await inflight;
          const swapped = ed.annotations().spans.map((x) => x.alts.map((a) => a.text).join('|')).join(';');
          if (swapped.includes('stale')) out.push('stale answer used ' + swapped);
          if (!swapped.includes('decision')) out.push('thesaurus lines missing ' + swapped);
          ed.unregisterAlternativesProvider('swap');

          // Bad registrations are refused.
          for (const bad of [null, {}, { id: 'x' }, { id: '', alternatives() {} }]) {
            try { ed.registerAlternativesProvider(bad); out.push('accepted ' + JSON.stringify(bad)); } catch (e) { /* refused */ }
          }
          // Destroying the editor unregisters its providers.
          ed.destroy();
          if (JSON.stringify(window.wasmBindings.alternatives_provider_ids()) !== '["thesaurus"]') out.push('ids after destroy ' + JSON.stringify(window.wasmBindings.alternatives_provider_ids()));
    "##;
    assert_eq!(run_async(body).await, "");
}

#[wasm_bindgen_test]
async fn test_destroying_one_editor_keeps_another_editors_provider_of_the_same_id() {
    let body = r##"
          const ids = () => JSON.stringify(ed.alternativesProviderIds());
          // A second editor object on the page (it needs no surface to hold
          // a registration) registers the same id with another model.
          const other = new MarkdownEditor(window.EditorConfig);
          ed.registerAlternativesProvider({ id: 'llm', model: 'first', alternatives: async () => ['from first'] });
          other.registerAlternativesProvider({ id: 'llm', model: 'second', alternatives: async () => ['from second'] });
          if (ids() !== '["thesaurus","llm"]') out.push('ids ' + ids());
          // Another holder leaving restores the remaining holder's model in the registry.
          const third = new MarkdownEditor(window.EditorConfig);
          third.registerAlternativesProvider({ id: 'llm', model: 'third', alternatives: async () => [] });
          third.destroy();
          // Destroying the other editor must not unregister the first's provider.
          other.destroy();
          if (ids() !== '["thesaurus","llm"]') out.push('ids after other.destroy ' + ids());
          const choice = teKg.at('choice');
          ed.surface.setSelectionOffsets(choice, choice + 6);
          await ed.alternativesForSelection();
          const span = ed.annotations().spans.find((x) => x.anchor.text === 'choice');
          const lines = span ? span.alts.map((a) => a.text + ':' + (a.model || '')).join(' ') : 'none';
          // Each editor's own model name is recorded, not the registry's.
          if (lines !== 'choice: decision:kg judgment:kg option:kg from first:first') out.push('lines ' + lines);
          // A bare string answer (no model from JS) takes the remaining holder's registered model.
          const direct = window.wasmBindings.alt_append_alternatives(choice, choice + 6, JSON.stringify({ llm: ['bare'] }));
          const bare = direct.after.alts.find((a) => a.text === 'bare');
          if (!bare || bare.model !== 'first') out.push('registry model ' + JSON.stringify(bare));
          // The last holder letting go removes it.
          ed.unregisterAlternativesProvider('llm');
          if (ids() !== '["thesaurus"]') out.push('ids after last ' + ids());
          if (other.unregisterAlternativesProvider('llm')) out.push('double unregister');
    "##;
    assert_eq!(run_async(body).await, "");
}
