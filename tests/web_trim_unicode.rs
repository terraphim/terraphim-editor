//! Real browser/WASM regression for Unicode KG protection, Trim and history.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn unicode_trim_reduces_and_preserves_originals_through_done_save_and_history() {
    support::trim::setup();
    let result = run_steps(&[
        r#"T.body = '🧠 Café cafe\u0301 STRAẞE Α Я ΟΣ·Α are basically quite useful. The editor is basically the only target here today.';
        ed.openDocument(T.body, 'unicode-trim');
        const api = ed.documentApi();
        api.kg_load_thesaurus(JSON.stringify({name:'unicode',data:{'café':{id:1,nterm:'café'},'straße':{id:2,nterm:'straße'},'α':{id:3,nterm:'α'},'я':{id:4,nterm:'я'},'ος':{id:5,nterm:'ος'}}}));
        const plan = api.trim_plan_json();
        if (plan.protectionCount !== 7 || !plan.kgProtected) out.push('original occurrence inventory');
        T.reduced = false;
        for (const level of ['slight','tighten','sharper','half']) {
          const made = api.trim_make_cuts(level, '[]');
          for (const exact of ['Café','cafe\u0301','STRAẞE','Α','Я','ΟΣ·Α']) if (!made.text.includes(exact)) out.push('lost exact original '+exact);
          T.reduced ||= made.text.length < T.body.length;
        }
        if (!T.reduced) out.push('no actual reduction');
        T.before = ed.saveDocument();
        const trim = ed.lab.trim; trim.select('sharper');
        if (!trim.active.length || trim.levelButtons.slice(1).some(b=>b.disabled)) out.push('no enabled safe cuts');
        trim.card.querySelector('.te-trim-done').click();
        if (!ed.annotations().ghosts.length) out.push('Done did not create ghosts');
        if (s.getText() !== T.body) out.push('Done rewrote original Unicode');
        T.after = ed.saveDocument();
        if (!s.undo() || ed.saveDocument() !== T.before) out.push('actual Undo failed');
        if (!s.redo() || ed.saveDocument() !== T.after) out.push('actual Redo failed');
        ed.openDocument(T.after, 'unicode-reopened');
        if (ed.saveDocument() !== T.after || s.getText() !== T.body) out.push('save/reopen bytes changed');
        if (api.trim_plan_json().protectionCount !== 7) out.push('reopen lost recognition');"#,
    ]).await;
    assert_eq!(result, "");
}
