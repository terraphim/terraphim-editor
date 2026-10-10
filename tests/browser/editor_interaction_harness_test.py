"""Transport/helper tests only: these do not constitute browser acceptance."""
import importlib.util
import copy
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch
from urllib.request import urlopen
from urllib.error import HTTPError

SPEC = importlib.util.spec_from_file_location('interaction', Path(__file__).resolve().parents[2] / 'scripts/ci/editor-interaction-check.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)
HEAD = '6' * 40


class TransportTests(unittest.TestCase):
    def manifest(self, data=b'public'):
        return {'schema': 1, 'sourceHead': HEAD, 'files': [
            {'path': 'index.html', 'sha256': M.digest(data), 'bytes': len(data)}]}

    def admission(self, root):
        # Transport-only approved index member; independent registration tested below.
        return M.Admission(Path(root).absolute(), Path(root).absolute() / 'manifest.json',
                           '0' * 64, HEAD, M.records(self.manifest()['files']), {}, '0' * 64)

    def registration(self, repo):
        root = repo / 'target/editor-acceptance-public/public-stage'
        sources = [{'path': name, 'sha256': '0' * 64, 'bytes': 1} for name in sorted(M.SOURCE_INPUTS)]
        files = self.manifest()['files']
        return {'schema': 1, 'root': str(root), 'manifest': str(root.parent / 'public-stage.manifest.json'),
                'manifestSha256': '0' * 64, 'sourceHead': HEAD, 'files': files,
                'sourceInputs': sources, 'provenance': {'sourceFingerprint': M.fingerprint(M.records(sources)), 'files': files}}

    def test_promotion_projection_and_hidden_crossing(self):
        cuts=lambda ranges:[{'start':a,'end':b} for a,b in ranges]
        self.assertEqual(M.promotion_ranges(cuts([(0,8),(0,8),(2,5),(8,10)])),[(0,8),(8,10)])
        with self.assertRaises(AssertionError): M.promotion_ranges(cuts([(0,20),(2,10),(5,15)]))

    def test_drag_observer_does_not_compute_expected_text_with_product_api(self):
        self.assertNotIn('requireDocumentApi',M.DRAG_OBSERVER)
        self.assertNotIn('overflow_pull_back_text',M.DRAG_OBSERVER)

    def test_independent_overflow_spacing_oracle(self):
        for body,at,text,expected in [('abc def',4,'X','abc Xdef'),('abc',0,'X','X\n\nabc'),('abc',3,'X','abc\n\nX'),('a\n\nb',2,'X','a\n\nX\n\nb'),('',0,'X','X'),('a\r\nb',2,'X','a\r\n\r\nX\r\n\r\nb'),('🦀 a',3,'X','🦀 Xa')]:
            with self.subTest(body=body,at=at):self.assertEqual(M.overflow_insertion(body,at,text),expected)

    def test_export_boundary_cleanup_preserves_unrelated_whitespace(self):
        cases=[('Keep this. Ghost',11,16,'Keep this.'),('# Heading\n\nBody',2,9,'#\n\nBody'),('A aside, B',2,7,'A, B'),('Drop\n\nKeep',0,4,'Keep'),('a  b x c',5,6,'a  b c'),('🦀 aside end',3,9,'🦀 end')]
        for text,start,end,expected in cases:
            g={'anchor':{'start':start,'end':end,'text':M.utf16_slice(text,start,end)}}
            with self.subTest(text=text):self.assertEqual(M.expected_export(text,[g]),expected)

    def test_hash(self):
        self.assertEqual(len(M.digest(b'public')), 64)

    def test_invalid_paths(self):
        for path in ['../private', '/private', 'a/../private', 'a//b', './a', 'a\\b', '%2e%2e/a', '']:
            with self.subTest(path=path), self.assertRaises(ValueError):
                M.relative(path)

    def test_nested_path(self):
        self.assertEqual(M.relative('vendor/public.js'), 'vendor/public.js')

    def test_identity(self):
        with self.assertRaises(ValueError):
            M.immutable_payload('.', self.manifest(), '7' * 40, self.admission('.'))

    def test_schema(self):
        manifest = self.manifest()
        manifest['schema'] = 2
        with self.assertRaises(ValueError):
            M.immutable_payload('.', manifest, HEAD, self.admission('.'))

    def test_duplicate(self):
        manifest = self.manifest()
        manifest['files'] *= 2
        with self.assertRaises(ValueError):
            M.immutable_payload('.', manifest, HEAD, self.admission('.'))

    def test_bad_size(self):
        manifest = self.manifest()
        manifest['files'][0]['bytes'] = True
        with self.assertRaises(ValueError):
            M.immutable_payload('.', manifest, HEAD, self.admission('.'))

    def test_snapshot(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder).resolve()
            path = folder / 'index.html'
            path.write_bytes(b'public')
            data = M.immutable_payload(folder, self.manifest(), HEAD, self.admission(folder))
            path.write_bytes(b'changed')
            self.assertEqual(data, {'index.html': b'public'})

    def test_drift(self):
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder).resolve()
            (folder / 'index.html').write_bytes(b'changed')
            with self.assertRaises(ValueError):
                M.immutable_payload(folder, self.manifest(), HEAD, self.admission(folder))

    def test_private_traversal_not_read(self):
        # Test sentinel only; not an owner's private file. No read may occur.
        manifest = self.manifest()
        manifest['files'].append({'path': '../fake-private.txt', 'sha256': '0' * 64, 'bytes': 10})
        with patch.object(Path, 'read_bytes', side_effect=AssertionError('must not read')) as read:
            with self.assertRaises(ValueError):
                M.immutable_payload('.', manifest, HEAD, self.admission('.'))
            read.assert_not_called()

    def test_symlink_private_not_read(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            sentinel = root / 'fake-private.txt'
            sentinel.write_text('fake private text')
            (root / 'index.html').symlink_to(sentinel)
            with patch.object(Path, 'read_bytes', side_effect=AssertionError('must not read')) as read:
                with self.assertRaises(ValueError):
                    M.immutable_payload(root, self.manifest(), HEAD, self.admission(root))
                read.assert_not_called()

    def test_symlink_ancestor(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            (root / 'real').mkdir()
            (root / 'alias').symlink_to(root / 'real', target_is_directory=True)
            with self.assertRaises(ValueError):
                M.confined(root / 'alias', 'index.html')

    def test_allowlist_http(self):
        server = M.http.server.ThreadingHTTPServer(('127.0.0.1', 0), M.handler_for({'index.html': b'public'}))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        base = 'http://127.0.0.1:' + str(server.server_port)
        try:
            with urlopen(base + '/acceptance/nested/index.html?ignored=public') as response:
                self.assertEqual(response.read(), b'public')
                self.assertEqual(response.headers['Cache-Control'], 'no-store')
            for name in ['/index.html', '/acceptance/nested/../fake-private.txt', '/acceptance/nested/%2e%2e/fake-private.txt']:
                with self.assertRaises(HTTPError) as error:
                    urlopen(base + name)
                self.assertEqual(error.exception.code, 404)
        finally:
            server.shutdown()
            server.server_close()
            thread.join()

    def test_public_role_seven_concepts(self):
        self.assertEqual(len({v['id'] for v in M.ROLE['data'].values()}), 7)
        json.dumps(M.ROLE)

    def test_utf16_surrogate_offsets(self):
        self.assertEqual(M.utf16_slice('A😀B', 1, 3), '😀')
        with self.assertRaises(UnicodeDecodeError):
            M.utf16_slice('A😀B', 1, 2)

    def test_utf16_out_of_bounds(self):
        with self.assertRaises(AssertionError):
            M.utf16_slice('public', 0, 99)

    def test_occurrence_enumeration_retains_repeated_aliases(self):
        matches = M.protected_occurrences('Decision choice decision. A large language model and llm use café.')
        self.assertEqual([m['id'] for m in matches].count(1), 3)
        self.assertEqual([m['id'] for m in matches].count(3), 2)

    def test_occurrence_boundaries(self):
        self.assertEqual(M.protected_occurrences('decisions useful cafes'), [])

    def test_protected_overlap_refused(self):
        with self.assertRaises(AssertionError):
            M.assert_protected('Decision and filler.', [{'start': 0, 'end': 4}])

    def test_public_fixture_safe_cut(self):
        text = 'Decision. Public filler.'
        self.assertEqual(len(M.assert_protected(text, [{'start': 10, 'end': len(text)}])), 1)

    def done_fixture(self):
        # PURE helper data, deliberately not an actual WASM receipt/browser proof.
        text = 'Decision. Manual. Filler.'
        manual = {'id': 'g1', 'anchor': {'start': 10, 'end': 17, 'text': 'Manual.'}}
        addition = {'id': 'g2', 'anchor': {'start': 18, 'end': 25, 'text': 'Filler.'}}
        before = {'body': text, 'counts': {'words': 3, 'chars': len(text)}, 'annotations': {
            'ghosts': [manual], 'spans': [], 'overflow': 'Public stash.',
            'setAside': {'ghosts': [], 'spans': []}, 'preservedBlock': None}}
        after = copy.deepcopy(before)
        after['annotations']['ghosts'].append(addition)
        after['exported'] = M.expected_export(text, after['annotations']['ghosts'])
        receipt = {'body': text,
                   'before': {'live': [manual], 'setAside': [], 'reservedIds': ['g1']},
                   'after': {'live': [manual, addition], 'setAside': [], 'reservedIds': ['g1', 'g2']}}
        return before, after, [{'start': 18, 'end': 25}], receipt

    def test_every_ghost_helper(self):
        before, after, active, receipt = self.done_fixture()
        self.assertEqual(len(M.verify_done(before, after, active, receipt)), 1)

    def test_receipt_missing_independent_range(self):
        before, after, active, receipt = self.done_fixture()
        active.append({'start': 17, 'end': 18})
        with self.assertRaises(AssertionError):
            M.verify_done(before, after, active, receipt)

    def test_manual_ghost_loss(self):
        before, after, active, receipt = self.done_fixture()
        after['annotations']['ghosts'].pop(0)
        receipt['after']['live'] = after['annotations']['ghosts']
        with self.assertRaises(AssertionError):
            M.verify_done(before, after, active, receipt)

    def test_receipt_body_mismatch(self):
        before, after, active, receipt = self.done_fixture()
        receipt['body'] = 'Different public fixture.'
        with self.assertRaises(AssertionError):
            M.verify_done(before, after, active, receipt)

    def test_export_ghost_text_not_removed(self):
        before, after, active, receipt = self.done_fixture()
        after['exported'] = after['body']
        with self.assertRaises(AssertionError):
            M.verify_done(before, after, active, receipt)

    def test_overflow_mutation(self):
        before, after, active, receipt = self.done_fixture()
        after['annotations']['overflow'] = 'Changed public stash.'
        with self.assertRaises(AssertionError):
            M.verify_done(before, after, active, receipt)

    def test_reserved_ids_remain_strings(self):
        before, after, active, receipt = self.done_fixture()
        receipt['after']['reservedIds'] = [1, 2]
        with self.assertRaises(AssertionError):
            M.verify_done(before, after, active, receipt)

    def test_embed_uses_actual_loader_and_public_host(self):
        payload = {name: b'public placeholder for transport-only test' for name in [
            'terraphim-editor.min.js', 'terraphim-editor.min.css', 'terraphim_editor.js', 'terraphim_editor_bg.wasm']}
        host = M.embed_payload(payload)[M.EMBED_PATH].decode()
        self.assertIn('lang="fr"', host)
        self.assertIn('await TeraphimEditor.create', host)
        self.assertNotIn('bindings:', host)
        self.assertEqual(host.count('<main '), 1)
        self.assertNotIn(M.EMBED_PATH, payload)

    def test_embed_missing_actual_assets_fails(self):
        with self.assertRaises(ValueError):
            M.embed_payload({})

    def test_embed_collision_fails(self):
        payload = {name: b'public' for name in [
            'terraphim-editor.min.js', 'terraphim-editor.min.css', 'terraphim_editor.js', 'terraphim_editor_bg.wasm', M.EMBED_PATH]}
        with self.assertRaises(ValueError):
            M.embed_payload(payload)

    def assert_no_reads(self, action):
        with (patch.object(Path, 'read_bytes', side_effect=AssertionError('content read forbidden')) as read,
              patch.object(Path, 'stat', side_effect=AssertionError('metadata read forbidden')) as stat):
            with self.assertRaises(ValueError):
                action()
            read.assert_not_called()
            stat.assert_not_called()

    def test_unregistered_ordinary_private_member_not_read(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            sentinel = root / 'private-corpus.json'
            sentinel.write_bytes(b'fake private sentinel; must never be opened')
            sentinel.chmod(0)
            try:
                manifest = self.manifest()
                manifest['files'].append({'path': sentinel.name, 'sha256': '0' * 64, 'bytes': 1})
                self.assert_no_reads(lambda: M.immutable_payload(root, manifest, HEAD, self.admission(root)))
            finally:
                sentinel.chmod(0o600)

    def test_private_member_disguised_as_static_not_read(self):
        manifest = self.manifest()
        manifest['files'].append({'path': 'private.js', 'sha256': '0' * 64, 'bytes': 1})
        self.assert_no_reads(lambda: M.immutable_payload('.', manifest, HEAD, self.admission('.')))

    def test_unregistered_delivery_root_not_read(self):
        self.assert_no_reads(lambda: M.immutable_payload('/unregistered-private', self.manifest(), HEAD, self.admission('.')))

    def test_manifest_unexpected_fields_not_read(self):
        manifest = self.manifest()
        manifest['privateMetadata'] = 'fake-private-text'
        self.assert_no_reads(lambda: M.immutable_payload('.', manifest, HEAD, self.admission('.')))

    def test_member_unexpected_fields_not_read(self):
        manifest = self.manifest()
        manifest['files'][0]['extra'] = 'fake-private-text'
        self.assert_no_reads(lambda: M.immutable_payload('.', manifest, HEAD, self.admission('.')))

    def test_registration_inappropriate_workspace_root_not_read(self):
        repo = Path('/public-test-repository')
        registration = self.registration(repo)
        registration['root'] = str(repo)
        self.assert_no_reads(lambda: M.admit(registration, repo, registration['manifest'], HEAD, repo))

    def test_registration_manifest_location_not_read(self):
        repo = Path('/public-test-repository')
        registration = self.registration(repo)
        self.assert_no_reads(lambda: M.admit(registration, registration['root'], '/unreadable-private.json', HEAD, repo))

    def test_private_manifest_disguised_inside_stage_slot_not_read(self):
        repo = Path('/public-test-repository')
        registration = self.registration(repo)
        registration['manifest'] = str(Path(registration['manifest']).parent / 'private.json')
        self.assert_no_reads(lambda: M.admit(registration, registration['root'], registration['manifest'], HEAD, repo))

    def test_source_cohort_missing_trim_not_read(self):
        repo = Path('/public-test-repository')
        registration = self.registration(repo)
        registration['sourceInputs'] = [x for x in registration['sourceInputs'] if x['path'] != 'public/js/trim.js']
        self.assert_no_reads(lambda: M.admit(registration, registration['root'], registration['manifest'], HEAD, repo))

    def test_unapproved_source_corpus_not_read(self):
        repo = Path('/public-test-repository')
        registration = self.registration(repo)
        registration['sourceInputs'].append({'path': 'private-corpus.json', 'sha256': '0' * 64, 'bytes': 1})
        self.assert_no_reads(lambda: M.admit(registration, registration['root'], registration['manifest'], HEAD, repo))

    def test_registered_source_symlink_refused_before_content_read(self):
        with tempfile.TemporaryDirectory() as folder:
            repo = Path(folder).resolve()
            registration = self.registration(repo)
            admission = M.admit(registration, registration['root'], registration['manifest'], HEAD, repo)
            source = repo / 'public/js/editor.js'
            source.parent.mkdir(parents=True)
            sentinel = repo / 'fake-private.txt'
            sentinel.write_text('fake private sentinel')
            sentinel.chmod(0)
            source.symlink_to(sentinel)
            try:
                with patch.object(Path, 'read_bytes', side_effect=AssertionError('must not read')) as read:
                    with self.assertRaises(ValueError):
                        M.admission_paths(admission, repo)
                    read.assert_not_called()
            finally:
                sentinel.chmod(0o600)

    def test_registered_manifest_symlink_refused_before_content_read(self):
        with tempfile.TemporaryDirectory() as folder:
            repo = Path(folder).resolve()
            registration = self.registration(repo)
            admission = M.admit(registration, registration['root'], registration['manifest'], HEAD, repo)
            admission.manifest.parent.mkdir(parents=True)
            sentinel = repo / 'fake-private.txt'
            sentinel.write_text('fake private sentinel')
            sentinel.chmod(0)
            admission.manifest.symlink_to(sentinel)
            try:
                with patch.object(Path, 'read_bytes', side_effect=AssertionError('must not read')) as read:
                    with self.assertRaises(ValueError):
                        M.admission_paths(admission, repo)
                    read.assert_not_called()
            finally:
                sentinel.chmod(0o600)

    def test_source_fingerprint_cohort_and_output_association(self):
        repo = Path('/public-test-repository')
        registration = self.registration(repo)
        admission = M.admit(registration, registration['root'], registration['manifest'], HEAD, repo)
        self.assertEqual(set(admission.sources), M.SOURCE_INPUTS)
        changed = copy.deepcopy(admission.sources)
        changed['public/js/trim.js']['sha256'] = '1' * 64
        self.assertNotEqual(M.fingerprint(changed), admission.source_fingerprint)
        registration['provenance']['sourceFingerprint'] = 'f' * 64
        self.assert_no_reads(lambda: M.admit(registration, registration['root'], registration['manifest'], HEAD, repo))

    def drop_receipts(self, text='Public note.'):
        return [{'type': 'dragstart', 'target': 'overflow', 'trusted': True, 'transfer': True, 'types': ['text/plain']},
                {'type': 'drop', 'target': 'surface', 'trusted': True, 'transfer': True, 'types': ['text/plain'],
                 'payload': text, 'payloadMatches': True}]

    def test_real_drop_payload_helper(self):
        M.verify_drop(self.drop_receipts(), 'Public note.', 'overflow', 'surface')

    def test_empty_native_drop_payload_refused(self):
        with self.assertRaises(AssertionError):
            M.verify_drop(self.drop_receipts(''), 'Public note.', 'overflow', 'surface')

    def test_drop_wrong_payload_type_refused(self):
        events = self.drop_receipts()
        events[1]['types'] = ['text/html']
        with self.assertRaises(AssertionError):
            M.verify_drop(events, 'Public note.', 'overflow', 'surface')

    def test_untrusted_drop_refused(self):
        events = self.drop_receipts()
        events[1]['trusted'] = False
        with self.assertRaises(AssertionError):
            M.verify_drop(events, 'Public note.', 'overflow', 'surface')

    def rendered(self, token, start, end, ghost=True):
        return {'tokens': [token], 'start': start, 'end': end, 'ghost': ghost,
                'trim': not ghost, 'opacity': 1, 'colorAlpha': 0.1}

    def test_fragmented_rendered_ghost_exact_id_range(self):
        before, after, active, receipt = self.done_fixture()
        segments = [self.rendered('ghosts:g1', 10, 14), self.rendered('ghosts:g1', 14, 17),
                    self.rendered('ghosts:g2', 18, 25)]
        M.verify_rendered_ghosts(segments, after['annotations']['ghosts'])
        M.verify_rendered_ghosts(segments[:2], before['annotations']['ghosts'])

    def test_missing_rendered_reopen_ghost_refused(self):
        _, after, _, _ = self.done_fixture()
        with self.assertRaises(AssertionError):
            M.verify_rendered_ghosts([self.rendered('ghosts:g1', 10, 17)], after['annotations']['ghosts'])

    def test_rendered_ghost_wrong_range_or_opaque_refused(self):
        before, _, _, _ = self.done_fixture()
        for segment in [self.rendered('ghosts:g1', 10, 16),
                        {**self.rendered('ghosts:g1', 10, 17), 'colorAlpha': 1}]:
            with self.assertRaises(AssertionError):
                M.verify_rendered_ghosts([segment], before['annotations']['ghosts'])

    def test_actual_preview_excludes_protected_occurrence(self):
        text = 'Decision. Filler.'
        M.verify_rendered_preview(text, [self.rendered('lab-trim:c1', 10, 17, False)], [{'start': 10, 'end': 17}])
        with self.assertRaises(AssertionError):
            M.verify_rendered_preview(text, [self.rendered('lab-trim:c1', 0, 8, False)], [{'start': 10, 'end': 17}])


if __name__ == '__main__':
    unittest.main()
