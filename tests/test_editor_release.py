"""Actual Git/archive/filesystem refusal tests; fixtures are not runtime qualification."""
import copy
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location('editor_release', Path(__file__).resolve().parents[1] / 'scripts/publish-editor-release.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def archive(path, members):
    with tarfile.open(path, 'w:gz') as output:
        for name, raw in members:
            entry = tarfile.TarInfo(name); entry.size = len(raw)
            output.addfile(entry, io.BytesIO(raw))


class EditorReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name); self.folder = self.root / 'candidate'; self.folder.mkdir()
        self.source = 'a' * 40; self.lock = 'b' * 64; self.tag = 'v0.1.0-rc.2'
        self.members = {name: ('fixture:' + name).encode() for name in ('index.html', 'terraphim_editor.js', 'terraphim_editor_bg.wasm', 'roles/default.json')}
        archive(self.folder / 'editor-candidate.tar.gz', self.members.items())
        self.manifest = {'schema': 1, 'sourceHead': self.source, 'sourceRepository': M.SOURCE_URL,
                         'cargoLockSha256': self.lock, 'assets': {name: sha(raw) for name, raw in self.members.items()}}
        self.write_manifest()
        self.graph = {'data': {'risk': {'id': 1, 'nterm': 'risk'}, 'concern': {'id': 1, 'nterm': 'risk'}}}
        clean = 'Risk: Timber. Decision: Backup.'
        browser = {'status': 'PASS', 'nativeKeyboardUndoRedo': True, 'saveReopen': True,
                   'actualDownloads': True, 'graph': self.graph, 'clean': clean}
        offline = {'status': 'PASS', 'engine': 'chrome', 'scenarios': [{'result': {'saveReopenExport': True}, 'externalRequests': [], 'freshContext': True}] * 2}
        self.outputs = {'story/knowledge-graph.json': json.dumps(self.graph).encode(),
                        'story/corpus/project-brief.md': clean.encode(),
                        'story/browser-receipt.json': json.dumps(browser).encode(),
                        'offline.json': json.dumps(offline).encode(),
                        'story/annotated.md': b'Explicit protocol fixture, not runtime evidence',
                        'wasm-host.log': b'# pass 15\n# fail 0\n', 'chrome-synonyms.log': b'fixture', 'chrome-offline.log': b'fixture'}
        self.report = {'status': 'PASS', 'tag': self.tag, 'sourceCommit': self.source, 'sourceRepository': M.SOURCE_URL,
                       'cargoLockSha256': self.lock, 'archiveSha256': M.digest(self.folder / 'editor-candidate.tar.gz'),
                       'manifestSha256': M.digest(self.folder / 'assets-manifest.json'), 'fullParityAccepted': False,
                       'gates': [{'name': name, 'status': 'PASS', 'logSha256': sha(self.outputs[name + '.log'])} for name in M.GATES]}
        self.write_outputs()

    def write_manifest(self):
        (self.folder / 'assets-manifest.json').write_text(json.dumps(self.manifest))

    def write_outputs(self):
        archive(self.folder / 'saved-outputs.tar.gz', self.outputs.items())
        self.report.update(savedOutputsSha256=M.digest(self.folder / 'saved-outputs.tar.gz'), outputs={n: sha(raw) for n, raw in self.outputs.items()})
        self.write_report()

    def write_report(self):
        (self.folder / 'qualification.json').write_text(json.dumps(self.report))

    def test_actual_archive_members_and_hashes_are_verified(self):
        self.assertEqual(M.check_assets(self.folder, self.source, self.lock), self.members)
        self.manifest['assets']['index.html'] = '0' * 64; self.write_manifest()
        with self.assertRaises(ValueError): M.check_assets(self.folder, self.source, self.lock)

    def test_wrong_source_repository_or_lock_refused(self):
        for key in ('sourceHead', 'sourceRepository', 'cargoLockSha256'):
            original = self.manifest[key]; self.manifest[key] = 'wrong'; self.write_manifest()
            with self.subTest(key=key), self.assertRaises(ValueError): M.check_assets(self.folder, self.source, self.lock)
            self.manifest[key] = original

    def test_unknown_members_and_embedded_bridge_refused(self):
        for name in ('extra.js', 'host-session.js'):
            items = {**self.members, name: b'extra'}
            archive(self.folder / 'editor-candidate.tar.gz', items.items())
            if name == 'host-session.js':
                self.manifest['assets'][name] = sha(b'extra'); self.write_manifest()
            with self.subTest(name=name), self.assertRaises(ValueError): M.check_assets(self.folder, self.source, self.lock)

    def test_archive_traversal_duplicates_links_and_empty_refused(self):
        path = self.folder / 'bad.tar.gz'
        for entries in ([('../escape', b'x')], [('/absolute', b'x')], [('a', b'x'), ('a', b'y')], [('a/./b', b'x')], []):
            archive(path, entries)
            with self.subTest(entries=entries), self.assertRaises(ValueError): M.safe_members(path)
        with tarfile.open(path, 'w:gz') as output:
            link = tarfile.TarInfo('link'); link.type = tarfile.SYMTYPE; link.linkname = 'elsewhere'; output.addfile(link)
        with self.assertRaises(ValueError): M.safe_members(path)

    def test_targeted_qualification_protocol_accepts_bound_fixture(self):
        self.assertEqual(M.check_qualification(self.folder, self.source, self.lock, self.tag), self.report)

    def test_fail_missing_duplicate_gates_and_full_parity_overclaim_refused(self):
        original = copy.deepcopy(self.report)
        changes = [{'fullParityAccepted': True}, {'status': 'INCOMPLETE'}, {'gates': self.report['gates'][:2]},
                   {'gates': [self.report['gates'][0]] * 3}, {'gates': [{**g, 'status': 'FAIL'} for g in self.report['gates']]},
                   {'archiveSha256': '0' * 64}, {'sourceCommit': 'c' * 40}]
        for change in changes:
            self.report = {**original, **change}; self.write_report()
            with self.subTest(change=change), self.assertRaises(ValueError): M.check_qualification(self.folder, self.source, self.lock, self.tag)

    def test_changed_saved_graph_or_clean_text_refused_even_with_rehashed_archive(self):
        original = dict(self.outputs)
        graph = copy.deepcopy(self.graph); graph['data']['concern']['id'] = 2
        for name, raw in [('story/knowledge-graph.json', json.dumps(graph).encode()), ('story/corpus/project-brief.md', b'Concern: supplied input')]:
            self.outputs = {**original, name: raw}; self.write_outputs()
            with self.subTest(name=name), self.assertRaises(ValueError): M.check_qualification(self.folder, self.source, self.lock, self.tag)

    def test_changed_output_hash_refused(self):
        self.report['outputs']['offline.json'] = '0' * 64; self.write_report()
        with self.assertRaises(ValueError): M.check_qualification(self.folder, self.source, self.lock, self.tag)

    def test_resume_only_identical_draft_assets_and_never_repair_public_release(self):
        files = [self.folder / 'editor-candidate.tar.gz', self.folder / 'assets-manifest.json']
        item = lambda p: {'name': p.name, 'state': 'uploaded', 'size': p.stat().st_size, 'digest': 'sha256:' + M.digest(p)}
        current = {'tag_name': self.tag, 'prerelease': True, 'draft': True, 'assets': [item(files[0])]}
        self.assertEqual(M.resume_plan(current, self.tag, files), [files[1]])
        current['draft'] = False
        with self.assertRaises(ValueError): M.resume_plan(current, self.tag, files)
        current['assets'].append(item(files[1]))
        self.assertEqual(M.resume_plan(current, self.tag, files), [])
        current['assets'][0]['digest'] = 'sha256:' + '0' * 64
        with self.assertRaises(ValueError): M.resume_plan(current, self.tag, files)

    def test_ambiguous_release_and_asset_metadata_refused(self):
        current = {'tag_name': self.tag, 'prerelease': True, 'draft': True, 'assets': []}
        with self.assertRaises(ValueError): M.select_release([[current], [current]], self.tag)
        file = self.folder / 'assets-manifest.json'
        for assets in ([{'name': 'surprise'}], [{'name': file.name}] * 2, [{'name': file.name, 'state': 'new', 'size': file.stat().st_size}]):
            current['assets'] = assets
            with self.assertRaises(ValueError): M.resume_plan(current, self.tag, [file])

    def test_log_counts_or_missing_annotated_output_cannot_be_forged_by_rehashing(self):
        original = dict(self.outputs)
        for name, raw in [('wasm-host.log', b'# pass 14\n# fail 0\n'), ('story/annotated.md', b'')]:
            self.outputs = {**original, name: raw}
            for gate in self.report['gates']:
                gate['logSha256'] = sha(self.outputs[gate['name'] + '.log'])
            self.write_outputs()
            with self.subTest(name=name), self.assertRaises(ValueError): M.check_qualification(self.folder, self.source, self.lock, self.tag)

    def test_wrong_publication_repository_is_refused_before_any_api_call(self):
        with self.assertRaises(ValueError): M.publish('other/repository', self.tag, [], self.source)

    def test_checksum_manifest_rejects_malformed_duplicate_names(self):
        valid = 'a' * 64 + '  editor-candidate.tar.gz\n'
        self.assertEqual(M.checksums(valid), {'editor-candidate.tar.gz': 'a' * 64})
        for value in (valid * 2, 'bad  file', 'a' * 64 + '  ../escape\n'):
            with self.assertRaises(ValueError): M.checksums(value)


class SourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.git('init', '-q')
        for name in ['public/roles/default.json', 'scripts/build-dist.sh', 'scripts/publish-editor-dist.py', 'scripts/check-public-dependencies.py', '.cargo/config.toml']:
            path = self.root / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_text('' if name.endswith('toml') else 'fixture')
        (self.root / 'Cargo.toml').write_text('[package]\nname="terraphim-editor"\nversion="0.1.0"\n')
        guard = M.load_module('public_dependencies', 'scripts/check-public-dependencies.py')
        (self.root / 'Cargo.lock').write_text('version=4\n' + ''.join('[[package]]\nname="'+n+'"\nversion="'+v+'"\nsource="'+guard.CRATES_IO+'"\nchecksum="'+'a'*64+'"\n' for n,v in guard.REQUIRED.items()))
        self.git('remote', 'add', 'origin', M.SOURCE_URL + '.git')
        self.git('add', '.')
        self.commit('fixture'); self.source = self.git('rev-parse', 'HEAD')
        self.git('update-ref', 'refs/remotes/origin/main', self.source)
        self.git('tag', 'v0.1.0-rc.2')

    def git(self, *args):
        return subprocess.check_output(['git', *args], cwd=self.root, text=True, stderr=subprocess.DEVNULL).strip()

    def commit(self, message):
        self.git('-c', 'user.name=Fixture', '-c', 'user.email=test@example.invalid', '-c', 'commit.gpgsign=false', 'commit', '-qm', message)

    def test_complete_file_package_checks_source_outputs_manifest_and_sums_together(self):
        fixture = EditorReleaseTests(); fixture.setUp(); self.addCleanup(fixture.temp.cleanup)
        fixture.source = self.source; fixture.lock = M.digest(self.root / 'Cargo.lock')
        fixture.manifest.update(sourceHead=self.source, cargoLockSha256=fixture.lock); fixture.write_manifest()
        fixture.report.update(sourceCommit=self.source, cargoLockSha256=fixture.lock,
                              manifestSha256=M.digest(fixture.folder / 'assets-manifest.json')); fixture.write_report()
        def item(name):
            p = fixture.folder / name
            return {'path': name, 'bytes': p.stat().st_size, 'sha256': M.digest(p)}
        archive_item = item('editor-candidate.tar.gz')
        manifest = {'schema': 1, 'releaseId': 'fixture', 'channel': 'candidate',
                    'artifacts': [{**archive_item, 'host': 'browser', 'version': fixture.tag, 'sourceCommit': self.source}],
                    'evidence': [{**item('qualification.json'), 'host': 'browser', 'kind': 'receipt', 'artifactSha256': archive_item['sha256']}],
                    'limitations': [{'host': 'browser', 'requirement': 'all41', 'status': 'OPEN', 'issue': 'fixture'}]}
        def write():
            (fixture.folder / 'release-manifest.json').write_text(json.dumps(manifest))
            (fixture.folder / 'SHA256SUMS').write_text(''.join(M.digest(fixture.folder / n) + '  ' + n + '\n' for n in M.FILES if n != 'SHA256SUMS'))
        write()
        source, files = M.verify_package(fixture.folder, fixture.tag, self.root)
        self.assertEqual(source, self.source); self.assertEqual({p.name for p in files}, set(M.FILES))
        manifest['limitations'][0]['status'] = 'RESOLVED'; write()
        with self.assertRaises(ValueError): M.verify_package(fixture.folder, fixture.tag, self.root)

    def test_clean_reviewed_rc_accepted(self):
        self.assertEqual(M.admit_source('v0.1.0-rc.2', self.root), self.source)

    def test_stable_wrong_component_and_version_tags_refused(self):
        for tag in ('main', 'v0.1.0', 'v0.2.0-rc.2', 'terraphim_lab-v0.1.1', 'v0.1.0-rc.2;echo unsafe'):
            with self.subTest(tag=tag), self.assertRaises(ValueError): M.admit_source(tag, self.root)

    def test_tag_mismatch_dirty_and_unreviewed_branch_refused(self):
        (self.root / 'new-file').write_text('changed')
        with self.assertRaises(ValueError): M.admit_source('v0.1.0-rc.2', self.root)
        self.git('add', '.'); self.commit('new commit')
        with self.assertRaises(ValueError): M.admit_source('v0.1.0-rc.2', self.root)
        self.git('tag', 'v0.1.0-rc.3')
        with self.assertRaisesRegex(ValueError, 'main'): M.admit_source('v0.1.0-rc.3', self.root)


class WorkflowTests(unittest.TestCase):
    def test_only_protected_dependent_job_can_publish_exact_qualified_artifact(self):
        workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/release.yml').read_text()
        build, publish = workflow.split('  publish:', 1)
        self.assertNotIn('contents: write', build)
        self.assertNotIn('GH_TOKEN:', build)
        self.assertIn('needs: candidate', publish)
        self.assertIn('environment: editor-release', publish)
        self.assertIn('artifact-ids: ${{ needs.candidate.outputs.artifact-id }}', publish)
        self.assertLess(publish.index('publish-editor-release.py verify'), publish.index('GH_TOKEN:'))
        self.assertIn("github.ref_type == 'tag'", publish)
        self.assertNotIn('--clobber', workflow)
        self.assertNotIn('continue-on-error', workflow)
        self.assertNotIn('pull_request_target', workflow)


if __name__ == '__main__':
    unittest.main()
