//! Actual production JS and real WASM checkpoint closures. Compile-only here;
//! execution/Firefox/Chromium and human acceptance require the parent gate.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn protected_done_is_one_persistent_undo_step_and_preserves_manual_ghosts() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(), trim=ed.lab.trim;
      trim.select('sharper');
      const ranges=trim.active;
      if (!ranges.length) out.push('missing cuts');
      const body=s.getText();
      // A disjoint manual ghost is not promoted or removed by Done/undo.
      const at=0; // fixture heading marker, outside all protected cut ranges
      api.ghost_range(at,at+1);
      const manual=api.document_annotations().ghosts[0].id;
      const history=s.history.length;
      if(trim.done()!==true) return 'Done setup failed: '+trim.live.textContent;
      if(s.getText()!==body || s.history.length!==history+1) out.push('body/history');
      if(ed.decorations.get('lab-trim').length || trim.plan) out.push('transients retained');
      if(!s.undo()) out.push('undo refused');
      if(JSON.stringify(api.document_annotations().ghosts.map(g=>g.id))!==JSON.stringify([manual])) out.push('manual lost');
      if(!s.redo()) out.push('redo refused');
      const saved=api.save_document();
      const ids=api.document_annotations().ghosts.map(g=>g.id);
      ed.openDocument(saved,'ghost-reopen');
      if(JSON.stringify(api.document_annotations().ghosts.map(g=>g.id))!==JSON.stringify(ids)) out.push('persistence identities');
    "#]).await, "");
}

#[wasm_bindgen_test]
async fn capacities_and_partial_record_failure_restore_exact_native_receipt_state() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;
      trim.select('sharper');s.historyLimit=1;
      const saved=api.save_document(), history=JSON.stringify(s.history), index=s.historyIndex;
      if(trim.done()!==false || api.save_document()!==saved || JSON.stringify(s.history)!==history || s.historyIndex!==index) out.push('capacity mutated');
      if(!trim.plan || !ed.decorations.get('lab-trim').length) out.push('capacity retired plan');
      s.historyLimit=2;
      const record=s.record;let recordEntered=false;
      s.record=function(){recordEntered=true;this.history.length=0;this.historyIndex=-1;throw Error('injected record fault');};
      if(trim.done()!==false) out.push('record fault accepted');
      s.record=record;
      if(!recordEntered) return 'record fault setup not reached: '+trim.live.textContent;
      if(api.save_document()!==saved || JSON.stringify(s.history)!==history || s.historyIndex!==index) out.push('failed record changed checkpoints');
      if(trim.done()!==true || !s.canUndo()) return 'capacity2 setup failed: '+trim.live.textContent;
      const ids=api.document_annotations().ghosts.map(g=>g.id);
      if(ids[0]!=='g1') out.push('failed record retained allocation');
      if(!s.undo() || !s.redo()) out.push('capacity2 history');
    "#]).await, "");
}
