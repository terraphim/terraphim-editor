// Production JS fault-path units with explicit DOM/main-WASM doubles.
// These are NOT browser execution, matcher conformance or Rust evidence.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');

function fixture() {
  const events = []; let observer; let registrations = 0;
  let snapshot = {generation:'1',loaded:true,protectionReady:true};
  const context = vm.createContext({console,URL,setTimeout,clearTimeout,AbortController,
    document:{activeElement:null,baseURI:'https://local/',addEventListener(){}},
    CustomEvent:class {constructor(type,options) {this.type=type;this.detail=options.detail;}}});
  context.window = context;
  for (const name of ['editor','trim']) vm.runInContext(fs.readFileSync(`public/js/${name}.js`,'utf8'),context);
  const api = {
    kg_role_snapshot_json:()=>JSON.stringify(snapshot),
    kg_set_role_change_listener:fn=>{ observer=fn; registrations++; },
    trim_plan_json:()=>({roleGeneration:snapshot.generation,kgProtected:true,cuts:[]}),
    trim_status:()=>({roleGeneration:snapshot.generation,kgProtected:true,active:[]}),
    trim_make_cuts:()=>({text:'text',edits:[]}),
  };
  const editor = Object.create(context.MarkdownEditor.prototype);
  Object.assign(editor,{documentApi:()=>api,root:{dataset:{},dispatchEvent:e=>events.push(e)},
    destroyed:false,graphClient:null,syncGraphRole:()=>events.push('graph'),alignDocumentModel(){}});
  const trim = Object.create(context.TeLabTrim.prototype);
  const layer = [];
  Object.assign(trim,{editor,destroyed:false,level:'original',plan:null,planText:null,kept:new Set(),active:[],
    status:null,walkIndex:-1,cardOpen:false,stale:false,resetTimer:null,
    surface:{getText:()=> 'text',root:{dataset:{}},destroyed:false},
    registry:{get:()=>layer,clear:()=>{layer.length=0;}},
    card:{contains:()=>false,hidden:true},walk:{hidden:true},live:{textContent:''},
    lab:{isOpen:false,pill:()=>null},protectionNote:{textContent:''},
    levelButtons:['original','slight','tighten','sharper','half'].map(level=>({dataset:{level},setAttribute(){}})),
  });
  return {editor,trim,api,events,layer,notify:()=>observer?.(),registrations:()=>registrations,
    setRole:value=>{snapshot=value;}};
}

test('one eager observer publishes editor metadata before coordinated graph suppression',()=>{
  const f=fixture(); f.editor.observeRolePublications(); f.editor.observeRolePublications();
  assert.equal(f.registrations(),1);
  f.editor.graphRoleCommitInProgress=true; f.notify();
  assert.equal(f.events.length,1); assert.equal(f.events[0].type,'te:role-change');
  assert.equal(f.events[0].detail.editor,f.editor);
  assert.deepEqual(Object.keys(f.events[0].detail).sort(),['editor','generation','loaded','protectionReady']);
  assert.equal(f.editor.graphClient,null);
  f.editor.graphRoleCommitInProgress=false; f.notify();
  assert.equal(f.events[1].type,'te:role-change'); assert.equal(f.events[2],'graph');
});

test('initial loaded snapshot is ready; absent/empty/unsupported role and missing export disable non-Original',()=>{
  const f=fixture(); assert.equal(f.trim.updateProtectionAvailability(),true);
  for (const snapshot of [null,{generation:'2',loaded:false,protectionReady:false},
    {generation:'3',loaded:true,protectionReady:false,protectionError:'unsupported_normalization'}]) {
    f.setRole(snapshot); assert.equal(f.trim.updateProtectionAvailability(),false);
    assert.equal(f.trim.levelButtons[0].disabled,false);
    assert.ok(f.trim.levelButtons.slice(1).every(b=>b.disabled));
    assert.equal(f.trim.select('half'),null);
  }
  delete f.api.kg_role_snapshot_json;
  assert.equal(f.trim.updateProtectionAvailability(),false);
});

test('same authoritative event retires plan, preview and keeps without model/history edits',()=>{
  const f=fixture(); f.layer.push({start:1,end:2}); f.trim.level='half';
  f.trim.plan={roleGeneration:'1'}; f.trim.kept.add(1); f.trim.cardOpen=true;
  f.editor.root.dispatchEvent=e=>{f.events.push(e); f.trim.onRoleChange();};
  f.editor.observeRolePublications(); f.setRole({generation:'2',loaded:false,protectionReady:false}); f.notify();
  assert.equal(f.trim.plan,null); assert.equal(f.trim.level,'original');
  assert.equal(f.layer.length,0); assert.equal(f.trim.kept.size,0); assert.equal(f.trim.card.hidden,true);
  assert.ok(f.trim.levelButtons.slice(1).every(b=>b.disabled));
});

test('cached body is insufficient: generation and protection flags must match',()=>{
  const f=fixture(); f.trim.plan={roleGeneration:'1',kgProtected:true}; f.trim.planText='text';
  f.setRole({generation:'2',loaded:true,protectionReady:true});
  assert.equal(f.trim.ensurePlan(f.api).roleGeneration,'2');
  f.trim.plan=null; f.api.trim_plan_json=()=>({roleGeneration:'2'});
  assert.throws(()=>f.trim.ensurePlan(f.api),/KG protection/); assert.equal(f.trim.plan,null);
});

test('unsupported document refusal disables actions and clears all effects atomically',()=>{
  const f=fixture(); f.trim.level='half'; f.layer.push({start:1,end:2});
  f.api.trim_plan_json=()=>{throw Error('unsupported_normalization');};
  assert.equal(f.trim.refresh(),null); assert.equal(f.trim.level,'original');
  assert.equal(f.layer.length,0); assert.equal(f.trim.card.hidden,true);
  assert.match(f.trim.live.textContent,/unsupported_normalization/);
  assert.ok(f.trim.levelButtons.slice(1).every(b=>b.disabled));
});

test('observer exceptions cannot block graph sync and destroyed editor ignores publications',()=>{
  const f=fixture(); f.editor.observeRolePublications();
  f.editor.root.dispatchEvent=()=>{throw Error('DOM double failure');}; f.notify();
  assert.deepEqual(f.events,['graph']); f.editor.destroyed=true; f.notify();
  assert.deepEqual(f.events,['graph']);
});
