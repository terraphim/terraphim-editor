//! Real WASM, matcher, model and production scripts. Compiled here; execution
//! belongs to the parent's serial load-guarded browser gate.
#![cfg(target_arch = "wasm32")]

mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn no_role_and_empty_role_fail_closed_but_unicode_role_is_supported() {
    support::trim::setup();
    assert_eq!(run_steps(&[
        r#"const api = ed.documentApi(); api.kg_clear_thesaurus();
        const trim = ed.lab.trim;
        if (!trim.levelButtons.slice(1).every(b => b.disabled)) out.push('no-role levels enabled');
        if (trim.levelButtons[0].disabled) out.push('Original disabled');
        if (!trim.protectionNote.textContent.includes('local role')) out.push('missing explanation');
        try { api.trim_plan_json(); out.push('no-role plan succeeded'); } catch (_) {}
        api.kg_load_thesaurus('{"name":"empty","data":{}}');
        if (!trim.levelButtons.slice(1).every(b => b.disabled)) out.push('empty-role levels enabled');
        api.kg_load_thesaurus('{"name":"unsupported","data":{"Straße":{"id":1,"nterm":"strasse"}}}');
        if (trim.levelButtons.slice(1).some(b => b.disabled)) out.push('unicode-role levels disabled');
        if (trim.protectionNote.textContent.includes('unsupported_normalization')) out.push('stale diagnostic');"#,
    ]).await, "");
}

#[wasm_bindgen_test]
async fn raw_publication_and_coordinated_commit_invalidate_without_graph_startup() {
    support::trim::setup();
    assert_eq!(run_steps(&[
        r#"T.roleEvents = []; ed.root.addEventListener('te:role-change', e => T.roleEvents.push(e.detail));
        const api = ed.documentApi();
        const trim = ed.lab.trim; trim.select('sharper');
        if (!trim.plan || !trim.active.length) out.push('initial protected preview absent');
        T.generation = trim.plan.roleGeneration;
        try { api.kg_load_thesaurus('invalid'); out.push('invalid role loaded'); } catch (_) {}
        if (T.roleEvents.length || !trim.plan) out.push('invalid role mutated preview');
        api.kg_load_thesaurus('{"name":"second","data":{"unmentioned":{"id":2,"nterm":"unmentioned"}}}');
        if (trim.plan || trim.level !== 'original' || ed.decorations.get('lab-trim').length) out.push('raw load failed to clear');
        if (T.roleEvents.length !== 1 || T.roleEvents[0].editor !== ed || T.roleEvents[0].generation === T.generation) out.push('publication metadata');
        if (ed.graphClient) out.push('protection imported graph runtime');
        try { api.trim_make_cuts('sharper','[]'); out.push('old plan accepted'); } catch (_) {}
        trim.select('sharper');
        const token = api.kg_prepare_role('{"name":"prepared","data":{"unmentioned":{"id":3,"nterm":"unmentioned"}}}');
        ed.graphRoleCommitInProgress = true;
        try { api.kg_commit_prepared_role(token); } finally { ed.graphRoleCommitInProgress = false; }
        if (trim.plan || ed.decorations.get('lab-trim').length || T.roleEvents.length !== 2) out.push('coordinated commit suppressed Lab');
        api.kg_clear_thesaurus();
        if (T.roleEvents.length !== 3 || T.roleEvents[2].loaded !== false) out.push('clear event');"#,
    ]).await, "");
}

#[wasm_bindgen_test]
async fn every_occurrence_protected_and_unicode_document_plans_without_mutation() {
    support::trim::setup();
    assert_eq!(run_steps(&[
        r#"const api = ed.documentApi();
        ed.openDocument('The editor, which owns its DOM, is basically the only target here today. It is basically useful.','public-protection');
        api.kg_load_thesaurus('{"name":"public","data":{"basically":{"id":1,"nterm":"basically"},"owns its dom":{"id":2,"nterm":"owns its dom"}}}');
        const trim = ed.lab.trim;
        const plan = api.trim_plan_json();
        if (plan.protectionCount !== 3 || plan.kgProtected !== true) out.push('complete inventory');
        for (const level of ['slight','tighten','sharper','half']) {
          const status = trim.select(level);
          if (!status || status.kgProtected !== true) { out.push('missing safe level'); continue; }
          for (const cut of status.active) {
            if (s.getText().slice(cut.start,cut.end).toLowerCase().includes('basically')) out.push('concept faded');
          }
          const made = api.trim_make_cuts(level,'[]');
          if ((made.text.match(/basically/g)||[]).length !== 2 || !made.text.includes('owns its DOM')) out.push('duplicate/phrase lost');
        }
        ed.openDocument('Straße cafe\u0301','unsupported-document');
        const history = s.history.length + ':' + s.historyIndex;
        if (!trim.select('half')) out.push('unicode document failed to plan');
        if (trim.levelButtons.slice(1).some(b => b.disabled)) out.push('unicode document levels disabled');
        if (s.getText() !== 'Straße cafe\u0301' || history !== s.history.length + ':' + s.historyIndex) out.push('refusal mutated text/history');
        if (trim.live.textContent.includes('unsupported_normalization')) out.push('stale refusal');"#,
    ]).await, "");
}
