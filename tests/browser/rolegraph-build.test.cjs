// Actual build-script shell regression; Cargo is an explicit external double.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {spawnSync} = require('node:child_process');

test('system Bash with no override reaches locked metadata, not an empty-array error',()=>{
  const root=path.resolve(__dirname,'../..');
  const temp=fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(),'rolegraph-shell-'));
  try {
    const record=path.join(temp,'args');
    fs.writeFileSync(path.join(temp,'cargo'),'#!/bin/sh\nprintf "%s\\n" "$@" > "$TE_CARGO_RECORD"\nexit 71\n',{mode:0o755});
    const env={...process.env,PATH:temp+':/usr/bin:/bin',TE_CARGO_RECORD:record};
    delete env.TE_ROLEGRAPH_SOURCE;
    const result=spawnSync('/bin/bash',['scripts/build-rolegraph.sh'],{cwd:root,env,encoding:'utf8'});
    assert.equal(result.status,71,result.stderr);
    const args=fs.readFileSync(record,'utf8').trim().split('\n');
    assert.equal(args[0],'metadata');
    assert.ok(args.includes('--locked'));
    assert.ok(!result.stderr.includes('unbound variable'));
  } finally {
    // Only this test's newly created private directory.
    for(const file of fs.readdirSync(temp)) fs.unlinkSync(path.join(temp,file));
    fs.rmdirSync(temp);
  }
});

test('production rejects a source override before invoking Cargo',()=>{
  const result=spawnSync('/bin/bash',['scripts/build-rolegraph.sh'],{
    cwd:path.resolve(__dirname,'../..'),
    env:{...process.env,TE_ROLEGRAPH_SOURCE:'/not/a/production/source'},encoding:'utf8'
  });
  assert.notEqual(result.status,0);
  assert.match(result.stderr,/source overrides are forbidden/);
});

test('workspace bridge uses exact custom-registry pins and no nested lock',()=>{
  const root=path.resolve(__dirname,'../..');
  assert.match(fs.readFileSync(path.join(root,'Cargo.toml'),'utf8'),/members = .*"crates\/terraphim_rolegraph_browser"/);
  const manifest=fs.readFileSync(path.join(root,'crates/terraphim_rolegraph_browser/Cargo.toml'),'utf8');
  assert.ok(!manifest.includes('[workspace]'));
  for(const [name,version] of [['terraphim_rolegraph','2.0.1'],['terraphim_types','1.22.3'],['terraphim_automata','2.1.0']]) {
    const line=manifest.split('\n').find(line=>line.startsWith(name+' ='));
    assert.ok(line.includes(`version = "=${version}"`));
    assert.ok(line.includes('registry = "terraphim"'));
    assert.ok(line.includes('default-features = false'));
  }
  assert.ok(!fs.existsSync(path.join(root,'crates/terraphim_rolegraph_browser/Cargo.lock')));
});

