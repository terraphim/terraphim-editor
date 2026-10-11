#!/usr/bin/env python3
"""Validate isolated release assets and publish without deleting unrelated extras.

External schema-1 metadata deliberately does not hash itself. No Git index writes.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys
import tempfile

HASH_ASSET = re.compile(
    r"(?:terraphim-editor-[0-9a-f]+(?:\.js|_bg\.wasm)|"
    r"(?:tokens|write-on|selection-menu|lab|blocks|alternatives-panel|overflow|persistence|trim)-[0-9a-f]{1,16}\.css)")
# wasm-bindgen emits these declarations with the worker bundle. They are
# optional metadata, admitted by exact name rather than a broad suffix rule.
DECLARATIONS = {"rolegraph/terraphim_rolegraph_browser.d.ts",
                "rolegraph/terraphim_rolegraph_browser_bg.wasm.d.ts"}
FIXED = {"index.html", "example.html", "terraphim-editor.min.js", "terraphim-editor.min.css",
         "terraphim_editor.js", "terraphim_editor_bg.wasm", "js/rolegraph-worker.js",
         "rolegraph/rolegraph-worker.js", "rolegraph/terraphim_rolegraph_browser.js",
         "rolegraph/terraphim_rolegraph_browser_bg.wasm",
         # The default role (issue #123): the editor embeds the same bytes at
         # compile time, and publishers ship them so a reader outside the WASM
         # module (terraphim-grep, a host) can take them with --thesaurus.
         "roles/default.json"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def relative(value):
    if not isinstance(value, str) or not value or "\\" in value:
        raise ValueError(f"invalid relative path: {value!r}")
    if PurePosixPath(value).is_absolute() or any(p in ("", ".", "..") for p in value.split("/")):
        raise ValueError(f"unsafe relative path: {value}")
    return value


def safe_path(root, name):
    path = root / relative(name)
    for item in (root, *root.parents, path, *path.parents):
        if item.is_symlink():
            raise ValueError(f"symlink forbidden: {item}")
    for parent in path.parents:
        if parent.exists() and not parent.is_dir():
            raise ValueError(f"non-directory ancestor: {parent}")
    return path


def entries(items):
    result = {}
    for item in items:
        name, sha = relative(item["path"]), item["sha256"]
        if name in result or not isinstance(sha, str) or not re.fullmatch(r"[0-9a-f]{64}", sha):
            raise ValueError(f"duplicate path or invalid SHA256: {name}")
        result[name] = sha
    return result


def inventory(stage):
    """All staged files must be production assets; unexpected files fail closed."""
    vendor = json.loads(safe_path(stage, "vendor/manifest.json").read_text())
    allowed = FIXED | {"vendor/manifest.json"} | {"vendor/" + p for p in entries(vendor["files"])}
    # Only bindgen snippets reachable from the generated glue are production.
    pending = list(stage.glob("terraphim-editor-*.js")) + [stage / "terraphim_editor.js", stage / "rolegraph/terraphim_rolegraph_browser.js"]
    visited = set()
    while pending:
        glue_path = pending.pop()
        glue_name = glue_path.relative_to(stage).as_posix()
        safe_path(stage, glue_name)
        if glue_name in visited:
            continue
        visited.add(glue_name)
        for reference in re.findall(r'''["'](\./snippets/[^"']+\.js)["']''', glue_path.read_text()):
            name = (glue_path.parent / reference).relative_to(stage).as_posix()
            safe_path(stage, name)
            allowed.add(name)
            pending.append(stage / name)
    files = {}
    for path in sorted(stage.rglob("*")):
        name = path.relative_to(stage).as_posix()
        safe_path(stage, name)
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError(f"nonregular stage entry: {name}")
        if name not in allowed | DECLARATIONS and not HASH_ASSET.fullmatch(name):
            raise ValueError(f"unexpected non-production stage file: {name}")
        files[name] = digest(path.read_bytes())
    missing = allowed - files.keys()
    if missing:
        raise ValueError(f"missing production files: {sorted(missing)}")
    glue = [p for p in files if re.fullmatch(r"terraphim-editor-[0-9a-f]+\.js", p)]
    wasm = [p for p in files if re.fullmatch(r"terraphim-editor-[0-9a-f]+_bg\.wasm", p)]
    if len(glue) != 1 or len(wasm) != 1 or glue[0][:-3] + "_bg.wasm" not in files:
        raise ValueError("expected exactly one hashed editor JS/WASM pair")
    return files


def git(repo, *args):
    return subprocess.check_output(["git", "-C", str(repo), *args])


def head_assets(repo, destination):
    names = git(repo, "ls-tree", "-rz", "--name-only", "HEAD", "--", destination).split(b"\0")
    prefix = destination + "/"
    return {n.decode()[len(prefix):]: digest(git(repo, "show", "HEAD:" + n.decode()))
            for n in names if n and n.decode().startswith(prefix)}


def load_manifest(path):
    path = Path(os.path.abspath(path))
    data = json.loads(safe_path(path.parent, path.name).read_text())
    if data.get("schema") != 1:
        raise ValueError("expected manifest schema 1")
    return data


def identity(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns,
            info.st_ctime_ns, info.st_mode, info.st_nlink)


def snapshot(repo, path):
    """Bind bytes to an open regular file and confirm its path still names it.

    This detects observed changes, not an atomic compare-and-swap guarantee.
    """
    safe_path(repo, path.relative_to(repo).as_posix())
    try:
        named = path.lstat()
    except FileNotFoundError:
        return None
    if not stat.S_ISREG(named.st_mode):
        raise ValueError(f"nonregular destination: {path.relative_to(repo)}")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        before = os.fstat(fd)
        chunks = []
        while True:
            chunk = os.read(fd, 1024 * 1024)
            if not chunk:
                break
            chunks.append(chunk)
        after = os.fstat(fd)
        safe_path(repo, path.relative_to(repo).as_posix())
        if identity(named) != identity(before) or identity(before) != identity(after) or identity(after) != identity(path.lstat()):
            raise ValueError(f"file changed during snapshot: {path.relative_to(repo)}")
        data = b"".join(chunks)
        return {"identity": identity(after), "sha256": digest(data), "data": data,
                "mode": stat.S_IMODE(after.st_mode)}
    finally:
        os.close(fd)


def require_state(repo, path, expected, label):
    if snapshot(repo, path) != expected:
        raise ValueError(f"{label}: {path.relative_to(repo)}")


def prepare_temp(repo, path, data, mode, prefix, artifacts, cleanup_errors):
    """Register inode ownership before write/seek/flush/close can fail."""
    safe_path(repo, path.relative_to(repo).as_posix())
    stream = tempfile.NamedTemporaryFile(dir=path.parent, prefix=prefix, delete=False)
    temp = Path(stream.name)
    artifact = {"path": temp, "inode": None}
    artifacts.append(artifact)
    root_error = None
    try:
        info = os.fstat(stream.fileno())
        artifact["inode"] = (info.st_dev, info.st_ino)
        if stream.write(data) != len(data):
            raise OSError("short temporary write")
        stream.flush()
        os.fchmod(stream.fileno(), mode)
        os.fsync(stream.fileno())
        os.lseek(stream.fileno(), 0, os.SEEK_SET)
        descriptor = os.fstat(stream.fileno())
    except BaseException as error:
        root_error = error
        raise
    finally:
        try:
            stream.close()
        except (OSError, ValueError) as close_error:
            if root_error is None:
                raise
            cleanup_errors.append({"path": temp.relative_to(repo).as_posix(), "errorType": type(close_error).__name__, "phase": "close"})
    prepared = snapshot(repo, temp)
    if prepared is None or prepared["identity"] != identity(descriptor) or prepared["data"] != data:
        raise ValueError(f"temporary file changed: {temp.relative_to(repo)}")
    return temp, prepared


def cleanup_artifacts(repo, artifacts, conflicts, cleanup_errors):
    """Unlink only paths still naming our created inode; never mask root errors."""
    for artifact in artifacts:
        path = artifact["path"]
        try:
            safe_path(repo, path.relative_to(repo).as_posix())
            try:
                info = path.lstat()
            except FileNotFoundError:
                continue
            if not stat.S_ISREG(info.st_mode) or (info.st_dev, info.st_ino) != artifact["inode"]:
                conflicts.append({"path": path.relative_to(repo).as_posix(), "reason": "temporary path no longer owned"})
                continue
            path.unlink()
        except (OSError, ValueError) as error:
            cleanup_errors.append({"path": path.relative_to(repo).as_posix(), "errorType": type(error).__name__})


def write_metadata(repo, path, report):
    """Exclusive report creation with identity-guarded partial-write cleanup."""
    safe_path(repo, path.relative_to(repo).as_posix())
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644)
    artifact = {"path": path, "inode": None}
    root_error = None
    conflicts, errors = [], []
    try:
        info = os.fstat(fd)
        artifact["inode"] = (info.st_dev, info.st_ino)
        data = (json.dumps(report, indent=2) + "\n").encode()
        offset = 0
        while offset < len(data):
            written = os.write(fd, data[offset:])
            if written == 0:
                raise OSError("short metadata write")
            offset += written
        os.fsync(fd)
    except BaseException as error:
        root_error = error
    finally:
        try:
            os.close(fd)
        except OSError as error:
            if root_error is None:
                root_error = error
            else:
                errors.append({"path": path.relative_to(repo).as_posix(), "errorType": type(error).__name__, "phase": "close"})
    if root_error is not None:
        cleanup_artifacts(repo, [artifact], conflicts, errors)
        root_error.recovery_conflicts = conflicts
        root_error.recovery_cleanup = errors
        raise root_error


def preserve_error(error, repo, transaction, conflicts, cleanup_errors, artifacts):
    """Recovery ledger contains paths/types/hashes, never private file bodies."""
    residual = []
    for artifact in artifacts:
        path = artifact["path"]
        try:
            path.lstat()
            residual.append(path.relative_to(repo).as_posix())
        except FileNotFoundError:
            pass
        except OSError as inspection_error:
            cleanup_errors.append({"path": path.relative_to(repo).as_posix(), "errorType": type(inspection_error).__name__, "phase": "residual inspection"})
    manual = bool(conflicts or cleanup_errors or residual)
    report = {"schema": 1, "status": "manual-recovery-required" if manual else "rollback-complete",
              "rootErrorType": type(error).__name__, "conflicts": conflicts,
              "cleanupErrors": cleanup_errors, "residualArtifacts": residual}
    ledger = transaction / "recovery.json"
    try:
        safe_path(repo, ledger.relative_to(repo).as_posix())
        with ledger.open("x") as output:
            json.dump(report, output, indent=2)
            output.write("\n")
    except (OSError, ValueError):
        manual = True
    if manual:
        error.args = (f"{error}; manual recovery required; recovery metadata: {ledger.relative_to(repo)}; retained backups: {transaction.relative_to(repo)}",)


def publish(repo, stage, files, destinations, approved=None, prior=None, dry_run=False, replace=os.replace, finalize=None):
    """Preflight and back up the entire batch before any destination mutation.

    Caller must validate production closure. Tests call this seam with synthetic
    assets, not as production release qualification. Replacements are file-atomic.
    """
    approved, prior = approved or {}, prior or {}
    payloads = {}
    for name, sha in files.items():
        payloads[name] = safe_path(stage, name).read_bytes()
        if digest(payloads[name]) != sha:
            raise ValueError(f"stage SHA mismatch: {name}")
    operations, collisions, states = [], [], {}
    for destination in destinations:
        root = safe_path(repo, destination)
        tracked = head_assets(repo, destination)
        old_owned = prior.get(destination, {})
        for name, sha in files.items():
            path = safe_path(root, name)
            states[path] = snapshot(repo, path)
            before = states[path]["data"] if states[path] is not None else None
            if name in tracked and (before is None or digest(before) != tracked[name]):
                collisions.append(f"{destination}/{name}: dirty or deleted tracked file")
                continue
            if before is not None and digest(before) != sha:
                before_sha = digest(before)
                if name in tracked and before_sha != tracked[name]:
                    collisions.append(f"{destination}/{name}: dirty tracked file")
                    continue
                if name not in tracked and before_sha != old_owned.get(name) and before_sha != approved.get(destination + "/" + name):
                    collisions.append(f"{destination}/{name}: unknown collision; needs path + expected-before SHA256 approval ({before_sha})")
                    continue
            operations.append((path, before, payloads[name]))
        for name, sha in {**old_owned, **tracked}.items():
            if name in files or not HASH_ASSET.fullmatch(name):
                continue
            path = safe_path(root, name)
            if not path.exists():
                continue
            states[path] = snapshot(repo, path)
            if states[path] is None or states[path]["sha256"] != sha:
                collisions.append(f"{destination}/{name}: dirty obsolete owned asset")
            else:
                operations.append((path, states[path]["data"], None))
    if collisions:
        raise ValueError("preflight refused; no destination writes:\n" + "\n".join(collisions))
    if dry_run:
        return []
    transaction_root = safe_path(repo, "target/release-transactions")
    transaction_root.mkdir(parents=True, exist_ok=True)
    transaction = Path(tempfile.mkdtemp(prefix="publish-", dir=transaction_root))
    modes = {path: states[path]["mode"] for path, before, _ in operations if before is not None}
    for i, (_, before, after) in enumerate(operations):
        if before is not None:
            (transaction / f"{i}.before").write_bytes(before)
        if after is not None:
            (transaction / f"{i}.after").write_bytes(after)
    (transaction / "operations.json").write_text(json.dumps([
        {"path": str(p.relative_to(repo)), "before": None if b is None else digest(b),
         "after": None if a is None else digest(a)} for p, b, a in operations], indent=2) + "\n")
    applied, created_dirs, artifacts, conflicts, cleanup_errors = [], [], [], [], []
    def check(path, expected, label):
        try:
            require_state(repo, path, expected, label)
        except (OSError, ValueError):
            conflicts.append({"path": path.relative_to(repo).as_posix(), "reason": label})
            raise
    try:
        for i, (path, before, after) in enumerate(operations):
            if after == before:
                check(path, states[path], "destination changed after preflight")
                continue
            if after is None:
                check(path, states[path], "destination changed after preflight")
                path.unlink()
                applied.append((i, path, before, None))
                states[path] = None
            else:
                missing, parent = [], path.parent
                while not parent.exists():
                    missing.append(parent)
                    parent = parent.parent
                for parent in reversed(missing):
                    safe_path(repo, parent.relative_to(repo).as_posix())
                    parent.mkdir()
                    info = parent.lstat()
                    created_dirs.append((parent, (info.st_dev, info.st_ino)))
                temp, prepared = prepare_temp(repo, path, after, modes.get(path, 0o644), ".release-", artifacts, cleanup_errors)
                require_state(repo, temp, prepared, "temporary file changed before install")
                # No directory creation, writing, chmod or flush between this check
                # and rename. os.replace still is NOT a filesystem compare-and-swap.
                check(path, states[path], "destination changed after preflight")
                replace(temp, path)
                applied.append((i, path, before, prepared))
                installed = snapshot(repo, path)
                if installed is None or installed["identity"][:2] != prepared["identity"][:2] or installed["data"] != after:
                    raise ValueError(f"destination changed during install: {path.relative_to(repo)}")
                applied[-1] = (i, path, before, installed)
                states[path] = installed
        for path, _, after in operations:
            check(path, states[path], "destination changed during verification")
        cleanup_artifacts(repo, artifacts, conflicts, cleanup_errors)
        if conflicts or cleanup_errors:
            raise OSError("temporary cleanup requires manual recovery")
        if finalize:
            finalize([str(path.relative_to(repo)) for path, _, after in operations if after is None])
    except BaseException as error:
        conflicts.extend(getattr(error, "recovery_conflicts", []))
        cleanup_errors.extend(getattr(error, "recovery_cleanup", []))
        for i, path, before, postimage in reversed(applied):
            try:
                # Check before preparing rollback and again immediately before its
                # mutation. Preserve any edit/substitution of an installed asset.
                require_state(repo, path, postimage, "rollback postimage no longer owned")
                if before is None:
                    path.unlink()
                else:
                    backup = safe_path(repo, (transaction / f"{i}.before").relative_to(repo).as_posix()).read_bytes()
                    if backup != before:
                        raise ValueError("rollback backup changed")
                    temp, prepared = prepare_temp(repo, path, before, modes[path], ".rollback-", artifacts, cleanup_errors)
                    require_state(repo, temp, prepared, "rollback temporary no longer owned")
                    require_state(repo, path, postimage, "rollback postimage no longer owned")
                    os.replace(temp, path)
            except (OSError, ValueError) as recovery_error:
                conflicts.append({"path": path.relative_to(repo).as_posix(), "reason": "rollback skipped or failed", "errorType": type(recovery_error).__name__})
        cleanup_artifacts(repo, artifacts, conflicts, cleanup_errors)
        for directory, inode in reversed(created_dirs):
            try:
                safe_path(repo, directory.relative_to(repo).as_posix())
                info = directory.lstat()
                if not stat.S_ISDIR(info.st_mode) or (info.st_dev, info.st_ino) != inode:
                    raise ValueError("created directory no longer owned")
                directory.rmdir()
            except (OSError, ValueError) as cleanup_error:
                cleanup_errors.append({"path": directory.relative_to(repo).as_posix(), "errorType": type(cleanup_error).__name__})
        preserve_error(error, repo, transaction, conflicts, cleanup_errors, artifacts)
        raise
    return [str(path.relative_to(repo)) for path, _, after in operations if after is None]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stage", required=True)
    parser.add_argument("--manifest", required=True, help="external target/release-manifests/*.json report")
    parser.add_argument("--allow-replace", help="schema 1 files: destination path + sha256 of expected BEFORE bytes")
    parser.add_argument("--prior-owned", help="previous successful publication report")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--source-head", help="HEAD captured before the isolated build; reject if it changed")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    stage = Path(os.path.abspath(args.stage))
    target = safe_path(repo, "target")
    if stage.parent != target or not stage.name.startswith("release-staging-") or not stage.is_dir():
        raise ValueError("stage must be an isolated target/release-staging-ID directory")
    safe_path(repo, stage.relative_to(repo).as_posix())
    manifest = Path(os.path.abspath(args.manifest))
    if manifest.parent != target / "release-manifests" or manifest.suffix != ".json":
        raise ValueError("metadata must be outside delivery trees in target/release-manifests/*.json")
    safe_path(repo, manifest.relative_to(repo).as_posix())
    if manifest.exists():
        raise ValueError("refusing to overwrite an existing publication report")
    if os.environ.get("TE_ROLEGRAPH_SOURCE"):
        raise ValueError("TE_ROLEGRAPH_SOURCE is forbidden for releases")
    # Verify both direct invocations and release-dist builds against admitted source.
    source_check = [sys.executable, "scripts/check-release-source.py"]
    if args.source_head:
        source_check.extend(["--expected-head", args.source_head])
    subprocess.run(source_check, cwd=repo, check=True, stdout=subprocess.DEVNULL)
    files = inventory(stage)
    for command in (["scripts/ci/rolegraph-production-check.py", "assets", str(stage)],
                    ["scripts/ci/runtime-assets-check.py", "--check", str(stage / "vendor")]):
        subprocess.run([sys.executable, *command], cwd=repo, check=True)
    approved = entries(load_manifest(args.allow_replace)["files"]) if args.allow_replace else {}
    prior = {}
    if args.prior_owned:
        previous = load_manifest(args.prior_owned)
        if previous.get("status") != "published":
            raise ValueError("prior ownership requires a successful publication report")
        prior = {d: entries(previous["files"]) for d in previous["destinations"]}
    destinations = ["dist", "package/dist"]
    source_head = git(repo, "rev-parse", "HEAD").decode().strip()
    if args.source_head and args.source_head != source_head:
        raise ValueError("source HEAD changed during build; rebuild from the frozen source")
    dirty = git(repo, "status", "--porcelain", "--untracked-files=normal").decode().splitlines()
    target.mkdir(exist_ok=True)
    with safe_path(repo, "target/release-publish.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        report = {"schema": 1, "status": "preflight-only" if args.dry_run else "published",
                  "sourceHead": source_head, "sourceStatus": dirty, "stage": str(stage.relative_to(repo)),
                  "destinations": destinations, "removed": [],
                  "files": [{"path": p, "sha256": sha, "bytes": safe_path(stage, p).stat().st_size} for p, sha in sorted(files.items())]}
        safe_path(repo, manifest.relative_to(repo).as_posix())
        manifest.parent.mkdir(parents=True, exist_ok=True)
        def finalize(removed):
            report["removed"] = removed
            write_metadata(repo, manifest, report)
        publish(repo, stage, files, destinations, approved, prior, args.dry_run, finalize=finalize)
        if args.dry_run:
            finalize([])
    print(f"release publication {report['status']}: {len(files)} assets -> {', '.join(destinations)}; metadata {manifest}")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        sys.exit(f"publish-editor-dist: {error}")
