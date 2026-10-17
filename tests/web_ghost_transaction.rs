//! Actual production JS and real WASM checkpoint closures. Compile-only here;
//! execution/Firefox/Chromium and human acceptance require the parent gate.
#![cfg(target_arch = "wasm32")]
mod support;
use support::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn disjoint_alternative_ghost_deletion_mixed_replay_publishes_final_status() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;
      api.kg_load_thesaurus('{"name":"status-public","data":{"clip":{"id":1,"nterm":"clip"}}}');
      ed.openDocument('# clip\n\nThe editor is basically the only target here today.\n','mixed-status');
      const body=s.getText(),at=body.indexOf('clip');
      const alternative=ed.alternativeOp('create_span','word',at,at+4,'snippet','human');
      trim.select('sharper');
      if(!trim.active.length || trim.active.some(r=>r.start<at+4 && at<r.end)) out.push('not disjoint');
      if(trim.done()!==true) return 'Done setup failed: '+trim.live.textContent;
      s.replaceRange(at,at+4,'',{source:'delete'});
      if(ed.setAsideCount!==1 || ed.warningKind!=='detached') out.push('initial detach status');
      if(!s.squashHistory(2)) out.push('squash refused');
      if(!s.undo()) out.push('mixed undo refused');
      let ann=api.document_annotations();
      if(s.getText()!==body || ann.ghosts.length || !ann.spans.some(span=>span.id===alternative.span)) out.push('undo model');
      if(ann.setAside.spans.length || ann.setAside.ghosts.length || ed.setAsideCount!==0 || ed.warningKind!==null || ed.warning!==null) out.push('undo stale status');
      if(ed.warningElement && !ed.warningElement.hidden) out.push('undo visible warning');
      if(!s.redo()) out.push('mixed redo refused');
      ann=api.document_annotations();
      const count=ann.setAside.spans.length+ann.setAside.ghosts.length;
      if(count!==1 || !ann.setAside.spans.some(span=>span.id===alternative.span) || !ann.ghosts.length) out.push('redo model');
      if(ed.setAsideCount!==count || ed.warningKind!=='set-aside' || !ed.warning.includes('1 annotation')) out.push('redo stale status');
      if(!ed.warningElement || ed.warningElement.hidden || !ed.warningElement.querySelector('.te-warning-text').textContent.includes('1 annotation')) out.push('redo accessible status');
    "#]).await, "");
}

#[wasm_bindgen_test]
async fn persistent_ghost_ids_above_js_safe_integer_are_not_coerced() {
    support::trim::setup();
    assert_eq!(run_steps(&[r#"
      const api=ed.documentApi(),trim=ed.lab.trim;trim.select('sharper');
      const body=s.getText(),at=0; // fixture heading marker, never a trim cut
      const block={version:1,spans:[],ghosts:[{id:'g9007199254740993',anchor:{start:at,end:at+1,text:body.slice(at,at+1)}}],overflow:''};
      ed.openDocument(body+'\n\n```terraphim-alternatives\n'+JSON.stringify(block)+'\n```\n','large-ids');
      trim.select('sharper');if(trim.done()!==true) return 'large-ID Done setup failed: '+trim.live.textContent;
      const ids=api.document_annotations().ghosts.map(g=>g.id);
      if(!ids.includes('g9007199254740994') || !ids.includes('g9007199254740993')) out.push('identity rounded');
      if(!s.undo() || api.document_annotations().ghosts[0].id!=='g9007199254740993') out.push('large-ID undo');
      if(!s.redo()) out.push('large-ID redo');
      ed.openDocument(api.save_document(),'large-ids-reopen');
      if(JSON.stringify(api.document_annotations().ghosts.map(g=>g.id))!==JSON.stringify(ids)) out.push('large-ID persistence');
    "#]).await, "");
}

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
