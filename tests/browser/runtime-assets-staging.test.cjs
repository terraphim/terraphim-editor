// Git ignore-rule regression, not a build or browser acceptance test.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const {spawnSync} = require('node:child_process');
const fs = require('node:fs');

function ignored(paths) {
  const result = spawnSync('git', ['check-ignore', '--no-index', '--stdin', '-z'], {
    input: paths.join('\0') + '\0', encoding: 'utf8',
  });
  assert.ok(result.status === 0 || result.status === 1, result.stderr);
  return result.stdout.split('\0').filter(Boolean);
}

test('all pinned source and release vendor members can be staged normally', () => {
  const manifest = JSON.parse(fs.readFileSync('public/vendor/manifest.json', 'utf8'));
  const members = ['manifest.json', ...manifest.files.map(file => file.path)];
  const paths = ['public/vendor/', 'dist/vendor/', 'package/dist/vendor/']
    .flatMap(base => members.map(member => base + member));
  assert.deepEqual(ignored(paths), []);
});

test('worker and snippet release paths can be staged normally', () => {
  assert.deepEqual(ignored([
    'dist/js/rolegraph-worker.js',
    'dist/rolegraph/terraphim_rolegraph_browser.js',
    'dist/rolegraph/terraphim_rolegraph_browser_bg.wasm',
    'dist/snippets/terraphim-editor-fixture/inline0.js',
  ]), []);
});

test('Cargo vendor output, credentials, caches and stray dist files remain ignored', () => {
  const paths = ['vendor/cargo-fixture/lib.rs', '.cargo/credentials.toml',
    'target/runtime-fixture.json', 'dist/private-fixture.json'];
  assert.deepEqual(ignored(paths), paths);
});
