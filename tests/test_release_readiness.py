"""Release gating against real files and actual retained ledger shapes."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('readiness', Path(__file__).resolve().parents[1] / 'scripts/check-release-readiness.py')
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class ReadinessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / 'editor.wasm').write_bytes(b'actual candidate bytes')
        self.artifact = self.file('editor.wasm') | {'host': 'chrome', 'version': '0.1.0', 'sourceCommit': 'a' * 40}
        self.ledger = {'requirements': [{'id': ident, 'accepted': True, 'status': 'PASS', 'remainingCriteria': '', 'checks': [{'status': 'PASS'}]} for ident in module.REQUIREMENTS]}
        self.ledger['supplemental'] = [{'id': ident, 'status': 'PASS', 'checks': [{'status': 'PASS'}]} for ident in module.SUPPLEMENTAL]
        self.manifest = {'schema': 1, 'releaseId': 'test-candidate', 'channel': 'candidate', 'artifacts': [self.artifact], 'evidence': [], 'limitations': []}
        self.write_ledger()

    def file(self, name):
        raw = (self.root / name).read_bytes()
        return {'path': name, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}

    def write_ledger(self):
        (self.root / 'ledger.json').write_text(json.dumps(self.ledger))
        self.manifest['evidence'] = [self.file('ledger.json') | {'host': 'chrome', 'kind': 'requirements', 'artifactSha256': self.artifact['sha256']}]

    def report(self):
        path = self.root / 'release.json'
        path.write_text(json.dumps(self.manifest))
        return module.validate_release(path, self.root)

    def gate(self):
        return module.evaluate_claim(self.report(), 'chrome', 'full-parity')

    def test_intact_candidate_and_complete_attestation(self):
        self.assertTrue(self.report()['valid'])
        self.assertTrue(self.gate()['allowed'])

    def test_actual_historical_ledger_cannot_be_promoted(self):
        actual = Path(__file__).resolve().parent.parent / 'docs/verification/refreshed-demos-20261011/independent-chrome-all-41-refreshed-20261011.json'
        self.ledger = json.loads(actual.read_text())
        self.write_ledger()
        self.assertTrue(self.report()['valid'])
        self.assertFalse(self.gate()['allowed'])

    def test_mutated_artifact_or_evidence_is_refused(self):
        for name in ['editor.wasm', 'ledger.json']:
            with self.subTest(name=name):
                original = (self.root / name).read_bytes()
                (self.root / name).write_bytes(original + b'changed')
                self.assertFalse(self.report()['valid'])
                (self.root / name).write_bytes(original)

    def test_missing_extra_duplicate_requirement_ids_are_denied(self):
        original = copy.deepcopy(self.ledger)
        for mutate in [lambda rows: rows.pop(), lambda rows: rows.append({'id': 'R-99.1'}), lambda rows: rows.append(copy.deepcopy(rows[0]))]:
            self.ledger = copy.deepcopy(original)
            mutate(self.ledger['requirements']); self.write_ledger()
            self.assertFalse(self.gate()['allowed'])

    def test_false_acceptance_fail_partial_and_empty_checks_denied(self):
        for change in [{'accepted': False}, {'status': 'PARTIAL'}, {'checks': [{'status': 'FAIL'}]}, {'checks': []}, {'remainingCriteria': 'Native drag not executed'}]:
            with self.subTest(change=change):
                original = copy.deepcopy(self.ledger)
                self.ledger['requirements'][0].update(change); self.write_ledger()
                self.assertFalse(self.gate()['allowed'])
                self.ledger = original

    def test_unsafe_paths_and_symlinks_refused(self):
        for name in ['../editor.wasm', '/editor.wasm', 'a/../../editor.wasm', 'https://example.com/file', 'a\\b']:
            self.manifest['artifacts'][0]['path'] = name
            self.assertFalse(self.report()['valid'])
        (self.root / 'link').symlink_to(self.root, target_is_directory=True)
        self.manifest['artifacts'][0]['path'] = 'link/editor.wasm'
        self.assertFalse(self.report()['valid'])

    def test_missing_files_wrong_types_and_binding_denied(self):
        original = copy.deepcopy(self.manifest)
        for change in [{'bytes': True}, {'sha256': 'not a hash'}, {'path': 'absent.wasm'}, {'sourceCommit': 'branch'}]:
            self.manifest = copy.deepcopy(original); self.manifest['artifacts'][0].update(change)
            self.assertFalse(self.report()['valid'])
        self.manifest = copy.deepcopy(original)
        self.manifest['evidence'][0]['artifactSha256'] = 'b' * 64
        self.assertFalse(self.report()['valid'])

    def test_missing_host_or_ledger_and_explicit_limitation_denied(self):
        self.assertFalse(module.evaluate_claim(self.report(), 'zed', 'full-parity')['allowed'])
        self.manifest['evidence'] = []
        self.assertFalse(self.gate()['allowed'])
        self.write_ledger()
        self.manifest['limitations'] = [{'host': 'chrome', 'requirement': 'R-6.4', 'status': 'OPEN', 'issue': 'https://example.com/issue'}]
        self.assertFalse(self.gate()['allowed'])

    def test_malformed_ledger_values_fail_closed(self):
        original = copy.deepcopy(self.ledger)
        for value in [[], None, {'requirements': [{'id': []}]}, {'requirements': [dict(original['requirements'][0], remainingCriteria=None)]}]:
            self.ledger = value; self.write_ledger()
            self.assertFalse(self.gate()['allowed'])

    def test_unknown_integrity_host_and_missing_supplemental_are_denied(self):
        self.assertFalse(module.evaluate_claim(self.report(), 'zed', 'integrity')['allowed'])
        self.ledger.pop('supplemental'); self.write_ledger()
        self.assertFalse(self.gate()['allowed'])

    def test_unknown_claim_fails_closed(self):
        self.assertFalse(module.evaluate_claim(self.report(), 'chrome', 'stable')['allowed'])


if __name__ == '__main__':
    unittest.main()