// Exercise the actual admission, integrity validators and publisher against a
// disposable Git repository. Asset files are fixtures; no editor execution or
// released-build qualification is claimed. No Git/Python/publisher tool doubles.
for (const scenario of ['publish', 'dry-run', 'missing-graph', 'runtime-corruption',
                        'dirty-source', 'changed-head', 'source-override']) {
  test(`real release pipeline ${scenario} preserves its source and destination contract`, () => {
    const root = path.resolve(__dirname, '../..');
    const temp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'release-pipeline-')));
    const env = {...process.env, PYTHONDONTWRITEBYTECODE:'1'};
    delete env.TE_ROLEGRAPH_SOURCE;
    const run = (command, args) => spawnSync(command, args, {cwd:temp, env, encoding:'utf8'});
    const git = (...args) => {
      const result = run('git', args);
      assert.equal(result.status, 0, result.stderr);
      return result.stdout.trim();
    };
    const write = (name, bytes) => {
      const file = path.join(temp, name);
      fs.mkdirSync(path.dirname(file), {recursive:true});
      fs.writeFileSync(file, bytes);
    };
    const destinations = ['dist/index.html', 'package/dist/index.html'];
    try {
      for (const name of ['scripts/release-dist.sh', 'scripts/check-release-source.py',
          'scripts/build-dist.sh', 'scripts/publish-editor-dist.py',
          'scripts/ci/rolegraph-production-check.py', 'scripts/ci/runtime-assets-check.py',
          'public/roles/default.json']) {
        write(name, fs.readFileSync(path.join(root, name)));
      }
      fs.cpSync(path.join(root, 'public/vendor'), path.join(temp, 'public/vendor'), {recursive:true});
      write('.gitignore', 'target/\n');
      for (const name of destinations) write(name, 'before publication\n');
      git('init', '-q');
      git('add', '.');
      git('-c', 'user.name=Release integration test', '-c', 'user.email=test@example.invalid',
          '-c', 'commit.gpgsign=false', 'commit', '-qm', 'Admit release fixture');
      const head = git('rev-parse', 'HEAD');
      const stage = path.join(temp, 'target/release-staging-integration');
      const manifest = path.join(temp, 'target/release-manifests/result.json');
      const files = ['index.html', 'example.html', 'terraphim-editor.min.js',
        'terraphim-editor.min.css', 'terraphim_editor.js', 'terraphim_editor_bg.wasm',
        'js/rolegraph-worker.js', 'rolegraph/rolegraph-worker.js',
        'rolegraph/terraphim_rolegraph_browser.js', 'rolegraph/terraphim_rolegraph_browser_bg.wasm',
        'roles/default.json', 'terraphim-editor-abcdef1234567890.js',
        'terraphim-editor-abcdef1234567890_bg.wasm'];
      for (const name of files) {
        const bytes = name.endsWith('.wasm') ? Buffer.from([0,97,115,109,1,0,0,0])
          : name === 'roles/default.json' ? fs.readFileSync(path.join(temp, 'public', name))
          : 'release boundary fixture\n';
        write(`target/release-staging-integration/${name}`, bytes);
      }
      fs.cpSync(path.join(temp, 'public/vendor'), path.join(stage, 'vendor'), {recursive:true});
      if (scenario === 'missing-graph') fs.unlinkSync(path.join(stage, 'rolegraph/terraphim_rolegraph_browser_bg.wasm'));
      if (scenario === 'runtime-corruption') {
        const vendor = JSON.parse(fs.readFileSync(path.join(stage, 'vendor/manifest.json'), 'utf8'));
        fs.appendFileSync(path.join(stage, 'vendor', vendor.files[0].path), 'tampered');
      }
      if (scenario === 'dirty-source') fs.appendFileSync(path.join(temp, 'public/roles/default.json'), '\n');
      if (scenario === 'source-override') env.TE_ROLEGRAPH_SOURCE = '/forbidden/release/source';
      const shellCase = ['dirty-source', 'source-override'].includes(scenario);
      const args = ['scripts/publish-editor-dist.py', '--stage', stage, '--manifest', manifest,
        '--source-head', scenario === 'changed-head' ? '0'.repeat(40) : head];
      if (scenario === 'dry-run') args.push('--dry-run');
      const result = shellCase ? run('/bin/bash', ['scripts/release-dist.sh', '--dry-run']) : run('python3', args);
      if (['publish', 'dry-run'].includes(scenario)) {
        assert.equal(result.status, 0, result.stderr);
        const report = JSON.parse(fs.readFileSync(manifest, 'utf8'));
        assert.equal(report.sourceHead, head);
        assert.equal(report.status, scenario === 'publish' ? 'published' : 'preflight-only');
        assert.deepEqual(report.sourceStatus, []);
        for (const name of destinations) assert.equal(fs.readFileSync(path.join(temp, name), 'utf8'),
          scenario === 'publish' ? 'release boundary fixture\n' : 'before publication\n');
        if (scenario === 'publish') {
          for (const destination of ['dist', 'package/dist']) {
            for (const item of report.files) {
              assert.deepEqual(fs.readFileSync(path.join(temp, destination, item.path)), fs.readFileSync(path.join(stage, item.path)));
            }
          }
        }
      } else {
        assert.notEqual(result.status, 0, `${scenario}: release unexpectedly succeeded`);
        const expected = {'missing-graph':/missing production files/, 'runtime-corruption':/hash mismatch/,
          'dirty-source':/modified or untracked files/, 'changed-head':/source HEAD changed/,
          'source-override':/TE_ROLEGRAPH_SOURCE is forbidden/};
        assert.match(result.stderr, expected[scenario]);
        assert.equal(fs.existsSync(manifest), false);
        for (const name of destinations) assert.equal(fs.readFileSync(path.join(temp, name), 'utf8'), 'before publication\n');
        if (shellCase) assert.deepEqual(fs.readdirSync(path.join(temp, 'target')), ['release-staging-integration']);
      }
      assert.equal(git('rev-parse', 'HEAD'), head);
      assert.equal(git('diff', '--cached'), '');
    } finally {
      fs.rmSync(temp, {recursive:true}); // Only this test's new disposable repository.
    }
  });
}

