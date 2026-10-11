// Run against built WASM, never a substituted engine or mocked binding.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const { pathToFileURL } = require('node:url');
const path = require('node:path');
const { TerraphimHostBridge } = require('../../public/js/host-session.js');
const assetDir = process.env.TERRAPHIM_HOST_WASM;
if (!assetDir) throw new Error('TERRAPHIM_HOST_WASM must name the actual generated WASM directory');
let bindings;
async function realEngine(session = 'a', source = 'abc') {
  if (!bindings) {
    bindings = await import(pathToFileURL(path.join(assetDir, 'terraphim_editor.mjs')));
    await bindings.default({ module_or_path: await fs.readFile(path.join(assetDir, 'terraphim_editor_bg.wasm')) });
  }
  return new bindings.TerraphimHostSession('file:///a.md', session, 1, source);
}
async function setup(session, source = 'abc') {
  const sent = [], adopted = [], conflicts = [];
  const bridge = new TerraphimHostBridge({ engine: await realEngine(session, source),
    send: m => sent.push(m), adopt: s => adopted.push(s), conflict: m => conflicts.push(m) });
  return { bridge, sent, adopted, conflicts };
}
function ack(proposal, revision) { return { ...proposal, type:'ack', revision }; }
test('actual WASM queues edits until exact host ACK and rejects malformed ACK', async () => {
  const {bridge,sent} = await setup('a');
  bridge.submit('abcd'); bridge.submit('abcde');
  assert.equal(sent.length,1);
  assert.throws(() => bridge.receive({...ack(sent[0],2), source:'wrong'}));
  assert.equal(bridge.current.source,'abc');
  bridge.receive(ack(sent[0],2));
  assert.equal(sent.length,2); assert.equal(sent[1].source,'abcde');
  bridge.receive(ack(sent[1],3));
  assert.equal(bridge.current.source,'abcde'); bridge.dispose();
});
test('two real views synchronize native history without independent undo', async () => {
  const a = await setup('a'), b = await setup('b');
  a.bridge.submit('abcd'); const p = a.sent[0];
  b.bridge.receive({...p,type:'sync',session:'b',revision:2});
  a.bridge.receive(ack(p,2));
  assert.equal(b.adopted[0],'abcd');
  a.bridge.command('undo'); assert.equal(a.sent[1].type,'undo');
  for (const view of [a,b]) view.bridge.receive({...p,type:'sync',session:view.bridge.current.session,source:'abc',revision:3});
  assert.equal(a.adopted.at(-1),'abc'); assert.equal(b.adopted.at(-1),'abc');
  a.bridge.dispose(); b.bridge.dispose();
});
test('external conflict preserves unacknowledged draft and pauses until recovery', async () => {
  const {bridge,sent,conflicts,adopted} = await setup('conflict');
  bridge.submit('local draft');
  bridge.receive({...sent[0],type:'sync',source:'external text',revision:2});
  assert.equal(conflicts[0].draft,'local draft');
  assert.equal(conflicts[0].source,'external text');
  assert.throws(() => bridge.submit('overwrite'));
  assert.equal(bridge.receive(ack(sent[0],3)),false);
  bridge.resume(); assert.equal(adopted.at(-1),'external text'); bridge.dispose();
});
test('save waits for ACK and disposal prevents late messages', async () => {
  const {bridge,sent} = await setup('barrier');
  bridge.submit('dirty'); bridge.command('save'); assert.equal(sent.length,1);
  bridge.receive(ack(sent[0],2)); assert.equal(sent[1].type,'save');
  bridge.dispose(); assert.equal(bridge.receive(ack(sent[0],3)),false);
  assert.throws(() => bridge.submit('late'));
});
test('queued Undo then Save waits for host completion and rebases revision', async () => {
  const {bridge,sent} = await setup('ordered');
  bridge.submit('abcd'); bridge.command('undo'); bridge.command('save');
  bridge.receive(ack(sent[0],2));
  assert.equal(sent.length,2); assert.equal(sent[1].type,'undo');
  bridge.receive({...sent[1],type:'sync',revision:3,source:'abc'});
  assert.equal(sent.length,2);
  bridge.receive({...sent[1],type:'command-complete',revision:3,source:'abc'});
  assert.equal(sent.length,3); assert.equal(sent[2].type,'save'); assert.equal(sent[2].revision,3);
  bridge.receive({...sent[2],type:'command-complete',revision:3,source:'abc'});
  assert.equal(bridge.commandPending,null); bridge.dispose();
});
test('two Undo commands execute separately and wrong completions cannot release queue', async () => {
  const {bridge,sent} = await setup('two-undo');
  bridge.command('undo'); bridge.command('undo');
  assert.equal(sent.length,1);
  assert.equal(bridge.receive({...sent[0],type:'command-complete',id:999,source:'abc'}),false);
  assert.equal(sent.length,1);
  bridge.receive({...sent[0],type:'command-complete',revision:2,source:'ab'});
  assert.equal(sent.length,2); assert.equal(sent[1].revision,2);
  bridge.receive({...sent[1],type:'command-complete',revision:3,source:'a'});
  assert.equal(bridge.current.source,'a'); bridge.dispose();
});

