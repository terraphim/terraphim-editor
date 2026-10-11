const {test}=require('node:test'); const assert=require('node:assert/strict'); const fs=require('node:fs/promises'); const {pathToFileURL}=require('node:url'); const path=require('node:path');
const dir=process.env.TERRAPHIM_HOST_WASM;
let api;
async function bindings(){if(!api){api=await import(pathToFileURL(path.join(dir,'terraphim_editor.mjs')));await api.default({module_or_path:await fs.readFile(path.join(dir,'terraphim_editor_bg.wasm'))});}return api;}
test('actual WASM authored synonym registration follows exact ACK, save/reopen and history',async()=>{
 const api=await bindings();const original='Decision matters. Café 😀\r\n';const a=new api.TerraphimHostSession('file:///a.md','a',1,original);
 try{const current=JSON.parse(a.snapshot());const proposal=JSON.parse(a.propose(JSON.stringify({...current,id:1,command:'alt_create',args:{kind:'word',start:0,end:8,text:'Choice'}})));const id=proposal.projection.annotations.spans[0].id;
 assert.deepEqual(JSON.parse(a.synonyms_for(id,'Choice')),[]);a.acknowledge(1,2,proposal.source);assert.deepEqual(JSON.parse(a.synonyms_for(id,'Choice')),['decision','choice']);
 const reopened=new api.TerraphimHostSession('file:///a.md','reopened',1,proposal.source);try{assert.deepEqual(JSON.parse(reopened.synonyms_for(id,'Choice')),['decision','choice']);}finally{reopened.free();}
 a.synchronize(3,original);assert.deepEqual(JSON.parse(a.synonyms_for(id,'Choice')),[]);a.synchronize(4,proposal.source);assert.deepEqual(JSON.parse(a.synonyms_for(id,'Choice')),['decision','choice']);
 const group=JSON.parse(a.snapshot()).projection.annotations.synonymGroups[0];assert.equal(group.markdown,'synonyms:: Decision, Choice\n');assert.equal(group.thesaurus.data.choice.id,group.thesaurus.data.decision.id);
 }finally{a.free();}
});
