#!/usr/bin/env python3
"""Actual Safari qualification via native safaridriver and stdlib WebDriver HTTP.

Requires user-enabled Remote Automation; never enables it or accesses a normal
Safari profile. Safari opens an automation window (no headless substitute).
"""
import argparse
import functools
import hashlib
import http.server
import json
import math
import pathlib
import platform
import re
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone


CASES = [('originalWorker', 'worker-rolegraph.html', 'workerResult', 33),
         ('reviewWorker', 'worker-rolegraph.html?fixture=review_scenario.json', 'workerResult', 123),
         ('delivery', 'delivery-rolegraph.html', 'deliveryResult', 27)]


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_):
        pass


def stage_assets(delivery, root, stage, metadata, hook=None):
    required = {'terraphim_editor_bg.wasm', 'terraphim-editor.min.js',
                'js/rolegraph-worker.js', 'rolegraph/terraphim_rolegraph_browser.js',
                'rolegraph/terraphim_rolegraph_browser_bg.wasm'}
    def files():
        return sorted(p for p in delivery.rglob('*') if p.is_file()
                      and p.suffix in {'.js', '.wasm', '.css'})
    def identity(path):
        s = path.stat()
        return s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns
    paths = files()
    before = {p: identity(p) for p in paths}
    captured = {p.relative_to(delivery).as_posix(): p.read_bytes() for p in paths}
    if not required.issubset(captured):
        raise RuntimeError(f'Missing built assets: {sorted(required - captured.keys())}')
    if paths != files() or any(identity(p) != before[p] for p in paths):
        raise RuntimeError('Delivery changed during capture')
    for source, name in [
        ('tests/browser/worker-rolegraph.html', 'worker-rolegraph.html'),
        ('tests/browser/delivery-rolegraph.html', 'delivery-rolegraph.html'),
        ('crates/terraphim_rolegraph_browser/tests/fixtures/scenario.json', 'scenario.json'),
        ('crates/terraphim_rolegraph_browser/tests/fixtures/review_scenario.json', 'review_scenario.json'),
    ]:
        captured[name] = (root / source).read_bytes()
    metadata['assets'] = {}
    for name, data in captured.items():
        metadata['assets'][name] = {'rawBytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}
        destination = stage / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
    manifest = json.dumps(metadata['assets'], sort_keys=True, separators=(',', ':')).encode()
    metadata['inputManifestSha256'] = hashlib.sha256(manifest).hexdigest()
    metadata['captureMethod'] = 'read-once bytes; source stat drift rejection; all staged JS/WASM/CSS and fixtures hashed'
    if hook:
        hook(stage)
    verify_stage(stage, metadata)


def verify_stage(stage, metadata):
    for name, asset in metadata['assets'].items():
        if hashlib.sha256((stage / name).read_bytes()).hexdigest() != asset['sha256']:
            raise RuntimeError(f'Staged input hash drift: {name}')


def webdriver(base, method, route, payload=None):
    data = None if payload is None else json.dumps(payload).encode()
    request = urllib.request.Request(base + route, data=data, method=method,
                                     headers={'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(request, timeout=65) as response:
            body = json.load(response)
    except urllib.error.HTTPError as error:
        body = json.loads(error.read())
        value = body.get('value', {})
        raise RuntimeError(f'WebDriver HTTP {error.code}: {value.get("error")}: {value.get("message")}') from error
    value = body.get('value')
    if isinstance(value, dict) and value.get('error'):
        raise RuntimeError(f'WebDriver: {value["error"]}: {value.get("message")}')
    return value


def session_request():
    # Native safaridriver matches 'mac' case-insensitively. Never request a
    # paired iOS/simulator host or a remoteHost capability.
    return {'capabilities': {'alwaysMatch': {'browserName': 'safari', 'platformName': 'mac'}}}


def validate_capabilities(capabilities):
    if not isinstance(capabilities, dict):
        raise RuntimeError('Missing Safari platform capabilities')
    browser = capabilities.get('browserName')
    host = capabilities.get('platformName')
    if not isinstance(browser, str) or browser.lower() != 'safari':
        raise RuntimeError('Actual Safari required; substitutes refused')
    # Accept native mac/macOS spellings only; missing platform is not a fallback.
    if not isinstance(host, str) or host.lower() not in {'mac', 'macos'}:
        raise RuntimeError('Native macOS Safari platformName required')


def case_summary(label, value, expected):
    if label not in {case[0] for case in CASES}:
        raise RuntimeError('Unknown qualification fixture')
    if not isinstance(value, dict):
        return {'ok': False, 'count': None, 'expectedCount': expected,
                'fixture': label, 'code': None, 'errorClass': 'invalid-result'}
    checks = value.get('checks')
    count = (len(checks) if isinstance(checks, list) else None) if label == 'delivery' else value.get('steps')
    if type(count) is not int or not 0 <= count <= 1000000:
        count = None
    # Whitelist aggregate evidence only; never emit snapshot/corpus contents.
    result = {'ok': value.get('ok') is True, 'count': count, 'expectedCount': expected,
              'userAgent': value.get('userAgent')}
    if not result['ok'] or count != expected:
        result.pop('userAgent')
        code = value.get('code')
        allowed_codes = {'storage_failed', 'timeout', 'invalid_snapshot', 'role_required',
                         'worker_failed', 'generation_mismatch', 'revision_mismatch'}
        code = code if isinstance(code, str) and code in allowed_codes else (None if code is None else 'unrecognized')
        error = value.get('error')
        # Classify bounded input against controlled harness signatures; never
        # copy arbitrary page text, assertion pointers, corpus, or snapshot.
        error = error[:512] if isinstance(error, str) else ''
        known = {'Error: WASM initialization failed': 'wasm-initialization',
                 'Error: Worker timeout': 'worker-timeout', 'Error: Worker failed': 'worker-failure',
                 'Error: Page length': 'worker-page-length', 'Error: Unknown fixture': 'unknown-fixture'}
        classification = known.get(error, 'harness-failure')
        if code in allowed_codes:
            classification = 'typed-' + code
        elif re.match(r'^Error: step [0-9]+ ', error):
            classification = 'worker-assertion'
        elif result['ok'] and count != expected:
            classification = 'count-mismatch'
        result.update({'fixture': label, 'code': code, 'errorClass': classification})
    return result


def validate_case(label, result):
    count, expected = result['count'], result['expectedCount']
    if not result['ok'] or count != expected:
        raise RuntimeError(f'{label}: qualification failed (ok={result["ok"]}, count={count}, expected={expected})')
    return result


def aggregate(label, value, expected):
    return validate_case(label, case_summary(label, value, expected))


def record_case(cases, label, value, expected):
    cases[label] = case_summary(label, value, expected)
    return validate_case(label, cases[label])


def cleanup(actions, failure):
    for name, action in actions:
        try:
            action()
        except Exception as error:
            failure(name, error)


def finalize(result):
    result['ok'] = (result.get('completed') is True and not result['runnerErrors']
                    and set(result['cases']) == {case[0] for case in CASES}
                    and all(case.get('ok') is True and case.get('count') == case.get('expectedCount')
                            for case in result['cases'].values()))
    result['exitCode'] = 0 if result['ok'] else 1
    return result['exitCode']


def main(argv=None, *, stage_hook=None, cleanup_hook=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('delivery', type=pathlib.Path)
    parser.add_argument('--result', required=True, type=pathlib.Path)
    parser.add_argument('--profile-label', required=True)
    parser.add_argument('--timeout', type=float, default=90)
    args = parser.parse_args(argv)
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error('--timeout must be finite and positive')
    root = pathlib.Path(__file__).resolve().parents[2]
    metadata = {'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
                'engine': 'actual Safari native WebDriver', 'delivery': str(args.delivery.resolve()),
                'buildProfileLabel': args.profile_label, 'offlineQualified': False}
    result = {'ok': False, 'cases': {}, 'runner': metadata, 'runnerErrors': []}
    def failure(where, error):
        result['runnerErrors'].append({'where': where, 'error': str(error)})
    session = None
    driver = server = thread = None
    try:
        metadata['driverVersion'] = subprocess.check_output(['/usr/bin/safaridriver', '--version'], text=True).strip()
        with tempfile.TemporaryDirectory(prefix='rolegraph-safari-') as temporary:
            stage = pathlib.Path(temporary) / 'assets'
            stage_assets(args.delivery.resolve(), root, stage, metadata, stage_hook)
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
                functools.partial(QuietHandler, directory=str(stage)))
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            with socket.socket() as port_socket:
                port_socket.bind(('127.0.0.1', 0))
                port = port_socket.getsockname()[1]
            base = f'http://127.0.0.1:{port}'
            # Driver diagnostics stay ephemeral; errors are reported through HTTP.
            with (pathlib.Path(temporary) / 'driver.log').open('w') as log:
                driver = subprocess.Popen(['/usr/bin/safaridriver', '--port', str(port)], stdout=log, stderr=log)
                try:
                    deadline = time.monotonic() + 15
                    while True:
                        if driver.poll() is not None:
                            raise RuntimeError(f'safaridriver exited {driver.returncode}')
                        try:
                            webdriver(base, 'GET', '/status')
                            break
                        except urllib.error.URLError:
                            if time.monotonic() >= deadline:
                                raise RuntimeError('safaridriver startup timeout')
                            time.sleep(.1)
                    created = webdriver(base, 'POST', '/session', session_request())
                    session = created['sessionId']
                    capabilities = created['capabilities']
                    metadata['browserVersion'] = capabilities.get('browserVersion')
                    validate_capabilities(capabilities)
                    metadata['platformName'] = capabilities['platformName']
                    route = '/session/' + session
                    webdriver(base, 'POST', route + '/timeouts', {'pageLoad': 60000, 'script': 10000, 'implicit': 0})
                    for label, page, variable, expected in CASES:
                        webdriver(base, 'POST', route + '/url', {'url': f'http://127.0.0.1:{server.server_port}/{page}'})
                        deadline = time.monotonic() + args.timeout
                        while True:
                            value = webdriver(base, 'POST', route + '/execute/sync',
                                {'script': f'return window.{variable} || null;', 'args': []})
                            if value is not None:
                                break
                            if time.monotonic() >= deadline:
                                raise RuntimeError(f'{label}: result timeout')
                            time.sleep(.1)
                        record_case(result['cases'], label, value, expected)
                        print(f'PASS actual Safari {label}: {expected}')
                    result['completed'] = True
                    verify_stage(stage, metadata)
                finally:
                    actions = []
                    if session:
                        actions.append(('session.delete', lambda: webdriver(base, 'DELETE', '/session/' + session)))
                    if cleanup_hook:
                        actions.append(('cleanup regression', cleanup_hook))
                    def stop_driver():
                        if driver.poll() is None:
                            driver.terminate()
                        try:
                            driver.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            driver.kill()
                            driver.wait(timeout=10)
                            raise RuntimeError('safaridriver required kill after terminate timeout')
                    actions.append(('driver.stop', stop_driver))
                    cleanup(actions, failure)
            # Server is cleaned before TemporaryDirectory removes served inputs.
            cleanup([('server.shutdown', server.shutdown), ('server.close', server.server_close),
                     ('server.join', lambda: thread.join(timeout=10))], failure)
            if thread.is_alive():
                failure('server.join', RuntimeError('server thread still alive'))
            server = None
    except Exception as error:
        failure('staging/browser execution', error)
    finally:
        if server is not None:
            cleanup([('server.shutdown', server.shutdown), ('server.close', server.server_close),
                     ('server.join', lambda: thread.join(timeout=10))], failure)
        finalize(result)
        args.result.parent.mkdir(parents=True, exist_ok=True)
        args.result.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps({'ok': result['ok'], 'cases': result['cases'], 'runnerErrors': result['runnerErrors'],
                          'exitCode': result['exitCode']}, indent=2))
    return result['exitCode']


if __name__ == '__main__':
    raise SystemExit(main())
