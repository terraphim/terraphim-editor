//! Mechanical partition of original real Lab tests; same fixtures and assertions.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
#[path = "support/lab_partition.rs"]
mod lab_partition;
use lab_partition::*;
wasm_bindgen_test_configure!(run_in_browser);

use terraphim_lab::{mark, LabAction, LabConfig};

fn expected_marks() -> String {
    let config = LabConfig::with_defaults().expect("default Lab config");
    let mut out = serde_json::Map::new();
    for action in LabAction::ALL {
        let id = serde_json::to_value(action).unwrap();
        let marks: Vec<serde_json::Value> = mark(FIXTURE, &config, action)
            .into_iter()
            .map(|m| serde_json::json!([m.start, m.end, m.kind]))
            .collect();
        out.insert(id.as_str().unwrap().to_string(), marks.into());
    }
    serde_json::Value::Object(out).to_string()
}

#[wasm_bindgen_test]
fn test_each_action_marks_exactly_the_engine_spans_and_clear_works() {
    setup();
    let src = format!(
        r##"(() => {{
          const expected = {expected};
          const out = [];
          const ed = teLab.editor();
          const s = ed.surface;
          const historyBefore = s.history.length + ':' + s.historyIndex;
          teLab.pill().click();
          const pop = teLab.popover();
          if (pop.hidden) out.push('popover not open');
          if (pop.getAttribute('role') !== 'dialog') out.push('role ' + pop.getAttribute('role'));
          const title = pop.querySelector('.te-lab-title').textContent;
          const sub = pop.querySelector('.te-lab-subtitle').textContent;
          if (title !== 'The Lab guide…' || sub !== 'what each idea does') out.push('header ' + title + ' / ' + sub);
          const labels = Array.from(pop.querySelectorAll('[role="menuitemradio"]')).map((b) => b.querySelector('.te-lab-action-label').textContent);
          const wantLabels = ['Fix punctuation and typos', 'Mark the weakest sentences', 'Mark sentences that run long',
            'Mark convoluted sentences', "Mark words that don't fit the tone", 'Mark hedges and filler'];
          if (JSON.stringify(labels) !== JSON.stringify(wantLabels)) out.push('labels ' + JSON.stringify(labels));
          // The trim slot is filled by trim.js (issue #15; tests/web_trim.rs).
          const trim = pop.querySelector('[data-slot="trim"]');
          if (!trim || trim.hidden || trim.querySelectorAll('[role="radio"]').length !== 5) out.push('trim slot missing or empty');
          // Anchored above the pill, horizontally centred on it.
          const pr = teLab.pill().getBoundingClientRect(), r = pop.getBoundingClientRect();
          if (!(r.bottom <= pr.top)) out.push('popover not above the pill: ' + r.bottom + ' vs ' + pr.top);
          if (Math.abs((r.left + r.right) / 2 - (pr.left + pr.right) / 2) > 1.5) out.push('popover not centred on the pill');

          const fence = teLab.fixture.indexOf('```');
          const fenceEnd = teLab.fixture.lastIndexOf('```') + 3;
          const headingEnd = teLab.fixture.indexOf('\n');
          for (const [id, want] of Object.entries(expected)) {{
            teLab.item(id).click();
            const got = teLab.triples();
            if (JSON.stringify(got) !== JSON.stringify(want)) out.push(id + ': ' + JSON.stringify(got) + ' != ' + JSON.stringify(want));
            if (want.length === 0) out.push(id + ': fixture marks nothing');
            if (s.getText() !== teLab.fixture) out.push(id + ': text changed');
            if (teLab.item(id).getAttribute('aria-checked') !== 'true') out.push(id + ': not checked');
            for (const m of teLab.marks()) {{
              if (m.start < headingEnd || (m.end > fence && m.start < fenceEnd)) out.push(id + ': protected text marked');
              const el = teLab.markEl(m.id);
              if (!el) {{ out.push(id + ' ' + m.id + ': not rendered'); continue; }}
              const cls = 'te-lab-mark--' + m.data.kind.replace(/_/g, '-');
              if (!el.classList.contains('te-lab-mark') || !el.classList.contains(cls)) out.push(id + ': classes ' + el.className);
              const desc = document.getElementById(el.getAttribute('aria-describedby'));
              if (!desc || !desc.textContent.includes(m.data.reason)) out.push(id + ': no accessible reason');
              if (el.getAttribute('data-lab-kind') !== m.data.kind) out.push(id + ': data-lab-kind');
              // A tint or underline, never the ghost's opacity.
              if (getComputedStyle(el).opacity !== '1') out.push(id + ': mark uses opacity');
            }}
            const legend = pop.querySelector('.te-lab-legend').textContent;
            if (!/\d/.test(legend)) out.push(id + ': legend ' + legend);
          }}
          if (!teTest.canonical()) out.push('surface not canonical');
          if (s.history.length + ':' + s.historyIndex !== historyBefore) out.push('marking touched the undo history');

          // Clear marks removes the layer, the rendered spans and the results.
          pop.querySelector('.te-lab-clear').click();
          if (teLab.marks().length !== 0) out.push('clear left marks');
          if (s.root.querySelector('.te-lab-mark')) out.push('clear left rendered marks');
          if (!pop.querySelector('.te-lab-results').hidden) out.push('results visible after clear');
          if (pop.querySelector('[role="menuitemradio"][aria-checked="true"]')) out.push('an action still checked');
          if (document.activeElement !== teLab.item('typos_and_punctuation') && !pop.contains(document.activeElement)) out.push('focus left the popover on clear');
          if (s.getText() !== teLab.fixture) out.push('clear changed text');
          return out.join('\n');
        }})()"##,
        expected = expected_marks(),
    );
    assert_eq!(js_string(&src), "");
}

