// Integration fault-path units using real editor/client JS and explicitly
// labelled Worker/main-WASM doubles. Not browser or Rust conformance evidence.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {execFileSync,spawnSync} = require('node:child_process');

function fixture() {
  const sent = []; const events = []; const held = [];
  let holdStats = false;
  class WorkerDouble {
    constructor() { this.listeners = {}; }
    addEventListener(type, fn) { this.listeners[type] = fn; }
    removeEventListener(type) { delete this.listeners[type]; }
    terminate() {}
    postMessage(request) {
      sent.push(request);
      const respond = () => {
        if (request.type === 'init') return this.listeners.message?.({data:{type:'ready'}});
        const activation = ['activate_role','clear_role'].includes(request.op);
        const result = request.op === 'prepare_role' || request.op === 'prepare_snapshot'
          ? {token:'worker-token',thesaurusText:request.payload.thesaurusText || 'old'}
          : request.op === 'export_snapshot' ? {snapshotText:'old-corpus'} : {};
        this.listeners.message?.({data:{version:1,requestId:request.requestId,
          generation:activation ? request.payload.nextGeneration : request.generation,
          revision:'0',ok:true,result}});
      };
      if (holdStats && request.op === 'stats') held.push(respond);
      else queueMicrotask(respond);
    }
  }
  let generation = 5n; let text = 'old'; let prepared; let listener;
  const notify = () => {try {listener?.();} catch (_) { /* Rust isolates observer errors. */ }};
  const api = {
    kg_set_role_change_listener: fn => {listener=fn;},
    kg_role_snapshot_json: () => JSON.stringify({generation:String(generation),thesaurusText:text,name:'role'}),
    kg_load_thesaurus: value => { if (value==='invalid') throw Error('invalid'); text=value; ++generation; prepared=null; notify(); return {concepts:1}; },
    kg_clear_thesaurus: () => {text=null; ++generation; prepared=null; notify();},
    kg_prepare_role: value => { prepared={value,generation};return 'main-token'; },
    kg_commit_prepared_role: () => {
      if (!prepared || prepared.generation !== generation) throw Error('stale');
      text=prepared.value; ++generation; prepared=null; notify();
      return JSON.stringify({generation:String(generation)});
    },
    kg_discard_prepared_role: () => {prepared=null;},
  };
  const context = vm.createContext({console,URL,setTimeout,clearTimeout,queueMicrotask,
    Worker:WorkerDouble,document:{baseURI:'https://local/',addEventListener(){}},
    CustomEvent:class {constructor(type,options) {this.type=type;this.detail=options.detail;}},
  });
  context.window=context;
  for (const name of ['rolegraph-client','editor']) {
    vm.runInContext(fs.readFileSync(`public/js/${name}.js`,'utf8'),context);
  }
  const editor = Object.create(context.MarkdownEditor.prototype);
  Object.assign(editor,{config:{},destroyed:false,graphClient:null,
    requireDocumentApi:()=>api,documentApi:()=>api,
    indicators:{flush(){throw Error('redraw failed');}},
    root:{dataset:{},dispatchEvent:event=>events.push(event)},
  });
  const changes = new Set();
  editor.surface = {getText:()=> 'original markdown',onChange:fn=>{changes.add(fn);return ()=>changes.delete(fn);}};
  return {editor,api,sent,events,changes,holdStats:()=>{holdStats=true;},release:()=>held.splice(0).forEach(fn=>fn()),listener:()=>listener};
}

for (const method of ['loadThesaurus','clearThesaurus']) {
  test(`${method} notifies the graph even when role redraw fails`,async()=>{
    const {editor,api}=fixture(); const graph=editor.rolegraph;
    await editor.graphRoleSync;
    assert.doesNotThrow(()=>editor[method]('direct'));
    await editor.graphRoleSync;
    assert.equal(graph.generation,JSON.parse(api.kg_role_snapshot_json()).generation);
    assert.equal(graph.state,'ready'); graph.destroy();
  });
}

for (const method of ['loadRole','importSnapshot']) {
  for (const direct of [false,true]) {
    test(`attachment follows ${method} after superseding ${direct?'direct':'lazy'} synchronization`,async()=>{
      const {editor,sent}=fixture(); const graph=editor.rolegraph;
      await graph.ready;
      if (direct) editor.loadThesaurus('direct');
      await graph[method]('new');
      await graph.attachDocument({id:'notes'});
      assert.equal(sent.filter(r=>r.op==='index_documents').length,1);
      editor.detachGraphDocument(); graph.destroy();
    });
  }
}

