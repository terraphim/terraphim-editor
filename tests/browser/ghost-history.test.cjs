// Actual production module methods with explicit native checkpoint/receipt and
// DOM doubles. NOT native model, WASM runtime, browser or human acceptance.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function fixture(limit = 500) {
  const context = vm.createContext({console, performance, TextEncoder, structuredClone,
    document:{activeElement:null,addEventListener(){}}, InputEvent:class {}, setTimeout, clearTimeout});
  context.window = context;
  for (const name of ['editor','trim','indicators']) vm.runInContext(fs.readFileSync(`public/js/${name}.js`,'utf8'), context);
  const surface = Object.create(context.EditorSurface.prototype);
  Object.assign(surface, {text:'abc', history:[], historyIndex:-1, historyLimit:limit, coalesceMs:1000,
    decorations:[], lastSelection:{start:1,end:2,direction:'backward'}, nextDecorationId:1,
    root:{childNodes:[{text:'abc'}],replaceChildren(...nodes){this.childNodes=nodes;},dispatchEvent(){}},changeListeners:new Set(),
    render(){this.renders=(this.renders||0)+1;this.root.childNodes=[{text:this.text,decorations:structuredClone(this.decorations)}];}, readSelection(){return this.lastSelection;},
    setSelectionOffsets(start,end=start,direction='none'){this.lastSelection={start,end,direction};}});
  surface.record('init', surface.lastSelection);
  const state={body:'abc',ghosts:[],reservations:[],receipts:[]};
  const calls=[]; let pending; let watermark=0;
  const receipt={body:'abc',before:{live:[],setAside:[],reservedIds:[]},
    after:{live:[{id:'g9007199254740994',anchor:{start:0,end:1,text:'a'}}],setAside:[],reservedIds:['g9007199254740994']}};
  const annotations=()=>({spans:[],ghosts:state.ghosts,setAside:{spans:[],ghosts:[]},overflow:''});
  const begin=(payload)=>{const token=`double-${++watermark}`;pending={token,before:structuredClone(state)};return {ok:true,token,...payload};};
  const api={open_document(){},apply_edit(){},document_body:()=>state.body,
    begin_trim_ghost_transaction(body,level,kept){calls.push(['trim',body,level,kept]);
      if(body!==state.body) throw Error('body-conflict');
      const result=begin({change:receipt});state.ghosts=structuredClone(receipt.after.live);
      state.reservations=['g9007199254740994'];state.receipts=[receipt];result.annotations=annotations();return result;},
    begin_document_history_replay(body,target,ops){calls.push(['replay',body,target,JSON.parse(ops)]);
      const result=begin({});state.body=target;for(const op of JSON.parse(ops)) if(op.kind==='ghost') state.ghosts=structuredClone(op.undo?receipt.before.live:receipt.after.live);
      result.annotations=annotations();return result;},
    finish_document_history_transaction(token){calls.push(['finish',token]);assert.equal(token,pending.token);pending=null;return {ok:true,finalized:true,token};},
    abort_document_history_transaction(token){calls.push(['abort',token]);assert.equal(token,pending.token);Object.assign(state,pending.before);pending=null;return {ok:true,rolledBack:true,token};},
    kg_role_snapshot_json:()=>JSON.stringify({generation:'1',loaded:true,protectionReady:true}),
  };
  const editor=Object.create(context.MarkdownEditor.prototype);
  const registry=Object.create(context.TeDecorationRegistry.prototype);
  Object.assign(registry,{surface,layers:new Map(),pending:false,batchDepth:0,applyListeners:new Set(),renderCount:0});
  Object.assign(editor,{surface,decorations:registry,config:{bindings:api},
    showWarning(message,kind=null){this.notice=message;this.warningKind=message ? kind : null;}});
  return {context,surface,editor,api,state,calls,receipt};
}

