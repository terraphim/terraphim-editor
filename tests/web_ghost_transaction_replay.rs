//! Actual production JS and real WASM checkpoint closures. Compile-only here;
//! execution/Firefox/Chromium and human acceptance require the parent gate.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn interleaved_native_write_refuses_abort_and_requires_explicit_canonical_reopen() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;trim.select('sharper');
      const record=s.record;let recordEntered=false;
      s.record=function(...args){recordEntered=true;record.apply(this,args);api.set_document_overflow('external notes');throw Error('interleaved');};
      if(trim.done()!==false || !ed.ghostHistoryConflict) out.push('missing conflict');s.record=record;
      if(!recordEntered) return 'interleaved fault setup not reached: '+trim.live.textContent;
      if(api.document_annotations().overflow!=='external notes') out.push('interleaved data overwritten');
      const canonical=api.save_document();
      ed.openDocument(canonical,'manual-recovery');
      if(ed.ghostHistoryConflict || s.canUndo() || s.canRedo()) out.push('canonical history not reset');
      if(api.document_annotations().overflow!=='external notes') out.push('recovery lost notes');
    "#]).await, "");
}

#[wasm_bindgen_test]
async fn all_six_real_operations_and_unicode_guard_use_actual_wasm_bridge() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;trim.select('sharper');
      if(trim.done()!==true) return 'six-kind Done setup failed: '+trim.live.textContent;
      if(!s.history[s.historyIndex]?.edits?.[0]?.ghost?.change) return 'six-kind setup missing actual ghost receipt';
      const body=s.getText(),receipt=s.history[s.historyIndex].edits[0].ghost.change;
      const at=body.indexOf('Café');
      const changed=api.alt_create_span('word',at,at+4,'Coffee','human');
      const span=changed.after;
      const empty={body,start:0,deletedText:'',insertedText:''};
      const operations=[{kind:'ghost',body,change:receipt,undo:true},
        {...empty,kind:'edit'}, {...empty,kind:'alt',span:span.id,target:span},
        {...empty,kind:'move',move:{start:0,end:body.length,to:body.length}},
        {...empty,kind:'swap',span:span.id,from:0,to:0},
        {...empty,kind:'overflow',before:'',after:''}];
      const begun=api.begin_document_history_replay(body,body,JSON.stringify(operations));
      const aborted=api.abort_document_history_transaction(begun.token);
      if(aborted.ok!==true || aborted.rolledBack!==true || aborted.token!==begun.token) out.push('exact ack');
      ed.openDocument('a😀中b','unicode');
      const before=api.save_document();
      try {api.begin_document_history_replay('a😀中b','bad',JSON.stringify([{kind:'edit',body:'bad',start:2,deletedText:'',insertedText:'X'}]));out.push('half-surrogate accepted');}catch(_){}
      if(api.save_document()!==before) out.push('unicode refusal mutated');
    "#]).await, "");
}
