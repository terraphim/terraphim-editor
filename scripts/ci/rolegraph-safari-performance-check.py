#!/usr/bin/env python3
"""Actual native macOS Safari synthetic performance measurement (0/1/2).

0 means all budgets measured and passed; 1 means failure; 2 means completed
query/memory measurement but unsupported Long Tasks (NOT qualification green).
Requires already-enabled Remote Automation. Never enables it or rebuilds assets.
"""
import argparse
import functools
import hashlib
import http.server
import json
import math
import os
import pathlib
import platform
import socket
import subprocess
import tempfile
import threading
import time
import types
import urllib.error
from datetime import datetime, timezone

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEPENDENCIES = {}


def identity(path):
    s = path.stat()
    return s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns


def load_helper(name, filename):
    path = ROOT / 'scripts/ci' / filename
    before = identity(path)
    data = path.read_bytes()
    if identity(path) != before:
        raise RuntimeError('Helper source changed during capture')
    DEPENDENCIES[filename] = {'sha256': hashlib.sha256(data).hexdigest(),
                             'rawBytes': len(data), 'identity': before}
    module = types.ModuleType(name)
    module.__file__ = str(path)
    exec(compile(data, str(path), 'exec'), module.__dict__)
    return module


SELF_PATH = pathlib.Path(__file__)
SELF_IDENTITY = identity(SELF_PATH)
SELF_BYTES = SELF_PATH.read_bytes()
if identity(SELF_PATH) != SELF_IDENTITY:
    raise RuntimeError('Runner source changed during capture')
SELF_HASH = hashlib.sha256(SELF_BYTES).hexdigest()
SAFARI = load_helper('safari_helpers', 'rolegraph-safari-check.py')
PERF = load_helper('performance_helpers', 'rolegraph-performance-check.py')


def stage_assets(delivery, stage, metadata, hook=None):
    # Reuse the identical Chrome/Firefox manifest schema and captured asset bytes.
    sources = [ROOT / 'tests/browser' / name for name in
               ('performance-rolegraph.html', 'performance-rolegraph-worker.js')]
    before = {p: identity(p) for p in sources}
    for filename, item in DEPENDENCIES.items():
        if identity(ROOT / 'scripts/ci' / filename) != item['identity']:
            raise RuntimeError('Helper source drift since immutable import')
    PERF.stage_assets(delivery, ROOT, stage, metadata, hook)
    if any(identity(p) != before[p] for p in sources):
        raise RuntimeError('Performance harness changed during capture')
    metadata['helperDependencies'] = {name: {k: v for k, v in item.items() if k != 'identity'}
                                      for name, item in DEPENDENCIES.items()}
    if identity(SELF_PATH) != SELF_IDENTITY:
        raise RuntimeError('Runner source drift since immutable capture')
    metadata['runnerSource'] = {'sha256': SELF_HASH, 'rawBytes': len(SELF_BYTES)}


def finalize(result):
    budgets = result.get('budgets', {})
    if not isinstance(budgets, dict):
        budgets = {}
    complete = result.get('completed') is True
    measured = (budgets.get('query') is True and budgets.get('workerWasm') is True
                and budgets.get('foreground') is True
                and result.get('visibility', {}).get('qualifiedForeground') is True)
    clean = not result['runnerErrors'] and not result.get('error')
    supported = result.get('longTaskSupported')
    result['ok'] = bool(clean and complete and measured and supported is True
                        and budgets.get('mainThread') is True)
    code = 0 if result['ok'] else 1
    if (clean and complete and measured and supported is False
            and 'mainThread' in budgets and budgets['mainThread'] is None):
        code = 2
    result['exitCode'] = code
    return code


def visibility_summary(result):
    visibility = result.get('visibility', {})
    samples = visibility.get('samples', [])
    checkpoints = visibility.get('checkpoints', {})
    def visible(state):
        return isinstance(state, dict) and state.get('visibilityState') == 'visible'
    valid = (len(samples) == 110 and all(isinstance(s, dict) and s.get('index') == i
              and visible(s.get('before')) and visible(s.get('after')) for i, s in enumerate(samples)))
    qualified = (visibility.get('invocation') in {'automatic', 'visible-request'} and valid
                 and visible(checkpoints.get('queriesBegin')) and visible(checkpoints.get('queriesEnd'))
                 and visibility.get('droppedChanges') == 0
                 and all(visible(s) for s in visibility.get('changes', []) if s.get('phase') == 'queries'))
    states = [s.get(key, {}).get('visibilityState') for s in samples for key in ('before', 'after')]
    return {'qualifiedForeground': bool(qualified), 'observedSampleStates': sorted(set(states) - {None}),
            'hiddenMeasuredSamples': sum(s.get('index', -1) >= 10 and
                any(s.get(key, {}).get('visibilityState') == 'hidden' for key in ('before', 'after')) for s in samples)}


