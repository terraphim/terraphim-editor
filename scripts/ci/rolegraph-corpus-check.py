#!/usr/bin/env python3
"""Local-only engineering corpus qualification of an already-built delivery.

Exit 0: bounded functional/performance checks pass, 1: failure, 2: incomplete
(small corpus and/or unsupported long-task observation). No rebuild/network.
Evidence contains aggregate counts/hashes only, never source paths or terms.
"""
import argparse
import functools
import gzip
import hashlib
import http.server
import json
import os
import pathlib
import tempfile
import threading
from datetime import datetime, timezone

TEMP = pathlib.Path('/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode')
MIB = 1024 ** 2


def encoded(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8')


def validate_documents(documents):
    if not documents or len(documents) > 1000:
        raise ValueError('document count bound')
    sizes = [len((d['title'] + d['body']).encode('utf-8')) for d in documents]
    if max(sizes) > MIB or sum(sizes) + sum(len(d['id'].encode('utf-8')) for d in documents) > 16 * MIB:
        raise ValueError('native source byte bound')
    if len({d['id'] for d in documents}) != len(documents):
        raise ValueError('duplicate identity')
    return sizes


def capture_profile(directory, thesaurus, label):
    paths = sorted(directory.rglob('*.md')) + [thesaurus]
    def identity(path):
        s = path.stat()
        return s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns
    before = [identity(p) for p in paths]
    captured = [p.read_bytes() for p in paths]
    if [identity(p) for p in paths] != before or sorted(directory.rglob('*.md')) != paths[:-1]:
        raise ValueError('source capture drift')
    documents = [{'id': f'{label}-{i:04d}', 'title': '', 'body': data.decode('utf-8')}
                 for i, data in enumerate(captured[:-1])]
    sizes = validate_documents(documents)
    text = captured[-1].decode('utf-8')
    parsed = json.loads(text)
    data = parsed['data']
    if not isinstance(data, dict) or not data or len(captured[-1]) > 16 * MIB:
        raise ValueError('thesaurus bound')
    for term, entry in data.items():
        if not isinstance(term, str) or not term or not isinstance(entry['nterm'], str) or not isinstance(entry['id'], int):
            raise ValueError('thesaurus shape')
    payload = {'label': label, 'documents': documents, 'thesaurusText': text}
    evidence = {'profile': label, 'documents': len(documents), 'sourceBytes': sum(sizes),
                'maxDocumentBytes': max(sizes), 'nativeCorpusBytes': sum(sizes) + sum(len(d['id'].encode('utf-8')) for d in documents), 'terms': len(data),
                'concepts': len({e['id'] for e in data.values()}),
                'thesaurusBytes': len(captured[-1]), 'payloadSha256': hashlib.sha256(encoded(payload)).hexdigest(),
                'sourceManifestSha256': hashlib.sha256(encoded([hashlib.sha256(b).hexdigest() for b in captured])).hexdigest(),
                'targetDocuments': 1000, 'targetStatus': 'incomplete: actual corpus below target' if len(documents) < 1000 else 'met'}
    return payload, evidence


def stage_assets(delivery, root, stage, stage_hook=None):
    required = {'js/rolegraph-worker.js', 'terraphim-editor.min.js',
                'rolegraph/terraphim_rolegraph_browser.js', 'rolegraph/terraphim_rolegraph_browser_bg.wasm'}
    paths = sorted(p for p in delivery.rglob('*') if p.is_file() and p.suffix in ['.js', '.wasm'])
    def identity(p):
        s = p.stat()
        return s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns
    before = [identity(p) for p in paths]
    capture = {p.relative_to(delivery).as_posix(): p.read_bytes() for p in paths}
    if not required.issubset(capture) or [identity(p) for p in paths] != before or paths != sorted(p for p in delivery.rglob('*') if p.is_file() and p.suffix in ['.js', '.wasm']):
        raise ValueError('built asset capture invalid/drift')
    for name in ['corpus-rolegraph.html', 'performance-rolegraph-worker.js']:
        capture[name] = (root / 'tests/browser' / name).read_bytes()
    manifest = {}
    for name, content in capture.items():
        destination = stage / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(content)
        manifest[name] = {'sha256': hashlib.sha256(content).hexdigest(), 'rawBytes': len(content),
                          'gzipBytes': len(gzip.compress(content, mtime=0)),
                          'scope': 'executed' if name in required or name in ['corpus-rolegraph.html', 'performance-rolegraph-worker.js'] else 'inventory-only; not loaded'}
    if stage_hook:
        stage_hook(stage)
    for name, asset in manifest.items():
        if hashlib.sha256((stage / name).read_bytes()).hexdigest() != asset['sha256']:
            raise ValueError('staged asset drift')
    return manifest


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass


def main(argv=None, *, close_browser=None, close_server=None, stage_hook=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('delivery', type=pathlib.Path)
    parser.add_argument('--corpus-root', required=True, type=pathlib.Path)
    parser.add_argument('--engine', choices=['chromium', 'firefox'], default='chromium')
    parser.add_argument('--executable')
    parser.add_argument('--profile-label', required=True, choices=['debug', 'release', 'mixed-root-dev-bridge-release', 'unknown'])
    parser.add_argument('--result', required=True, type=pathlib.Path)
    parser.add_argument('--regression-checks', action='store_true', help='Actual-browser cleanup and staged-byte drift regressions')
    args = parser.parse_args(argv)
    result = {'ok': False, 'engine': args.engine, 'buildProfileLabel': args.profile_label,
              'utc': datetime.now(timezone.utc).isoformat()}
    # Output confined to approved local scratch; never commit private evidence.
    if args.result.resolve().parent != TEMP or not args.result.name.startswith('rolegraph-corpus-'):
        parser.error('result must be rolegraph-corpus-* in approved scratch directory')
    if args.regression_checks:
        return regression_checks(args)
    try:
        from playwright.sync_api import sync_playwright
        root = pathlib.Path(__file__).resolve().parents[2]
        profiles = [('kg', 'docs/src/kg', 'docs/src/thesaurus.json'),
                    ('engineer', 'terraphim_server/fixtures/haystack', 'terraphim_server/fixtures/haystack/Engineer_thesaurus.json')]
        inputs, metadata = [], []
        for label, directory, terms in profiles:
            payload, evidence = capture_profile(args.corpus_root / directory, args.corpus_root / terms, label)
            inputs.append(payload)
            metadata.append(evidence)
        result['corpora'] = metadata
        if hasattr(os, 'getloadavg'):
            result['hostLoadAverage'] = os.getloadavg()
        with tempfile.TemporaryDirectory(prefix='rolegraph-corpus-', dir=TEMP) as temporary:
            stage = pathlib.Path(temporary)
            result['assets'] = stage_assets(args.delivery.resolve(), root, stage, stage_hook)
            result['inputManifestSha256'] = hashlib.sha256(encoded(result['assets'])).hexdigest()
            payload_bytes = encoded(inputs)
            (stage / 'local-corpus.json').write_bytes(payload_bytes)
            if (stage / 'local-corpus.json').read_bytes() != payload_bytes:
                raise ValueError('staged corpus drift')
            result['captureMethod'] = 'read-once bytes; source stat drift rejection; staged hash verification; no release build manifest'
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(QuietHandler, directory=str(stage)))
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                with sync_playwright() as playwright:
                    browser = getattr(playwright, args.engine).launch(headless=True, **({'executable_path': args.executable} if args.executable else {}))
                    try:
                        result['browserVersion'] = browser.version
                        context = browser.new_context()
                        origin = f'http://127.0.0.1:{server.server_port}'
                        context.route('**/*', lambda route: route.continue_() if route.request.url.startswith(origin + '/') else route.abort())
                        page = context.new_page()
                        errors = []
                        page.on('pageerror', lambda error: errors.append('pageerror'))
                        page.goto(origin + '/corpus-rolegraph.html')
                        page.wait_for_function('Boolean(window.corpusResult)', timeout=120000)
                        result.update(page.evaluate('window.corpusResult'))
                        result['pageErrorCount'] = len(errors)
                        if errors:
                            result['ok'] = False
                    finally:
                        (close_browser or (lambda b: b.close()))(browser)
            finally:
                try:
                    server.shutdown()
                finally:
                    try:
                        (close_server or (lambda s: s.server_close()))(server)
                    finally:
                        thread.join(timeout=5)
                        if thread.is_alive():
                            raise RuntimeError('server thread cleanup')
    except Exception as error:
        # Exception messages may contain private paths, terms or response bodies.
        result['ok'] = False
        result['runnerErrorType'] = type(error).__name__
    code = 1
    if result.get('ok'):
        code = 2 if not result.get('longTaskSupported') or any(c['documents'] < 1000 for c in result['corpora']) else 0
    result['boundedChecksPassed'] = bool(result.get('ok'))
    result['ok'] = code == 0
    result['qualification'] = 'failed' if code == 1 else 'incomplete' if code == 2 else 'passed'
    result['exitCode'] = code
    args.result.write_bytes(encoded(result) + b'\n')
    print(json.dumps({'qualification': result['qualification'], 'exitCode': code}))
    return code


def regression_checks(args):
    def browser_failure(browser):
        browser.close()
        raise RuntimeError('injected after real browser cleanup')
    def server_failure(server):
        server.server_close()
        raise RuntimeError('injected after real server cleanup')
    def drift(stage):
        with (stage / 'js/rolegraph-worker.js').open('ab') as stream:
            stream.write(b'\n// injected staged drift\n')
    for label, seam in [('browser-close', {'close_browser': browser_failure}),
                        ('server-close', {'close_server': server_failure}),
                        ('stage-drift', {'stage_hook': drift})]:
        output = args.result.with_name(f'{args.result.stem}-{label}.json')
        argv = [str(args.delivery), '--corpus-root', str(args.corpus_root), '--engine', args.engine,
                '--profile-label', args.profile_label, '--result', str(output)]
        if args.executable:
            argv += ['--executable', args.executable]
        code = main(argv, **seam)
        evidence = json.loads(output.read_text())
        assert code == 1 and not evidence['ok'] and evidence.get('runnerErrorType'), label
        if label != 'stage-drift':
            assert evidence.get('completed'), 'Real measurement must complete before cleanup injection'
        print(f'REGRESSION PASS {args.engine} {label}')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
