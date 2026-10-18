"""Runner regressions only; these do not stand in for actual Safari evidence."""
import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest import mock

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('safari_runner', ROOT / 'scripts/ci/rolegraph-safari-check.py')
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class RunnerTests(unittest.TestCase):
    def test_session_request_requires_mac_without_remote_host(self):
        self.assertEqual(RUNNER.session_request(), {'capabilities': {
            'alwaysMatch': {'browserName': 'safari', 'platformName': 'mac'}}})

    def test_returned_platform_required_and_validated(self):
        for name in ['mac', 'Mac', 'macOS', 'MACOS']:
            RUNNER.validate_capabilities({'browserName': 'Safari', 'platformName': name})
        for name in [None, '', 'ios', 'linux', 'windows', 'mac OS X', 7]:
            with self.assertRaises(RuntimeError):
                RUNNER.validate_capabilities({'browserName': 'safari', 'platformName': name})
        with self.assertRaises(RuntimeError):
            RUNNER.validate_capabilities({'browserName': 'webkit', 'platformName': 'mac'})
        with self.assertRaises(RuntimeError):
            RUNNER.validate_capabilities({'browserName': 'safari'})

    def test_nonfinite_and_nonpositive_timeout_rejected_before_resources(self):
        with mock.patch.object(RUNNER, 'stage_assets') as stage, \
                mock.patch.object(RUNNER.subprocess, 'check_output') as driver, \
                mock.patch.object(RUNNER.subprocess, 'Popen', side_effect=RuntimeError('unexpected launch')) as launch:
            for timeout in ['nan', 'inf', '-inf', '0', '-1']:
                with self.assertRaises(SystemExit) as error:
                    RUNNER.main(['unused', '--result', 'unused.json', '--profile-label', 'test',
                                 '--timeout=' + timeout])
                self.assertEqual(error.exception.code, 2)
            stage.assert_not_called()
            driver.assert_not_called()
            launch.assert_not_called()

    def test_failed_case_retains_only_controlled_diagnostics(self):
        cases = {}
        value = {'ok': False, 'checks': [], 'code': 'storage_failed',
                 'error': 'Error: arbitrary PRIVATE corpus and snapshot',
                 'userAgent': 'arbitrary PRIVATE', 'snapshot': 'PRIVATE'}
        with self.assertRaises(RuntimeError):
            RUNNER.record_case(cases, 'delivery', value, 27)
        self.assertEqual(cases['delivery'], {'ok': False, 'count': 0, 'expectedCount': 27,
            'fixture': 'delivery', 'code': 'storage_failed', 'errorClass': 'typed-storage_failed'})
        self.assertNotIn('PRIVATE', json.dumps(cases))
        with self.assertRaises(RuntimeError):
            RUNNER.record_case(cases, 'originalWorker', {'ok': False,
                'error': 'Error: WASM initialization failed', 'code': 'PRIVATE'}, 33)
        self.assertEqual(cases['originalWorker']['errorClass'], 'wasm-initialization')
        self.assertEqual(cases['originalWorker']['code'], 'unrecognized')
        with self.assertRaises(RuntimeError):
            RUNNER.record_case(cases, 'reviewWorker', {'ok': False, 'error': 'Error: step 12 query /ok PRIVATE'}, 123)
        self.assertEqual(cases['reviewWorker']['errorClass'], 'worker-assertion')
        self.assertNotIn('PRIVATE', json.dumps(cases))
        with self.assertRaises(RuntimeError):
            RUNNER.record_case(cases, 'originalWorker', {'ok': False, 'steps': 'PRIVATE',
                'error': 'PRIVATE' * 10000}, 33)
        self.assertEqual(cases['originalWorker']['errorClass'], 'harness-failure')
        self.assertIsNone(cases['originalWorker']['count'])
        self.assertLess(len(json.dumps(cases['originalWorker'])), 200)
        self.assertNotIn('PRIVATE', json.dumps(cases))

    def test_counts_and_failed_results_cannot_pass(self):
        for label, _, _, count in RUNNER.CASES:
            value = {'ok': True, 'steps': count, 'checks': ['check'] * count}
            self.assertTrue(RUNNER.aggregate(label, value, count)['ok'])
            with self.assertRaises(RuntimeError):
                RUNNER.aggregate(label, value, count + 1)
            value['ok'] = False
            with self.assertRaises(RuntimeError):
                RUNNER.aggregate(label, value, count)

    def test_cleanup_failure_does_not_skip_other_resources(self):
        calls, errors = [], []
        def failing():
            calls.append('first')
            raise RuntimeError('cleanup failed')
        RUNNER.cleanup([('first', failing), ('second', lambda: calls.append('second'))],
                       lambda where, error: errors.append(where))
        self.assertEqual(calls, ['first', 'second'])
        self.assertEqual(errors, ['first'])

    def test_cleanup_error_overrides_completed_green_cases(self):
        result = {'completed': True, 'runnerErrors': [], 'cases': {
            label: {'ok': True, 'count': count, 'expectedCount': count}
            for label, _, _, count in RUNNER.CASES}}
        self.assertEqual(RUNNER.finalize(result), 0)
        def failing():
            raise RuntimeError('cleanup failed after successful execution')
        RUNNER.cleanup([('session.delete', failing)],
            lambda where, error: result['runnerErrors'].append({'where': where, 'error': str(error)}))
        self.assertEqual(RUNNER.finalize(result), 1)
        self.assertFalse(result['ok'])

    def test_staging_hash_drift_rejected_without_delivery_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            base = pathlib.Path(directory)
            delivery = base / 'delivery'
            names = ['terraphim_editor_bg.wasm', 'terraphim-editor.min.js', 'js/rolegraph-worker.js',
                     'rolegraph/terraphim_rolegraph_browser.js', 'rolegraph/terraphim_rolegraph_browser_bg.wasm']
            for name in names:
                path = delivery / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'qualification test bytes')
            before = {name: (delivery / name).read_bytes() for name in names}
            def drift(stage):
                (stage / names[0]).write_bytes(b'drift')
            with self.assertRaisesRegex(RuntimeError, 'Staged input hash drift'):
                RUNNER.stage_assets(delivery, ROOT, base / 'stage', {}, drift)
            self.assertEqual(before, {name: (delivery / name).read_bytes() for name in names})

    def test_missing_delivery_is_non_green(self):
        with tempfile.TemporaryDirectory() as directory:
            result = pathlib.Path(directory) / 'result.json'
            code = RUNNER.main([directory, '--result', str(result), '--profile-label', 'regression'])
            evidence = json.loads(result.read_text())
            self.assertEqual(code, 1)
            self.assertFalse(evidence['ok'])
            self.assertTrue(evidence['runnerErrors'])


if __name__ == '__main__':
    unittest.main()