test('capacity 1 refuses protected begin before any native call',()=>{
  const f=fixture(1);assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/capacity/);
  assert.equal(f.calls.length,0);assert.equal(f.surface.history.length,1);
});

test('forced annotation step preserves exact receipt IDs and undoes/redoes model first',()=>{
  const f=fixture(2); f.editor.commitTrimGhosts('sharper',[]);
  assert.equal(f.surface.history.length,2);
  assert.equal(f.surface.history[1].edits[0].ghost.change.after.live[0].id,'g9007199254740994');
  assert.equal(f.surface.undo(),true);assert.equal(f.state.ghosts.length,0);
  assert.equal(f.surface.redo(),true);assert.equal(f.state.ghosts[0].id,'g9007199254740994');
});

const snapshot = f => JSON.stringify({history:f.surface.history,index:f.surface.historyIndex,text:f.surface.text,
  selection:f.surface.lastSelection,decorations:f.surface.decorations,state:f.state,
  nodes:f.surface.root.childNodes,layers:Array.from(f.editor.decorations.layers)});

for (const limit of [2,500]) test(`capacity ${limit}: real forced recording and redo truncation`,()=>{
  const f=fixture(limit);f.editor.commitTrimGhosts('sharper',[]);f.surface.undo();
  assert.equal(f.surface.canRedo(),true);f.editor.commitTrimGhosts('sharper',[]);
  assert.equal(f.surface.canRedo(),false);assert.equal(f.surface.canUndo(),true);
});

for (const fault of ['record','paint','finish','bad-finish']) test(`${fault} failure rolls back complete owned state and redo`,()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);f.surface.undo();
  const before=snapshot(f);
  if(fault==='record') f.surface.record=function(){this.history.length=1;this.historyIndex=0;this.text='partial';throw Error('record fault');};
  if(fault==='paint') f.surface.render=()=>{throw Error('paint fault');};
  if(fault==='finish') f.api.finish_document_history_transaction=()=>{throw Error('finish fault');};
  if(fault==='bad-finish') f.api.finish_document_history_transaction=()=>({ok:1,finalized:true,token:'wrong'});
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[1]),/fault|acknowledgement/);
  assert.equal(snapshot(f),before);assert.equal(f.editor.ghostHistoryConflict,undefined);
});

test('capacity refusal preserves an existing redo branch with no native begin',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);f.surface.undo();f.surface.historyLimit=1;
  const before=snapshot(f),calls=f.calls.length;
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/capacity/);
  assert.equal(snapshot(f),before);assert.equal(f.calls.length,calls);
});

test('mixed undo passes intermediate bodies and notifies only after finish without duplicate mirror edits',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  f.surface.text='abcX';f.state.body='abcX';
  f.surface.record('typing',f.surface.lastSelection,{start:3,deletedText:'',insertedText:'X'});
  assert.equal(f.surface.squashHistory(2),true);
  let emitted=0,duplicate=0,phase;
  f.surface.onChange(change=>{phase=[change.edit,f.calls.at(-1)[0],f.editor.suppressModelSync];emitted++;});
  f.api.apply_edit=()=>{duplicate++;};f.surface.onChange(change=>f.editor.mirrorEdit(change));
  assert.equal(f.surface.undo(),true);
  const ops=f.calls.find(c=>c[0]==='replay')[3];
  assert.deepEqual(ops.map(o=>[o.kind,o.body]),[['edit','abc'],['ghost','abc']]);
  assert.equal(emitted,1);assert.deepEqual(phase,[null,'finish',true]);assert.equal(duplicate,0);assert.equal(f.surface.text,'abc');
  assert.equal(f.surface.redo(),true);assert.equal(f.surface.text,'abcX');
});

