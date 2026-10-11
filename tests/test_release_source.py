"""Real Git/filesystem checks of the release source admission boundary."""
import importlib.util
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
        for name in ("public/roles/default.json", "scripts/build-dist.sh", "scripts/publish-editor-dist.py", ".gitignore"):
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("target/\n" if name == ".gitignore" else "admitted\n")
        self.git("add", ".")
        self.git("-c", "user.name=Release Test", "-c", "user.email=release-test@example.invalid", "-c", "commit.gpgsign=false", "commit", "-qm", "fixture")
        self.head = self.git("rev-parse", "HEAD").strip()

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.repo, text=True)

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


if __name__ == "__main__":
    unittest.main()
