// Unit tests: bridge double only. Real WASM + browser Worker qualification is separate.
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {Worker}=require('node:worker_threads');
const {pathToFileURL}=require('node:url');
const path=require('node:path');
const {once}=require('node:events');
function worker() {
  const url=pathToFileURL(path.resolve(__dirname,'../../public/js/rolegraph-worker.js')).href;
  return new Worker(`const {parentPort}=require('node:worker_threads');globalThis.self={addEventListener:(_,fn)=>parentPort.on('message',data=>fn({data})),postMessage:data=>parentPort.postMessage(data)};import(${JSON.stringify(url)}).then(()=>parentPort.postMessage({type:'boot'}));`,{eval:true});
}
test('module worker dynamically initializes glue and delegates opaque dispatch',async t=>{
  const w=worker();t.after(()=>w.terminate());await once(w,'message');
  const glue=`export default async function init(config){if(config.module_or_path!=='explicit-wasm-url')throw Error('url');}export class BrowserRoleGraph{dispatch(text){const r=JSON.parse(text);return JSON.stringify({version:1,requestId:r.requestId,generation:r.generation,revision:'0',ok:true,result:{snapshotText:'{opaque large IDs 9007199254740993}',op:r.op}});}}`;
  let next=once(w,'message');w.postMessage({type:'init',glueUrl:'data:text/javascript;base64,'+Buffer.from(glue).toString('base64'),wasmUrl:'explicit-wasm-url'});assert.deepEqual((await next)[0],{type:'ready'});
  next=once(w,'message');w.postMessage({version:1,requestId:'1',generation:'0',expectedRevision:null,op:'export_snapshot',payload:{}});const reply=(await next)[0];assert.equal(reply.result.op,'export_snapshot');assert.equal(reply.result.snapshotText,'{opaque large IDs 9007199254740993}');
});
test('initialization and missing bridge failures are typed without raw exception text',async t=>{
  const w=worker();t.after(()=>w.terminate());await once(w,'message');let next=once(w,'message');w.postMessage({version:1,requestId:'early',generation:'0',expectedRevision:null,op:'stats',payload:{}});assert.equal((await next)[0].error.code,'worker_failed');
  next=once(w,'message');w.postMessage({type:'init',glueUrl:'data:text/javascript,throw new Error("secret corpus")',wasmUrl:'x'});const reply=(await next)[0];assert.equal(reply.error.code,'worker_failed');assert.ok(!JSON.stringify(reply).includes('secret corpus'));
});