#[wasm_bindgen_test]
fn test_accepting_a_proposal_is_one_undo_step_and_edits_drop_touched_marks() {
    setup();
    let result = js_string(
        r##"(() => {
          const out = [];
          const ed = teLab.editor();
          const s = ed.surface;
          const lab = teLab.lab();
          teLab.pill().click();
          teLab.item('typos_and_punctuation').click();
          const rows = Array.from(teLab.popover().querySelectorAll('.te-lab-proposal'));
          if (rows.length !== 2) out.push('proposals ' + rows.length);
          if (s.getText() !== teLab.fixture) out.push('marking changed text');
          const first = rows[0];
          if (first.querySelector('.te-lab-from').textContent !== 'recieve' || first.querySelector('.te-lab-to').textContent !== 'receive') {
            out.push('row ' + first.textContent);
          }
          if (rows[1].querySelector('.te-lab-from').textContent !== '␣,') out.push('spaces not shown: ' + rows[1].textContent);

          // Accept applies exactly the proposal, once.
          const changes = [];
          const off = s.onChange((c) => changes.push([c.edit.start, c.edit.deletedText, c.edit.insertedText, c.source]));
          const undoBefore = s.historyIndex;
          first.querySelector('.te-lab-accept').click();
          off();
          const accepted = teLab.fixture.replace('recieve', 'receive');
          if (s.getText() !== accepted) out.push('accepted text ' + JSON.stringify(s.getText().slice(0, 120)));
          const at = teLab.fixture.indexOf('recieve');
          if (JSON.stringify(changes) !== JSON.stringify([[at, 'recieve', 'receive', 'lab']])) out.push('changes ' + JSON.stringify(changes));
          if (s.historyIndex !== undoBefore + 1) out.push('not one undo step: ' + (s.historyIndex - undoBefore));
          const left = teLab.marks();
          if (left.length !== 1 || left[0].data.kind !== 'punctuation') out.push('marks after accept ' + JSON.stringify(left));
          if (teLab.popover().querySelectorAll('.te-lab-proposal').length !== 1) out.push('proposal list not refreshed');
          if (!teLab.popover().contains(document.activeElement)) out.push('focus left the popover on accept');
          if (ed.exportDocument().indexOf('receive') < 0) out.push('model not updated');

          // Undo restores the original text in one step.
          s.undo();
          if (s.getText() !== teLab.fixture) out.push('undo did not restore');
          if (s.historyIndex !== undoBefore) out.push('undo index');

          // A stale mark is refused: accept checks the text under the mark.
          if (lab.accept('nope')) out.push('accepted a missing mark');

          // Edits drop the marks they land in and move the rest.
          teLab.item('hedges_and_filler').click();
          const before = teLab.marks();
          const really = before.find((m) => m.data.text === 'really');
          const after = before.filter((m) => m.start > really.start);
          s.replaceRange(really.start + 2, really.start + 2, 'x', { source: 'typing' });
          const now = teLab.marks();
          if (now.some((m) => m.data.text === 'really')) out.push('touched mark kept');
          if (now.length !== before.length - 1) out.push('other marks dropped: ' + now.length + ' of ' + before.length);
          for (const m of after) {
            const moved = now.find((n) => n.id === m.id);
            if (!moved || moved.start !== m.start + 1) out.push('mark not moved ' + m.id);
          }
          const legend = teLab.popover().querySelector('.te-lab-legend').textContent;
          const total = (legend.match(/\d+/g) || []).reduce((a, n) => a + Number(n), 0);
          if (total !== now.length) out.push('legend not refreshed: ' + legend);
          return out.join('\n');
        })()"##,
    );
    assert_eq!(result, "");
}
