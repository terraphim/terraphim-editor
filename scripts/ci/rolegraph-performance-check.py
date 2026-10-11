#!/usr/bin/env python3
"""Qualify already-built RoleGraph with a provisional synthetic desktop corpus.

Exit 0: all measurable budgets pass; 1: failed/error; 2: unsupported long-task
observation (not a green qualification). Requires existing Python Playwright.
Copies built delivery into a temporary directory; never rebuilds or modifies it.
"""
import argparse
import functools
import gzip
import hashlib
import http.server
import json
import os
import pathlib
import platform
import tempfile
import threading
from datetime import datetime, timezone

from playwright.sync_api import sync_playwright


def main(argv=None, *, close_browser=None, close_server=None, stage_hook=None):
    # Explicit external-resource seams for regression tests; never replace engines.
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('delivery', type=pathlib.Path)
    parser.add_argument('--engine', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--executable', help='Existing Chrome executable, if Playwright Chromium is unavailable')
    parser.add_argument('--profile-label', required=True, help='Build provenance; not inferred from filenames')
    parser.add_argument('--result', required=True, type=pathlib.Path)
    parser.add_argument('--regression-checks', action='store_true',
                        help='Run real-browser cleanup and staged-byte drift failure regressions')
    args = parser.parse_args(argv)
    if args.regression_checks:
        return regression_checks(args)
    root = pathlib.Path(__file__).resolve().parents[2]
    delivery = args.delivery.resolve()
    metadata = {'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
                'buildProfileLabel': args.profile_label, 'delivery': str(delivery),
                'engine': args.engine, 'executable': args.executable}
    if hasattr(os, 'getloadavg'):
        metadata['hostLoadAverage'] = os.getloadavg()
    result = {'ok': False}
    def failure(where, error):
        result['ok'] = False
        result.setdefault('runnerErrors', []).append({'where': where, 'error': str(error)})
        result['runnerError'] = '; '.join(f"{e['where']}: {e['error']}" for e in result['runnerErrors'])

    try:
        with tempfile.TemporaryDirectory(prefix='rolegraph-performance-') as temporary:
            stage = pathlib.Path(temporary)
            stage_assets(delivery, root, stage, metadata, stage_hook)
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
                functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(stage)))
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                with sync_playwright() as playwright:
                    browser = getattr(playwright, args.engine).launch(headless=True,
                        **({'executable_path': args.executable} if args.executable else {}))
                    try:
                        metadata['browserVersion'] = browser.version
                        context = browser.new_context(viewport={'width':1280, 'height':800})
                        context.route('https://**', lambda route: route.abort())
                        page = context.new_page()
                        errors = []
                        page.on('pageerror', lambda error: errors.append(str(error)))
                        page.goto(f'http://127.0.0.1:{server.server_port}/performance-rolegraph.html')
                        page.wait_for_function('Boolean(window.performanceResult)', timeout=120000)
                        result.update(page.evaluate('window.performanceResult'))
                        result['pageErrors'] = errors
                        if errors:
                            result['ok'] = False
                    finally:
                        try:
                            (close_browser or (lambda b: b.close()))(browser)
                        except Exception as error:
                            failure('browser.close', error)
            except Exception as error:
                failure('browser execution', error)
            finally:
                # A failed cleanup must not prevent the other cleanup attempts.
                for name, action in [('server.shutdown', server.shutdown),
                        ('server.close', lambda: (close_server or (lambda s: s.server_close()))(server)),
                        ('server.thread.join', thread.join)]:
                    try:
                        action()
                    except Exception as error:
                        failure(name, error)
    except Exception as error:
        failure('staging/runtime cleanup', error)
    result['runner'] = metadata
    code = 1
    if not result.get('runnerError'):
        if result.get('ok'):
            code = 0
        elif (result.get('completed') and result.get('budgets', {}).get('mainThread') is None
                and result['budgets']['query'] and result['budgets']['workerWasm'] and not result.get('pageErrors')):
            code = 2
    result['exitCode'] = code
    args.result.parent.mkdir(parents=True, exist_ok=True)
    args.result.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: result.get(key) for key in
        ['ok', 'completed', 'error', 'runnerError', 'timings', 'stats', 'peakWorkerWasmBytes', 'budgets', 'exitCode']}, indent=2))
    print(f'Evidence: {args.result.resolve()}')
    return code


def stage_assets(delivery, root, stage, metadata, stage_hook=None):
    required = {'js/rolegraph-worker.js', 'terraphim-editor.min.js',
                'rolegraph/terraphim_rolegraph_browser.js', 'rolegraph/terraphim_rolegraph_browser_bg.wasm'}
    def paths():
        return sorted(p for p in delivery.rglob('*') if p.is_file() and p.suffix in ['.wasm', '.js'])
    def identity(path):
        stat = path.stat()
        return (stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns)
    files = paths()
    before = {p: identity(p) for p in files}
    captured = {str(p.relative_to(delivery)): p.read_bytes() for p in files}
    if not required.issubset(captured):
        raise RuntimeError(f'Missing built assets: {sorted(required - captured.keys())}')
    if files != paths() or any(identity(p) != before[p] for p in files):
        raise RuntimeError('Delivery changed during capture; use a stable build')
    harness = {name: (root / 'tests/browser' / name).read_bytes() for name in
               ['performance-rolegraph.html', 'performance-rolegraph-worker.js']}
    captured.update(harness)
    # Exactly these captured immutable bytes are both inventoried and staged.
    metadata['assets'] = {}
    for name, data in captured.items():
        metadata['assets'][name] = {'rawBytes': len(data), 'gzipBytes': len(gzip.compress(data, mtime=0)),
            'sha256': hashlib.sha256(data).hexdigest(),
            'scope': 'executed' if name in required or name in harness else 'inventory-only; not loaded'}
        destination = stage / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    manifest = json.dumps(metadata['assets'], sort_keys=True, separators=(',', ':')).encode()
    metadata['inputManifestSha256'] = hashlib.sha256(manifest).hexdigest()
    metadata['captureMethod'] = 'read-once bytes; source stat drift rejection; staged hash verification; no release build manifest'
    if stage_hook:
        stage_hook(stage)
    for name, asset in metadata['assets'].items():
        if hashlib.sha256((stage / name).read_bytes()).hexdigest() != asset['sha256']:
            raise RuntimeError(f'Staged input hash drift: {name}')


def regression_checks(args):
    # Actual runner and actual installed browser: only external cleanup/staging faults.
    def fail_browser(browser):
        browser.close()
        raise RuntimeError('injected after actual browser.close')
    def fail_server(server):
        server.server_close()
        raise RuntimeError('injected after actual server.server_close')
    def drift(stage):
        with (stage / 'js/rolegraph-worker.js').open('ab') as stream:
            stream.write(b'\n// injected staged-byte drift\n')
    for label, seam in [('browser-close', {'close_browser': fail_browser}),
                        ('server-close', {'close_server': fail_server}),
                        ('stage-drift', {'stage_hook': drift})]:
        output = args.result.with_name(f'{args.result.stem}-{label}.json')
        argv = [str(args.delivery), '--engine', args.engine, '--profile-label', args.profile_label, '--result', str(output)]
        if args.executable:
            argv += ['--executable', args.executable]
        code = main(argv, **seam)
        evidence = json.loads(output.read_text())
        assert code == 1 and evidence['exitCode'] == 1 and not evidence['ok'] and evidence.get('runnerError'), label
        if label != 'stage-drift':
            assert evidence.get('completed'), 'Cleanup test must follow a completed real graph measurement'
        print(f'REGRESSION PASS {args.engine} {label}: actual runner exit 1')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