test('native whole-group last receipt refusal happens before cursor, paint, selection or DOM',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  f.surface.text='abcX';f.state.body='abcX';f.surface.record('typing',f.surface.lastSelection,{start:3,deletedText:'',insertedText:'X'});
  f.surface.squashHistory(2);const before=snapshot(f),renders=f.surface.renders;
  f.api.begin_document_history_replay=(body,target,json)=>{const ops=JSON.parse(json);assert.equal(ops[0].body,'abc');assert.equal(ops[1].kind,'ghost');throw Error('last receipt conflict (native double)');};
  assert.equal(f.surface.undo(),false);assert.equal(snapshot(f),before);assert.equal(f.surface.renders,renders);
  assert.match(f.editor.notice,/last receipt conflict/);
});

for (const fault of ['throw','coerced','wrong-token']) test(`abort ${fault} never claims rollback; block and preserve interleaved state`,()=>{
  const f=fixture();
  f.surface.record=function(){this.text='external';f.state.body='external';throw Error('record callback');};
  f.api.abort_document_history_transaction=()=>{
    if(fault==='throw') throw Error('session-conflict');
    return {ok:fault==='coerced'?1:true,rolledBack:true,token:fault==='wrong-token'?'foreign':'double-1'};
  };
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/rollback not acknowledged/);
  assert.equal(f.state.body,'external');assert.equal(f.surface.text,'external');assert.ok(f.editor.ghostHistoryConflict);
  const calls=f.calls.length;assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/conflict/);
  assert.equal(f.calls.length,calls);assert.throws(()=>f.editor.alignDocumentModel(f.api),/explicitly reopen/);
});

test('role event during recording stays authoritative after native document abort',()=>{
  const f=fixture();let role='1';f.api.kg_role_snapshot_json=()=>role;
  f.editor.decorations={layers:new Map([['lab-trim',[{id:'fade',start:1,end:2}]]])};
  f.surface.decorations=[{id:'lab-trim:fade',start:1,end:2}];
  f.surface.record=function(){role='2';f.editor.decorations.layers.set('lab-trim',[]);this.decorations=[];throw Error('role changed');};
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/role changed/);
  assert.equal(f.editor.decorations.layers.get('lab-trim').length,0);
  assert.equal(f.surface.decorations.length,0);assert.equal(f.surface.history.length,1);
});

test('all six kinds transport real metadata without numeric receipt/ID coercion',()=>{
  const f=fixture();const ghost={start:0,deletedText:'',insertedText:'',ghost:{change:f.receipt,undo:false}};
  const steps=[ghost,
    {start:3,deletedText:'',insertedText:'X'},
    {start:0,deletedText:'',insertedText:'',alt:{span:'s1',after:null,before:null}},
    {start:0,deletedText:'',insertedText:'',move:{start:0,end:1,to:1}},
    {start:0,deletedText:'',insertedText:'',swap:{span:'s1',from:0,to:1}},
    {start:0,deletedText:'',insertedText:'',overflow:{before:'',after:'note'}}];
  const result=f.context.MarkdownEditor.nativeGhostHistoryOperations('abc',steps,'abcX');
  assert.deepEqual(JSON.parse(result.json).map(o=>o.kind),['ghost','edit','alt','move','swap','overflow']);
  assert.equal(JSON.parse(result.json)[0].change.after.live[0].id,'g9007199254740994');
  const inverse=f.context.EditorSurface.invertStep(ghost);
  assert.equal(inverse.ghost.change,f.receipt);assert.equal(inverse.ghost.undo,true);
});

for (const fault of ['unknown','conflicting','wrong-body','float','count','missing']) test(`invalid mixed metadata ${fault} refuses before native mutation`,()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  const step=f.surface.history[1].edits[0];
  if(fault==='unknown')step.surprise=true;
  if(fault==='conflicting')step.swap={span:'s1',from:0,to:1};
  if(fault==='wrong-body')f.surface.history[0].text='wrong';
  if(fault==='float')step.start=0.5;
  if(fault==='count')f.surface.history[1].edits=Array(4097).fill(step);
  if(fault==='missing')f.surface.history[1].edits=null;
  const before=snapshot(f),calls=f.calls.length;
  assert.equal(f.surface.undo(),false);assert.equal(snapshot(f),before);assert.equal(f.calls.length,calls);
});

