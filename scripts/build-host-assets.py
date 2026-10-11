#!/usr/bin/env python3
"""Build local host assets from one stable source fingerprint; never publish."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ['runtime-assets', 'config', 'chrome', 'indicators', 'selection-menu', 'trim',
           'lab', 'blocks', 'alternatives-panel', 'overflow', 'persistence', 'rolegraph-storage',
           'rolegraph-client', 'editor', 'terraphim-editor']
STYLES = ['public/styles.css'] + ['public/css/' + name + '.css' for name in
    ['tokens', 'write-on', 'selection-menu', 'lab', 'trim', 'blocks', 'alternatives-panel', 'overflow', 'persistence']]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def sources():
    names = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '--',
        'src', 'public', 'crates', 'templates', 'Cargo.toml', 'Cargo.lock'], cwd=ROOT).decode().splitlines()
    return {name: digest(ROOT / name) for name in sorted(set(names)) if (ROOT / name).is_file()}

def run(args, env=None):
    subprocess.run([str(a) for a in args], cwd=ROOT, env=env, check=True)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--wasm-bindgen', required=True, type=Path)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists() and any(out.iterdir()):
        raise SystemExit('Refusing to replace a nonempty asset directory')
    if out == ROOT or out in (ROOT / 'dist', ROOT / 'package'):
        raise SystemExit('Use a separate build directory')
    before = sources()
    env = dict(os.environ)
    # Keep compiler caches alongside this isolated build, not global Cargo paths.
    env['CARGO_BUILD_BUILD_DIR'] = str(out.parent / 'cargo-build')
    env['CARGO_TARGET_DIR'] = str(out.parent / 'cargo-target')
    env['WASM_BINDGEN'] = str(args.wasm_bindgen.resolve())
    run(['cargo', 'build', '--release', '--target', 'wasm32-unknown-unknown', '--locked', '--offline'], env)
    out.mkdir(parents=True, exist_ok=True)
    run([args.wasm_bindgen, Path(env['CARGO_TARGET_DIR']) / 'wasm32-unknown-unknown/release/terraphim_editor.wasm',
         '--target', 'web', '--out-dir', out], env)
    run(['terser', *['public/js/' + s + '.js' for s in SCRIPTS], '-o', out / 'terraphim-editor.min.js'])
    run(['cleancss', *STYLES, '-o', out / 'terraphim-editor.min.css'])
    shutil.copy2(ROOT / 'public/js/host-session.js', out / 'host-session.js')
    shutil.copytree(ROOT / 'public/vendor', out / 'vendor')
    (out / 'js').mkdir()
    shutil.copy2(ROOT / 'public/js/rolegraph-worker.js', out / 'js/rolegraph-worker.js')
    run(['bash', 'scripts/build-rolegraph.sh', out / 'rolegraph'], env)
    role = ROOT / 'public/roles/default.json'
    if role.is_file():
        (out / 'roles').mkdir()
        shutil.copy2(role, out / 'roles/default.json')
    if sources() != before:
        raise SystemExit('Source changed during build; output is unqualified, rebuild separately')
    names = subprocess.check_output(['fd', '--type', 'f', '--hidden', '.', str(out)]).decode().splitlines()
    assets = {str(Path(name).relative_to(out)): digest(Path(name)) for name in sorted(names)}
    manifest = {'protocol': 1, 'sourceHead': subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT).decode().strip(),
                'sources': before, 'assets': assets, 'qualification': 'BUILD ONLY; runtime acceptance pending'}
    (out / 'host-assets.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'Built {len(assets)} local assets; stable {len(before)}-file fingerprint')

if __name__ == '__main__':
    main()
