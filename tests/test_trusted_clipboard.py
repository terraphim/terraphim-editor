"""Real recorded Gecko output parser tests; historical fixtures never admit CI."""
import copy
import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('trusted_clipboard', ROOT / 'scripts/qualify-trusted-clipboard.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


class TrustedClipboardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / 'firefox'
        shutil.copytree(ROOT / 'tests/fixtures/trusted-clipboard-gecko', self.output)
        self.receipt = json.loads((self.output / 'clipboard-receipt.json').read_text())
        self.runtime = json.loads((self.output / 'runtime-assets.json').read_text())
        self.manifest = {'assets': self.runtime['served']}

    def write(self):
        (self.output / 'clipboard-receipt.json').write_text(json.dumps(self.receipt))
        (self.output / 'runtime-assets.json').write_text(json.dumps(self.runtime))

    def test_actual_historical_gecko_outputs_validate_only_intrinsic_proof(self):
        value = M.validate_browser(self.output, 'firefox', self.manifest, self.runtime['manifestSha256'])
        self.assertEqual(value['pastedSha256'], M.digest(self.output / 'actual-pasted.txt'))
        self.assertEqual(value['copiedSha256'], M.digest(self.output / 'actual-copied.txt'))
        self.assertEqual(value['browser'], '153.0')

    def test_recorded_gecko_cannot_be_relabelled_chrome(self):
        with self.assertRaises(ValueError):
            M.validate_browser(self.output, 'chrome', self.manifest, self.runtime['manifestSha256'])

    def test_missing_copy_or_changed_actual_outputs_refused(self):
        for name in ('actual-copied.txt', 'actual-pasted.txt'):
            old = (self.output / name).read_bytes()
            (self.output / name).write_bytes(b'wrong actual output')
            with self.subTest(name=name), self.assertRaises(ValueError):
                M.validate_browser(self.output, 'firefox', self.manifest, self.runtime['manifestSha256'])
            (self.output / name).write_bytes(old)
        (self.output / 'actual-copied.txt').unlink()
        with self.assertRaises((ValueError, OSError)):
            M.validate_browser(self.output, 'firefox', self.manifest, self.runtime['manifestSha256'])

    def test_paste_only_untrusted_missing_html_caret_or_rich_nodes_refused(self):
        original = copy.deepcopy(self.receipt)
        changes = [lambda x: x.update(status='PASS_PASTE_ONLY'),
                   lambda x: x['pasteProof'].update(caret=7),
                   lambda x: x['pasteProof'].update(richNodes=1),
                   lambda x: x['pasteProof']['events'][0].update(isTrusted=False),
                   lambda x: x['pasteProof']['events'][0].update(types=['text/plain']),
                   lambda x: x.update(priorClipboardRead=True),
                   lambda x: x.update(priorClipboardRestored=True),
                   lambda x: x.update(firefoxSha256='not a digest')]
        for change in changes:
            self.receipt = copy.deepcopy(original)
            change(self.receipt)
            self.write()
            with self.subTest(receipt=self.receipt), self.assertRaises(ValueError):
                M.validate_browser(self.output, 'firefox', self.manifest, self.runtime['manifestSha256'])

    def test_stale_runtime_or_mutated_served_asset_refused(self):
        original = copy.deepcopy(self.runtime)
        for change in ({'status': 'FAIL'}, {'exitCode': 1}, {'manifestSha256': '0' * 64}, {'mismatches': ['bad']}):
            self.runtime = {**original, **change}
            self.write()
            with self.subTest(change=change), self.assertRaises(ValueError):
                M.validate_browser(self.output, 'firefox', self.manifest, original['manifestSha256'])
        self.runtime = original
        self.runtime['served']['terraphim_editor_bg.wasm'] = '0' * 64
        self.write()
        with self.assertRaises(ValueError):
            M.validate_browser(self.output, 'firefox', self.manifest, original['manifestSha256'])

    def test_old_recorded_receipt_cannot_admit_fresh_ci(self):
        with self.assertRaises((ValueError, OSError)):
            M.verify_admission(self.output, self.output, 'a' * 40, {'runId': '123', 'runAttempt': '1'})

    def test_real_recorded_output_hashes_cannot_bind_stale_source_run_or_candidate(self):
        # Metadata protocol fixture; its recorded bytes do not claim a new run.
        source = 'a' * 40
        expected = {'context': {'runId': '12', 'runAttempt': '1', 'sourceCommit': source},
                    'archiveSha256': self.runtime['manifestSha256'],
                    'manifestSha256': self.runtime['manifestSha256'],
                    'cargoLockSha256': 'b' * 64,
                    'harnesses': {'fixture-scope': 'c' * 64},
                    'outputs': {name: M.digest(self.output / name) for name in ('actual-pasted.txt', 'actual-copied.txt')}}
        report = {'schema': 1, 'status': 'PASS', **expected}
        M.validate_binding(report, expected, source)
        for key in expected:
            changed = copy.deepcopy(report)
            changed[key] = {'stale': True} if isinstance(expected[key], dict) else '0' * 64
            with self.subTest(key=key), self.assertRaises(ValueError):
                M.validate_binding(changed, expected, source)
        with self.assertRaises(ValueError):
            M.validate_binding(report, expected, 'd' * 40)

    def test_run_context_requires_actual_same_repository_workflow_and_attempt(self):
        environment = {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': M.REPOSITORY,
                       'GITHUB_WORKFLOW_REF': M.REPOSITORY + '/.github/workflows/public-root.yml@refs/pull/7/merge',
                       'GITHUB_RUN_ID': '123', 'GITHUB_RUN_ATTEMPT': '1', 'GITHUB_SHA': 'a' * 40}
        self.assertEqual(M.context(environment)['sourceCommit'], 'a' * 40)
        for key, value in [('GITHUB_ACTIONS', 'false'), ('GITHUB_REPOSITORY', 'other/repository'),
                           ('GITHUB_WORKFLOW_REF', M.REPOSITORY + '/.github/workflows/untrusted.yml@main'),
                           ('GITHUB_RUN_ID', ''), ('GITHUB_RUN_ATTEMPT', '0')]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                M.context({**environment, key: value})


class OwnedProcessTests(unittest.TestCase):
    """Real subprocess lifecycle checks, not mocked browser/native outcomes."""
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)

    def assert_not_running(self, pid):
        end = time.monotonic() + 2
        while time.monotonic() < end:
            status = subprocess.run(['ps', '-o', 'stat=', '-p', str(pid)], text=True, capture_output=True)
            if not status.stdout.strip() or status.stdout.strip().startswith('Z'):
                return
            time.sleep(0.025)
        self.fail('Owned subprocess remains running: ' + str(pid))

    def test_timeout_cleans_actual_wrapper_and_child_without_driver_metadata(self):
        script = ("import subprocess,sys,time,pathlib; "
                  "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']); "
                  "pathlib.Path(sys.argv[1]).write_text(str(child.pid));time.sleep(60)")
        child_file = self.folder / 'child.pid'
        with (self.folder / 'timeout.log').open('w') as log:
            with self.assertRaises(subprocess.TimeoutExpired):
                M.run_owned([sys.executable, '-c', script, str(child_file)], ROOT, dict(os.environ),
                            log, self.folder, '/missing-driver', timeout=1)
        self.assert_not_running(int(child_file.read_text()))
        wrapper = json.loads((self.folder / 'owned-wrapper.json').read_text())
        self.assert_not_running(wrapper['processGroup'])
        self.assertFalse((self.folder / 'owned-process.json').exists())

    def test_failed_wrapper_cleans_actual_lingering_child_and_preserves_exit_code(self):
        script = ("import subprocess,sys,pathlib,os; "
                  "child=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)']); "
                  "pathlib.Path(sys.argv[1]).write_text(str(child.pid));os._exit(7)")
        child_file = self.folder / 'child.pid'
        with (self.folder / 'failure.log').open('w') as log:
            actual = M.run_owned([sys.executable, '-c', script, str(child_file)], ROOT, dict(os.environ),
                                 log, self.folder, timeout=5)
        self.assertEqual(actual, 7)
        self.assert_not_running(int(child_file.read_text()))


if __name__ == '__main__':
    unittest.main()
