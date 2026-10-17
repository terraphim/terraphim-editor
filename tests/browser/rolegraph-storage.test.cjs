// Unit tests: IndexedDB transaction double, NOT real-browser durability evidence.
const {test}=require('node:test');
const assert=require('node:assert/strict');
require('../../public/js/rolegraph-storage.js');
function databaseDouble() {
  const entries=new Map(); const transactions=[]; let opens=0;
  const db={objectStoreNames:{contains:()=>true},close(){this.closed=true;},transaction(){
    const tx={objectStore(){return {put(value,key){tx.write={value,key};},get(key){const request={}; queueMicrotask(()=>{request.result=entries.get(key);request.onsuccess?.();});return request;}}},abort(){tx.onabort?.();}}; transactions.push(tx); return tx;
  }};
  const indexedDB={open(){opens++; const request={}; queueMicrotask(()=>{request.result=db;request.onsuccess();});return request;}};
  return {db,indexedDB,transactions,entries,get opens(){return opens;},complete(){const tx=transactions.at(-1);if(tx.write)entries.set(tx.write.key,tx.write.value);tx.oncomplete();},fail(){transactions.at(-1).onabort();}};
}
test('opt-in and explicit namespace required; disabled never opens DB',async()=>{
  const d=databaseDouble();const s=new TeRoleGraphStorage({enabled:false,indexedDB:d.indexedDB}); await assert.rejects(s.save('x'),{code:'storage_failed'});assert.equal(d.opens,0);
  assert.throws(()=>new TeRoleGraphStorage({enabled:true,indexedDB:d.indexedDB}),{code:'invalid_input'});
});
test('opaque text saves only on transaction completion, loads exact bytes',async()=>{
  const d=databaseDouble();const s=new TeRoleGraphStorage({enabled:true,namespace:'embed-a',indexedDB:d.indexedDB});
  const text='{"graphJson":"{\"id\":9007199254740993}"}';let saved=false;const p=s.save(text).then(()=>saved=true);await new Promise(setImmediate);assert.equal(saved,false);d.complete();await p;
  const load=s.load();await new Promise(setImmediate);d.complete();assert.equal(await load,text);s.close();assert.ok(d.db.closed);assert.equal(d.entries.size,1);
});
test('aborted overwrite preserves previous checkpoint',async()=>{
  const d=databaseDouble();const s=new TeRoleGraphStorage({enabled:true,namespace:'embed-b',indexedDB:d.indexedDB});
  const p=s.save('previous');await new Promise(setImmediate);d.complete();await p;
  const bad=s.save('next');await new Promise(setImmediate);d.fail();await assert.rejects(bad,{code:'storage_failed'});assert.deepEqual([...d.entries.values()],['previous']);s.close();
});
test('open denial is typed; close aborts unfinished transactions',async()=>{
  const denied={open(){const r={};queueMicrotask(()=>r.onerror());return r;}};
  await assert.rejects(new TeRoleGraphStorage({enabled:true,namespace:'denied',indexedDB:denied}).load(),{code:'storage_failed'});
  const d=databaseDouble();const s=new TeRoleGraphStorage({enabled:true,namespace:'close',indexedDB:d.indexedDB});const p=s.save('data');await new Promise(setImmediate);s.close();await assert.rejects(p,{code:'storage_failed'});assert.equal(d.entries.size,0);
});
