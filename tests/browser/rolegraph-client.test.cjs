// Unit tests: messaging double, NOT an end-to-end graph engine.
const { test } = require('node:test');
const assert = require('node:assert/strict');
require('../../public/js/rolegraph-client.js');
class WorkerDouble {
  constructor() { this.sent = []; this.listeners = {}; }
  addEventListener(type, fn) { this.listeners[type] = fn; }
  removeEventListener(type) { delete this.listeners[type]; }
  postMessage(message) { this.sent.push(message); if (message.type === 'init') queueMicrotask(() => this.emit({type:'ready'})); }
  emit(data) { this.listeners.message?.({data}); }
  reply(result = {}, overrides = {}) { const r = this.sent.at(-1); this.emit({version:1,requestId:r.requestId,generation:r.op === 'activate_role' || r.op === 'clear_role' ? r.payload.nextGeneration : r.generation,revision:'0',ok:true,result,...overrides}); }
  terminate() { this.terminated = true; }
}
async function fixture(options={}) { const w = new WorkerDouble(); const c = new TeRoleGraphClient({assetBase:'https://host/nested/',workerFactory:()=>w,...options}); await c.ready; return {c,w}; }
test('initial generation, explicit nested assets and exact-string replies', async () => {
  const {c,w}=await fixture(); assert.equal(w.sent[0].glueUrl,'https://host/nested/rolegraph/terraphim_rolegraph_browser.js');
  const p=c.command('stats'); assert.equal(w.sent.at(-1).generation,'0'); w.reply({rank:'9007199254740993'}); assert.equal((await p).rank,'9007199254740993'); c.destroy();
});
test('queue bound, abort queued calls, destroy rejects everything',async()=>{
  const {c,w}=await fixture(); const first=c.command('stats'); const ac=new AbortController(); const queued=c.command('stats',{}, {signal:ac.signal}); ac.abort(); await assert.rejects(queued,{code:'cancelled'});
  const jobs=Array.from({length:32},()=>c.command('stats')); await assert.rejects(c.command('stats'),{code:'resource_limit'});
  c.destroy(); await assert.rejects(first,{code:'destroyed'}); await Promise.all(jobs.map(p=>assert.rejects(p,{code:'destroyed'}))); assert.ok(w.terminated);
});
test('invalid envelope fails transport; unrelated late replies ignored',async()=>{
  const {c,w}=await fixture(); const p=c.command('stats'); w.emit({requestId:'unknown'}); w.reply({}, {generation:'99'}); await assert.rejects(p,{code:'worker_failed'}); assert.equal(c.state,'failed'); c.destroy();
});
test('sync role guards queries and latest sync wins',async()=>{
  const {c,w}=await fixture(); const a=c.syncRole({generation:'1',thesaurusText:'a'}); await Promise.resolve();
  await assert.rejects(c.command('query',{text:'a'}),{code:'syncing'});
  const b=c.syncRole({generation:'2',thesaurusText:'b'}); w.reply({token:'a',thesaurusText:'a'}); await assert.rejects(a,{code:'stale_generation'});
  await new Promise(setImmediate); assert.equal(w.sent.at(-1).payload.thesaurusText,'b'); w.reply({token:'b',thesaurusText:'b'}); await new Promise(setImmediate); w.reply(); await b; assert.equal(c.generation,'2'); assert.equal(c.state,'ready'); c.destroy();
});
test('load candidate coordinates main prepare/commit; corrupt import retains role',async()=>{
  const calls=[]; const {c,w}=await fixture({roleCoordinator:{prepare:async text=>{calls.push(text);return 'main';},commit:token=>calls.push(token),discard:async()=>{}}});
  const load=c.loadRole('valid'); await new Promise(setImmediate); w.reply({token:'candidate',thesaurusText:'valid'}); await new Promise(setImmediate); assert.equal(w.sent.at(-1).op,'activate_role'); w.reply(); await load; assert.deepEqual(calls,['valid','main']);
  const bad=c.importSnapshot('corrupt'); await new Promise(setImmediate); w.reply({}, {ok:false,error:{code:'invalid_snapshot',message:'invalid'}}); await assert.rejects(bad,{code:'invalid_snapshot'}); assert.equal(c.state,'ready'); assert.equal(c.generation,'1'); c.destroy();
});
test('query watchdog terminates worker rather than speculatively restoring',async()=>{
  const {c,w}=await fixture({queryTimeout:10}); await assert.rejects(c.command('stats'),{code:'timeout'}); assert.ok(w.terminated); assert.equal(c.state,'failed'); c.destroy();
});
test('startup failure is typed and teardown safe',async()=>{
  const c=new TeRoleGraphClient({assetBase:'https://host/',workerFactory:()=>{throw Error('private detail');}}); await assert.rejects(c.ready,{code:'worker_failed'}); await assert.rejects(c.command('stats'),{code:'worker_failed'}); c.destroy();
});
test('authoritative sync failure never exposes previous graph and can retry',async()=>{
  const {c,w}=await fixture();const bad=c.syncRole({generation:'8',thesaurusText:'bad'});await new Promise(setImmediate);w.reply({}, {ok:false,error:{code:'invalid_input',message:'bad'}});await assert.rejects(bad,{code:'invalid_input'});assert.equal(c.state,'failed');await assert.rejects(c.command('stats'),{code:'worker_failed'});
  const next=c.syncRole({generation:'9',thesaurusText:'good'});await new Promise(setImmediate);w.reply({token:'good',thesaurusText:'good'});await new Promise(setImmediate);w.reply();await next;assert.equal(c.generation,'9');c.destroy();
});
test('failed main commit restores a real previous snapshot before ready',async()=>{
  const calls=[];const {c,w}=await fixture({roleCoordinator:{prepare:()=> 'main',commit:()=>{throw Error('commit');},discard:t=>calls.push(t)}});
  const sync=c.syncRole({generation:'5',thesaurusText:'old'});await new Promise(setImmediate);w.reply({token:'old',thesaurusText:'old'});await new Promise(setImmediate);w.reply();await sync;
  const load=c.loadRole('new');await new Promise(setImmediate);w.reply({token:'new',thesaurusText:'new'});await new Promise(setImmediate);assert.equal(w.sent.at(-1).op,'export_snapshot');w.reply({snapshotText:'real-checkpoint'});await new Promise(setImmediate);w.reply();await new Promise(setImmediate);assert.equal(w.sent.at(-1).op,'prepare_snapshot');assert.equal(w.sent.at(-1).payload.snapshotText,'real-checkpoint');w.reply({token:'rollback',thesaurusText:'old'});await new Promise(setImmediate);w.reply();await assert.rejects(load,/commit/);assert.equal(c.generation,'5');assert.equal(c.state,'ready');assert.deepEqual(calls,['main']);c.destroy();
});
test('inflight cancellation invalidates transport; restore uses real validation',async()=>{
  const {c,w}=await fixture();const abort=new AbortController();const p=c.command('stats',{}, {signal:abort.signal});abort.abort();await assert.rejects(p,{code:'cancelled'});assert.ok(w.terminated);c.destroy();
  const f=await fixture({roleCoordinator:{prepare:()=> 'main',commit:()=>{},discard:()=>{}}});f.c.storage={load:async()=> 'stored opaque wrapper',close(){}};
  const restore=f.c.restoreCheckpoint();await new Promise(setImmediate);assert.equal(f.w.sent.at(-1).op,'prepare_snapshot');assert.equal(f.w.sent.at(-1).payload.snapshotText,'stored opaque wrapper');f.w.reply({}, {ok:false,error:{code:'invalid_snapshot',message:'corrupt'}});await assert.rejects(restore,{code:'invalid_snapshot'});assert.equal(f.c.generation,'0');f.c.destroy();
});
test('watchdog recovers only when a real checkpoint exists',async()=>{
  const {c,w}=await fixture({queryTimeout:30});const sync=c.syncRole({generation:'3',thesaurusText:'valid'});await new Promise(setImmediate);w.reply({token:'valid',thesaurusText:'valid'});await new Promise(setImmediate);w.reply();await sync;
  c.storage={save:async()=>{},close(){}};const save=c.saveCheckpoint();w.reply({snapshotText:'checkpoint-from-bridge'});await save;
  await assert.rejects(c.command('query',{text:'x',mode:'graph'}),{code:'timeout'});await new Promise(setImmediate);assert.equal(c.state,'syncing');assert.equal(w.sent.at(-1).op,'prepare_snapshot');assert.equal(w.sent.at(-1).payload.snapshotText,'checkpoint-from-bridge');w.reply({token:'restored',thesaurusText:'valid'});await new Promise(setImmediate);w.reply();await c.lifecycle;assert.equal(c.state,'ready');assert.equal(c.generation,'3');c.destroy();
});
test('revision regression and malformed response are refused',async()=>{
  const {c,w}=await fixture();const a=c.command('index_documents',{documents:[]});w.reply({}, {revision:'2'});await a;const b=c.command('stats');assert.equal(w.sent.at(-1).expectedRevision,'2');w.reply({}, {revision:'1'});await assert.rejects(b,{code:'worker_failed'});c.destroy();
  const f=await fixture();const p=f.c.command('stats');f.w.reply({}, {version:2});await assert.rejects(p,{code:'worker_failed'});f.c.destroy();
});
test('latest main sync discards a pending asynchronous load preparation',async()=>{
  let finishPrepare;let commits=0;const discarded=[];const {c,w}=await fixture({roleCoordinator:{prepare:()=>new Promise(resolve=>finishPrepare=resolve),commit:()=>commits++,discard:t=>discarded.push(t)}});
  const load=c.loadRole('candidate');await new Promise(setImmediate);w.reply({token:'candidate',thesaurusText:'candidate'});await new Promise(setImmediate);const sync=c.syncRole({generation:'10',thesaurusText:'authoritative'});finishPrepare('prepared-main');await assert.rejects(load,{code:'stale_generation'});await new Promise(setImmediate);w.reply({token:'authoritative',thesaurusText:'authoritative'});await new Promise(setImmediate);w.reply();await sync;assert.equal(c.generation,'10');assert.equal(commits,0);assert.deepEqual(discarded,['prepared-main']);c.destroy();
});

