//! Actual production JS and real WASM checkpoint closures. Compile-only here;
//! execution/Firefox/Chromium and human acceptance require the parent gate.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn mixed_typing_and_ghost_group_uses_intermediate_body_and_atomic_last_refusal() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;
      trim.select('sharper');if(trim.done()!==true) return 'mixed Done setup failed: '+trim.live.textContent;
      const body=s.getText();s.replaceRange(body.length,body.length,'X',{source:'typing'});
      if(!s.squashHistory(2)) out.push('squash');
      if(!s.undo() || s.getText()!==body || api.document_annotations().ghosts.length) out.push('mixed undo');
      if(!s.redo() || s.getText()!==body+'X') out.push('mixed redo');
      // Interleave real native manual metadata; last ghost replay must refuse
      // even though its earlier text deletion would have been valid.
      api.ghost_range(0,1);
      const history=JSON.stringify(s.history),index=s.historyIndex,text=s.getText(),saved=api.save_document();
      if(s.undo()!==false) out.push('receipt conflict accepted');
      if(s.getText()!==text || api.save_document()!==saved || JSON.stringify(s.history)!==history || s.historyIndex!==index) out.push('partial mixed mutation');
    "#]).await, "");
}

#[wasm_bindgen_test]
async fn role_invalidation_during_record_aborts_document_but_never_resurrects_fades() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;
      trim.select('sharper');const saved=api.save_document(),history=JSON.stringify(s.history);
      const record=s.record;let recordEntered=false;
      s.record=function(...args){recordEntered=true;record.apply(this,args);api.kg_clear_thesaurus();};
      if(trim.done()!==false) out.push('role invalidation accepted');s.record=record;
      if(!recordEntered) return 'role fault setup not reached: '+trim.live.textContent;
      if(api.save_document()!==saved || JSON.stringify(s.history)!==history) out.push('document rollback failed');
      if(trim.plan || ed.decorations.get('lab-trim').length || trim.kept.size) out.push('role fades resurrected');
    "#]).await, "");
}
