"""Static generated-delivery contract; not browser or WASM execution evidence.

Run units normally; use --check DISTRIBUTION after a real isolated Trunk build.
"""
import argparse
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest
from unittest import mock
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parents[2]
CSS_URL = re.compile(r'''url\(\s*["']?([^\)"']+)["']?\s*\)''')


def ast_references(sources):
    if not sources:
        return []
    try:
        result = subprocess.run(['node', str(ROOT / 'tests/browser/nested-build-imports.cjs')],
                                input=json.dumps({'sources': sources}), text=True,
                                capture_output=True, timeout=20, check=True)
        parsed = json.loads(result.stdout)
        if parsed.get('parser') != 'acorn' or parsed.get('version') != '8.14.0' or len(parsed['references']) != len(sources):
            raise ValueError('invalid AST helper result')
        return parsed['references']
    except subprocess.CalledProcessError as error:
        mismatch = re.search(r'^AST extraction FAIL: parser-version-mismatch required=8\.14\.0 observed=([a-zA-Z0-9.-]{1,64});',
                             error.stderr or '', re.M)
        if mismatch:
            raise ValueError(f'AST parser-version-mismatch required=8.14.0 observed={mismatch.group(1)}; no lexical fallback') from error
        raise ValueError('AST extraction unavailable/failed; no lexical fallback') from error
    except (OSError, subprocess.SubprocessError, ValueError, KeyError) as error:
        raise ValueError('AST extraction unavailable/failed; no lexical fallback') from error


class Resources(HTMLParser):
    def __init__(self):
        super().__init__()
        self.references = []
        self.scripts = []
        self.script = None

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == 'base':
            raise ValueError('base tag is not the nested-delivery repair')
        if tag == 'link' and set(attrs.get('rel', '').split()) & {'stylesheet', 'preload', 'modulepreload'}:
            if attrs.get('href'):
                self.references.append(attrs['href'])
        if tag == 'script':
            if attrs.get('src'):
                self.references.append(attrs['src'])
            self.script = []

    def handle_data(self, data):
        if self.script is not None:
            self.script.append(data)

    def handle_endtag(self, tag):
        if tag == 'script' and self.script is not None:
            self.scripts.append(''.join(self.script))
            self.script = None


def html_references(html):
    parser = Resources()
    parser.feed(html)
    references = list(parser.references)
    for parsed in ast_references(parser.scripts):
        references.extend(parsed['modules'])
        references.extend(parsed['wasm'])
    return references


def resolve_local(root, importer, reference):
    parsed = urlsplit(reference)
    if parsed.scheme or parsed.netloc or not parsed.path or parsed.path.startswith('/') or '%' in parsed.path or '\\' in parsed.path:
        raise ValueError(f'non-relative generated runtime reference: {reference}')
    target = (importer.parent / parsed.path).resolve()
    if not target.is_relative_to(root.resolve()) or not target.is_file() or target.is_symlink():
        raise ValueError(f'missing/escaping generated runtime reference: {reference}')
    return target


def check_delivery(root):
    if root.is_symlink() or any(path.is_symlink() for path in root.rglob('*')):
        raise ValueError('symlinked delivery is not accepted')
    root = root.resolve()
    index = root / 'index.html'
    if not index.is_file():
        raise ValueError('built index.html missing')
    references = html_references(index.read_text())
    if not any(urlsplit(ref).path.endswith('.wasm') for ref in references):
        raise ValueError('generated WASM loader/preload reference missing')
    if not any(urlsplit(ref).path.endswith('.css') for ref in references):
        raise ValueError('generated stylesheet reference missing')
    visited = set()
    pending = [resolve_local(root, index, reference) for reference in references]
    while pending:
        path = pending.pop()
        if path in visited:
            continue
        visited.add(path)
        if path.suffix == '.js':
            children = ast_references([path.read_text()])[0]['modules']
        elif path.suffix == '.css':
            text = path.read_text()
            if re.search(r'@import\b', text):
                raise ValueError('unexpected CSS import requires an explicit closure rule')
            children = [ref for ref in CSS_URL.findall(text) if not ref.startswith('data:')]
        else:
            children = []
        pending.extend(resolve_local(root, path, reference) for reference in children)
    return len(visited)


