"""Runner-only regressions; no mocked engine is claimed as Safari measurement."""
import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('safari_perf', ROOT / 'scripts/ci/rolegraph-safari-performance-check.py')
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class RunnerTests(unittest.TestCase):
    def test_native_capability_helpers_reused(self):
        self.assertEqual(RUNNER.SAFARI.session_request()['capabilities']['alwaysMatch'],
                         {'browserName': 'safari', 'platformName': 'mac'})
        for platform in ('mac', 'macOS'):
            RUNNER.SAFARI.validate_capabilities({'browserName': 'Safari', 'platformName': platform})
        for caps in ({'browserName': 'webkit', 'platformName': 'mac'},
                     {'browserName': 'Safari', 'platformName': 'ios'}, {'browserName': 'Safari'}):
            with self.assertRaises(RuntimeError):
                RUNNER.SAFARI.validate_capabilities(caps)

    def test_invalid_timeouts_rejected_before_resources(self):
        with mock.patch.object(RUNNER.subprocess, 'check_output') as launch:
            for option in ('--timeout', '--ready-timeout', '--hidden-dwell'):
                for value in ('nan', 'inf', '-inf', '0', '-1'):
                    with self.assertRaises(SystemExit) as error:
                        RUNNER.main(['unused', '--result', 'unused', '--profile-label', 'test', option + '=' + value])
                    self.assertEqual(error.exception.code, 2)
            launch.assert_not_called()
            with self.assertRaises(SystemExit):
                RUNNER.main(['unused', '--result', 'unused', '--profile-label', 'test', '--hidden-dwell=121'])

    def test_finalization_never_waives_safari_long_tasks(self):
        def result():
            return {'completed': True, 'runnerErrors': [], 'longTaskSupported': True,
                    'budgets': {'query': True, 'workerWasm': True, 'mainThread': True, 'foreground': True},
                    'visibility': {'qualifiedForeground': True}}
        value = result()
        self.assertEqual(RUNNER.finalize(value), 0)
        value = result()
        value.update(longTaskSupported=False)
        value['budgets']['mainThread'] = None
        self.assertEqual(RUNNER.finalize(value), 2)
        self.assertFalse(value['ok'])
        for change in ({'completed': False}, {'error': 'harness failure'},
                       {'runnerErrors': [{'where': 'cleanup'}]}, {'longTaskSupported': None}):
            value = result()
            value.update(change)
            self.assertEqual(RUNNER.finalize(value), 1)
        for budget in ('query', 'workerWasm', 'mainThread', 'foreground'):
            value = result()
            value['budgets'][budget] = False
            self.assertEqual(RUNNER.finalize(value), 1)
        value = result()
        value.update(longTaskSupported=False, budgets={'query': True, 'workerWasm': True})
        self.assertEqual(RUNNER.finalize(value), 1)
        value['budgets'] = None
        self.assertEqual(RUNNER.finalize(value), 1)

    def test_visibility_gate_cannot_be_replaced_by_requested_context(self):
        visible = {'elapsedMs': 1, 'visibilityState': 'visible', 'hasFocus': False}
        hidden = {'elapsedMs': 2, 'visibilityState': 'hidden', 'hasFocus': False}
        def value():
            return {'completed': True, 'runnerErrors': [], 'visibility': {
                'invocation': 'visible-request', 'checkpoints': {'queriesBegin': visible, 'queriesEnd': visible},
                'samples': [{'index': i, 'before': visible, 'after': visible} for i in range(110)],
                'changes': [], 'droppedChanges': 0}}
        self.assertTrue(RUNNER.visibility_summary(value())['qualifiedForeground'])
        for indices in ([0], list(range(110))):
            result = value()
            for index in indices:
                result['visibility']['samples'][index]['after'] = hidden
            self.assertFalse(RUNNER.visibility_summary(result)['qualifiedForeground'])
        result = value()
        result['visibility']['changes'] = [{'phase': 'queries', **hidden}]
        self.assertFalse(RUNNER.visibility_summary(result)['qualifiedForeground'])
        result = value()
        result['visibility']['samples'].pop()
        self.assertFalse(RUNNER.visibility_summary(result)['qualifiedForeground'])
        result = value()
        result['visibility']['droppedChanges'] = 1
        self.assertFalse(RUNNER.visibility_summary(result)['qualifiedForeground'])
        result = value()
        result['visibility']['invocation'] = 'hidden-request'
        self.assertFalse(RUNNER.visibility_summary(result)['qualifiedForeground'])
        result = value()
        result['visibility']['checkpoints']['queriesEnd'] = hidden
        self.assertFalse(RUNNER.visibility_summary(result)['qualifiedForeground'])
        # Summary emits only aggregate state/counts, never unrelated page values.
        result = value()
        result['visibility']['privateQuery'] = 'PRIVATE'
        self.assertNotIn('PRIVATE', json.dumps(RUNNER.visibility_summary(result)))

    def test_instrumentation_is_ordinal_and_outside_query_timer(self):
        text = (ROOT / 'tests/browser/performance-rolegraph.html').read_text()
        self.assertIn('window.startPerformance', text)
        self.assertLess(text.index('const visibilityBefore = observeVisibility();'), text.index('const documents = await client.query'))
        self.assertLess(text.index('const ms = performance.now() - started;'), text.index('const visibilityAfter = observeVisibility();'))
        self.assertIn('samples.push({index: i, before: visibilityBefore, after: visibilityAfter})', text)
        self.assertNotIn('visibilityBefore.term', text)

    def test_poll_timeout_and_request_helper(self):
        with mock.patch.object(RUNNER.time, 'monotonic', side_effect=[0, 0, 2, 2]), \
                mock.patch.object(RUNNER.time, 'sleep'), \
                mock.patch.object(RUNNER.SAFARI, 'webdriver', return_value=None) as request:
            with self.assertRaisesRegex(RuntimeError, 'polling timeout'):
                RUNNER.wait_value('http://127.0.0.1:1', '/session/test', 'return null;', 1)
            self.assertEqual(request.call_count, 1)
        with mock.patch.object(RUNNER.SAFARI, 'webdriver', side_effect=RuntimeError('HTTP failed')):
            with self.assertRaisesRegex(RuntimeError, 'HTTP failed'):
                RUNNER.wait_value('http://127.0.0.1:1', '/session/test', 'return null;', 1)

    def test_all_cleanup_actions_attempted(self):
        server, thread = mock.Mock(), mock.Mock()
        server.shutdown.side_effect = RuntimeError('shutdown failure')
        server.server_close.side_effect = RuntimeError('close failure')
        thread.is_alive.return_value = True
        errors = []
        RUNNER.stop_server(server, thread, lambda where, error: errors.append(where))
        self.assertEqual(errors, ['server.shutdown', 'server.close', 'server.join'])
        thread.join.assert_called_once_with(timeout=10)

    def test_driver_timeout_kills_and_reports_failure(self):
        driver = mock.Mock()
        driver.poll.return_value = None
        driver.wait.side_effect = [RUNNER.subprocess.TimeoutExpired('driver', 10), 0]
        with self.assertRaisesRegex(RuntimeError, 'required kill'):
            RUNNER.stop_driver(driver)
        driver.terminate.assert_called_once()
        driver.kill.assert_called_once()
        self.assertEqual(driver.wait.call_count, 2)

    def test_driver_terminate_failure_still_attempts_kill(self):
        driver = mock.Mock()
        driver.poll.return_value = None
        driver.terminate.side_effect = RuntimeError('terminate failed')
        with self.assertRaisesRegex(RuntimeError, 'terminate failed'):
            RUNNER.stop_driver(driver)
        driver.kill.assert_called_once()
        driver.wait.assert_called_once_with(timeout=10)

    def test_failure_writes_non_green_evidence(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / 'result.json'
            with mock.patch.object(RUNNER.platform, 'platform', return_value='test macOS'), \
                    mock.patch.object(RUNNER.subprocess, 'check_output', side_effect=RuntimeError('driver unavailable')):
                code = RUNNER.main(['unused', '--result', str(output), '--profile-label', 'mixed dev/release provisional synthetic'])
            value = json.loads(output.read_text())
            self.assertEqual(code, 1)
            self.assertEqual(value['exitCode'], 1)
            self.assertFalse(value['ok'])
            self.assertTrue(value['runnerErrors'])

    def test_main_resource_failures_cleanup_and_write_evidence(self):
        # Entirely runner semantics: these doubles are NOT measured engine passes.
        for fault in ('cleanup', 'capability', 'post-measurement-drift', 'window-operation'):
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                output = pathlib.Path(directory) / 'result.json'
                driver, server, thread = mock.Mock(), mock.Mock(), mock.Mock()
                driver.poll.return_value = None
                server.server_port = 123
                thread.is_alive.return_value = False
                calls = []
                def request(base, method, route, payload=None):
                    calls.append((method, route))
                    if fault == 'window-operation' and route.endswith('/window/new'):
                        raise RuntimeError('unsupported new window')
                    if route == '/session':
                        return {'sessionId': 'test', 'capabilities': {'browserName': 'safari',
                            'platformName': 'ios' if fault == 'capability' else 'mac'}}
                    return {}
                def cleanup_fault():
                    raise RuntimeError('injected cleanup failure')
                with mock.patch.object(RUNNER.platform, 'platform', return_value='test macOS'), \
                        mock.patch.object(RUNNER.subprocess, 'check_output', return_value='test driver'), \
                        mock.patch.object(RUNNER.subprocess, 'Popen', return_value=driver), \
                        mock.patch.object(RUNNER, 'stage_assets'), \
                        mock.patch.object(RUNNER.http.server, 'ThreadingHTTPServer', return_value=server), \
                        mock.patch.object(RUNNER.threading, 'Thread', return_value=thread), \
                        mock.patch.object(RUNNER.SAFARI, 'webdriver', side_effect=request), \
                        mock.patch.object(RUNNER.SAFARI, 'verify_stage', side_effect=
                            RuntimeError('staged drift') if fault == 'post-measurement-drift' else None), \
                        mock.patch.object(RUNNER, 'wait_value', return_value={'completed': True,
                            'longTaskSupported': False, 'budgets': {'query': True, 'workerWasm': True, 'mainThread': None}}):
                    code = RUNNER.main(['unused', '--result', str(output), '--profile-label', 'runner test',
                                        '--visibility-context', 'hidden-request' if fault == 'window-operation' else 'visible'],
                                       cleanup_hook=cleanup_fault if fault == 'cleanup' else None)
                value = json.loads(output.read_text())
                self.assertEqual(code, 1)
                self.assertFalse(value['ok'])
                self.assertTrue(value['runnerErrors'])
                self.assertIn(('DELETE', '/session/test'), calls)
                driver.terminate.assert_called_once()
                driver.wait.assert_called_once_with(timeout=10)
                server.shutdown.assert_called_once()
                server.server_close.assert_called_once()
                thread.join.assert_called_once_with(timeout=10)

    def test_staging_manifest_identical_to_existing_performance_runner(self):
        with tempfile.TemporaryDirectory() as directory:
            base = pathlib.Path(directory)
            delivery = base / 'delivery'
            for name in ('js/rolegraph-worker.js', 'terraphim-editor.min.js',
                         'rolegraph/terraphim_rolegraph_browser.js', 'rolegraph/terraphim_rolegraph_browser_bg.wasm'):
                path = delivery / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'runner unit test asset, not an engine')
            first, second = {}, {}
            RUNNER.PERF.stage_assets(delivery, ROOT, base / 'existing', first)
            RUNNER.stage_assets(delivery, base / 'safari', second)
            self.assertEqual(first['assets'], second['assets'])
            self.assertEqual(first['inputManifestSha256'], second['inputManifestSha256'])
            self.assertEqual(set(second['helperDependencies']),
                             {'rolegraph-safari-check.py', 'rolegraph-performance-check.py'})
            for name, asset in second['assets'].items():
                self.assertEqual(RUNNER.hashlib.sha256((base / 'safari' / name).read_bytes()).hexdigest(), asset['sha256'])
            def drift(stage):
                (stage / 'js/rolegraph-worker.js').write_bytes(b'drift')
            with self.assertRaisesRegex(RuntimeError, 'Staged input hash drift'):
                RUNNER.stage_assets(delivery, base / 'drift', {}, drift)

    def test_harness_source_drift_rejected(self):
        with mock.patch.object(RUNNER.PERF, 'stage_assets'), \
                mock.patch.object(RUNNER, 'identity', side_effect=[(1,), (1,),
                    *[item['identity'] for item in RUNNER.DEPENDENCIES.values()], (2,)]):
            with self.assertRaisesRegex(RuntimeError, 'harness changed'):
                RUNNER.stage_assets(pathlib.Path('unused'), pathlib.Path('unused'), {})


if __name__ == '__main__':
    unittest.main()
