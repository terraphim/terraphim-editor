// Orchestration unit tests with DOM doubles; NOT browser/offline acceptance.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
function harness() {
  const links = [];
  const window = {};
  const context = { window, URL, Set, Map, Promise, Error, TypeError,
    document: { baseURI: 'http://localhost/nested/editor/index.html',
      querySelectorAll: () => links,
      createElement: () => ({ addEventListener(name, fn) { this[name] = fn; },
        remove() { links.splice(links.indexOf(this), 1); } }),
      head: { appendChild(link) { links.push(link); } } },
    customElements: { get() { return undefined; } }, setTimeout, clearTimeout,
  };
  vm.runInNewContext(fs.readFileSync('public/js/runtime-assets.js', 'utf8'), context);
  return { assets: window.TerraphimRuntimeAssets, links, context };
}
test('asset base validation preserves nested script base and rejects unsafe origins', () => {
  const { assets } = harness();
  assert.equal(assets.resolveBase({}, 'http://localhost/nested/editor/bundle.js'), 'http://localhost/nested/editor/');
  for (const assetBase of ['file:///tmp/', 'data:text/plain,a', 'https://user:pass@localhost/', 'https://cdn.example/']) {
    assert.throws(() => assets.resolveBase({ assetBase }, 'http://localhost/a.js'));
  }
  assert.equal(assets.resolveBase({assetBase: 'https://assets.example/base/', allowExternalAssets: true}, 'http://localhost/a.js'), 'https://assets.example/base/');
});
test('styles wait for actual load, deduplicate, and failed stylesheet can retry', async () => {
  const { assets, links } = harness();
  const url = 'http://localhost/nested/editor/vendor/a.css';
  const first = assets.loadStyle(url);
  assert.equal(assets.loadStyle(url), first);
  let ready = false;
  first.then(() => { ready = true; });
  await Promise.resolve();
  assert.equal(ready, false);
  links[0].load();
  await first;
  const bad = assets.loadStyle(url + '?bad');
  links[1].error();
  await assert.rejects(bad, /stylesheet/);
  const retry = assets.loadStyle(url + '?bad');
  assert.equal(links.length, 2);
  links[1].load();
  await retry;
});
test('finite icon resolver never falls back remotely or accepts path injection', () => {
  const { assets } = harness();
  const resolver = assets.iconResolver('http://localhost/nested/vendor/');
  assert.equal(resolver('question-circle'), 'http://localhost/nested/vendor/assets/icons/question-circle.svg');
  for (const name of ['../../a', 'https://example/a', 'unknown', '<b>', 'constructor']) {
    assert.equal(resolver(name), '');
  }
});
test('host-owned existing custom elements require explicit opt-out', async () => {
  const { assets, context } = harness();
  context.customElements.get = () => function HostElement() {};
  await assert.rejects(assets.load({assetBase: 'http://localhost/nested/editor/'}), /loadShoelace: false/);
});
test('concurrent boots deduplicate and incompatible page-global bases fail explicitly', async () => {
  const { assets, links } = harness();
  const options = {assetBase: 'http://localhost/nested/editor/'};
  const first = assets.load(options);
  assert.equal(assets.load(options), first);
  assert.equal(links.length, 5);
  await assert.rejects(assets.load({assetBase: 'http://localhost/other/'}), /Incompatible/);
  links.slice().forEach(link => link.error());
  await assert.rejects(first, /stylesheet/);
  const retry = assets.load(options);
  assert.notEqual(retry, first);
  assert.equal(links.length, 5);
  links.slice().forEach(link => link.error());
  await assert.rejects(retry, /stylesheet/);
});
