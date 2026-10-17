'use strict';
const {firefox} = require(process.env.TERRAPHIM_PLAYWRIGHT_MODULE || 'playwright');
const fs = require('node:fs/promises');
const {spawnSync} = require('node:child_process');
const assert = require('node:assert/strict');
const output = process.env.TERRAPHIM_STORY_OUT;
const readClipboard = () => {
  const result = spawnSync('/usr/bin/pbpaste');
  assert.equal(result.status, 0, 'Read local clipboard');
  return result.stdout;
};
const writeClipboard = value => {
  const result = spawnSync('/usr/bin/pbcopy', {input:value});
  assert.equal(result.status, 0, 'Write local clipboard');
};
(async () => {
  assert.equal(process.platform, 'darwin', 'This fixture verifies actual macOS clipboard input');
  const prior = readClipboard(); // Never print or persist the user's clipboard.
  const pasted = 'p1\np2\np3';
  const copied = 'p1\np2';
  let browser;
  try {
    browser = await firefox.launch({headless:true});
    const page = await browser.newPage({viewport:{width:1140,height:900}});
    await page.goto(process.env.TERRAPHIM_STORY_URL);
    const source = page.getByRole('textbox', {name:'Markdown source',exact:true});
    await source.waitFor();
    await source.fill('Hello');
    const capabilities = await page.evaluate(value => {
      const data = new DataTransfer(); data.setData('text/plain', value);
      const event = new ClipboardEvent('paste', {clipboardData:data});
      const button = document.createElement('button');
      button.textContent = 'Copy public clipboard fixture';
      button.onclick = async () => {
        try {await navigator.clipboard.writeText(value);button.dataset.copied='true';}
        catch (error) {button.dataset.error=error.name;}
      };
      document.body.append(button);
      return {platform:navigator.platform,syntheticDataPreserved:event.clipboardData?.getData('text/plain')===value};
    }, pasted);
    await fs.writeFile(output+'/browser-capabilities.json',JSON.stringify(capabilities,null,2)+'\n');
    await page.getByRole('button', {name:'Copy public clipboard fixture',exact:true}).click();
    await page.waitForFunction(() => document.querySelector('button[data-copied="true"]'), undefined, {timeout:5000});
    await source.press('Meta+ArrowUp');
    await source.press('Meta+v');
    await assert.doesNotReject(async () => {
      await page.waitForFunction(() => document.querySelector('[role="textbox"][aria-label="Markdown source"]').innerText === 'p1\np2\np3Hello', undefined, {timeout:5000});
    });
    await source.press('Home');
    await source.press('Meta+ArrowUp');
    await source.press('Shift+ArrowDown');
    await source.press('Shift+ArrowRight');
    await source.press('Shift+ArrowRight');
    await source.press('Meta+c');
    const target = page.locator('#clipboard-roundtrip');
    await page.evaluate(() => {const field=document.createElement('textarea');field.id='clipboard-roundtrip';document.body.append(field);});
    await target.press('Meta+v');
    const actual = await target.inputValue();
    await fs.writeFile(output+'/actual-pasted.txt', await source.innerText());
    assert(actual === copied, 'Clipboard did not match the selected public test text');
    await fs.writeFile(output+'/actual-copied.txt', actual);
    await fs.writeFile(output+'/clipboard-receipt.json',JSON.stringify({status:'PASS',browser:browser.version(),scope:'Actual trusted keyboard paste/copy using the browser clipboard; public text written by a trusted fixture button, editor unchanged',pastedText:await source.innerText(),copiedText:actual},null,2)+'\n');
  } finally {
    await browser?.close();
    const current = readClipboard().toString('utf8');
    if (current === pasted || current === copied) writeClipboard(prior);
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
