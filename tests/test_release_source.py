"""Real Git/filesystem checks of the release source admission boundary."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("release_source", Path(__file__).resolve().parents[1] / "scripts/check-release-source.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ReleaseSourceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.git("init", "-q")
        for name in ("public/roles/default.json", "scripts/build-dist.sh", "scripts/publish-editor-dist.py", "Cargo.toml", "scripts/check-public-dependencies.py", ".gitignore"):
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("target/\n" if name == ".gitignore" else "admitted\n")
        (self.repo / ".cargo").mkdir()
        (self.repo / ".cargo/config.toml").write_text("# Public registry defaults\n")
        lock = 'version = 4\n' + ''.join('[[package]]\nname="' + name + '"\nversion="' + version + '"\nsource="' + MODULE.PUBLIC_DEPENDENCIES.CRATES_IO + '"\nchecksum="' + "a" * 64 + '"\n' for name, version in MODULE.PUBLIC_DEPENDENCIES.REQUIRED.items())
        (self.repo / "Cargo.lock").write_text(lock)
        self.git("remote", "add", "origin", MODULE.PUBLIC_REPOSITORY + ".git")
        self.git("add", ".")
        self.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "fixture")
        self.head = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True)

    def test_local_release_rejects_override_before_build_and_uses_public_locked_order(self):
        root = Path(__file__).resolve().parents[1]
        script = root / 'scripts/release-dist.sh'
        subprocess.run(['bash', '-n', str(script)], check=True)
        result = subprocess.run(['bash', str(script), '--dry-run'],
                                env={**os.environ, 'TE_ROLEGRAPH_SOURCE': '/fixture/forbidden'},
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('TE_ROLEGRAPH_SOURCE is forbidden', result.stderr)
        self.assertNotIn('isolated stage', result.stdout)
        text = script.read_text()
        commands = ['python3 scripts/check-release-source.py', 'export RUSTUP_TOOLCHAIN=1.96',
                    'bash scripts/ci/registry.sh', 'bash scripts/ci/run.sh --network cargo fetch --locked',
                    'bash scripts/ci/run.sh trunk build --release --locked',
                    'python3 scripts/check-release-source.py --expected-head', 'python3 scripts/publish-editor-dist.py']
        positions = [text.index(command) for command in commands]
        self.assertEqual(positions, sorted(positions))

    def test_clean_commit_and_ignored_build_outputs(self):
        (self.repo / "target").mkdir()
        (self.repo / "target/output").write_text("build bytes")
        self.assertEqual(MODULE.check_source(self.repo, self.head), self.head)

    def test_modified_and_untracked_inputs_are_refused(self):
        target = self.repo / "public/roles/default.json"
        target.write_text("changed\n")
        with self.assertRaises(ValueError):
            MODULE.check_source(self.repo)
        self.git("restore", "public/roles/default.json")
        (self.repo / "untracked-role.json").write_text("unadmitted")
        with self.assertRaises(ValueError):
            MODULE.check_source(self.repo)

    def test_changed_head_and_missing_required_input_are_refused(self):
        with self.assertRaises(ValueError):
            MODULE.check_source(self.repo, "0" * 40)
        self.git("rm", "-q", "public/roles/default.json")
        self.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "remove role")
        with self.assertRaises(subprocess.CalledProcessError):
            MODULE.check_source(self.repo)

    def test_private_unrelated_and_credential_bearing_origins_are_refused_without_echo(self):
        for url in ("https://git.terraphim.cloud/terraphim/terraphim-editor", "https://github.com/other/editor", "https://fixture-secret@github.com/terraphim/terraphim-editor", "https://github.com/terraphim/terraphim-editor?token=fixture-secret", "https://github.com:fixture-secret/terraphim/terraphim-editor"):
            self.git("remote", "set-url", "origin", url)
            with self.subTest(url=url), self.assertRaises(ValueError) as caught:
                MODULE.check_source(self.repo)
            self.assertNotIn("fixture-secret", str(caught.exception))

    def test_public_ssh_origin_is_admitted(self):
        for url in ("git@github.com:terraphim/terraphim-editor.git", "ssh://git@github.com/terraphim/terraphim-editor.git"):
            self.git("remote", "set-url", "origin", url)
            self.assertEqual(MODULE.check_source(self.repo), self.head)

    def test_committed_private_lock_and_source_replacement_are_refused(self):
        lock = self.repo / "Cargo.lock"
        lock.write_text(lock.read_text().replace(MODULE.PUBLIC_DEPENDENCIES.CRATES_IO, "sparse+https://example.invalid/private/"))
        self.git("add", ".")
        self.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "private lock")
        with self.assertRaisesRegex(ValueError, "public crates.io"):
            MODULE.check_source(self.repo)
        lock.write_text(lock.read_text().replace("sparse+https://example.invalid/private/", MODULE.PUBLIC_DEPENDENCIES.CRATES_IO))
        (self.repo / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with="private"\n')
        self.git("add", ".")
        self.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "source replacement")
        with self.assertRaisesRegex(ValueError, "replace public"):
            MODULE.check_source(self.repo)


if __name__ == "__main__":
    unittest.main()
