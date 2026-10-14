"""Public graph metadata admission and refusals; full Cargo output is checked in release CI."""
import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("rolegraph_provenance", Path(__file__).resolve().parents[1] / "scripts/ci/rolegraph-production-check.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class RolegraphPublicProvenanceTests(unittest.TestCase):
    def metadata(self):
        return {"packages": [{"name": name, "version": version, "source": MODULE.REGISTRY, "repository": MODULE.REPOSITORY} for name, version in MODULE.VERSIONS.items()] + [{"name": "terraphim_rolegraph_browser", "id": "path+file:///fixture/browser#0.1.0"}], "workspace_members": ["path+file:///fixture/browser#0.1.0"]}

    def test_public_registry_and_source_shape_accepted(self):
        MODULE.metadata(self.metadata())

    def test_private_registry_path_patch_and_git_fork_refused(self):
        for source in ("sparse+https://git.terraphim.cloud/api/packages/terraphim/cargo/", None, "git+https://github.com/other/fork#0123"):
            data = self.metadata(); data["packages"][0]["source"] = source
            with self.subTest(source=source), self.assertRaises(ValueError): MODULE.metadata(data)

    def test_wrong_repository_version_or_duplicate_refused(self):
        for field, value in (("repository", "https://git.terraphim.cloud/terraphim/terraphim-core"), ("version", "0.0.0")):
            data = self.metadata(); data["packages"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError): MODULE.metadata(data)
        data = self.metadata(); data["packages"].append(copy.deepcopy(data["packages"][0]))
        with self.assertRaises(ValueError): MODULE.metadata(data)

    def test_browser_bridge_must_be_actual_workspace_member(self):
        data = self.metadata(); data["workspace_members"] = []
        with self.assertRaises(ValueError): MODULE.metadata(data)


if __name__ == "__main__":
    unittest.main()