test('large bounded receipt transport and UTF-8 byte +1 refusal',()=>{
  const f=fixture();const padding='x'.repeat(1024*1024);f.receipt.after.live[0].anchor.before=padding;
  f.editor.commitTrimGhosts('sharper',[]);assert.equal(f.surface.history[1].edits[0].ghost.change.after.live[0].anchor.before,padding);
  assert.throws(()=>f.context.MarkdownEditor.ghostTransportBounds('é'.repeat(8*1024*1024)+'x','','[]'),/16 MiB/);
});

test('legacy typing still coalesces and ghost metadata never coalesces',()=>{
  const f=fixture();f.surface.text='abcd';f.surface.record('typing',null,{start:3,deletedText:'',insertedText:'d'});
  f.surface.text='abcde';f.surface.record('typing',null,{start:4,deletedText:'',insertedText:'e'});
  assert.equal(f.surface.history.length,2);assert.equal(f.surface.history[1].edits.length,1);
  const append=f.context.EditorSurface.appendStep([{start:0,deletedText:'',insertedText:'',ghost:{change:f.receipt,undo:false}}],{start:0,deletedText:'',insertedText:'X'},'abc');
  assert.equal(append.length,2);assert.ok(append[0].ghost);
});

function trimFixture(f) {
  const trim=Object.create(f.context.TeLabTrim.prototype);
  Object.assign(trim,{editor:f.editor,cardOpen:true,level:'sharper',kept:new Set([1]),plan:{safe:true},
    planText:'abc',walkIndex:2,stopWalk(){this.stopped=true;},reset(){this.cardOpen=false;this.kept.clear();},
    announce(message){this.notice=message;}});
  return trim;
}

test('actual Done retires UI only after final acknowledgement',()=>{
  const f=fixture(),trim=trimFixture(f);let observed=false;
  const finish=f.api.finish_document_history_transaction;
  f.api.finish_document_history_transaction=t=>{assert.equal(trim.cardOpen,true);assert.equal(trim.walkIndex,2);observed=true;return finish(t);};
  assert.equal(trim.done(),true);assert.equal(observed,true);assert.equal(trim.plan,null);assert.equal(trim.cardOpen,false);
});

test('actual Done uses announce, not destructive B refusal, on record failure',()=>{
  const f=fixture(),trim=trimFixture(f);const plan=trim.plan;
  f.surface.record=()=>{throw Error('record refused');};trim.refuse=()=>assert.fail('must not retire safe plan');
  assert.equal(trim.done(),false);assert.equal(trim.plan,plan);assert.equal(trim.cardOpen,true);
  assert.equal(trim.kept.size,1);assert.equal(trim.walkIndex,2);assert.equal(trim.stopped,undefined);assert.match(trim.notice,/record refused/);
});

test('protected no-op clears Done UI without issuing history or claiming an undo step',()=>{
  const f=fixture(),trim=trimFixture(f),before=snapshot(f);
  f.api.begin_trim_ghost_transaction=()=>null;
  assert.equal(trim.done(),true);assert.equal(snapshot(f),before);assert.equal(trim.plan,null);
  assert.match(trim.notice,/no cuts to save/);assert.equal(f.calls.length,0);
});

test('protected native begin refusal leaves redo, selection, registry and DOM intact',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);f.surface.undo();
  const before=snapshot(f);f.api.begin_trim_ghost_transaction=()=>{throw {kind:'protection-refusal',error:'stale role/body'};};
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]));assert.equal(snapshot(f),before);
});

test('unknown nested metadata survives inversion and refuses instead of being discarded',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  f.surface.history[1].edits[0].ghost.extra='not allowed';const before=snapshot(f),calls=f.calls.length;
  assert.equal(f.surface.undo(),false);assert.equal(snapshot(f),before);assert.equal(f.calls.length,calls);
});

