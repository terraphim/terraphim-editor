// Run the existing real user-flow fixture against immutable packaged assets.
'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const http = require('node:http');
const {spawn} = require('node:child_process');
const {createHash} = require('node:crypto');
const assets = path.resolve(process.env.TERRAPHIM_AUTHORITY_ASSETS || '');
const output = path.resolve(process.env.TERRAPHIM_STORY_OUT || '');
if (!process.env.TERRAPHIM_AUTHORITY_ASSETS || !process.env.TERRAPHIM_STORY_OUT) throw Error('Explicit assets and output paths required');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const served = {};
const html = '<!doctype html><meta charset="utf-8"><div id="editor"></div><script src="/terraphim-editor.min.js"></script><script>window.storyStartup=TeraphimEditor.create(document.querySelector("#editor"),{value:"",dirtyTitle:false,assetBase:location.origin+"/"}).then(editor=>{window.embeddedEditor=editor;window.terraphimEditor=editor.markdownEditor;});</script>';
const types = {'.js':'text/javascript','.css':'text/css','.json':'application/json','.wasm':'application/wasm','.svg':'image/svg+xml','.woff2':'font/woff2'};
const server = http.createServer(async (request,response) => {
  try {
    const name = decodeURIComponent(new URL(request.url,'http://localhost').pathname);
    if (name === '/') { response.writeHead(200,{'content-type':'text/html; charset=utf-8'}); response.end(html); return; }
    const file = path.resolve(assets,'.'+name);
    if (!file.startsWith(assets+path.sep)) throw Error('Outside frozen assets');
    const bytes = await fs.readFile(file);
    served[path.relative(assets,file)] = hash(bytes);
    response.writeHead(200,{'content-type':types[path.extname(file)]||'application/octet-stream'});
    response.end(bytes);
  } catch {response.writeHead(404);response.end();}
});
(async () => {
  await fs.mkdir(output,{recursive:true});
  await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
  let exitCode;
  try {
    const origin = `http://127.0.0.1:${server.address().port}/`;
    const script = process.env.TERRAPHIM_STORY_SCRIPT || 'synonym-sharing.cjs';
    if (!/^[a-z0-9-]+\.cjs$/.test(script)) throw Error('A browser fixture filename is required');
    const child = spawn(process.execPath,[path.join(__dirname,script)],{stdio:'inherit',env:{...process.env,TERRAPHIM_STORY_URL:origin}});
    exitCode = await new Promise((resolve,reject)=>{child.once('error',reject);child.once('exit',(code,signal)=>resolve(signal?1:code));});
    const manifestPath = process.env.TERRAPHIM_ASSET_MANIFEST || path.join(assets,'host-assets.json');
    const manifest = JSON.parse(await fs.readFile(manifestPath,'utf8'));
    const mismatches = Object.entries(served).filter(([name,digest])=>manifest.assets[name]!==digest);
    await fs.writeFile(path.join(output,'runtime-assets.json'),JSON.stringify({status:exitCode===0&&mismatches.length===0?'PASS':'FAIL',exitCode,assets,manifestSha256:hash(await fs.readFile(manifestPath)),served,mismatches},null,2)+'\n');
    if(mismatches.length)exitCode=1;
  } finally {await new Promise(resolve=>server.close(resolve));}
  process.exitCode=exitCode;
})().catch(error=>{console.error(error);server.close();process.exitCode=1;});