// These external Cargo/bindgen doubles test orchestration, not the graph engine.
function buildDouble({metadataExit=0,version='0.2.129',wrongSource=false}={}) {
  const root=path.resolve(__dirname,'../..');
  const temp=fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(),'rolegraph-contract-'));
  const packages=[['terraphim_rolegraph','2.0.1'],['terraphim_types','1.22.3'],['terraphim_automata','2.1.0'],['terraphim_rolegraph_core','0.1.1'],['terraphim-markdown-parser','1.20.5']].map(([name,version])=>({name,version,source:wrongSource?null:'sparse+https://git.terraphim.cloud/api/packages/terraphim/cargo/'}));
  packages.push({name:'wasm-bindgen',version:'0.2.129'},{name:'terraphim_rolegraph_browser',id:'bridge'});
  fs.writeFileSync(path.join(temp,'metadata.json'),JSON.stringify({packages,workspace_members:['bridge'],target_directory:temp}));
  fs.writeFileSync(path.join(temp,'cargo'),`#!/bin/sh\nprintf '%s\\n' "$*" >> "$TE_DOUBLE/args"\nif [ "$1" = metadata ]; then cat "$TE_DOUBLE/metadata.json"; exit ${metadataExit}; fi\n`,{mode:0o755});
  fs.writeFileSync(path.join(temp,'wasm-bindgen'),`#!/bin/sh\nif [ "$1" = --version ]; then echo 'wasm-bindgen ${version}'; exit; fi\nwhile [ "$1" != --out-dir ]; do shift; done\nshift\nprintf 'glue' > "$1/terraphim_rolegraph_browser.js"\nprintf 'wasm' > "$1/terraphim_rolegraph_browser_bg.wasm"\n`,{mode:0o755});
  const env={...process.env,PATH:temp+':/usr/bin:/bin',TE_DOUBLE:temp,WASM_BINDGEN:path.join(temp,'wasm-bindgen')};
  delete env.TE_ROLEGRAPH_SOURCE;
  const result=spawnSync('/bin/bash',['scripts/build-rolegraph.sh',path.join(temp,'out')],{cwd:root,env,encoding:'utf8'});
  const args=fs.readFileSync(path.join(temp,'args'),'utf8');
  fs.rmSync(temp,{recursive:true}); // Only this test's newly allocated directory.
  return {result,args};
}

test('stale locked resolver failure stops before build',()=>{
  const {result,args}=buildDouble({metadataExit:71});
  assert.equal(result.status,71); assert.ok(!args.includes('build'));
});
test('wrong bindgen stops before build',()=>{
  const {result,args}=buildDouble({version:'0.2.99'});
  assert.notEqual(result.status,0); assert.match(result.stderr,/does not match locked crate/);
  assert.ok(!args.includes('build'));
});
test('nonregistry graph metadata stops before build',()=>{
  const {result,args}=buildDouble({wrongSource:true});
  assert.notEqual(result.status,0); assert.match(result.stderr,/expected one published/);
  assert.ok(!args.includes('build'));
});
test('actual shell selects locked workspace package and metadata target directory',()=>{
  const {result,args}=buildDouble();
  assert.equal(result.status,0,result.stderr);
  assert.match(args,/build --locked --release --target wasm32-unknown-unknown -p terraphim_rolegraph_browser/);
});
test('missing graph assets and referenced snippets fail completeness',()=>{
  const root=path.resolve(__dirname,'../..');
  const temp=fs.mkdtempSync(path.join(process.env.TMPDIR || os.tmpdir(),'rolegraph-assets-'));
  const check=()=>spawnSync('python3',['scripts/ci/rolegraph-production-check.py','graph-assets',temp],{cwd:root,encoding:'utf8'});
  try {
    assert.notEqual(check().status,0);
    fs.writeFileSync(path.join(temp,'terraphim_rolegraph_browser.js'),'import "./snippets/missing/test.js";');
    fs.writeFileSync(path.join(temp,'terraphim_rolegraph_browser_bg.wasm'),'wasm');
    assert.notEqual(check().status,0);
    fs.mkdirSync(path.join(temp,'snippets/missing'),{recursive:true});
    fs.writeFileSync(path.join(temp,'snippets/missing/test.js'),'snippet');
    assert.equal(check().status,0);
  } finally { fs.rmSync(temp,{recursive:true}); }
});