test('rejected host command cancels later commands rather than stalling', async () => {
  const {bridge,sent} = await setup('reject-command');
  bridge.command('undo'); bridge.command('save');
  bridge.receive({...sent[0],type:'reject',source:'abc',revision:1,reason:'Host refused undo'});
  assert.equal(bridge.commandPending,null); assert.equal(bridge.barriers.length,0);
  bridge.submit('abcd'); assert.equal(sent.at(-1).type,'proposal'); bridge.dispose();
});

test('command completion before sync preserves newer local input as a conflict', async () => {
  const {bridge,sent,conflicts,adopted} = await setup('completion-race');
  bridge.command('undo'); bridge.submit('new local text');
  assert.equal(sent.length,1);
  bridge.receive({...sent[0],type:'command-complete',revision:2,source:'ab'});
  assert.equal(bridge.paused,true); assert.equal(conflicts[0].draft,'new local text');
  assert.equal(bridge.current.source,'ab'); assert.deepEqual(adopted,[]);
  bridge.dispose();
});

test('LF presentation submits CRLF native output with correctly rebased second-line anchors', async () => {
  const local = await realEngine('ui', 'First\nbanana');
  const snap = JSON.parse(local.snapshot());
  const ui = JSON.parse(local.propose(JSON.stringify({...snap,id:1,command:'alt_create',
    args:{kind:'word',start:6,end:12,text:'apple'}})));
  const {bridge,sent} = await setup('crlf', 'First\r\nbanana');
  bridge.submit(ui.source);
  assert.equal(sent[0].source.replace(/\r\n/g,'' ).includes('\n'),false);
  assert.equal(sent[0].projection.annotations.spans[0].anchor.start,7);
  assert.equal(bindings.host_presentation_source(sent[0].source),ui.source);
  bridge.receive(ack(sent[0],2));
  assert.equal(bridge.paused,false); assert.equal(bridge.current.projection.body,'First\r\nbanana');
  bridge.dispose(); local.free();
});


test('obsolete rejection cannot cancel a newer multi-view proposal', async () => {
  const {bridge,sent,conflicts} = await setup('late-reject');
  bridge.submit('first draft'); const old = sent[0];
  bridge.receive({...old,type:'sync',revision:2,source:'external'});
  bridge.resume(); bridge.submit('second draft'); const fresh = sent.at(-1);
  assert.equal(bridge.receive({...old,type:'reject',revision:2,source:'external'}),false);
  assert.equal(bridge.pending.proposal.id,fresh.id); assert.equal(bridge.latest,'second draft');
  assert.equal(bridge.paused,false); assert.equal(conflicts.length,1);
  bridge.receive(ack(fresh,3)); assert.equal(bridge.current.source,'second draft'); bridge.dispose();
});

test('export freezes acknowledged source while newer typing waits behind the command', async () => {
  const {bridge,sent} = await setup('export-freeze','acknowledged A');
  bridge.command('export'); const request = sent[0]; bridge.submit('unacknowledged B');
  assert.equal(bridge.exportSnapshot({...request,type:'export-request'}),'acknowledged A');
  assert.equal(sent.length,1);
  assert.throws(() => bridge.exportSnapshot({...request,id:999}));
  bridge.receive({...request,type:'command-complete',source:'acknowledged A'});
  assert.equal(sent[1].type,'proposal'); assert.equal(sent[1].source,'unacknowledged B');
  bridge.dispose();
});

test('host history owns accepted alternatives but retains genuinely uncommitted input undo', () => {
  const owns = TerraphimHostBridge.ownsHistory;
  assert.equal(owns({inSurface:true}),true);
  assert.equal(owns({alternative:true,isNew:false,value:'apple',committed:'apple'}),true);
  assert.equal(owns({alternative:true,isNew:false,value:'unfinished',committed:'apple'}),false);
  assert.equal(owns({alternative:true,isNew:true,value:'new draft',committed:''}),false);
  assert.equal(owns({}),false);
});


test('stale export refusal releases the barrier while retaining newer local text', async () => {
  const {bridge,sent,conflicts} = await setup('stale-export','A');
  bridge.command('export'); const request = sent[0]; bridge.submit('B');
  assert.throws(() => bridge.exportSnapshot({...request,revision:0}));
  bridge.receive({...request,type:'reject',source:'A',reason:'Export snapshot changed'});
  assert.equal(bridge.commandPending,null); assert.equal(bridge.paused,true);
  assert.equal(conflicts[0].draft,'B'); assert.equal(conflicts[0].reason,'Export snapshot changed');
  bridge.resume(); bridge.submit('C'); assert.equal(sent.at(-1).type,'proposal'); bridge.dispose();
});

test('failed export with clean source releases once and an obsolete duplicate cannot cancel later edit', async () => {
  const {bridge,sent} = await setup('failed-export','A');
  bridge.command('export'); const request = sent[0];
  const refusal = {...request,type:'reject',source:'A',reason:'Filesystem refused write'};
  assert.equal(bridge.receive(refusal),true); assert.equal(bridge.commandPending,null);
  assert.equal(bridge.paused,false); bridge.submit('B'); const fresh = sent.at(-1);
  assert.equal(bridge.receive(refusal),false); assert.equal(bridge.pending.proposal.id,fresh.id);
  bridge.receive(ack(fresh,2)); assert.equal(bridge.current.source,'B'); bridge.dispose();
});