class NestedDeliveryContract(unittest.TestCase):
    def test_build_config_explicit_relative_public_url(self):
        config = (ROOT / 'Trunk.toml').read_text()
        build = re.search(r'^\[build\]\s*\n(.*?)(?=^\[|\Z)', config, re.M | re.S).group(1)
        self.assertRegex(build, r'''(?m)^public_url\s*=\s*(["'])\./\1\s*(?:#.*)?$''')

    def test_ordinary_from_variables_strings_and_comments_are_not_imports(self):
        html = '''<html><head></head><body><script>
const from = document.createElement('span');
from.textContent = 'from';
const description = "from './not-a-module.js'";
const words = 'module_or_path: "./not-wasm.wasm"';
// import './comment-only.js'; export * from './also-comment.js';
/* import('./not-executed.js'); */
</script></body></html>'''
        self.assertEqual(html_references(html), [])

    def test_ast_static_import_export_dynamic_and_wasm_literals(self):
        source = '''import {
  value
} from './multi-line.js';
export {value} from './named.js'; export * from './all.js';
import('./literal.js'); import(`./template.js`); import(computedURL);
const options = {module_or_path: './actual.wasm'};
const phrase = "module_or_path: './not-actual.wasm'";
'''
        parsed = ast_references([source])[0]
        self.assertEqual(set(parsed['modules']), {'./multi-line.js', './named.js', './all.js', './literal.js', './template.js'})
        self.assertEqual(parsed['wasm'], ['./actual.wasm'])

    def test_ast_parser_failure_is_explicit_not_a_lexical_fallback(self):
        with self.assertRaisesRegex(ValueError, 'no lexical fallback'):
            ast_references(['import malformed syntax !!!'])

    def test_mismatched_parser_version_fails_before_parse(self):
        # Confined VM loader double: does not override real tooling or expose a
        # production parser-selection option. Exercise the actual helper main.
        script = r'''
const fs = require('node:fs'), vm = require('node:vm'), assert = require('node:assert/strict');
let parsed = false, output = '', error = '';
const processDouble = {env: process.env, exitCode: 0,
  stdout: {write(value) {output += value;}}, stderr: {write(value) {error += value;}}};
const parserDouble = {version:'8.99.0', parse() {parsed = true; return {type:'Program',body:[]};}};
vm.runInNewContext(fs.readFileSync(process.argv[1], 'utf8'), {
  require(name) {return name === 'acorn' ? parserDouble : require(name);},
  process:processDouble, Buffer,
});
assert.equal(parsed, false, 'mismatched parser must never parse');
assert.equal(processDouble.exitCode, 1);
assert.equal(output, '');
assert.match(error, /parser-version-mismatch/);
assert.match(error, /required=8\.14\.0/);
assert.match(error, /observed=8\.99\.0/);
'''
        subprocess.run(['node', '-e', script, str(ROOT / 'tests/browser/nested-build-imports.cjs')],
                       input=json.dumps({'sources': ['const fixture = 1;']}), text=True,
                       capture_output=True, timeout=20, check=True)

    def test_actual_parser_reports_required_identity(self):
        result = subprocess.run(['node', str(ROOT / 'tests/browser/nested-build-imports.cjs')],
                                input=json.dumps({'sources': []}), text=True,
                                capture_output=True, timeout=20, check=True)
        parsed = json.loads(result.stdout)
        self.assertEqual(parsed['parser'], 'acorn')
        self.assertEqual(parsed['version'], '8.14.0')
        self.assertEqual(parsed['references'], [])

    def test_wrapper_preserves_only_sanitized_version_mismatch_diagnostic(self):
        error = subprocess.CalledProcessError(1, ['synthetic-helper'], stderr=
            'AST extraction FAIL: parser-version-mismatch required=8.14.0 observed=8.99.0; no install\nprivate/path\n')
        with mock.patch.object(subprocess, 'run', side_effect=error):
            with self.assertRaisesRegex(ValueError, 'required=8.14.0 observed=8.99.0') as caught:
                ast_references(['const fixture = 1;'])
        self.assertNotIn('private/path', str(caught.exception))

    def test_literal_dynamic_root_and_remote_imports_still_fail(self):
        for reference in ('/outside/module.js', 'https://remote.invalid/module.js'):
            with self.subTest(reference=reference), self.temp() as temp:
                root = self.synthetic(temp)
                entry = root / 'editor-abcd.js'
                entry.write_text(entry.read_text() + f'\nimport({json.dumps(reference)});')
                with self.assertRaisesRegex(ValueError, 'non-relative'):
                    check_delivery(root)

    def test_actual_prehistory_has_observed_root_absolute_defect(self):
        baseline = ROOT / 'target/alternative-control-prehistory-check'
        if not (baseline / 'index.html').is_file():
            self.skipTest('immutable prehistory output unavailable; synthetic regression controls still run')
        references = html_references((baseline / 'index.html').read_text())
        self.assertTrue(any(re.match(r'/tokens-[a-f0-9]+\.css$', ref) for ref in references))
        self.assertTrue(any(re.match(r'/terraphim-editor-[a-f0-9]+_bg\.wasm$', ref) for ref in references))
        with self.assertRaisesRegex(ValueError, 'non-relative'):
            check_delivery(baseline)

    def synthetic(self, directory):
        root = Path(directory)
        (root / 'index.html').write_text('''<!doctype html><html><head>
<link rel="stylesheet" href="./tokens-abcd.css">
<link rel="modulepreload" href="./editor-abcd.js">
<link rel="preload" as="fetch" href="./editor-abcd_bg.wasm">
</head><body><a href="/host-owned-navigation">not a resource</a>
<script type="module">import init from './editor-abcd.js';
await init({module_or_path:'./editor-abcd_bg.wasm'});</script></body></html>''')
        (root / 'tokens-abcd.css').write_text('body { color: black; }')
        (root / 'editor-abcd.js').write_text("import './snippets/bridge.js'; export default function init() {}")
        (root / 'editor-abcd_bg.wasm').write_bytes(b'synthetic WASM fixture, not executed')
        (root / 'snippets').mkdir()
        (root / 'snippets/bridge.js').write_text('export const fixture = true;')
        return root

    def temp(self):
        scratch = ROOT / 'target/nested-path-unit'
        scratch.mkdir(parents=True, exist_ok=True)
        return tempfile.TemporaryDirectory(dir=scratch)

    def test_relative_synthetic_output_and_module_closure(self):
        with self.temp() as temp:
            self.assertEqual(check_delivery(self.synthetic(temp)), 4)

    def test_inline_module_and_wasm_root_paths_fail_even_with_local_preloads(self):
        for old, new in [("from './editor-abcd.js'", "from '/editor-abcd.js'"),
                         ("module_or_path:'./editor-abcd_bg.wasm'", "module_or_path:'/editor-abcd_bg.wasm'")]:
            with self.subTest(reference=new), self.temp() as temp:
                root = self.synthetic(temp)
                index = root / 'index.html'
                index.write_text(index.read_text().replace(old, new))
                with self.assertRaisesRegex(ValueError, 'non-relative'):
                    check_delivery(root)

    def test_missing_transitive_module_and_external_css_url_fail(self):
        with self.temp() as temp:
            root = self.synthetic(temp)
            (root / 'snippets/bridge.js').unlink()
            with self.assertRaisesRegex(ValueError, 'missing'):
                check_delivery(root)
        with self.temp() as temp:
            root = self.synthetic(temp)
            (root / 'tokens-abcd.css').write_text('body {background:url(https://remote.invalid/icon.svg)}')
            with self.assertRaisesRegex(ValueError, 'non-relative'):
                check_delivery(root)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', type=Path)
    args, rest = parser.parse_known_args()
    if args.check:
        try:
            print(f'nested-delivery: PASS, {check_delivery(args.check)} reachable relative assets')
        except (ValueError, OSError) as error:
            raise SystemExit(f'nested-delivery: FAIL: {error}')
    else:
        unittest.main(argv=[__file__, *rest])