test('attachment propagates current lane failure and removes listeners',async()=>{
  const {editor,changes}=fixture(); const graph=editor.rolegraph;
  await editor.graphRoleSync;
  const failure=Object.assign(Error('current failure'),{code:'worker_failed'});
  const listeners=graph.listeners.size;
  graph.lifecycle=Promise.reject(failure); graph.lifecycle.catch(()=>{});
  await assert.rejects(graph.attachDocument({id:'notes'}),error=>error===failure);
  assert.equal(changes.size,0);
  assert.equal(graph.listeners.size,listeners);
  assert.equal(editor.graphAttachment,null); graph.destroy();
});

test('attachment follows a replacement lane while waiting and rejects detached attachment',async()=>{
  const {editor,sent,changes}=fixture(); const graph=editor.rolegraph; await editor.graphRoleSync;
  let rejectOld;
  graph.lifecycle=new Promise((_,reject)=>{rejectOld=reject;});
  const attachment=graph.attachDocument({id:'notes'});
  const load=graph.loadRole('replacement');
  rejectOld(Object.assign(Error('superseded'),{code:'stale_generation'}));
  await load; await attachment;
  assert.equal(sent.filter(r=>r.op==='index_documents').length,1);
  let resolveLane; graph.lifecycle=new Promise(resolve=>{resolveLane=resolve;});
  const detached=graph.attachDocument({id:'cancelled'});
  editor.detachGraphDocument(); resolveLane();
  await assert.rejects(detached,/cancelled/);
  assert.equal(changes.size,0);
  assert.equal(sent.filter(r=>r.op==='index_documents').length,1); graph.destroy();
});

for (const mutation of ['kg_load_thesaurus','kg_clear_thesaurus','kg_commit_prepared_role']) {
  test(`raw ${mutation} synchronously invalidates active and queued old jobs`,async()=>{
    const {editor,api,holdStats,release}=fixture(); const graph=editor.rolegraph;
    await editor.graphRoleSync;
    holdStats();
    const active=graph.command('stats'); const queued=graph.command('stats');
    const activeRefused=assert.rejects(active,{code:'stale_generation'});
    const queuedRefused=assert.rejects(queued,{code:'stale_generation'});
    if (mutation==='kg_commit_prepared_role') api.kg_prepare_role('external');
    api[mutation]('external');
    assert.equal(graph.state,'syncing');
    await Promise.all([activeRefused,queuedRefused]); release();
    await editor.graphRoleSync;
    assert.equal(graph.generation,JSON.parse(api.kg_role_snapshot_json()).generation);
    assert.equal(graph.hasRole,mutation!=='kg_clear_thesaurus');
    graph.destroy();
  });
}

test('raw invalid load does not notify or change main/graph',async()=>{
  const {editor,api}=fixture(); const graph=editor.rolegraph; await editor.graphRoleSync;
  const before=api.kg_role_snapshot_json(); const epoch=graph.epoch;
  assert.throws(()=>api.kg_load_thesaurus('invalid'));
  assert.equal(api.kg_role_snapshot_json(),before); assert.equal(graph.epoch,epoch);
  assert.equal(graph.state,'ready'); graph.destroy();
});

test('editor wrappers publish exactly one authoritative sync',async()=>{
  const {editor}=fixture(); const graph=editor.rolegraph; await editor.graphRoleSync;
  for (const method of ['loadThesaurus','clearThesaurus']) {
    const epoch=graph.epoch; editor[method]('new');
    assert.equal(graph.epoch,epoch+1); await editor.graphRoleSync;
  }
  graph.destroy();
});

test('observer registration is lazy and removed on editor teardown',async()=>{
  const {editor,api,listener}=fixture();
  editor.loadThesaurus('before'); assert.equal(listener(),undefined);
  const graph=editor.rolegraph; await editor.graphRoleSync; assert.equal(typeof listener(),'function');
  Object.assign(editor,{abortController:{abort(){}},createdNodes:[],warningListeners:new Set(),
    indicators:null,surface:null});
  editor.destroy(); assert.equal(listener(),undefined);
  api.kg_clear_thesaurus(); assert.equal(graph.state,'destroyed');
});