test('string ghost direction is not boolean-coerced by inversion',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  f.surface.history[1].edits[0].ghost.undo='false';const calls=f.calls.length;
  assert.equal(f.surface.undo(),false);assert.equal(f.calls.length,calls);
});

test('4096 kept indices reach native; 4097 refuse before begin',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',Array(4096).fill(0));
  assert.equal(JSON.parse(f.calls[0][3]).length,4096);const before=snapshot(f),calls=f.calls.length;
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',Array(4097).fill(0)),/Too many/);
  assert.equal(snapshot(f),before);assert.equal(f.calls.length,calls);
});

test('silent record drop refuses and aborts rather than reporting Done success',()=>{
  const f=fixture(),before=snapshot(f);f.surface.record=()=>{};
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/not retained/);assert.equal(snapshot(f),before);
});

test('explicit canonical recovery uses exact ack and resets conflict/history',()=>{
  const f=fixture();f.editor.ghostHistoryConflict={token:'conflicted-token'};
  f.api.recover_document_history_transaction=(token,source)=>({ok:true,recovered:true,rolledBack:false,token,
    opened:{body:source,kind:null,warning:null,unresolved:0}});
  f.api.sync_document_body=body=>{f.state.body=body;return {changed:false,unresolved:0};};
  f.editor.setAsideCount=0;f.surface.history.push({text:'old',edits:[]});f.surface.historyIndex=1;
  f.editor.openDocument('canonical','new-name');
  assert.equal(f.editor.ghostHistoryConflict,null);assert.equal(f.surface.text,'canonical');
  assert.equal(f.surface.history.length,1);assert.equal(f.surface.canUndo(),false);assert.equal(f.surface.canRedo(),false);
});

test('malformed explicit recovery keeps the manual block and surface unchanged',()=>{
  const f=fixture();f.editor.ghostHistoryConflict={token:'conflicted-token'};
  f.api.recover_document_history_transaction=token=>({ok:true,recovered:true,rolledBack:false,token,
    opened:{body:'bad',kind:'malformed',warning:'invalid annotations',unresolved:0}});
  const before=snapshot(f);
  assert.throws(()=>f.editor.openDocument('bad','not-adopted'),/malformed/);
  assert.ok(f.editor.ghostHistoryConflict);assert.equal(f.editor.ghostHistoryConflict.token,null);
  assert.equal(snapshot(f),before);assert.equal(f.editor.documentKey,undefined);
});

test('undo finish failure restores cursor and annotations only after acknowledged abort',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);const before=snapshot(f);
  f.api.finish_document_history_transaction=()=>{throw Error('undo finish fault');};
  assert.equal(f.surface.undo(),false);assert.equal(snapshot(f),before);
});

test('redo guard failure preserves redo branch and last selection',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);f.surface.undo();const before=snapshot(f);
  f.api.begin_document_history_replay=()=>{throw Error('redo guard refused');};
  assert.equal(f.surface.redo(),false);assert.equal(snapshot(f),before);assert.equal(f.surface.canRedo(),true);
});

test('real registry paint callback conflict never overwrites external native state',()=>{
  const f=fixture();f.editor.decorations.onApply(()=>{f.state.body='external-native';throw Error('paint callback');});
  f.api.abort_document_history_transaction=()=>{throw Error('session-conflict');};
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/rollback not acknowledged/);
  assert.equal(f.state.body,'external-native');assert.ok(f.editor.ghostHistoryConflict);
});

test('missing begin token cannot be silently retired by ordinary document open',()=>{
  const f=fixture();let opened=0;f.api.open_document=()=>{opened++;};
  f.api.begin_trim_ghost_transaction=()=>undefined;
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/begin acknowledgement/);
  assert.equal(f.editor.ghostHistoryConflict.nativeRetired,false);
  assert.throws(()=>f.editor.openDocument('replacement'),/token unavailable/);assert.equal(opened,0);
});

