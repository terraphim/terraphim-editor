'use strict';
// Real W3C input in the explicitly selected installed browser. Public payload
// is written only by an actual trusted fixture click. No privileged clipboard
// injection, synthetic key/clipboard events, prior clipboard capture or restore.
const fs = require('node:fs/promises');
const path = require('node:path');
const net = require('node:net');
const {spawn, spawnSync} = require('node:child_process');
const {createHash} = require('node:crypto');
const assert = require('node:assert/strict');
const output = process.env.TERRAPHIM_STORY_OUT;
const family = process.env.TERRAPHIM_BROWSER_FAMILY;
assert(['chrome','firefox'].includes(family),'Explicit actual browser family required');
const executable = process.env.TERRAPHIM_BROWSER_EXECUTABLE;
const driverExecutable = process.env.TERRAPHIM_DRIVER_EXECUTABLE;
assert(executable && driverExecutable,'Explicit actual browser and driver paths required');
const elementKey = 'element-6066-11e4-a52e-4f735466cecf';
const KEY = {meta:'\uE03D',null:'\uE000',up:'\uE013',down:'\uE015',right:'\uE014',shift:'\uE008',backspace:'\uE003',home:'\uE011'};
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const pause = ms => new Promise(resolve => setTimeout(resolve,ms));
const assertPublicClipboard = expected => {
  const script = 'ObjC.import("AppKit"); function run(argv) { return JSON.parse(argv[0]).includes(ObjC.unwrap($.NSPasteboard.generalPasteboard.stringForType($.NSPasteboardTypeString))); }';
  const result = spawnSync('/usr/bin/osascript',['-l','JavaScript','-e',script,JSON.stringify(Array.isArray(expected)?expected:[expected])],{encoding:'utf8'});
  assert.equal(result.status,0,'OS public-fixture clipboard equality check failed');
  assert.equal(result.stdout.trim(),'true','Clipboard differs from public fixture; refusing to paste');
};
const freePort = () => new Promise((resolve,reject) => {
  const server=net.createServer();server.once('error',reject);
  server.listen(0,'127.0.0.1',()=>{const port=server.address().port;server.close(error=>error?reject(error):resolve(port));});
});
(async () => {
  assert.equal(process.platform,'darwin','This fixture verifies the actual macOS clipboard');
  assert(output && process.env.TERRAPHIM_STORY_URL,'Explicit output and frozen asset server URL required');
  await fs.mkdir(output,{recursive:true});
  const browserPath=await fs.realpath(executable),driverPath=await fs.realpath(driverExecutable);
  const port=await freePort(),origin=`http://127.0.0.1:${port}`;
  const driver=spawn(driverPath,family==='firefox'?['--host','127.0.0.1','--port',String(port)]:['--port='+port,'--allowed-ips=127.0.0.1'],{detached:true,stdio:['ignore','pipe','pipe']});
  await fs.writeFile(path.join(output,'owned-process.json'),JSON.stringify({processGroup:driver.pid,driverPath,browserPath},null,2)+'\n');
  let driverLog='',session,startupError;
  driver.on('error',error=>{startupError=error;});
  driver.stdout.on('data',bytes=>{driverLog+=bytes.toString();});driver.stderr.on('data',bytes=>{driverLog+=bytes.toString();});
  const command=async(method,route,body)=>{
    const response=await fetch(origin+route,{method,headers:{'content-type':'application/json'},...(body===undefined?{}:{body:JSON.stringify(body)}),signal:AbortSignal.timeout(30000)});
    const packet=await response.json();
    if(!response.ok || packet.value?.error)throw new Error(`WebDriver ${route}: ${packet.value?.error||response.status}: ${packet.value?.message||''}`);
    return packet.value;
  };
  const waitFor=async(check,label)=>{const end=Date.now()+5000;do{const result=await check();if(result)return result;await pause(25);}while(Date.now()<end);throw new Error(`Timed out after 5s: ${label}`);};
  const route=tail=>`/session/${session.sessionId}${tail}`;
  const script=(source,args=[])=>command('POST',route('/execute/sync'),{script:source,args});
  const locate=selector=>command('POST',route('/element'),{using:'css selector',value:selector}).then(value=>value[elementKey]);
  const keys=(element,text)=>command('POST',route(`/element/${element}/value`),{text});
  const property=(element,name)=>command('GET',route(`/element/${element}/property/${name}`));
  const publicPageSnapshot=()=>script(`const s=window.terraphimEditor?.surface,root=document.querySelector('[role="textbox"][aria-label="Markdown source"]'),active=document.activeElement,selection=getSelection(),offset=(node,n)=>{if(!root||!node||!root.contains(node))return null;const range=document.createRange();range.selectNodeContents(root);range.setEnd(node,n);return range.toString().length;};return {sourceText:root?.innerText??null,modelText:s?.getText()??null,sourceFocused:active===root,documentFocused:document.hasFocus(),visibilityState:document.visibilityState,nativeSelection:selection?{anchorNode:selection.anchorNode?.nodeName,anchorOffset:selection.anchorOffset,focusNode:selection.focusNode?.nodeName,focusOffset:selection.focusOffset,isCollapsed:selection.isCollapsed,anchorInSource:root?.contains(selection.anchorNode),focusInSource:root?.contains(selection.focusNode)}:null,activeElement:active?{tag:active.tagName,id:active.id,label:active.getAttribute('aria-label')}:null,selectionDomUtf16:selection?{anchor:offset(selection.anchorNode,selection.anchorOffset),focus:offset(selection.focusNode,selection.focusOffset)}:null,selectedText:selection?.toString()??null,pasteEvents:window.__publicPasteEvents??[],richNodes:root?.querySelectorAll('b,img').length??null};`);
  const pasted='p1\np2\np3',rawPasted='p1\r\np2\rp3',copied='p1\np2';
  let failure;
  try {
    await waitFor(async()=>{if(startupError)throw startupError;try{return await command('GET','/status');}catch{return false;}},'owned WebDriver ready');
    session=await command('POST','/session',{capabilities:{alwaysMatch:{browserName:family,...(family==='firefox'?{'moz:firefoxOptions':{binary:browserPath,args:[]}}:{'goog:chromeOptions':{binary:browserPath,args:['--no-first-run','--no-default-browser-check']}})}}});
    assert.equal(session.capabilities.browserName,family,'Actual driver browser differs from requested family');
    await fs.writeFile(path.join(output,'browser-identity.json'),JSON.stringify({browserFamily:family,platform:process.platform,headless:false,browser:session.capabilities.browserVersion,buildID:session.capabilities['moz:buildID'],platformName:session.capabilities.platformName,browserPath,browserSha256:hash(await fs.readFile(browserPath)),driverPath,driverSha256:hash(await fs.readFile(driverPath)),driverVersion:spawnSync(driverPath,['--version'],{encoding:'utf8'}).stdout.trim(),nodeVersion:process.version},null,2)+'\n');
    await command('POST',route('/window/rect'),{width:1140,height:900});
    await command('POST',route('/timeouts'),{implicit:0,pageLoad:30000,script:5000});
    await command('POST',route('/url'),{url:process.env.TERRAPHIM_STORY_URL});
    await waitFor(()=>script('return Boolean(window.embeddedEditor && window.terraphimEditor)'), 'actual editor startup complete');
    const source=await waitFor(async()=>{try{return await locate('[role="textbox"][aria-label="Markdown source"]');}catch{return false;}},'actual Markdown source');
    await command('POST',route(`/element/${source}/click`),{});
    await keys(source,KEY.meta+'a'+KEY.null);
    await keys(source,KEY.backspace);
    await keys(source,'Hello');
    await waitFor(async()=>(await property(source,'innerText'))==='Hello','actual keyboard setup accepted by the editor');
    await script(`window.__publicPasteEvents=[];document.addEventListener('paste',event=>window.__publicPasteEvents.push({isTrusted:event.isTrusted,types:Array.from(event.clipboardData?.types||[]),target:event.target.id||event.target.getAttribute('aria-label')}),{capture:true,passive:true});const raw=arguments[0];const button=document.createElement('button');button.id='public-clipboard-fixture';button.textContent='Copy public clipboard fixture';button.onclick=async()=>{try{if(typeof ClipboardItem!=='function')throw new Error('ClipboardItem unsupported');await navigator.clipboard.write([new ClipboardItem({'text/plain':new Blob([raw],{type:'text/plain'}),'text/html':new Blob(['<b>rich</b><img src=x>'],{type:'text/html'})})]);button.dataset.copied='true';}catch(error){button.dataset.error=error.name+': '+error.message;}};document.body.append(button);`,[rawPasted]);
    const button=await locate('#public-clipboard-fixture');
    await command('POST',route(`/element/${button}/click`),{});
    await waitFor(async()=>{const state=await script("const b=document.querySelector('#public-clipboard-fixture');return {copied:b.dataset.copied,error:b.dataset.error}");if(state.error)throw new Error('Trusted ClipboardItem setup failed: '+state.error);return state.copied==='true';},'trusted public mixed-format copy activation');
    await keys(source,KEY.meta+KEY.up+KEY.null);
    assertPublicClipboard([rawPasted,pasted,'p1\rp2\rp3','p1\r\np2\r\np3']);
    await fs.writeFile(path.join(output,'before-native-paste-public-page.json'),JSON.stringify(await publicPageSnapshot(),null,2)+'\n');
    await keys(source,KEY.meta+'v'+KEY.null);
    await waitFor(async()=>(await property(source,'innerText'))===pasted+'Hello','actual public paste output');
    const pasteProof=await script(`const root=arguments[0],selection=window.getSelection();if(!selection||!selection.isCollapsed||!root.contains(selection.anchorNode))throw new Error('Caret outside source after paste');const range=document.createRange();range.selectNodeContents(root);range.setEnd(selection.anchorNode,selection.anchorOffset);return {caret:range.toString().length,richNodes:root.querySelectorAll('b,img').length,events:window.__publicPasteEvents.slice()};`,[{[elementKey]:source}]);
    assert.equal(pasteProof.caret,pasted.length,'Native caret differs after normalized plain-text paste');
    assert.equal(pasteProof.richNodes,0,'Rich HTML entered the Markdown source');
    assert(pasteProof.events.some(event=>event.isTrusted&&event.types.includes('text/plain')&&event.types.includes('text/html')),'Actual trusted paste did not carry both plain and HTML formats');
    await fs.writeFile(path.join(output,'paste-proof.json'),JSON.stringify({status:'PASS_PASTE_ONLY',...pasteProof,pastedText:await property(source,'innerText')},null,2)+'\n');
    await keys(source,KEY.meta+KEY.up+KEY.null);
    await keys(source,KEY.shift+KEY.down+KEY.null);
    await keys(source,KEY.shift+KEY.right+KEY.null);
    await keys(source,KEY.shift+KEY.right+KEY.null);
    assert.equal(await script('return window.getSelection().toString()'),copied,'Actual selected source differs from public fixture');
    await keys(source,KEY.meta+'c'+KEY.null);
    await script("const field=document.createElement('textarea');field.id='clipboard-roundtrip';document.body.append(field);");
    const target=await locate('#clipboard-roundtrip');
    assertPublicClipboard(copied);
    await keys(target,KEY.meta+'v'+KEY.null);
    const actual=await property(target,'value');
    assert(actual===copied,'Clipboard output differs from selected public fixture');
    const actualPasted=await property(source,'innerText');assert(actualPasted===pasted+'Hello','Public paste source changed unexpectedly');
    await fs.writeFile(path.join(output,'actual-pasted.txt'),actualPasted);
    await fs.writeFile(path.join(output,'actual-copied.txt'),actual);
    await fs.writeFile(path.join(output,'clipboard-receipt.json'),JSON.stringify({status:'PASS',scope:'Actual installed browser via W3C native clicks/keys and public clipboard fixture; not synthetic ClipboardEvent qualification',platform:process.platform,headless:false,inputTransport:'W3C native clicks/keys',browserFamily:family,browser:session.capabilities.browserVersion,buildID:session.capabilities['moz:buildID'],browserPath,browserSha256:hash(await fs.readFile(browserPath)),driverPath,driverSha256:hash(await fs.readFile(driverPath)),driverVersion:spawnSync(driverPath,['--version'],{encoding:'utf8'}).stdout.trim(),viewport:await script('return [innerWidth,innerHeight]'),priorClipboardRead:false,priorClipboardRestored:false,preflight:'Only OS boolean equality to public text before each native paste',pasteProof,pastedText:actualPasted,copiedText:actual},null,2)+'\n');
  } catch(error) {
    failure=error;
    await fs.writeFile(path.join(output,'failure.json'),JSON.stringify({status:'FAIL',error:String(error),priorClipboardRead:false,priorClipboardRestored:false},null,2)+'\n');
    // Read only this fresh public-fixture page after failure; no clipboard
    // contents are read, no state/selection/handler is changed, no retry occurs.
    if(session){
      try{
        const observed=await publicPageSnapshot();
        await fs.writeFile(path.join(output,'failure-public-page.json'),JSON.stringify(observed,null,2)+'\n');
      }catch(diagnostic){await fs.writeFile(path.join(output,'failure-readback-error.txt'),String(diagnostic)+'\n');}
      try{const png=await command('GET',route('/screenshot'));await fs.writeFile(path.join(output,'failure-public-page.png'),Buffer.from(png,'base64'));}
      catch(diagnostic){await fs.writeFile(path.join(output,'failure-screenshot-error.txt'),String(diagnostic)+'\n');}
    }

  }
  finally {
    if(session){try{await command('DELETE',route(''));}catch(error){driverLog+='\nSession cleanup: '+error.message;}}
    if(Number.isInteger(driver.pid)){try{process.kill(-driver.pid,'SIGTERM');}catch(error){if(error.code!=='ESRCH')throw error;}}
    await fs.writeFile(path.join(output,'webdriver.log'),driverLog);
  }
  if(failure)throw failure;
})().catch(error=>{console.error(error);process.exitCode=1;});