const tick = () => new Promise(setImmediate);
async function oldRole(c,w) {
  const sync=c.syncRole({generation:'5',thesaurusText:'OLD'});
  await tick();w.reply({token:'old',thesaurusText:'OLD'});await tick();w.reply();await sync;
}
async function activateCandidate(c,w) {
  const outcome=c.loadRole('A').then(()=>null,e=>e);
  await tick();w.reply({token:'A',thesaurusText:'A'});await tick();
  assert.equal(w.sent.at(-1).op,'export_snapshot');w.reply({snapshotText:'EXACT-OLD-CORPUS-9007199254740993'});await tick();
  assert.equal(w.sent.at(-1).op,'activate_role');return {outcome};
}
async function reconcileOld(w) {
  await tick();assert.equal(w.sent.at(-1).op,'prepare_snapshot');
  assert.equal(w.sent.at(-1).payload.snapshotText,'EXACT-OLD-CORPUS-9007199254740993');
  w.reply({token:'rollback-old',thesaurusText:'OLD'});await tick();
  assert.equal(w.sent.at(-1).payload.nextGeneration,'5');w.reply();await tick();
}
for (const next of ['reject-prepare','reject-commit','authoritative-sync']) {
  test(`superseded activation reconciles OLD corpus before ${next}`,async t=>{
    let main='OLD';const commits=[];const states=[];
    const {c,w}=await fixture({onStateChange:s=>states.push({...s}),roleCoordinator:{prepare:text=>text,commit:token=>{commits.push(token);throw Error('main stays OLD');},discard:()=>{}}});
    t.after(()=>c.destroy());await oldRole(c,w);states.length=0;
    const {outcome:first}=await activateCandidate(c,w);
    assert.equal(w.sent.at(-1).op,'activate_role');
    const second=(next==='authoritative-sync' ? (main='NEW',c.syncRole({generation:'10',thesaurusText:'NEW'})) : c.importSnapshot('B')).then(()=>null,e=>e);
    w.reply();await reconcileOld(w);assert.equal((await first).code,'stale_generation');
    assert.ok(!states.some(s=>s.state==='ready'));
    if (next==='reject-prepare') {
      assert.equal(w.sent.at(-1).op,'prepare_snapshot');assert.equal(w.sent.at(-1).payload.snapshotText,'B');
      w.reply({}, {ok:false,error:{code:'invalid_snapshot',message:'bad B'}});
      assert.equal((await second).code,'invalid_snapshot');assert.equal(c.generation,'5');assert.equal(main,'OLD');assert.deepEqual(commits,[]);
    } else if (next==='reject-commit') {
      w.reply({token:'B',thesaurusText:'B'});await tick();assert.equal(w.sent.at(-1).op,'export_snapshot');w.reply({snapshotText:'EXACT-OLD-CORPUS-9007199254740993'});
      await tick();assert.equal(w.sent.at(-1).payload.nextGeneration,'6');w.reply();await reconcileOld(w);
      assert.equal((await second).code,'invalid_input');assert.deepEqual(commits,['B']);assert.equal(c.generation,'5');assert.equal(main,'OLD');
    } else {
      assert.equal(w.sent.at(-1).op,'prepare_role');assert.equal(w.sent.at(-1).payload.thesaurusText,'NEW');
      w.reply({token:'NEW',thesaurusText:'NEW'});await tick();w.reply();assert.equal(await second,null);assert.equal(c.generation,'10');assert.equal(main,'NEW');assert.deepEqual(commits,[]);
    }
    assert.equal(c.state,'ready');assert.ok(states.filter(s=>s.state==='ready').every(s=>s.generation===(next==='authoritative-sync'?'10':'5')));
  });
}
for (const boundary of ['load','import','sync','clear']) {
  for (const op of ['query','index_documents']) {
    test(`active ${op} rejects at ${boundary} boundary but reply still reconciles`,async t=>{
      const {c,w}=await fixture({roleCoordinator:{prepare:()=> 'main',commit:()=>{},discard:()=>{}}});t.after(()=>c.destroy());
      let settled=false;const result=c.command(op,op==='query'?{text:'old',mode:'graph'}:{documents:[]}).then(value=>{settled=true;return {value};},e=>{settled=true;return {error:e};});
      const transition=(boundary==='load'?c.loadRole('NEW'):boundary==='import'?c.importSnapshot('NEW'):c.syncRole({generation:'10',thesaurusText:boundary==='clear'?null:'NEW'})).then(()=>null,e=>e);
      await tick();assert.equal(settled,true);assert.equal((await result).error.code,'stale_generation');
      w.reply({oldRoleResult:true},{revision:op==='index_documents'?'1':'0'});await tick();
      assert.equal(c.state,'syncing');assert.equal(w.sent.at(-1).expectedRevision,op==='index_documents'?'1':'0');
      if (boundary!=='clear') {w.reply({token:'NEW',thesaurusText:'NEW'},{revision:op==='index_documents'?'1':'0'});await tick();}
      w.reply();assert.equal(await transition,null);assert.equal(c.state,'ready');assert.equal(c.generation,boundary==='load'||boundary==='import'?'1':'10');
    });
  }
}
test('a later import cannot skip a pending authoritative synchronization',async t=>{
  const states=[];const {c,w}=await fixture({onStateChange:s=>states.push({...s}),roleCoordinator:{prepare:t=>t,commit:()=>{throw Error('refused');},discard:()=>{}}});t.after(()=>c.destroy());states.length=0;
  const sync=c.syncRole({generation:'10',thesaurusText:'MAIN-NEW'}).then(()=>null,e=>e);
  const load=c.importSnapshot('candidate').then(()=>null,e=>e);
  await tick();assert.equal(w.sent.at(-1).payload.thesaurusText,'MAIN-NEW');w.reply({token:'main-new',thesaurusText:'MAIN-NEW'});await tick();w.reply();
  assert.equal((await sync).code,'stale_generation');await tick();assert.equal(c.hasRole,true);assert.equal(w.sent.at(-1).op,'prepare_snapshot');w.reply({token:'candidate',thesaurusText:'candidate'});
  await tick();assert.equal(w.sent.at(-1).op,'export_snapshot');w.reply({snapshotText:'EXACT-MAIN-NEW-CORPUS'});await tick();assert.equal(w.sent.at(-1).payload.nextGeneration,'11');w.reply();
  await tick();assert.equal(w.sent.at(-1).payload.snapshotText,'EXACT-MAIN-NEW-CORPUS');w.reply({token:'rollback-main-new',thesaurusText:'MAIN-NEW'});await tick();assert.equal(w.sent.at(-1).payload.nextGeneration,'10');w.reply();
  assert.equal((await load).code,'invalid_input');assert.equal(c.generation,'10');assert.equal(c.state,'ready');assert.ok(states.filter(s=>s.state==='ready').every(s=>s.generation==='10'));
});
test('failed authoritative sync cannot expose the old graph through a queued import',async t=>{
  const {c,w}=await fixture({roleCoordinator:{prepare:t=>t,commit:()=>{},discard:()=>{}}});t.after(()=>c.destroy());await oldRole(c,w);
  const sync=c.syncRole({generation:'10',thesaurusText:'MAIN-NEW'}).then(()=>null,e=>e);const load=c.importSnapshot('candidate').then(()=>null,e=>e);
  await tick();w.reply({}, {ok:false,error:{code:'invalid_input',message:'rejected'}});assert.equal((await sync).code,'invalid_input');assert.equal((await load).code,'worker_failed');assert.equal(c.state,'failed');
});
test('failed stale-activation rollback blocks subsequent imports instead of guessing a baseline',async t=>{
  const {c,w}=await fixture({roleCoordinator:{prepare:t=>t,commit:()=>{},discard:()=>{}}});t.after(()=>c.destroy());await oldRole(c,w);
  const {outcome:first}=await activateCandidate(c,w);const load=c.importSnapshot('B').then(()=>null,e=>e);w.reply();await tick();assert.equal(w.sent.at(-1).payload.snapshotText,'EXACT-OLD-CORPUS-9007199254740993');
  w.reply({}, {ok:false,error:{code:'invalid_snapshot',message:'cannot recover'}});assert.equal((await first).code,'stale_generation');assert.equal((await load).code,'worker_failed');assert.equal(c.state,'failed');
});
test('async main commit is rejected before candidate activation',async t=>{
  let called=false;const {c,w}=await fixture({roleCoordinator:{prepare:t=>t,commit:async()=>{called=true;},discard:()=>{}}});t.after(()=>c.destroy());await assert.rejects(c.loadRole('A'),{code:'invalid_input'});assert.equal(called,false);assert.equal(w.sent.length,1);assert.equal(c.generation,'0');
});
