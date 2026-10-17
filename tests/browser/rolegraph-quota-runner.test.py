#!/usr/bin/env python3
"""Quota runner control-plane regressions; no mock IndexedDB/graph engines."""
import hashlib
import importlib.util
import json
import os
import pathlib
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location('quota_runner', ROOT / 'scripts/ci/rolegraph-quota-check.py')
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class QuotaRunnerRegression(unittest.TestCase):
    def evidence(self):
        # Pure predicate inputs, never injected into a live browser/qualification.
        return {'baselineSaved': True, 'previousBytes': 1000, 'retainedBytes': 1000,
                'previousSha256': 'a'*64, 'retainedSha256': 'a'*64,
                'refusal': 'storage_failed', 'previousCheckpointRetained': True,
                'unsavedGraphAvailable': True, 'mainRoleAssociated': True,
                'database': 'te-rolegraph:test', 'cacheWaitMs': 33000,
                'quotaDiagnostics': {'afterOverride': {'overrideActive': True, 'quota': 1},
                                     'afterWait': {'overrideActive': True, 'quota': 1}},
                'transactionEvents': [{'phase': 'measured-save', 'database': 'te-rolegraph:test',
                    'eventType': 'abort', 'targetKind': 'transaction', 'transactionMode': 'readwrite',
                    'transactionId': 1, 'errorName': 'QuotaExceededError', 'bubbles': True}]}

    def test_commit_abort_is_real_required_telemetry(self):
        self.assertTrue(RUNNER.qualify(self.evidence()))
        r = self.evidence()
        r['transactionEvents'][0].update(eventType='error', targetKind='request')
        self.assertTrue(RUNNER.qualify(r))

    def test_unrelated_or_malformed_events_cannot_qualify(self):
        for change in [{'phase':'baseline'}, {'database':'other'}, {'transactionMode':'readonly'},
                       {'errorName':'AbortError'}, {'eventType':'complete'}, {'bubbles':False},
                       {'transactionId':None}, {'targetKind':'database'}]:
            with self.subTest(change=change):
                r = self.evidence(); r['transactionEvents'][0].update(change)
                self.assertFalse(RUNNER.qualify(r))

    def test_every_acceptance_condition_required(self):
        for key, value in [('refusal', None), ('previousCheckpointRetained', False),
                           ('unsavedGraphAvailable', False), ('mainRoleAssociated', False),
                           ('cacheWaitMs', 29999), ('transactionEvents', []), ('baselineSaved', False),
                           ('previousBytes', 0), ('retainedBytes', 999), ('retainedSha256', 'b'*64)]:
            r = self.evidence(); r[key] = value
            self.assertFalse(RUNNER.qualify(r), key)
        for label in ['afterOverride', 'afterWait']:
            r = self.evidence(); r['quotaDiagnostics'][label]['overrideActive'] = False
            self.assertFalse(RUNNER.qualify(r))
        r = self.evidence(); r['runnerErrors'] = [{'error':'cleanup failed'}]
        self.assertFalse(RUNNER.qualify(r))

    def test_actual_server_cleanup_failure_continues_and_writes_failure_evidence(self):
        result = {'ok': True}
        server = ThreadingHTTPServer(('127.0.0.1', 0), SimpleHTTPRequestHandler)
        thread = threading.Thread(target=server.serve_forever, daemon=True); thread.start()
        completed = []
        def failing_shutdown():
            server.shutdown(); completed.append('shutdown')
            raise RuntimeError('injected after real server.shutdown')
        RUNNER.cleanup_step(result, 'shutdown', failing_shutdown)
        RUNNER.cleanup_step(result, 'socket', lambda: (server.server_close(), completed.append('socket')))
        RUNNER.cleanup_step(result, 'thread', lambda: (thread.join(), completed.append('thread')))
        self.assertEqual(completed, ['shutdown', 'socket', 'thread'])
        self.assertFalse(thread.is_alive())
        with tempfile.TemporaryDirectory() as temporary:
            output = pathlib.Path(temporary) / 'failure.json'
            self.assertEqual(RUNNER.write_result(result, output), 1)
            evidence = json.loads(output.read_text())
            self.assertFalse(evidence['ok']); self.assertEqual(evidence['exitCode'], 1)
            self.assertIn('injected', evidence['runnerErrors'][0]['error'])

    def test_manifest_covers_inline_host_and_exact_staged_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            source = pathlib.Path(temporary) / 'source'; source.mkdir()
            (source/'index.html').write_text('<script>inline client and storage</script>')
            (source/'root-hash.js').write_bytes(b'root-glue')
            (source/'root-hash_bg.wasm').write_bytes(b'root-wasm')
            (source/'rolegraph').mkdir(); (source/'js').mkdir()
            for name in ['rolegraph/terraphim_rolegraph_browser.js',
                         'rolegraph/terraphim_rolegraph_browser_bg.wasm', 'js/rolegraph-worker.js']:
                (source/name).write_bytes(name.encode())
            stage = pathlib.Path(temporary) / 'stage'; stage.mkdir()
            metadata = {}; RUNNER.stage_delivery(source, stage, metadata)
            assets = metadata['assets']
            for name, asset in assets.items():
                self.assertEqual(asset['sha256'], hashlib.sha256((stage/name).read_bytes()).hexdigest())
            self.assertIn('index.html', assets); self.assertIn('root-hash.js', assets)
            self.assertIn('rolegraph/terraphim_rolegraph_browser.js', assets)
            manifest = json.dumps(assets, sort_keys=True, separators=(',', ':')).encode()
            self.assertEqual(metadata['inputManifestSha256'], hashlib.sha256(manifest).hexdigest())
            def drift(directory):
                (directory/'index.html').write_text('changed inline storage')
            with self.assertRaisesRegex(RuntimeError, 'hash drift'):
                RUNNER.stage_delivery(source, stage, {}, stage_hook=drift)

    @unittest.skipUnless(os.environ.get('ROLEGRAPH_QUOTA_LIVE_EXECUTABLE'), 'opt-in real Chrome; waits 33 seconds')
    def test_live_cleanup_fault_cannot_mask_actual_quota_failure(self):
        executable = os.environ['ROLEGRAPH_QUOTA_LIVE_EXECUTABLE']
        delivery = os.environ.get('ROLEGRAPH_QUOTA_LIVE_DELIVERY', str(ROOT/'target/rolegraph-delivery'))
        with tempfile.TemporaryDirectory(prefix='rolegraph-quota-live-regression-') as temporary:
            output = pathlib.Path(temporary)/'cleanup-failure.json'
            code = RUNNER.main([delivery, '--executable', executable, '--result', str(output)], cleanup_fault='quota.reset')
            evidence = json.loads(output.read_text())
            self.assertTrue(evidence['completed'])
            self.assertEqual(evidence['refusal'], 'storage_failed')
            self.assertTrue(evidence['previousCheckpointRetained'])
            self.assertEqual(code, 1); self.assertEqual(evidence['exitCode'], 1)
            self.assertFalse(evidence['ok'])
            self.assertFalse(evidence['quotaDiagnostics']['afterReset']['overrideActive'])
            later = {'telemetry.remove','editor.destroy','namespace.delete','context.close','browser.close',
                     'server.shutdown','server.close','server.thread.join'}
            self.assertTrue(later.issubset({step['step'] for step in evidence['cleanup'] if step['ok']}))
            self.assertTrue(any(error['where']=='quota.reset' and 'injected' in error['error'] for error in evidence['runnerErrors']))


if __name__ == '__main__':
    unittest.main()
