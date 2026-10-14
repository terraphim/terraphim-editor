"""Release refusals tested with real Git histories, TOML files and tar archives."""
import copy
import importlib.util
import io
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("release", ROOT / "scripts/crates_release.py")
r = importlib.util.module_from_spec(spec)
spec.loader.exec_module(r)


class ReleaseContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.plan = r.load_plan(ROOT)
        (self.root / "release").mkdir()
        (self.root / "release/crates.json").write_text(json.dumps(self.plan))
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers = ["crates/*"]\n')
        for index, package in enumerate(self.plan["packages"]):
            path = self.root / package["path"]
            path.parent.mkdir(parents=True)
            content = '[package]\nname = "' + package["name"] + '"\nversion = "' + package["version"] + '"\npublish = ["crates-io"]\nrepository = "https://github.com/terraphim/terraphim-editor"\ndescription = "Release fixture"\nlicense = "Apache-2.0"\n'
            if index:
                dependency = self.plan["packages"][index - 1]
                content += '\n[dependencies]\n"' + dependency["name"] + '" = { version = "' + dependency["version"] + '", path = "../' + dependency["name"] + '" }\n'
            path.write_text(content)
            (path.parent / "src").mkdir()
            (path.parent / "src/lib.rs").write_text("pub fn answer() -> u32 { 42 }\n")
            (path.parent / "Cargo.lock").write_text('version = 4\n[[package]]\nname = "' + package["name"] + '"\nversion = "' + package["version"] + '"\n')
        self.git("init", "-q")
        self.git("config", "user.name", "Release Test")
        self.git("config", "user.email", "release-test@example.invalid")
        self.commit()
        self.head = self.git("rev-parse", "HEAD")
        self.item = self.plan["packages"][0]
        self.tag = self.item["name"] + "-v" + self.item["version"]
        for item in self.plan["packages"]:
            directory = str(Path(item["path"]).parent)
            item["source"] = {
                "commit": self.head,
                "tree": self.git("rev-parse", self.head + ":" + directory),
                "lockSha256": hashlib.sha256((self.root / directory / "Cargo.lock").read_bytes()).hexdigest(),
            }
            self.git("tag", item["name"] + "-v" + item["version"], self.head)
        (self.root / "release/crates.json").write_text(json.dumps(self.plan))
        self.commit()
        self.publisher_head = self.git("rev-parse", "HEAD")
        self.git("update-ref", "refs/remotes/origin/main", self.publisher_head)
        self.env = {"GITHUB_REPOSITORY": self.plan["repository"], "GITHUB_REF": "refs/heads/main",
                    "GITHUB_SHA": self.publisher_head, "GITHUB_EVENT_NAME": "workflow_dispatch"}

    def git(self, *args):
        return r.git(self.root, *args)

    def commit(self):
        self.git("add", ".")
        self.git("commit", "-qm", "Fixture")

    def test_reviewed_exact_tag_and_order_pass(self):
        self.assertEqual(r.validate_identity(self.root, self.plan, self.tag, self.env), (self.item, self.head))
        r.validate_manifests(self.root, self.plan)

    def test_wrong_event_repository_ref_sha_or_version_refused(self):
        cases = {"GITHUB_EVENT_NAME": "pull_request_target", "GITHUB_REPOSITORY": "other/core",
                 "GITHUB_REF": "refs/tags/" + self.tag, "GITHUB_SHA": "0" * 40}
        for key, value in cases.items():
            with self.subTest(key=key), self.assertRaises(ValueError):
                r.validate_identity(self.root, self.plan, self.tag, {**self.env, key: value})
        with self.assertRaises(ValueError):
            r.selected_package(self.plan, self.tag + "-other")

    def test_untracked_source_refused(self):
        (self.root / "unreviewed.rs").write_text("// unreviewed")
        with self.assertRaisesRegex(ValueError, "Dirty"):
            r.validate_identity(self.root, self.plan, self.tag, self.env)

    def test_publisher_outside_main_refused(self):
        (self.root / "new.txt").write_text("change")
        self.commit()
        new = self.git("rev-parse", "HEAD")
        self.git("tag", "-f", self.tag)
        with self.assertRaises(subprocess.CalledProcessError):
            r.validate_identity(self.root, self.plan, self.tag, {**self.env, "GITHUB_SHA": new})

    def test_moved_tag_refused(self):
        self.git("tag", "-f", self.tag, self.publisher_head)
        with self.assertRaisesRegex(ValueError, "Moved"):
            r.validate_identity(self.root, self.plan, self.tag, self.env)

    def test_unapproved_source_and_changed_tree_lock_refused(self):
        for field in (None, "commit", "tree", "lockSha256"):
            bad = copy.deepcopy(self.plan)
            if field is None:
                bad["packages"][0]["source"] = None
            else:
                bad["packages"][0]["source"][field] = "0" * (64 if field == "lockSha256" else 40)
            with self.subTest(field=field), self.assertRaises(ValueError):
                r.validate_identity(self.root, bad, self.tag, self.env)

    def test_later_package_requires_exact_previously_approved_sibling(self):
        bad = copy.deepcopy(self.plan)
        later = bad["packages"][1]
        bad["packages"][0]["source"]["tree"] = "0" * 40
        with self.assertRaisesRegex(ValueError, "Sibling source differs"):
            r.validate_identity(self.root, bad, later["name"] + "-v" + later["version"], self.env)
        bad["packages"][0]["source"] = None
        with self.assertRaisesRegex(ValueError, "earlier package sources"):
            r.validate_identity(self.root, bad, later["name"] + "-v" + later["version"], self.env)

    def test_exact_approved_branch_source_accepted_without_merging_root_migration(self):
        # Publisher main remains independently buildable while reviewed package
        # source lives on a divergent branch until its downstream core exists.
        self.git("checkout", "--quiet", "--detach", self.head)
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers=["crates/*"]\n[package]\nname="future-editor"\nversion="0.1.0"\n[dependencies]\nnot-yet-published-editor-core="=99.0.0"\n')
        self.commit()
        source = self.git("rev-parse", "HEAD")
        self.git("tag", "-f", self.tag, source)
        self.git("checkout", "--quiet", "--detach", self.publisher_head)
        self.item["source"]["commit"] = source
        (self.root / "release/crates.json").write_text(json.dumps(self.plan))
        self.commit()
        publisher = self.git("rev-parse", "HEAD")
        self.git("update-ref", "refs/remotes/origin/main", publisher)
        self.assertNotEqual(subprocess.run(["git", "-C", str(self.root), "merge-base", "--is-ancestor", source, publisher]).returncode, 0)
        self.assertEqual(r.validate_identity(self.root, self.plan, self.tag, {**self.env, "GITHUB_SHA": publisher}), (self.item, source))
        r.validate_manifests(self.root, self.plan, source)
        self.assertNotIn("future-editor", (self.root / "Cargo.toml").read_text())

    def test_manifest_version_and_private_publish_refused(self):
        path = self.root / self.item["path"]
        original = path.read_text()
        for before, after in [(self.item["version"], "99.0.0"), ('["crates-io"]', '["terraphim"]'),
                              ('https://github.com/terraphim/terraphim-editor', 'https://git.terraphim.cloud/terraphim/terraphim-editor')]:
            path.write_text(original.replace(before, after))
            with self.subTest(after=after), self.assertRaises(ValueError):
                r.validate_manifests(self.root, self.plan)

    def test_reversed_dependency_order_refused(self):
        bad = copy.deepcopy(self.plan)
        bad["packages"].reverse()
        with self.assertRaisesRegex(ValueError, "order"):
            r.validate_manifests(self.root, bad)

    def test_private_registry_git_and_patch_refused(self):
        path = self.root / self.item["path"]
        original = path.read_text()
        for dep in ['registry = "terraphim", version = "1"', 'git = "https://example.invalid/repo"']:
            path.write_text(original + '\n[dependencies]\nother = {' + dep + '}\n')
            with self.assertRaisesRegex(ValueError, "non-public"):
                r.validate_manifests(self.root, self.plan)
        path.write_text(original)
        with (self.root / "Cargo.toml").open("a") as output:
            output.write('\n[patch.crates-io]\nother = { path = "elsewhere" }\n')
        with self.assertRaisesRegex(ValueError, "overrides"):
            r.validate_manifests(self.root, self.plan)

    def test_source_replacement_refused(self):
        (self.root / ".cargo").mkdir()
        (self.root / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with = "private"\n')
        with self.assertRaisesRegex(ValueError, "override"):
            r.validate_manifests(self.root, self.plan)

    def test_plan_duplicates_and_escape_refused(self):
        path = self.root / "release/crates.json"
        path.write_text('{"schema":1,"schema":1}')
        with self.assertRaisesRegex(ValueError, "Duplicate"):
            r.load_plan(self.root)
        bad = copy.deepcopy(self.plan)
        bad["packages"][0]["path"] = "../Cargo.toml"
        path.write_text(json.dumps(bad))
        with self.assertRaisesRegex(ValueError, "manifest path"):
            r.load_plan(self.root)

    def package(self, extra=None, source=None, dependency=""):
        archive_path = self.root / "test.crate"
        prefix = self.item["name"] + "-" + self.item["version"]
        content = {'Cargo.toml': '[package]\nname="' + self.item["name"] + '"\nversion="' + self.item["version"] + '"\n' + dependency,
                   '.cargo_vcs_info.json': json.dumps({'git': {'sha1': source or self.head}, 'path_in_vcs': str(Path(self.item['path']).parent)}),
                   'Cargo.lock': 'version=4\n[[package]]\nname="' + self.item['name'] + '"\nversion="' + self.item['version'] + '"\n',
                   'src/lib.rs': 'pub fn example() {}'}
        with tarfile.open(archive_path, "w:gz") as archive:
            for name, text in content.items():
                info = tarfile.TarInfo(prefix + "/" + name)
                raw = text.encode(); info.size = len(raw)
                archive.addfile(info, io.BytesIO(raw))
            if extra:
                info = tarfile.TarInfo(extra); info.size = 1
                archive.addfile(info, io.BytesIO(b"x"))
        return archive_path

    def test_real_archive_source_hash_and_list_pass(self):
        path = self.package()
        result = r.inspect_package(path, self.item, self.head)
        self.assertEqual(result["sha256"], r.digest(path))
        self.assertEqual(len(result["files"]), 4)

    def test_real_archive_unsafe_paths_duplicate_secrets_and_source_refused(self):
        prefix = self.item["name"] + "-" + self.item["version"]
        for name in ["../outside", "/absolute", prefix + "/.env", prefix + "/src/lib.rs", prefix + "/./src.rs", prefix + "//src.rs"]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                r.inspect_package(self.package(extra=name), self.item, self.head)
        with self.assertRaisesRegex(ValueError, "source commit"):
            r.inspect_package(self.package(source="0" * 40), self.item, self.head)

    def test_packaged_private_dependency_refused(self):
        path = self.package(dependency='\n[dependencies.other]\nversion="1"\nregistry-index="sparse+https://example.invalid/"\n')
        with self.assertRaisesRegex(ValueError, "crates.io-only"):
            r.inspect_package(path, self.item, self.head)

    def test_unpacked_package_is_external_and_exact(self):
        package = self.package()
        with tempfile.TemporaryDirectory() as external:
            destination = Path(external) / "unpacked"
            crate = r.unpack_package(package, self.item, self.head, destination, self.root)
            self.assertEqual((crate / "src/lib.rs").read_text(), "pub fn example() {}")
            self.assertFalse((crate / ".cargo/config.toml").exists())
            with self.assertRaisesRegex(ValueError, "new directory"):
                r.unpack_package(package, self.item, self.head, destination, self.root)
        with self.assertRaisesRegex(ValueError, "outside"):
            r.unpack_package(package, self.item, self.head, self.root / "unpacked", self.root)

    def test_unsafe_archive_is_rejected_before_any_extraction(self):
        package = self.package(extra="../outside")
        with tempfile.TemporaryDirectory() as external:
            destination = Path(external) / "unpacked"
            with self.assertRaises(ValueError):
                r.unpack_package(package, self.item, self.head, destination, self.root)
            self.assertFalse(destination.exists())

    def test_lock_refuses_git_private_registry_missing_checksum_and_unknown_local(self):
        public = 'version=4\n[[package]]\nname="external"\nversion="1.0.0"\nsource="registry+https://github.com/rust-lang/crates.io-index"\nchecksum="' + "a" * 64 + '"\n'
        r.validate_public_lock(public, set())
        for text in (public.replace("registry+https://github.com/rust-lang/crates.io-index", "git+https://example.invalid/private"), public.replace("registry+https://github.com/rust-lang/crates.io-index", "sparse+https://example.invalid/"), public.replace("a" * 64, ""), 'version=4\n[[package]]\nname="external"\nversion="1.0.0"\n'):
            with self.subTest(lock=text), self.assertRaises(ValueError):
                r.validate_public_lock(text, set())

    def test_stage_preserves_clean_git_source_without_root_workspace(self):
        stage = self.root.parent / (self.root.name + "-stage")
        cargo_home = self.root.parent / (self.root.name + "-cargo-home")
        self.addCleanup(shutil.rmtree, stage, True)
        self.addCleanup(shutil.rmtree, cargo_home, True)
        result = r.stage_source(self.root, self.plan, self.item, self.head, stage, cargo_home)
        self.assertFalse((stage / "Cargo.toml").exists())
        self.assertFalse((stage / ".cargo").exists())
        self.assertEqual(r.git(stage, "rev-parse", "HEAD"), self.head)
        self.assertEqual(r.git(stage, "status", "--porcelain"), "")
        self.assertEqual((stage / self.item["path"]).read_bytes(), (self.root / self.item["path"]).read_bytes())
        self.assertTrue(cargo_home.is_dir())
        self.assertTrue(result["files"])

    def test_stage_refuses_reused_cargo_home_and_source_destination(self):
        with self.assertRaisesRegex(ValueError, "new external"):
            r.stage_source(self.root, self.plan, self.item, self.head, self.root / "stage", self.root.parent / "unused-cargo")
        home = self.root / "existing-cargo-home"
        home.mkdir()
        (home / "config.toml").write_text('[source.crates-io]\nreplace-with="private"\n')
        with self.assertRaisesRegex(ValueError, "Cargo home"):
            r.stage_source(self.root, self.plan, self.item, self.head, self.root.parent / "unused-stage", home)

    def test_stage_detects_modified_sparse_checkout_bytes(self):
        stage = self.root.parent / (self.root.name + "-stage")
        cargo_home = self.root.parent / (self.root.name + "-cargo-home")
        self.addCleanup(shutil.rmtree, stage, True)
        self.addCleanup(shutil.rmtree, cargo_home, True)
        r.stage_source(self.root, self.plan, self.item, self.head, stage, cargo_home)
        (stage / self.item["path"]).write_text("changed")
        with self.assertRaisesRegex(ValueError, "Staged source"):
            r.check_staged_source(self.root, self.plan, self.head, stage)

    def test_stage_rejects_symlinks_and_crate_local_source_overrides(self):
        crate = self.root / Path(self.item["path"]).parent
        (crate / "escape").symlink_to("../../Cargo.toml")
        self.commit()
        with self.assertRaisesRegex(ValueError, "regular"):
            r.source_files(self.root, self.plan, self.git("rev-parse", "HEAD"))
        (crate / "escape").unlink()
        (crate / ".cargo").mkdir()
        (crate / ".cargo/config.toml").write_text('[source.crates-io]\nreplace-with="elsewhere"\n')
        self.commit()
        with self.assertRaisesRegex(ValueError, "override"):
            r.source_files(self.root, self.plan, self.git("rev-parse", "HEAD"))

    @unittest.skipUnless(shutil.which("cargo"), "Cargo required for actual archive provenance test")
    def test_actual_cargo_package_has_original_commit_path_without_workspace_cycle(self):
        # The editor root references a future unpublished core; the package must
        # not resolve that workspace when bootstrapping its own dependencies.
        with (self.root / "Cargo.toml").open("a") as output:
            output.write('\n[package]\nname="future-editor"\nversion="0.1.0"\n[dependencies]\nnot-yet-published-editor-core="=99.0.0"\n')
        self.commit()
        head = self.git("rev-parse", "HEAD")
        stage = self.root.parent / (self.root.name + "-stage")
        cargo_home = self.root.parent / (self.root.name + "-cargo-home")
        self.addCleanup(shutil.rmtree, stage, True)
        self.addCleanup(shutil.rmtree, cargo_home, True)
        r.stage_source(self.root, self.plan, self.item, head, stage, cargo_home)
        env = os.environ.copy()
        env.update(CARGO_HOME=str(cargo_home), CARGO_TARGET_DIR=str(self.root / "package-target"), CARGO_BUILD_BUILD_DIR=str(self.root / "package-build"))
        for key in list(env):
            if key.startswith("CARGO_REGISTRIES_") or key.startswith("CARGO_SOURCE_"):
                del env[key]
        result = subprocess.run(["cargo", "package", "--offline", "--locked", "--manifest-path", str(stage / self.item["path"])], cwd=stage, env=env, text=True, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr)
        archive = self.root / "package-target/package" / (self.item["name"] + "-" + self.item["version"] + ".crate")
        proof = r.inspect_package(archive, self.item, head)
        self.assertTrue(proof["sha256"])
        r.check_staged_source(self.root, self.plan, head, stage)

    def test_bootstrap_is_never_an_auth_fallback_for_an_existing_name(self):
        with self.assertRaisesRegex(ValueError, "existence"):
            r.authentication_policy("bootstrap", 200, {}, self.plan)
        with self.assertRaisesRegex(ValueError, "owner"):
            r.authentication_policy("trusted", 200, {"users": [{"login": "other"}]}, self.plan)
        r.authentication_policy("bootstrap", 404, {}, self.plan)
        r.authentication_policy("trusted", 200, {"users": [{"login": self.plan["owner"]}]}, self.plan)

    def test_workflow_separates_bootstrap_oidc_and_qualification(self):
        workflow = (ROOT / ".github/workflows/publish-crates.yml").read_text()
        self.assertNotIn("pull_request:", workflow)
        contracts = (ROOT / ".github/workflows/crate-contracts.yml").read_text()
        for path in ("scripts/crates_release.py", "tests/test_crates_release.py", ".github/workflows/publish-crates.yml", "release/**"):
            self.assertIn(path, contracts)
        self.assertIn("python3 -m unittest discover", contracts)
        self.assertNotIn("secrets.", contracts)
        self.assertIn("github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'", workflow)
        self.assertNotIn("  push:", workflow)
        self.assertNotIn("github.ref_name", workflow)
        self.assertIn("options: [terraphim_alternatives, terraphim_lab]", workflow)
        self.assertIn('validate --crate "$CRATE"', workflow)
        self.assertEqual(workflow.count("ref: ${{ github.sha }}"), 3)
        self.assertIn("environment: crates-io-bootstrap", workflow)
        self.assertIn("github.event_name == 'workflow_dispatch' && inputs.auth_mode == 'bootstrap'", workflow)
        self.assertEqual(workflow.count("id-token: write"), 1)
        self.assertEqual(workflow.count("secrets.CARGO_REGISTRY_TOKEN"), 1)
        self.assertIn("rust-lang/crates-io-auth-action@c6f97d42243bad5fab37ca0427f495c86d5b1a18", workflow)
        for forbidden in ["--allow-dirty", "--no-verify", "op read", "crate_list"]:
            self.assertNotIn(forbidden, workflow)
        qualify = workflow.split("  publish:")[0]
        self.assertNotIn("secrets.", qualify)
        self.assertIn("cargo +\"$TOOLCHAIN\" test --locked", qualify)
        self.assertIn("cargo +\"$TOOLCHAIN\" clippy --locked", qualify)
        self.assertIn('cd "$RUNNER_TEMP/editor-crates-source/crates/$CRATE"', qualify)
        self.assertEqual(workflow.count('scripts/crates_release.py stage --tag'), 3)
        self.assertEqual(workflow.count('scripts/crates_release.py compare --tag'), 2)
        self.assertLess(workflow.index('scripts/crates_release.py compare --tag'), workflow.index('Obtain temporary crates.io publish token'))
        self.assertNotIn('cargo +"$TOOLCHAIN" publish --locked --registry crates-io -p', workflow)
        self.assertIn('CARGO_HOME: ${{ runner.temp }}/editor-crates-cargo', workflow)
        self.assertIn('rustup target add wasm32-unknown-unknown --toolchain "$TOOLCHAIN"', qualify)
        self.assertIn('cargo +"$TOOLCHAIN" check --locked --target wasm32-unknown-unknown --lib', qualify)
        unpack = qualify.index('scripts/crates_release.py unpack')
        isolated_cd = qualify.index('cd "$RUNNER_TEMP/unpacked/$CRATE-$VERSION"')
        archive_tests = qualify.index('cargo +"$TOOLCHAIN" test --locked --manifest-path Cargo.toml --all-targets')
        qualified = qualify.index('scripts/crates_release.py package')
        self.assertLess(unpack, isolated_cd)
        self.assertLess(isolated_cd, archive_tests)
        self.assertLess(archive_tests, qualified)


if __name__ == "__main__":
    unittest.main()