def positive(value):
    number = float(value)
    if not math.isfinite(number) or number <= 0:
        raise argparse.ArgumentTypeError('timeout must be finite and positive')
    return number


def hidden_dwell(value):
    number = positive(value)
    if number > 120:
        raise argparse.ArgumentTypeError('hidden dwell must not exceed 120 seconds')
    return number


def wait_value(base, route, script, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = SAFARI.webdriver(base, 'POST', route + '/execute/sync',
                                 {'script': script, 'args': []})
        if value is not None:
            return value
        time.sleep(min(.1, max(0, deadline - time.monotonic())))
    raise RuntimeError('Performance result polling timeout')


def stop_driver(driver):
    if driver.poll() is None:
        try:
            driver.terminate()
        except Exception:
            driver.kill()
            driver.wait(timeout=10)
            raise
    try:
        driver.wait(timeout=10)
    except subprocess.TimeoutExpired:
        driver.kill()
        driver.wait(timeout=10)
        raise RuntimeError('safaridriver required kill after terminate timeout')


def stop_server(server, thread, failure):
    SAFARI.cleanup([('server.shutdown', server.shutdown), ('server.close', server.server_close),
                    ('server.join', lambda: thread.join(timeout=10))], failure)
    if thread.is_alive():
        failure('server.join', RuntimeError('Server thread still alive'))


def main(argv=None, *, stage_hook=None, cleanup_hook=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('delivery', type=pathlib.Path)
    parser.add_argument('--result', required=True, type=pathlib.Path)
    parser.add_argument('--profile-label', required=True)
    parser.add_argument('--timeout', type=positive, default=120)
    parser.add_argument('--ready-timeout', type=positive, default=15)
    parser.add_argument('--visibility-context', choices=['visible', 'hidden-request'], default='visible')
    parser.add_argument('--hidden-dwell', type=hidden_dwell, default=45,
                        help='Bounded unpolled second-tab interval; observed state determines validity')
    args = parser.parse_args(argv)
    metadata = {'utc': datetime.now(timezone.utc).isoformat(), 'platform': platform.platform(),
                'engine': 'actual Safari native WebDriver', 'delivery': str(args.delivery.resolve()),
                'buildProfileLabel': args.profile_label, 'offlineQualified': False,
                'visibilityContextRequested': args.visibility_context,
                'networkScope': 'local synthetic harness; no external endpoint accepted; not network-isolated',
                'timeoutsSeconds': {'ready': args.ready_timeout, 'poll': args.timeout,
                                    'webdriverRequest': 65, 'driverStop': 10, 'serverJoin': 10,
                                    'hiddenDwell': args.hidden_dwell}}
    if hasattr(os, 'getloadavg'):
        metadata['hostLoadAverage'] = os.getloadavg()
    result = {'ok': False, 'runner': metadata, 'runnerErrors': []}
    def failure(where, error):
        result['runnerErrors'].append({'where': where, 'error': str(error)})
    try:
        metadata['driverVersion'] = subprocess.check_output(
            ['/usr/bin/safaridriver', '--version'], text=True, timeout=10).strip()
        with tempfile.TemporaryDirectory(prefix='rolegraph-safari-performance-') as temporary:
            stage = pathlib.Path(temporary) / 'assets'
            stage_assets(args.delivery.resolve(), stage, metadata, stage_hook)
            server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
                functools.partial(SAFARI.QuietHandler, directory=str(stage)))
            server.daemon_threads = True
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            started = False
            try:
                thread.start()
                started = True
                with socket.socket() as port_socket:
                    port_socket.bind(('127.0.0.1', 0))
                    port = port_socket.getsockname()[1]
                base = f'http://127.0.0.1:{port}'
                with (pathlib.Path(temporary) / 'driver.log').open('w') as log:
                    driver = subprocess.Popen(['/usr/bin/safaridriver', '--port', str(port)],
                                              stdout=log, stderr=log)
                    session = None
                    try:
                        deadline = time.monotonic() + args.ready_timeout
                        while time.monotonic() < deadline:
                            if driver.poll() is not None:
                                raise RuntimeError('safaridriver exited before ready')
                            try:
                                SAFARI.webdriver(base, 'GET', '/status')
                                break
                            except urllib.error.URLError:
                                time.sleep(.1)
                        else:
                            raise RuntimeError('safaridriver startup timeout')
                        created = SAFARI.webdriver(base, 'POST', '/session', SAFARI.session_request())
                        session = created['sessionId']
                        caps = created['capabilities']
                        metadata['capabilities'] = caps
                        SAFARI.validate_capabilities(caps)
                        metadata['browserVersion'] = caps.get('browserVersion')
                        metadata['platformName'] = caps['platformName']
                        route = '/session/' + session
                        SAFARI.webdriver(base, 'POST', route + '/timeouts',
                                         {'pageLoad': 60000, 'script': 10000, 'implicit': 0})
                        SAFARI.webdriver(base, 'POST', route + '/url',
                            {'url': f'http://127.0.0.1:{server.server_port}/performance-rolegraph.html?controlled=1'})
                        handle = SAFARI.webdriver(base, 'GET', route + '/window')
                        SAFARI.webdriver(base, 'POST', route + '/window', {'handle': handle})
                        metadata['windowRect'] = SAFARI.webdriver(base, 'GET', route + '/window/rect')
                        wait_value(base, route, 'return typeof window.startPerformance === "function" ? true : null;', args.timeout)
                        invocation = 'visible-request' if args.visibility_context == 'visible' else 'hidden-request'
                        metadata['foregroundRequest'] = 'WebDriver window selection and window.focus(); observed state, not assumed'
                        metadata['invocationState'] = SAFARI.webdriver(base, 'POST', route + '/execute/sync',
                            {'script': 'window.focus(); window.startPerformance(arguments[0]); '
                                       'return {visibilityState: document.visibilityState, hasFocus: document.hasFocus()};',
                             'args': [invocation]})
                        if args.visibility_context == 'hidden-request':
                            # No OS/application activation, fake visibility, or offscreen-as-hidden claim.
                            other = SAFARI.webdriver(base, 'POST', route + '/window/new', {'type': 'tab'})
                            SAFARI.webdriver(base, 'POST', route + '/window', {'handle': other['handle']})
                            SAFARI.webdriver(base, 'POST', route + '/url', {'url': 'about:blank'})
                            metadata['hiddenAttempt'] = 'second automation tab selected; bounded unpolled dwell; switch back to read result'
                            time.sleep(args.hidden_dwell)
                            SAFARI.webdriver(base, 'POST', route + '/window', {'handle': handle})
                        value = wait_value(base, route, 'return window.performanceResult || null;', args.timeout)
                        if not isinstance(value, dict):
                            raise RuntimeError('Invalid synthetic performance result')
                        # Only the local immutable synthetic harness is executed; no host corpus is read.
                        result.update({k: v for k, v in value.items() if k not in {'runner', 'runnerErrors', 'exitCode'}})
                        result.setdefault('visibility', {}).update(visibility_summary(result))
                        result.setdefault('budgets', {})['foreground'] = result['visibility']['qualifiedForeground']
                        SAFARI.verify_stage(stage, metadata)
                    finally:
                        actions = []
                        if session:
                            actions.append(('session.delete', lambda: SAFARI.webdriver(base, 'DELETE', '/session/' + session)))
                        if cleanup_hook:
                            actions.append(('cleanup regression', cleanup_hook))
                        actions.append(('driver.stop', lambda: stop_driver(driver)))
                        SAFARI.cleanup(actions, failure)
            finally:
                if started:
                    stop_server(server, thread, failure)
                else:
                    SAFARI.cleanup([('server.close', server.server_close)], failure)
    except Exception as error:
        failure('staging/browser execution', error)
    finally:
        finalize(result)
        args.result.parent.mkdir(parents=True, exist_ok=True)
        args.result.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps({key: result.get(key) for key in
            ('ok', 'completed', 'timings', 'stats', 'peakWorkerWasmBytes', 'budgets', 'runnerErrors', 'exitCode')}, indent=2))
    return result['exitCode']


if __name__ == '__main__':
    raise SystemExit(main())
