// Startup ordering units with explicit DOM/editor-initialization doubles.
// These are not evidence of browser component, font or WASM readiness.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');

function fixture(ready, {embed = false, assets = true} = {}) {
  const listeners = new Map();
  const errors = [];
  let initialized = 0;
  const app = {querySelector: () => ({})};
  const context = vm.createContext({
    AbortController, setTimeout,
    console: {error: (...args) => errors.push(args)},
    document: {
      getElementById: id => id === 'app' ? app : null,
      addEventListener: (name, fn) => listeners.set(name, fn),
    },
  });
  context.window = context;
  context.TE_EMBED = embed;
  context.EditorConfig = {shortcuts: [], commands: []};
  if (assets) context.TerraphimRuntimeAssets = {ready};
  vm.runInContext(fs.readFileSync('public/js/editor.js', 'utf8'), context);
  context.MarkdownEditor.prototype.initialize = () => { ++initialized; };
  return {start: () => listeners.get('DOMContentLoaded')(),
    count: () => initialized, errors};
}

test('full-page initialization waits for runtime readiness', async () => {
  let resolve;
  const f = fixture(new Promise(done => { resolve = done; }));
  const started = f.start();
  assert.equal(f.count(), 0);
  resolve();
  await started;
  assert.equal(f.count(), 1);
  assert.equal(f.errors.length, 0);
});

test('runtime rejection is reported without initializing', async () => {
  let reject;
  const f = fixture(new Promise((_, fail) => { reject = fail; }));
  const started = f.start();
  reject(new Error('synthetic asset failure'));
  await started;
  assert.equal(f.count(), 0);
  assert.equal(f.errors.length, 1);
});

test('missing runtime readiness fails closed', async () => {
  const f = fixture(undefined, {assets: false});
  await f.start();
  assert.equal(f.count(), 0);
  assert.equal(f.errors.length, 1);
});

test('embedded hosts are not initialized by the full-page listener', async () => {
  const f = fixture(undefined, {embed: true, assets: false});
  await f.start();
  assert.equal(f.count(), 0);
  assert.equal(f.errors.length, 0);
});