test('acknowledged native abort plus DOM restore failure retains explicit recovery path',()=>{
  const f=fixture();f.surface.record=()=>{throw Error('record fault');};
  f.surface.root.replaceChildren=()=>{throw Error('DOM restore fault');};
  assert.throws(()=>f.editor.commitTrimGhosts('sharper',[]),/native rollback acknowledged but surface restoration failed/);
  assert.equal(f.state.ghosts.length,0);assert.equal(f.editor.ghostHistoryConflict.nativeRetired,true);
  assert.equal(f.editor.ghostHistoryConflict.token,null);
  f.surface.record=f.context.EditorSurface.prototype.record;
  f.api.open_document=body=>{f.state.body=body;return {body,kind:null,warning:null,unresolved:0};};
  f.api.sync_document_body=()=>({changed:false,unresolved:0});
  f.editor.openDocument('canonical');assert.equal(f.editor.ghostHistoryConflict,null);assert.equal(f.surface.text,'canonical');
});

test('successful mixed replay publishes aggregate authoritative status only after exact finish',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  Object.assign(f.editor,{setAsideCount:1,warningKind:'detached',notice:'previous detach'});
  const begin=f.api.begin_document_history_replay,finish=f.api.finish_document_history_transaction;
  let target={spans:[],ghosts:[]},finished=false;
  f.api.begin_document_history_replay=(...args)=>{const result=begin(...args);result.annotations.setAside=target;return result;};
  f.api.finish_document_history_transaction=token=>{
    assert.equal(f.editor.setAsideCount,finished ? 0 : 1);
    assert.equal(f.editor.warningKind,finished ? null : 'detached');
    const result=finish(token);finished=true;return result;
  };
  assert.equal(f.surface.undo(),true);assert.equal(f.editor.setAsideCount,0);
  assert.equal(f.editor.warningKind,null);assert.equal(f.editor.notice,null);
  target={spans:[{id:'disjoint-alt'}],ghosts:[{id:'unresolved-ghost'}]};
  assert.equal(f.surface.redo(),true);assert.equal(f.editor.setAsideCount,2);
  assert.equal(f.editor.warningKind,'set-aside');assert.match(f.editor.notice,/2 annotations/);
});

for(const fault of ['begin','finish','bad-ack']) test(`mixed ${fault} refusal/acknowledged abort preserves prior annotation status`,()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  Object.assign(f.editor,{setAsideCount:3,warningKind:'detached',notice:'prior detached warning'});
  if(fault==='begin')f.api.begin_document_history_replay=()=>{throw Error('guard refusal');};
  if(fault==='finish')f.api.finish_document_history_transaction=()=>{throw Error('finish refusal');};
  if(fault==='bad-ack')f.api.finish_document_history_transaction=token=>({ok:true,finalized:1,token});
  const before=snapshot(f);
  const steps=f.surface.history[1].edits.map(f.context.EditorSurface.invertStep);
  assert.throws(()=>f.editor.replayGhostHistory(0,'undo',steps));
  assert.equal(f.editor.setAsideCount,3);assert.equal(f.editor.warningKind,'detached');
  assert.equal(f.editor.notice,'prior detached warning');
  assert.equal(snapshot(f),before);
  if(fault!=='begin')assert.equal(f.calls.at(-1)[0],'abort');
});

test('successful replay preserves malformed-block warning policy while updating count',()=>{
  const f=fixture();f.editor.commitTrimGhosts('sharper',[]);
  Object.assign(f.editor,{setAsideCount:1,warningKind:'malformed',notice:'preserved malformed block'});
  assert.equal(f.surface.undo(),true);assert.equal(f.editor.setAsideCount,0);
  assert.equal(f.editor.warningKind,'malformed');assert.equal(f.editor.notice,'preserved malformed block');
});
