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
