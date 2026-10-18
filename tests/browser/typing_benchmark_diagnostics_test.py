"""Helper safety regressions only: synthetic files, no browser/build launch."""

import hashlib
import contextlib
import io
import os
from pathlib import Path
import tempfile
import types
import unittest
from unittest.mock import patch


REPO = Path(__file__).resolve().parents[2]
SCRATCH = Path('/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode')
HELPER = REPO / 'scripts/ci/typing-benchmark-diagnostics.py'
LOG = 'docs/validation/evidence/rolegraph-browser/20261009/production/rolegraph-production-editor-browser-bench-controlled-rerun.log'
PASS = 'web_bench: ok (29s wall, finished in 15.81s, load 4.06) test result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out\n'
FAKE = 'FAKE_ONLY_PRIVATE_MARKER'


class DiagnosticsTests(unittest.TestCase):
    def setUp(self):
        self.module = types.ModuleType('typing_diagnostics_test_module')
        self.module.__file__ = str(HELPER)
        exec(compile(HELPER.read_text(), str(HELPER), 'exec'), self.module.__dict__)
        self.temp = tempfile.TemporaryDirectory(prefix='typing-diagnostics-test-', dir=SCRATCH)
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.root = self.base / 'repo'
        self.root.mkdir()
        self.module.ROOT = self.root
        self.module.SOURCE_FILES = frozenset({'src/example.rs'})

    def approved_log(self, text, expected=None):
        path = self.root / LOG
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        self.module.APPROVED_LOGS = {LOG: expected or hashlib.sha256(text.encode()).hexdigest()}
        return path

    def reject(self, callback):
        with self.assertRaises(ValueError) as caught:
            callback()
        self.assertNotIn(FAKE, str(caught.exception))
        self.assertNotIn(str(self.base), str(caught.exception))

    def test_source_ancestor_symlink_no_content_read(self):
        private = self.base / 'fake-private'
        private.mkdir()
        (private / 'example.rs').write_text(FAKE)
        (self.root / 'src').symlink_to(private, target_is_directory=True)
        with patch.object(self.module.os, 'read', wraps=os.read) as read:
            self.reject(lambda: self.module.identity(self.root / 'src/example.rs'))
            read.assert_not_called()

    def test_source_internal_ancestor_symlink_rejected(self):
        actual = self.root / 'actual'
        actual.mkdir()
        (actual / 'example.rs').write_text(FAKE)
        (self.root / 'src').symlink_to(actual, target_is_directory=True)
        self.reject(lambda: self.module.identity(self.root / 'src/example.rs'))

    def test_source_leaf_symlink_rejected(self):
        (self.root / 'src').mkdir()
        fake = self.base / 'fake.rs'
        fake.write_text(FAKE)
        (self.root / 'src/example.rs').symlink_to(fake)
        self.reject(lambda: self.module.identity(self.root / 'src/example.rs'))

    def test_source_unapproved_json_rejected_before_read(self):
        path = self.root / 'crates/fake/kg/private.json'
        path.parent.mkdir(parents=True)
        path.write_text(FAKE)
        with patch.object(self.module.os, 'read', wraps=os.read) as read:
            self.reject(lambda: self.module.identity(path))
            read.assert_not_called()

    def test_source_outside_root_rejected(self):
        path = self.base / 'example.rs'
        path.write_text(FAKE)
        self.reject(lambda: self.module.identity(path))

    def test_confined_source_hash(self):
        path = self.root / 'src/example.rs'
        path.parent.mkdir()
        path.write_text('public synthetic source')
        self.assertEqual(self.module.identity(path)['sha256'], hashlib.sha256(path.read_bytes()).hexdigest())

    def test_unapproved_log_with_valid_prefix_rejected_before_read(self):
        path = self.root / 'docs/validation/evidence/web_bench-unapproved.log'
        path.parent.mkdir(parents=True)
        path.write_text(PASS + FAKE)
        with patch.object(self.module.os, 'read', wraps=os.read) as read:
            self.reject(lambda: self.module.retained_log(str(path)))
            read.assert_not_called()

    def test_approved_hash_change_rejected(self):
        path = self.approved_log(PASS)
        path.write_text(PASS + FAKE)
        self.reject(lambda: self.module.retained_log(LOG))

    def test_log_suffix_injection_rejected_even_matching_manifest_hash(self):
        self.approved_log(PASS.rstrip('\n') + ' ' + FAKE + '\n')
        self.reject(lambda: self.module.retained_log(LOG))

    def test_metric_prefix_injection_rejected(self):
        self.approved_log('Typing latency (asserted) ' + FAKE + '\n')
        self.reject(lambda: self.module.retained_log(LOG))

    def test_oversized_log_rejected_before_content_read(self):
        self.approved_log('x' * (1024 * 1024 + 1))
        with patch.object(self.module.os, 'read', wraps=os.read) as read:
            self.reject(lambda: self.module.retained_log(LOG))
            read.assert_not_called()

    def test_evidence_ancestor_symlink_rejected(self):
        path = self.approved_log(PASS)
        actual = self.base / 'fake-evidence'
        path.parent.rename(actual)
        path.parent.symlink_to(actual, target_is_directory=True)
        self.reject(lambda: self.module.retained_log(LOG))

    def test_parsed_log_contains_no_raw_lines(self):
        self.approved_log(PASS)
        result = self.module.retained_log(LOG)
        self.assertNotIn('benchmark_lines', result)
        self.assertEqual(result['metrics'][0]['harness_seconds'], 15.81)

    def test_unavailable_ps_typed(self):
        with patch.object(self.module, 'command', return_value=None):
            result = self.module.telemetry()
        self.assertEqual(result['process_aggregate']['status'], 'NOT_AVAILABLE')
        self.assertIsNone(result['process_aggregate']['metrics'])

    def test_empty_ps_not_zero_measurement(self):
        with patch.object(self.module, 'command', return_value=''):
            result = self.module.telemetry()
        self.assertEqual(result['process_aggregate']['status'], 'NOT_AVAILABLE')
        self.assertIsNone(result['process_aggregate']['metrics'])

    def test_malformed_ps_not_zero_measurement(self):
        with patch.object(self.module, 'command', return_value='malformed'):
            result = self.module.telemetry()
        self.assertEqual(result['process_aggregate']['status'], 'NOT_AVAILABLE')

    def test_ps_aggregate_numeric_only(self):
        with patch.object(self.module, 'command', return_value='12.5 100\n0.0 200\n'):
            result = self.module.telemetry()
        self.assertEqual(result['process_aggregate']['metrics']['pcpu_sum'], 12.5)
        self.assertEqual(result['process_aggregate']['metrics']['rss_kib_sum'], 300)

    def test_vm_stat_numeric_parser(self):
        raw = 'Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free: 12.\nPages active: 34.\n'
        with patch.object(self.module.platform, 'system', return_value='Darwin'), patch.object(self.module, 'command', side_effect=['0.1 100\n', raw, '17179869184\n']):
            result = self.module.telemetry()
        self.assertEqual(result['vm_stat']['metrics']['page_size_bytes'], 16384)
        self.assertEqual(result['vm_stat']['metrics']['pages_free'], 12)

    def test_vm_stat_unavailable_typed(self):
        with patch.object(self.module.platform, 'system', return_value='Darwin'), patch.object(self.module, 'command', return_value=None):
            result = self.module.telemetry()
        self.assertEqual(result['vm_stat']['status'], 'NOT_AVAILABLE')

    def test_wait_noise_validated_not_emitted(self):
        self.approved_log('  web_bench: waiting for load 5.98 to drop below 5 (0s of 300s); busiest: Telegram 125.7%, WindowServer 52.9%\n' + PASS)
        result = self.module.retained_log(LOG)
        self.assertEqual(len(result['metrics']), 1)
        self.assertNotIn('Telegram', str(result))

    def test_root_ancestor_symlink_rejected(self):
        actual = self.base / 'actual-root'
        self.root.rename(actual)
        self.root.symlink_to(actual, target_is_directory=True)
        (actual / 'src').mkdir()
        (actual / 'src/example.rs').write_text(FAKE)
        self.reject(lambda: self.module.identity(self.root / 'src/example.rs'))

    def test_symlink_substitution_after_metadata_rejected(self):
        path = self.root / 'src/example.rs'
        path.parent.mkdir()
        path.write_text('public synthetic source')
        private = self.base / 'fake-private'
        private.mkdir()
        (private / 'example.rs').write_text(FAKE)
        original_open = os.open
        def substitute(name, flags, *args, **kwargs):
            if name == 'src':
                path.parent.rename(self.root / 'original-src')
                path.parent.symlink_to(private, target_is_directory=True)
            return original_open(name, flags, *args, **kwargs)
        with patch.object(self.module.os, 'open', side_effect=substitute), patch.object(self.module.os, 'read', wraps=os.read) as read:
            self.reject(lambda: self.module.identity(path))
            read.assert_not_called()

    def test_cli_unapproved_log_no_commands_or_stdout(self):
        output = io.StringIO()
        with patch.object(self.module.sys, 'argv', ['helper', '--run-id', 'synthetic', '--log', 'web_bench;fake.log']), patch.object(self.module, 'command') as command, contextlib.redirect_stdout(output):
            self.reject(self.module.main)
            command.assert_not_called()
        self.assertEqual(output.getvalue(), '')

    def test_incomplete_telemetry_nonzero_collection_status(self):
        output = io.StringIO()
        with patch.object(self.module.sys, 'argv', ['helper', '--run-id', 'synthetic']), patch.object(self.module, 'snapshot', return_value=({}, {'complete': False})), patch.object(self.module, 'command', return_value=None), contextlib.redirect_stdout(output):
            self.assertEqual(self.module.main(), 3)
        self.assertIn('METADATA_INCOMPLETE', output.getvalue())

    def test_untrusted_cli_argument_error_does_not_echo(self):
        output = io.StringIO()
        with patch.object(self.module.sys, 'argv', ['helper', '--run-id', 'synthetic', '--allow-path', FAKE]), contextlib.redirect_stderr(output):
            self.reject(self.module.main)
        self.assertEqual(output.getvalue(), '')


if __name__ == '__main__':
    unittest.main()
