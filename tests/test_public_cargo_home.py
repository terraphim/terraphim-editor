"""Actual filesystem tests for fresh per-run CI Cargo storage."""
import importlib.util
from pathlib import Path
import tempfile
import json
import subprocess
import sys
import unittest

SPEC = importlib.util.spec_from_file_location("public_cargo_home", Path(__file__).resolve().parents[1] / "scripts/ci/prepare-public-cargo-home.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PublicCargoHomeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "checkout"
        self.root.mkdir()

    def test_old_source_replacement_credentials_and_cache_are_removed(self):
        home = MODULE.prepare(self.root)
        (home / "config.toml").write_text('[source.crates-io]\nreplace-with="old-private-source"\n')
        (home / "credentials.toml").write_text("# non-secret test fixture\n")
        (home / "registry").mkdir()
        (home / "registry/old-cache").write_text("old cache")
        self.assertEqual(MODULE.prepare(self.root), home)
        self.assertEqual(list(home.iterdir()), [])

    def test_other_target_outputs_and_global_home_untouched(self):
        sibling = self.root / "target/compiled-output"
        sibling.parent.mkdir(); sibling.write_text("keep")
        global_home = Path(self.temp.name) / "global-cargo"
        global_home.mkdir(); (global_home / "config.toml").write_text("keep global")
        MODULE.prepare(self.root)
        self.assertEqual(sibling.read_text(), "keep")
        self.assertEqual((global_home / "config.toml").read_text(), "keep global")

    def test_symlinked_target_or_home_is_refused(self):
        outside = Path(self.temp.name) / "outside"
        outside.mkdir(); sentinel = outside / "sentinel"; sentinel.write_text("keep")
        target = self.root / "target"; target.symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError): MODULE.prepare(self.root)
        target.unlink(); target.mkdir()
        (target / "ci-cargo-home").symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ValueError): MODULE.prepare(self.root)
        self.assertEqual(sentinel.read_text(), "keep")

    def test_internal_symlink_is_removed_without_following_it(self):
        home = MODULE.prepare(self.root)
        outside = Path(self.temp.name) / "outside.txt"; outside.write_text("keep")
        (home / "old-cache-link").symlink_to(outside)
        MODULE.prepare(self.root)
        self.assertEqual(outside.read_text(), "keep")

    def test_ci_wrapper_preserves_machine_readable_stdout(self):
        root = Path(__file__).resolve().parents[1]
        result = subprocess.run(['bash', str(root / 'scripts/ci/run.sh'), sys.executable,
                                 '-c', 'import json; print(json.dumps({"actual": "metadata-output"}))'],
                                capture_output=True, text=True, check=True)
        self.assertEqual(json.loads(result.stdout), {'actual': 'metadata-output'})
        self.assertTrue(result.stderr.startswith('+ '))

    def test_workflows_prepare_before_cargo(self):
        root = Path(__file__).resolve().parents[1]
        entry = (root / "scripts/ci/registry.sh").read_text()
        self.assertLess(entry.index("check-public-dependencies.py"), entry.index("prepare-public-cargo-home.py"))
        for path in (".gitea/workflows/ci.yml", ".github/workflows/release.yml", ".github/workflows/public-root.yml"):
            text = (root / path).read_text()
            self.assertLess(text.index("scripts/ci/registry.sh"), text.index("cargo fetch" if path.startswith(".github") else "cargo fmt"))


if __name__ == "__main__":
    unittest.main()
