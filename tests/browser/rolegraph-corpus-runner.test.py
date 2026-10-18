"""Capture regressions; no browser/graph substitutes."""
import importlib.util
import json
import pathlib
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('corpus', ROOT / 'scripts/ci/rolegraph-corpus-check.py')
corpus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corpus)


class CaptureTests(unittest.TestCase):
    def test_utf8_and_opaque_capture(self):
        with tempfile.TemporaryDirectory(prefix='rolegraph-corpus-test-', dir=corpus.TEMP) as directory:
            root = pathlib.Path(directory)
            (root / 'private-title.md').write_bytes('知識\r\n'.encode())
            thesaurus = root / 'terms.json'
            thesaurus.write_text(json.dumps({'name': 'private', 'data': {'term': {'id': 1, 'nterm': 'term'}}}))
            payload, evidence = corpus.capture_profile(root, thesaurus, 'kg')
            self.assertEqual(evidence['sourceBytes'], len('知識\r\n'.encode()))
            self.assertEqual(payload['documents'][0]['id'], 'kg-0000')
            self.assertEqual(payload['documents'][0]['title'], '')
            self.assertNotIn('private', json.dumps(evidence))
            self.assertNotIn('知識', json.dumps(evidence, ensure_ascii=False))

    def test_limits_use_native_utf8_bytes(self):
        with self.assertRaises(ValueError):
            corpus.validate_documents([{'id': 'x', 'title': '', 'body': '知' * (1024 * 1024 // 3 + 1)}])
        with self.assertRaises(ValueError):
            corpus.validate_documents([{'id': str(i), 'title': '', 'body': ''} for i in range(1001)])
        with self.assertRaises(ValueError):
            corpus.validate_documents([{'id': str(i), 'title': '', 'body': 'x' * 1024**2} for i in range(17)])

    def test_empty_corpus_rejected(self):
        with self.assertRaises(ValueError):
            corpus.validate_documents([])

    def test_recursive_capture_and_no_source_mutation(self):
        with tempfile.TemporaryDirectory(prefix='rolegraph-corpus-test-', dir=corpus.TEMP) as directory:
            root = pathlib.Path(directory)
            (root / 'nested').mkdir()
            path = root / 'nested' / 'private.md'
            path.write_bytes(b'unchanged')
            thesaurus = root / 'terms.json'
            thesaurus.write_text(json.dumps({'name': 'private', 'data': {'term': {'id': 1, 'nterm': 'term'}}}))
            payload, evidence = corpus.capture_profile(root, thesaurus, 'engineer')
            self.assertEqual(evidence['documents'], 1)
            self.assertEqual(path.read_bytes(), b'unchanged')
            self.assertEqual(payload['documents'][0]['id'], 'engineer-0000')

    def test_staged_drift_rejected_using_actual_delivery(self):
        delivery = ROOT / 'target/rolegraph-delivery'
        if not delivery.exists():
            self.skipTest('already-built real delivery unavailable')
        original = (delivery / 'js/rolegraph-worker.js').read_bytes()
        def drift(stage):
            with (stage / 'js/rolegraph-worker.js').open('ab') as stream:
                stream.write(b'// drift')
        with tempfile.TemporaryDirectory(prefix='rolegraph-corpus-test-', dir=corpus.TEMP) as directory:
            with self.assertRaises(ValueError):
                corpus.stage_assets(delivery, ROOT, pathlib.Path(directory), drift)
        self.assertEqual((delivery / 'js/rolegraph-worker.js').read_bytes(), original)


if __name__ == '__main__':
    unittest.main()
