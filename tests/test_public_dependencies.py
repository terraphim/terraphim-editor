"""Public builds must resolve from the registry, not operator path overrides."""
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("public_dependencies", Path(__file__).resolve().parents[1] / "scripts/check-public-dependencies.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PublicDependencyAdmissionTests(unittest.TestCase):
    def lock(self):
        return {"package": [
            {"name": name, "version": version, "source": MODULE.CRATES_IO, "checksum": "a" * 64}
            for name, version in MODULE.REQUIRED.items()
        ] + [{"name": "terraphim-editor", "version": "0.1.0"}]}

    def test_real_registry_identities_are_admitted(self):
        self.assertEqual(MODULE.problems(self.lock()), [])

    def test_private_registry_source_is_rejected(self):
        lock = self.lock()
        lock["package"][0]["source"] = "sparse+https://git.terraphim.cloud/api/packages/terraphim/cargo/"
        self.assertTrue(any("source" in issue for issue in MODULE.problems(lock)))

    def test_public_git_is_not_a_registry_release(self):
        lock = self.lock()
        lock["package"][0]["source"] = "git+https://github.com/terraphim/terraphim-core#0123456"
        self.assertTrue(any("source" in issue for issue in MODULE.problems(lock)))

    def test_operator_path_patch_cannot_qualify_a_public_build(self):
        lock = self.lock()
        del lock["package"][0]["source"]
        self.assertTrue(any("unpublished local" in issue for issue in MODULE.problems(lock)))

    def test_missing_or_wrong_dependency_version_is_rejected(self):
        lock = self.lock()
        lock["package"].pop(0)
        self.assertTrue(MODULE.problems(lock))
        lock = self.lock()
        lock["package"][0]["version"] = "0.0.0"
        self.assertTrue(MODULE.problems(lock))

    def test_missing_archive_checksum_is_rejected(self):
        lock = self.lock()
        del lock["package"][0]["checksum"]
        self.assertTrue(any("checksum" in issue for issue in MODULE.problems(lock)))

    def test_old_extra_dependency_and_unknown_local_package_are_rejected(self):
        lock = self.lock()
        lock["package"].append({"name": "terraphim_types", "version": "1.22.2", "source": MODULE.CRATES_IO, "checksum": "b" * 64})
        lock["package"].append({"name": "unreviewed_local", "version": "0.1.0"})
        self.assertTrue(any("unexpected version" in issue for issue in MODULE.problems(lock)))
        self.assertTrue(any("unpublished local" in issue for issue in MODULE.problems(lock)))


if __name__ == "__main__":
    unittest.main()
