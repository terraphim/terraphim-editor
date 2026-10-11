// Actual native-saved files reopened by the installed extension's real WASM.
// This proves derived KG registration for those outputs, not a native webview query.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const [assets, directory] = process.argv.slice(2);
  if (!assets || !directory) throw new Error('Usage: node verify-native-synonyms.cjs INSTALLED_EDITOR_ASSETS NATIVE_OUTPUT_DIRECTORY');
  const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'terraphim-native-synonym-proof-'));
  try {
    await fs.copyFile(path.join(assets, 'terraphim_editor.js'), path.join(temporary, 'engine.mjs'));
    const api = await import(pathToFileURL(path.join(temporary, 'engine.mjs')));
    await api.default({ module_or_path: await fs.readFile(path.join(assets, 'terraphim_editor_bg.wasm')) });
    const checks = [];
    const all = ['paperclip', 'eraser', 'thumbtack'];
    const reduced = ['paperclip', 'eraser'];
    for (const [name, ghostCount, terms] of [
      ['ghost-saved.md', 1, all], ['ghost-undo-saved.md', 0, all],
      ['alternative-removal-saved.md', 1, reduced],
      ['alternative-undo-saved.md', 1, all], ['alternative-redo-saved.md', 1, reduced],
    ]) {
      const source = await fs.readFile(path.join(directory, name), 'utf8');
      const engine = new api.TerraphimHostSession(pathToFileURL(path.join(directory, name)).href, name, 1, source);
      try {
        const projection = JSON.parse(engine.snapshot()).projection;
        const annotations = projection.annotations;
        assert.equal(annotations.ghosts.length, ghostCount);
        assert.equal(annotations.synonymGroups.length, 1);
        const group = annotations.synonymGroups[0];
        assert.equal(group.markdown, 'synonyms:: ' + terms.join(', ') + '\n');
        const span = annotations.spans[0];
        const ids = [];
        for (const term of terms) {
          assert.deepEqual(JSON.parse(engine.synonyms_for(span.id, term)), terms);
          ids.push(group.thesaurus.data[term].id);
        }
        assert.equal(new Set(ids).size, 1);
        if (!terms.includes('thumbtack')) assert.deepEqual(JSON.parse(engine.synonyms_for(span.id, 'thumbtack')), []);
        checks.push({ name, ghostCount, directive: group.markdown, registeredTerms: terms, conceptIds: ids,
          removedTermAbsent: !terms.includes('thumbtack') });
      } finally { engine.free(); }
    }
    console.log(JSON.stringify({ result: 'PASS', qualification: 'Actual native files reopened in the installed real WASM; native webview query not asserted', checks }, null, 2));
  } finally { await fs.rm(temporary, { recursive: true }); }
})().catch(error => { console.error(error); process.exitCode = 1; });
