#!/usr/bin/env python3
"""Synthetic transaction unit tests; NOT production release qualification."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("publisher", ROOT / "scripts/publish-editor-dist.py")
P = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(P)
SCRATCH = Path("/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode")


class PublisherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="distribution-unit-", dir=SCRATCH)
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.stage = self.repo / "target/release-staging-test"
        self.stage.mkdir(parents=True)
        self.git("init", "-q")
        self.git("config", "user.name", "Synthetic unit fixture")
        self.git("config", "user.email", "fixture@example.invalid")
        self.write("dist/index.html", b"old")
        self.write("dist/terraphim-editor-123456789abcdef0.js", b"obsolete")
        self.git("add", "dist")
        self.git("commit", "-qm", "synthetic baseline")
        self.write_stage("index.html", b"new")

    def git(self, *args):
        return subprocess.check_output(["git", "-C", str(self.repo), *args], stderr=subprocess.STDOUT)

    def write(self, name, data):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def write_stage(self, name, data):
        path = self.stage / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def run_publish(self, **kwargs):
        files = {p.relative_to(self.stage).as_posix(): P.digest(p.read_bytes())
                 for p in self.stage.rglob("*") if p.is_file()}
        return P.publish(self.repo, self.stage, files, ["dist", "package/dist"], **kwargs)

    def test_preserves_extras_and_unknown_old_hash_and_cleans_owned(self):
        self.write("dist/notes.txt", b"user")
        unknown = "dist/terraphim-editor-d0697d7d1ae1409.js"
        self.write(unknown, b"protected")
        self.write("package/README.md", b"package user content")
        removed = self.run_publish()
        self.assertEqual(removed, ["dist/terraphim-editor-123456789abcdef0.js"])
        for destination in ("dist", "package/dist"):
            self.assertEqual((self.repo / destination / "index.html").read_bytes(), b"new")
        self.assertEqual((self.repo / unknown).read_bytes(), b"protected")
        self.assertEqual((self.repo / "dist/notes.txt").read_bytes(), b"user")
        self.assertEqual((self.repo / "package/README.md").read_bytes(), b"package user content")
        self.assertEqual(self.git("diff", "--cached"), b"")

    def test_unknown_collision_refuses_entire_batch(self):
        self.write("package/dist/index.html", b"untracked package")
        with self.assertRaisesRegex(ValueError, "unknown collision"):
            self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")
        self.assertTrue((self.repo / "dist/terraphim-editor-123456789abcdef0.js").exists())
        self.assertFalse((self.repo / "target/release-transactions").exists())

    def test_approval_requires_exact_path_and_before_hash(self):
        self.write("package/dist/index.html", b"unknown")
        for approval in ({"dist/index.html": P.digest(b"unknown")},
                         {"package/dist/index.html": P.digest(b"wrong")}):
            with self.assertRaisesRegex(ValueError, "unknown collision"):
                self.run_publish(approved=approval)
        self.run_publish(approved={"package/dist/index.html": P.digest(b"unknown")})
        self.assertEqual((self.repo / "package/dist/index.html").read_bytes(), b"new")

    def test_identical_untracked_collision_is_safe(self):
        self.write("package/dist/index.html", b"new")
        self.run_publish()

    def test_dirty_tracked_even_identical_to_new_refused(self):
        self.write("dist/index.html", b"new")
        with self.assertRaisesRegex(ValueError, "dirty"):
            self.run_publish()
        self.assertFalse((self.repo / "package").exists())

    def test_deleted_tracked_refused(self):
        (self.repo / "dist/index.html").unlink()
        with self.assertRaisesRegex(ValueError, "deleted tracked"):
            self.run_publish()

    def test_dirty_obsolete_refused(self):
        self.write("dist/terraphim-editor-123456789abcdef0.js", b"dirty")
        with self.assertRaisesRegex(ValueError, "dirty obsolete"):
            self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")

    def test_prior_owned_exact_bytes_cleanup(self):
        name = "terraphim-editor-abcdef1234567890_bg.wasm"
        self.write("package/dist/" + name, b"owned")
        self.run_publish(prior={"package/dist": {name: P.digest(b"owned")}})
        self.assertFalse((self.repo / "package/dist" / name).exists())

    def test_prior_owned_dirty_refused(self):
        name = "terraphim-editor-abcdef1234567890_bg.wasm"
        self.write("package/dist/" + name, b"dirty")
        with self.assertRaisesRegex(ValueError, "dirty obsolete"):
            self.run_publish(prior={"package/dist": {name: P.digest(b"owned")}})

    def test_mid_copy_failure_rolls_back_both_destinations(self):
        self.write_stage("js/worker.js", b"worker")
        count = 0

        def fail(source, destination):
            nonlocal count
            count += 1
            if count == 4:
                raise OSError("injected copy failure")
            os.replace(source, destination)

        with self.assertRaisesRegex(OSError, "injected"):
            self.run_publish(replace=fail)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")
        self.assertEqual((self.repo / "dist/terraphim-editor-123456789abcdef0.js").read_bytes(), b"obsolete")
        self.assertFalse((self.repo / "dist/js").exists())
        self.assertFalse((self.repo / "package").exists())
        self.assertFalse(list(self.repo.rglob(".release-*")))

    def test_dry_run_never_changes_destinations(self):
        self.run_publish(dry_run=True)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")
        self.assertFalse((self.repo / "package").exists())

    def test_report_failure_also_rolls_back(self):
        def fail_report(removed):
            raise OSError("injected metadata failure")
        with self.assertRaisesRegex(OSError, "metadata failure"):
            self.run_publish(finalize=fail_report)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")
        self.assertTrue((self.repo / "dist/terraphim-editor-123456789abcdef0.js").exists())
        self.assertFalse((self.repo / "package").exists())

    def temp_writer(self, action):
        original = P.tempfile.NamedTemporaryFile
        def factory(*args, **kwargs):
            stream = original(*args, **kwargs)
            write = stream.write
            if kwargs.get("prefix") == ".release-":
                def intercepted(data):
                    return action(stream, write, data)
                stream.write = intercepted
            return stream
        return mock.patch.object(P.tempfile, "NamedTemporaryFile", factory)

    def test_pending_external_edit_during_temp_write_is_preserved(self):
        def edit(stream, write, data):
            result = write(data)
            self.write("dist/index.html", b"external pending edit")
            return result
        with self.temp_writer(edit):
            with self.assertRaisesRegex(ValueError, "changed after preflight"):
                self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"external pending edit")
        self.assertFalse((self.repo / "package").exists())

    def test_external_edit_before_later_failure_is_not_rolled_back(self):
        self.write_stage("js/worker.js", b"worker")
        count = 0
        def fail(source, destination):
            nonlocal count
            count += 1
            if count == 2:
                self.write("dist/index.html", b"external installed edit")
                raise OSError("injected later failure")
            os.replace(source, destination)
        with self.assertRaisesRegex(OSError, "injected later failure") as error:
            self.run_publish(replace=fail)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"external installed edit")
        self.assertIn("manual recovery", str(error.exception))
        ledger = next((self.repo / "target/release-transactions").glob("*/recovery.json"))
        report = json.loads(ledger.read_text())
        self.assertEqual(report["status"], "manual-recovery-required")
        self.assertTrue(report["conflicts"])

    def test_partial_temp_write_failure_cleans_owned_artifact(self):
        def fail(stream, write, data):
            write(data[:1])
            raise OSError("injected partial write failure")
        with self.temp_writer(fail):
            with self.assertRaisesRegex(OSError, "injected partial write failure"):
                self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")
        self.assertFalse(list(self.repo.rglob(".release-*")))

    def test_external_temp_path_substitution_is_preserved(self):
        external = None
        def fail(stream, write, data):
            nonlocal external
            external = Path(stream.name)
            external.unlink()
            external.write_bytes(b"external temp substitution")
            raise OSError("injected temp substitution failure")
        with self.temp_writer(fail):
            with self.assertRaisesRegex(OSError, "injected temp substitution failure") as error:
                self.run_publish()
        self.assertEqual(external.read_bytes(), b"external temp substitution")
        self.assertIn("manual recovery", str(error.exception))

    def test_same_bytes_inode_replacement_after_preflight_refused(self):
        replacement_inode = None
        def edit(stream, write, data):
            nonlocal replacement_inode
            result = write(data)
            external = self.write("external-edit", b"old")
            replacement_inode = external.stat().st_ino
            os.replace(external, self.repo / "dist/index.html")
            return result
        with self.temp_writer(edit):
            with self.assertRaisesRegex(ValueError, "changed after preflight"):
                self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").stat().st_ino, replacement_inode)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")

    def test_same_postimage_bytes_external_write_refuses_rollback(self):
        self.write_stage("js/worker.js", b"worker")
        count = 0
        def fail(source, destination):
            nonlocal count
            count += 1
            if count == 2:
                self.write("dist/index.html", b"new")
                raise OSError("injected same-byte external edit")
            os.replace(source, destination)
        with self.assertRaisesRegex(OSError, "manual recovery"):
            self.run_publish(replace=fail)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"new")

    def test_temp_flush_lseek_fsync_and_close_failures_cleanup(self):
        original = P.tempfile.NamedTemporaryFile
        for phase in ("flush", "lseek", "fsync", "close"):
            with self.subTest(phase=phase):
                def factory(*args, **kwargs):
                    stream = original(*args, **kwargs)
                    if phase == "flush":
                        stream.flush = mock.Mock(side_effect=OSError("injected flush failure"))
                    if phase == "close":
                        close = stream.close
                        def fail_close():
                            close()
                            raise OSError("injected close failure")
                        stream.close = fail_close
                    return stream
                with mock.patch.object(P.tempfile, "NamedTemporaryFile", factory):
                    if phase in ("lseek", "fsync"):
                        with mock.patch.object(P.os, phase, side_effect=OSError(f"injected {phase} failure")):
                            with self.assertRaisesRegex(OSError, f"injected {phase}"):
                                self.run_publish()
                    else:
                        with self.assertRaisesRegex(OSError, f"injected {phase}"):
                            self.run_publish()
                self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")
                self.assertFalse(list(self.repo.rglob(".release-*")))

    def test_partial_write_and_close_failure_keeps_original_error(self):
        original = P.tempfile.NamedTemporaryFile
        def factory(*args, **kwargs):
            stream = original(*args, **kwargs)
            write, close = stream.write, stream.close
            def fail_write(data):
                write(data[:1])
                raise OSError("original partial write failure")
            def fail_close():
                close()
                raise OSError("secondary close failure")
            stream.write, stream.close = fail_write, fail_close
            return stream
        with mock.patch.object(P.tempfile, "NamedTemporaryFile", factory):
            with self.assertRaisesRegex(OSError, "original partial write failure") as error:
                self.run_publish()
        self.assertIn("manual recovery", str(error.exception))
        self.assertFalse(list(self.repo.rglob(".release-*")))
        ledger = next((self.repo / "target/release-transactions").glob("*/recovery.json"))
        self.assertTrue(json.loads(ledger.read_text())["cleanupErrors"])

    def test_unlink_failure_reports_residual_without_masking_root(self):
        unlink = Path.unlink
        def fail_unlink(path, *args, **kwargs):
            if path.name.startswith(".release-"):
                raise OSError("secondary unlink failure")
            return unlink(path, *args, **kwargs)
        def fail_write(stream, write, data):
            write(data[:1])
            raise OSError("original private-body-not-for-ledger")
        with self.temp_writer(fail_write), mock.patch.object(Path, "unlink", fail_unlink):
            with self.assertRaisesRegex(OSError, "original private-body-not-for-ledger") as error:
                self.run_publish()
        self.assertIn("manual recovery", str(error.exception))
        ledger = next((self.repo / "target/release-transactions").glob("*/recovery.json"))
        report = json.loads(ledger.read_text())
        self.assertEqual(report["status"], "manual-recovery-required")
        self.assertTrue(report["residualArtifacts"])
        self.assertTrue(report["cleanupErrors"])
        self.assertNotIn("private-body-not-for-ledger", ledger.read_text())
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")

    def test_external_new_file_at_deleted_path_is_not_restored_over(self):
        name = "dist/terraphim-editor-123456789abcdef0.js"
        count = 0
        def fail(source, destination):
            nonlocal count
            count += 1
            if count == 2:
                self.write(name, b"external recreation")
                raise OSError("injected after owned deletion")
            os.replace(source, destination)
        with self.assertRaisesRegex(OSError, "manual recovery"):
            self.run_publish(replace=fail)
        self.assertEqual((self.repo / name).read_bytes(), b"external recreation")
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")

    def test_rollback_rechecks_after_preparing_restore_temp(self):
        original = P.tempfile.NamedTemporaryFile
        def factory(*args, **kwargs):
            stream = original(*args, **kwargs)
            if kwargs.get("prefix") == ".rollback-":
                write = stream.write
                def edit(data):
                    result = write(data)
                    self.write("dist/index.html", b"external during rollback")
                    return result
                stream.write = edit
            return stream
        def fail_report(removed):
            raise OSError("original report failure")
        with mock.patch.object(P.tempfile, "NamedTemporaryFile", factory):
            with self.assertRaisesRegex(OSError, "original report failure") as error:
                self.run_publish(finalize=fail_report)
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"external during rollback")
        self.assertIn("manual recovery", str(error.exception))
        self.assertFalse(list(self.repo.rglob(".rollback-*")))

    def test_partial_rollback_temp_failure_reports_manual_recovery(self):
        original = P.tempfile.NamedTemporaryFile
        def factory(*args, **kwargs):
            stream = original(*args, **kwargs)
            if kwargs.get("prefix") == ".rollback-":
                write = stream.write
                def fail(data):
                    write(data[:1])
                    raise OSError("secondary rollback write failure")
                stream.write = fail
            return stream
        def fail_report(removed):
            raise OSError("original report failure")
        with mock.patch.object(P.tempfile, "NamedTemporaryFile", factory):
            with self.assertRaisesRegex(OSError, "original report failure") as error:
                self.run_publish(finalize=fail_report)
        self.assertIn("manual recovery", str(error.exception))
        self.assertFalse(list(self.repo.rglob(".rollback-*")))
        ledger = next((self.repo / "target/release-transactions").glob("*/recovery.json"))
        report = json.loads(ledger.read_text())
        self.assertEqual(report["status"], "manual-recovery-required")
        self.assertTrue(report["conflicts"])
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"new")

    def test_rollback_symlink_ancestor_is_never_followed(self):
        self.write_stage("js/worker.js", b"worker")
        self.write("external/index.html", b"external user bytes")
        count = 0
        def fail(source, destination):
            nonlocal count
            count += 1
            if count == 2:
                os.rename(self.repo / "dist", self.repo / "moved-dist")
                (self.repo / "dist").symlink_to(self.repo / "external", target_is_directory=True)
                raise OSError("original symlink failure")
            os.replace(source, destination)
        with self.assertRaisesRegex(OSError, "original symlink failure") as error:
            self.run_publish(replace=fail)
        self.assertEqual((self.repo / "external/index.html").read_bytes(), b"external user bytes")
        self.assertIn("manual recovery", str(error.exception))

    def test_manifest_ancestor_symlinks_rejected(self):
        self.write("external/approval.json", b'{"schema":1,"files":[]}')
        (self.repo / "approval-dir").symlink_to(self.repo / "external", target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            P.load_manifest(self.repo / "approval-dir/approval.json")
        with self.assertRaisesRegex(ValueError, "symlink"):
            P.write_metadata(self.repo, self.repo / "approval-dir/output.json", {"schema": 1})
        self.assertFalse((self.repo / "external/output.json").exists())

    def test_metadata_partial_write_failure_cleanup_and_substitution(self):
        path = self.repo / "report.json"
        write = P.os.write
        count = 0
        def fail(fd, data):
            nonlocal count
            count += 1
            if count == 1:
                return write(fd, data[:1])
            raise OSError("original metadata write failure")
        with mock.patch.object(P.os, "write", fail):
            with self.assertRaisesRegex(OSError, "original metadata write failure"):
                P.write_metadata(self.repo, path, {"schema": 1})
        self.assertFalse(path.exists())
        def substitute(fd):
            path.unlink()
            path.write_bytes(b"external report")
            raise OSError("original metadata fsync failure")
        with mock.patch.object(P.os, "fsync", substitute):
            with self.assertRaisesRegex(OSError, "original metadata fsync failure") as error:
                P.write_metadata(self.repo, path, {"schema": 1})
        self.assertEqual(path.read_bytes(), b"external report")
        self.assertTrue(error.exception.recovery_conflicts)

    def test_non_directory_ancestor_preflight(self):
        self.write("package", b"protected file")
        with self.assertRaisesRegex(ValueError, "non-directory ancestor"):
            self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")

    def test_traversal_duplicate_and_invalid_hash_manifest(self):
        for name in ("../x", "/x", "x/../y", "x//y", "./x", "x\\y"):
            with self.assertRaises(ValueError):
                P.entries([{"path": name, "sha256": "a" * 64}])
        with self.assertRaises(ValueError):
            P.entries([{"path": "x", "sha256": "bad"}])
        with self.assertRaises(ValueError):
            P.entries([{"path": "x", "sha256": "a" * 64}] * 2)

    def test_source_hash_and_source_symlink_refused(self):
        with self.assertRaisesRegex(ValueError, "SHA mismatch"):
            P.publish(self.repo, self.stage, {"index.html": "a" * 64}, ["dist"])
        (self.stage / "index.html").unlink()
        (self.stage / "index.html").symlink_to(self.repo / "dist/index.html")
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.run_publish()

    def test_destination_symlink_refused(self):
        (self.repo / "package").symlink_to(self.stage, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            self.run_publish()
        self.assertEqual((self.repo / "dist/index.html").read_bytes(), b"old")

    def production_shape(self):
        for name in P.FIXED | {"terraphim-editor-abcdef1234567890.js", "terraphim-editor-abcdef1234567890_bg.wasm"}:
            self.write_stage(name, b"synthetic")
        self.write_stage("vendor/manifest.json", json.dumps({"schema": 1, "files": []}).encode())

    def test_inventory_curated_hashes_and_rejects_private_or_harness(self):
        self.production_shape()
        actual = P.inventory(self.stage)
        self.assertEqual(actual["index.html"], P.digest(b"synthetic"))
        for name in ("browser-test.html", "rawsearch.json", "corpus/private.txt", "demos/out/video.mp4"):
            self.write_stage(name, b"not production")
            with self.assertRaisesRegex(ValueError, "non-production"):
                P.inventory(self.stage)
            (self.stage / name).unlink()

    def test_cli_actual_rejection_exit_status(self):
        result = subprocess.run(["python3", str(ROOT / "scripts/publish-editor-dist.py"),
                                 "--stage", str(self.stage), "--manifest", str(self.repo / "report.json")],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("isolated target/release-staging", result.stderr)

    def test_inventory_rejects_orphan_snippet_and_symlink_directory(self):
        self.production_shape()
        self.write_stage("snippets/browser-harness/inline0.js", b"test")
        with self.assertRaisesRegex(ValueError, "non-production"):
            P.inventory(self.stage)
        (self.stage / "snippets/browser-harness/inline0.js").unlink()
        (self.stage / "linked").symlink_to(self.repo / "dist", target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "symlink"):
            P.inventory(self.stage)


if __name__ == "__main__":
    unittest.main()