test('older WASM supports ordinary teardown but cannot start an unobserved graph',()=>{
  const {editor,api}=fixture(); delete api.kg_set_role_change_listener;
  assert.throws(()=>editor.rolegraph,/coordination API/);
  assert.equal(editor.graphClient,null);
  Object.assign(editor,{abortController:{abort(){}},createdNodes:[],warningListeners:new Set(),
    indicators:null,surface:null});
  assert.doesNotThrow(()=>editor.destroy());
});

test('build script metadata refuses a stale standalone lock without rewriting it',()=>{
  const directory=fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(),'rolegraph-lock-test-'));
  try {
    const manifest=path.join(directory,'Cargo.toml');
    const initial='[package]\nname="lock_probe"\nversion="0.1.0"\nedition="2021"\n[lib]\npath="lib.rs"\n';
    fs.writeFileSync(manifest,initial); fs.writeFileSync(path.join(directory,'lib.rs'),'');
    const cargo=execFileSync('which',['cargo'],{encoding:'utf8'}).trim();
    execFileSync(cargo,['generate-lockfile','--offline','--manifest-path',manifest],{stdio:'pipe'});
    const lock=path.join(directory,'Cargo.lock'); const before=fs.readFileSync(lock,'utf8');
    const dependency=path.join(directory,'dependency'); fs.mkdirSync(dependency);
    fs.writeFileSync(path.join(dependency,'Cargo.toml'),initial.replace('lock_probe','local_dependency'));
    fs.writeFileSync(path.join(dependency,'lib.rs'),'');
    fs.writeFileSync(manifest,initial+'[dependencies]\nlocal_dependency={path="dependency"}\n');
    const bin=path.join(directory,'bin'); fs.mkdirSync(bin);
    // Run the actual build script, redirecting only its manifest to a tiny real
    // Cargo fixture. Never touch the concurrently owned bridge lockfile.
    fs.writeFileSync(path.join(bin,'cargo'),'#!/bin/bash\nargs=()\nwhile [ "$#" -gt 0 ]; do\nif [ "$1" = --manifest-path ]; then args+=("$1" "$TEST_MANIFEST"); shift 2; else args+=("$1"); shift; fi\ndone\nprintf "%s\\n" "${args[@]}" > "$TEST_TRACE"\nexec "$REAL_CARGO" "${args[@]}"\n',{mode:0o755});
    const trace=path.join(directory,'trace');
    // System macOS Bash 3 treats the script's empty ARGS array as unset.
    const bash=fs.existsSync('/opt/homebrew/bin/bash')?'/opt/homebrew/bin/bash':'bash';
    const result=spawnSync(bash,['scripts/build-rolegraph.sh'],{encoding:'utf8',env:{...process.env,
      TE_ROLEGRAPH_SOURCE:'',PATH:bin+path.delimiter+process.env.PATH,
      REAL_CARGO:cargo,TEST_MANIFEST:manifest,TEST_TRACE:trace}});
    assert.notEqual(result.status,0);
    assert.ok(fs.existsSync(trace),result.stderr);
    assert.equal(fs.readFileSync(lock,'utf8'),before);
    assert.match(fs.readFileSync(trace,'utf8'),/\n--locked\n/);
    assert.match(result.stderr,/lock file|lockfile/);
  } finally {fs.rmSync(directory,{recursive:true,force:true});}
});

for (const method of ['loadRole','importSnapshot']) {
  test(`${method} cannot roll graph back after successful main commit if redraw fails`,async()=>{
    const {editor,api,sent,events}=fixture(); const graph=editor.rolegraph;
    await editor.graphRoleSync;
    const epoch=graph.epoch;
    const preparations=sent.filter(r=>r.op==='prepare_role').length;
    await graph[method]('new');
    assert.equal(graph.epoch,epoch+1); // Own commit observer must not add a sync/reset.
    assert.equal(sent.filter(r=>r.op==='prepare_role').length,preparations+(method==='loadRole'?1:0));
    assert.equal(graph.generation,JSON.parse(api.kg_role_snapshot_json()).generation);
    assert.equal(graph.state,'ready');
    assert.equal(sent.filter(r=>r.op==='prepare_snapshot').length,method==='importSnapshot'?1:0);
    assert.ok(events.some(e=>e.type==='te:rolegraph-error' && e.detail.code==='render_failed'));
    graph.destroy();
  });
}
