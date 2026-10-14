#!/usr/bin/env python3
"""Validate reviewed public packages and verify immutable registry readback."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib
import urllib.error
import urllib.request

NAME = re.compile(r"[A-Za-z0-9_-]+")
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?")
MAX_CRATE_BYTES = 10 * 1024 * 1024


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "Duplicate JSON key")
            result[key] = value
        return result
    return json.loads(Path(path).read_text(), object_pairs_hook=unique)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def load_plan(root):
    plan = read_json(root / "release/crates.json")
    require(set(plan) == {"schema", "repository", "owner", "toolchain", "packages", "prerequisites"}, "Unexpected plan fields")
    require(type(plan["schema"]) is int and plan["schema"] == 1, "Unsupported plan")
    require(re.fullmatch(r"terraphim/[A-Za-z0-9_.-]+", plan["repository"]), "Unexpected repository")
    require(NAME.fullmatch(plan["owner"]) is not None, "Invalid owner")
    require(re.fullmatch(r"[0-9]+\.[0-9]+(?:\.[0-9]+)?", plan["toolchain"]), "Unpinned toolchain")
    require(isinstance(plan["packages"], list) and plan["packages"], "Empty package allowlist")
    names = set()
    for item in plan["packages"]:
        require(set(item) == {"name", "version", "path", "source"}, "Unexpected package fields")
        require(NAME.fullmatch(item["name"]) and VERSION.fullmatch(item["version"]), "Invalid package identity")
        require(item["name"] not in names, "Duplicate package")
        names.add(item["name"])
        require(item["path"] == "crates/" + item["name"] + "/Cargo.toml", "Unexpected manifest path")
        source = item["source"]
        if source is not None:
            require(set(source) == {"commit", "tree", "lockSha256"}, "Unexpected approved source fields")
            require(re.fullmatch(r"[0-9a-f]{40}", source["commit"]) and re.fullmatch(r"[0-9a-f]{40}", source["tree"]) and re.fullmatch(r"[0-9a-f]{64}", source["lockSha256"]), "Invalid approved source identity")
    require(isinstance(plan["prerequisites"], list), "Invalid prerequisites")
    for item in plan["prerequisites"]:
        require(set(item) == {"name", "version"}, "Unexpected prerequisite fields")
        require(NAME.fullmatch(item["name"]) and VERSION.fullmatch(item["version"]), "Invalid prerequisite")
        require(item["name"] not in names, "Duplicate prerequisite")
        names.add(item["name"])
    return plan


def selected_package(plan, tag):
    matches = [p for p in plan["packages"] if tag == p["name"] + "-v" + p["version"]]
    require(len(matches) == 1, "Tag is not an allowed exact package version")
    return matches[0]


def approved_source(root, plan, item, tag):
    source = item["source"]
    require(source is not None, "Package source is not yet independently reviewed and approved on main")
    require(git(root, "rev-parse", "refs/tags/" + tag + "^{commit}") == source["commit"], "Moved or unapproved source tag")
    directory = str(PurePosixPath(item["path"]).parent)
    require(git(root, "rev-parse", source["commit"] + ":" + directory) == source["tree"], "Approved package tree differs")
    lock = subprocess.check_output(["git", "-C", str(root), "show", source["commit"] + ":" + directory + "/Cargo.lock"])
    require(hashlib.sha256(lock).hexdigest() == source["lockSha256"], "Approved package lockfile differs")
    # A later Lab source must contain the exact alternatives tree that was
    # separately reviewed and published, not a changed version with the same number.
    for previous in plan["packages"][:plan["packages"].index(item)]:
        require(previous["source"] is not None, "Approve and publish earlier package sources first")
        path = str(PurePosixPath(previous["path"]).parent)
        require(git(root, "rev-parse", source["commit"] + ":" + path) == previous["source"]["tree"], "Sibling source differs from previously approved package")
    return source["commit"]


def validate_identity(root, plan, tag, env):
    item = selected_package(plan, tag)
    require(env.get("GITHUB_REPOSITORY") == plan["repository"], "Wrong repository")
    require(env.get("GITHUB_EVENT_NAME") == "workflow_dispatch", "Unsupported event")
    require(env.get("GITHUB_REF") == "refs/heads/main", "Publisher workflow must execute from main")
    publisher = git(root, "rev-parse", "HEAD")
    require(env.get("GITHUB_SHA") == publisher, "Publisher workflow/source mismatch")
    subprocess.run(["git", "-C", str(root), "merge-base", "--is-ancestor", publisher, "refs/remotes/origin/main"], check=True)
    require(not git(root, "status", "--porcelain"), "Dirty publisher source tree")
    return item, approved_source(root, plan, item, tag)


def dependencies(manifest):
    for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
        yield kind, manifest.get(kind, {})
    for target in manifest.get("target", {}).values():
        for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
            yield kind, target.get(kind, {})


def validate_public_lock(text, local_packages):
    lock = tomllib.loads(text)
    require(lock.get("version") in {3, 4} and lock.get("package"), "Missing public dependency lock")
    for package in lock["package"]:
        if "source" not in package:
            require((package.get("name"), package.get("version")) in local_packages, "Unreviewed local lock dependency")
        else:
            require(package["source"] == "registry+https://github.com/rust-lang/crates.io-index", "Lock dependency is not crates.io")
            require(re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")), "Registry lock dependency lacks checksum")


def validate_manifests(root, plan, source_commit=None):
    def read(path):
        if source_commit is None:
            return (root / path).read_text()
        return subprocess.check_output(["git", "-C", str(root), "show", source_commit + ":" + path], text=True)
    manifest = tomllib.loads(read("Cargo.toml"))
    allowed = {p["name"]: {"path": str(PurePosixPath(p["path"]).parent), "version": p["version"]} for p in plan["packages"]}
    require(not manifest.get("replace") and manifest.get("patch", {}) in ({}, {"crates-io": allowed}), "Release source overrides are forbidden")
    config_path = root / ".cargo/config.toml"
    config_exists = config_path.exists() if source_commit is None else subprocess.run(["git", "-C", str(root), "cat-file", "-e", source_commit + ":.cargo/config.toml"], capture_output=True).returncode == 0
    if config_exists:
        config = tomllib.loads(read(".cargo/config.toml"))
        require(not config.get("source") and not config.get("registries") and config.get("registry", {}).get("default", "crates-io") == "crates-io", "Registry source override is forbidden")
    order = {p["name"]: i for i, p in enumerate(plan["packages"])}
    versions = {p["name"]: p["version"] for p in plan["packages"]}
    for item in plan["packages"]:
        path = root / item["path"]
        require(path.resolve().is_relative_to(root.resolve()) and not path.is_symlink(), "Manifest escapes checkout")
        data = tomllib.loads(read(item["path"]))
        require(not data.get("workspace") and not data.get("patch") and not data.get("replace"), "Package must be standalone without source overrides")
        require(not any(isinstance(v, dict) and v.get("workspace") for v in data["package"].values()), "Package metadata must not require its parent workspace")
        package = data["package"]
        require(package["name"] == item["name"] and package["version"] == item["version"], "Manifest/version mismatch")
        require(package.get("publish", ["crates-io"]) == ["crates-io"], "Package is not restricted to crates.io")
        require(package.get("repository") == "https://github.com/" + plan["repository"], "Package repository is not the public source")
        require(package.get("description") and (package.get("license") or package.get("license-file")), "Missing package metadata")
        for kind, table in dependencies(data):
            for name, dep in table.items():
                require(not isinstance(dep, dict) or not dep.get("workspace"), "Dependency must not require its parent workspace")
                if isinstance(dep, str):
                    dep = {"version": dep}
                require("git" not in dep and dep.get("registry", "crates-io") == "crates-io", "Dependency requires a non-public source")
                actual = dep.get("package", name)
                if actual in order:
                    require(order[actual] < order[item["name"]], "Dependency order is invalid")
                    require(dep.get("version", "").removeprefix("=") == versions[actual], "Dependency version differs from plan")
                    if "path" in dep:
                        expected_path = (root / "crates" / actual).resolve()
                        require((path.parent / dep["path"]).resolve() == expected_path, "Path dependency escapes reviewed package source")
                else:
                    require("path" not in dep, "Path dependency is not a reviewed package")
                    if actual.startswith("terraphim"):
                        prerequisite_versions = {p["name"]: p["version"] for p in plan["prerequisites"]}
                        require(actual in prerequisite_versions and dep.get("version", "").removeprefix("=") == prerequisite_versions[actual], "Unreviewed public prerequisite")


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def registry_get(path):
    require(path.startswith("/api/v1/crates/"), "Unexpected registry path")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    request = urllib.request.Request("https://crates.io" + path, headers={"User-Agent": "terraphim-release (https://github.com/terraphim/terraphim-editor)"})
    try:
        with opener.open(request, timeout=30) as response:
            data = response.read(4 * 1024 * 1024 + 1)
            require(len(data) <= 4 * 1024 * 1024, "Registry response too large")
            return response.status, json.loads(data)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return 404, {}
        raise ValueError("Registry read failed") from None


def authentication_policy(mode, status, owners, plan):
    """An API token is only admitted for a name which does not yet exist."""
    require(mode in {"trusted", "bootstrap"}, "Unknown authentication mode")
    require(status == (404 if mode == "bootstrap" else 200), "Authentication mode does not match crate existence")
    if mode == "trusted":
        require(any(p.get("login") == plan["owner"] for p in owners.get("users", [])), "Expected owner not found")


def validate_registry(plan, item, mode):
    status, _ = registry_get("/api/v1/crates/" + item["name"])
    owners = {}
    if mode == "trusted" and status == 200:
        owner_status, owners = registry_get("/api/v1/crates/" + item["name"] + "/owners")
        require(owner_status == 200, "Cannot verify owner")
    authentication_policy(mode, status, owners, plan)
    status, _ = registry_get("/api/v1/crates/" + item["name"] + "/" + item["version"])
    require(status == 404, "Version already exists; inspect immutable output before retrying")
    required = plan["prerequisites"] + plan["packages"][:plan["packages"].index(item)]
    for previous in required:
        status, data = registry_get("/api/v1/crates/" + previous["name"] + "/" + previous["version"])
        require(status == 200 and data.get("version", {}).get("yanked") is False, "Publish dependencies first in reviewed order")


def source_files(root, plan, head):
    """Read exact committed regular files, never local ignored files or config."""
    directories = [str(PurePosixPath(p["path"]).parent) for p in plan["packages"]]
    raw = subprocess.check_output(["git", "-C", str(root), "ls-tree", "-r", "-z", head, "--", *directories])
    files = {}
    for entry in raw.split(b"\0"):
        if not entry:
            continue
        metadata, name = entry.split(b"\t", 1)
        mode, kind, _ = metadata.decode().split()
        name = name.decode()
        require(mode in {"100644", "100755"} and kind == "blob", "Only regular source files may be staged")
        require(".cargo" not in PurePosixPath(name).parts, "Package-local source override is forbidden")
        require(not any(p in {".env", ".credentials", "credentials.toml", ".git"} for p in PurePosixPath(name).parts), "Forbidden source content")
        data = subprocess.check_output(["git", "-C", str(root), "show", head + ":" + name])
        files[name] = hashlib.sha256(data).hexdigest()
    require(files and len(files) <= 10000, "Invalid staged file count")
    return files


def check_staged_source(root, plan, head, destination):
    require(git(destination, "rev-parse", "HEAD") == head, "Staged source commit differs")
    require(not (destination / "Cargo.toml").exists() and not (destination / ".cargo").exists(), "Staged source inherited the editor workspace or Cargo config")
    files = source_files(root, plan, head)
    for name, expected in files.items():
        path = destination / name
        require(path.is_file() and not path.is_symlink() and path.resolve().is_relative_to(destination.resolve()) and digest(path) == expected, "Staged source bytes differ")
    require(not git(destination, "status", "--porcelain"), "Staged source is dirty")
    return files


def stage_source(root, plan, item, head, destination, cargo_home):
    """Sparse checkout retains Cargo's real commit/path provenance without the root workspace."""
    require(not destination.exists() and not destination.resolve().is_relative_to(root.resolve()), "Staging requires a new external directory")
    require(not cargo_home.exists() and not cargo_home.resolve().is_relative_to(root.resolve()), "Cargo home must be new and external")
    files = source_files(root, plan, head)
    lock = str(PurePosixPath(item["path"]).parent / "Cargo.lock")
    require(lock in files, "Commit the selected standalone public dependency lockfile before tagging")
    lock_text = subprocess.check_output(["git", "-C", str(root), "show", head + ":" + lock], text=True)
    validate_public_lock(lock_text, {(p["name"], p["version"]) for p in plan["packages"]})
    subprocess.run(["git", "clone", "--quiet", "--shared", "--no-checkout", str(root), str(destination)], check=True)
    subprocess.run(["git", "-C", str(destination), "sparse-checkout", "init", "--no-cone"], check=True)
    patterns = "".join("/" + str(PurePosixPath(p["path"]).parent) + "/\n" for p in plan["packages"])
    subprocess.run(["git", "-C", str(destination), "sparse-checkout", "set", "--no-cone", "--stdin"], input=patterns, text=True, check=True)
    subprocess.run(["git", "-C", str(destination), "checkout", "--quiet", "--detach", head], check=True)
    check_staged_source(root, plan, head, destination)
    cargo_home.mkdir(parents=True)
    return {"sourceCommit": head, "files": files, "cargoHomeFresh": True}


def inspect_package(path, item, source_commit):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size <= MAX_CRATE_BYTES, "Invalid package archive")
    prefix = item["name"] + "-" + item["version"]
    names, unpacked = [], 0
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            parts = PurePosixPath(member.name).parts
            require(member.isfile() and member.name == str(PurePosixPath(member.name)) and len(parts) > 1 and parts[0] == prefix and all(p not in {".", ".."} for p in parts), "Unsafe package member")
            require(member.name not in names and len(names) < 10000, "Duplicate or excessive package members")
            require(not any(p in {".env", ".credentials", "credentials.toml", ".git"} for p in parts), "Forbidden package content")
            unpacked += member.size
            require(unpacked <= 100 * 1024 * 1024, "Package expands beyond limit")
            names.append(member.name)
        normalized = tomllib.loads(archive.extractfile(prefix + "/Cargo.toml").read().decode())
        require(normalized["package"]["name"] == item["name"] and normalized["package"]["version"] == item["version"], "Archive identity differs")
        for _, table in dependencies(normalized):
            for dep in table.values():
                if isinstance(dep, dict):
                    require(not any(k in dep for k in ("git", "path", "registry", "registry-index")), "Packaged dependency is not crates.io-only")
        validate_public_lock(archive.extractfile(prefix + "/Cargo.lock").read().decode(), {(item["name"], item["version"])})
        provenance = json.load(archive.extractfile(prefix + "/.cargo_vcs_info.json"))
        require(provenance.get("git", {}).get("sha1") == source_commit and not provenance.get("git", {}).get("dirty", False), "Archive source commit differs")
        require(provenance.get("path_in_vcs") == str(PurePosixPath(item["path"]).parent), "Archive source path differs")
    require(prefix + "/.cargo_vcs_info.json" in names, "Missing Cargo source provenance")
    return {"sha256": digest(path), "bytes": path.stat().st_size, "files": sorted(names)}


def download_readback(item, expected):
    url = "https://static.crates.io/crates/" + item["name"] + "/" + item["name"] + "-" + item["version"] + ".crate"
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(urllib.request.Request(url, headers={"User-Agent": "terraphim-release (https://github.com/terraphim/terraphim-editor)"}), timeout=30) as response:
        raw = response.read(MAX_CRATE_BYTES + 1)
        require(response.status == 200 and len(raw) <= MAX_CRATE_BYTES, "Registry download failed")
    require(hashlib.sha256(raw).hexdigest() == expected["sha256"] and len(raw) == expected["bytes"], "Downloaded crate differs from qualified archive")
    return {"url": url, "sha256": expected["sha256"], "bytes": len(raw)}


def unpack_package(archive, item, source_commit, destination, source_root):
    """Validate first, then copy regular members into a new external directory."""
    inspect_package(archive, item, source_commit)
    require(not destination.exists() and not destination.resolve().is_relative_to(source_root.resolve()), "Unpacked tests require a new directory outside the source workspace")
    destination.mkdir(parents=True)
    with tarfile.open(archive, "r:gz") as package:
        for member in package:
            target = destination / member.name
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("xb") as output:
                output.write(package.extractfile(member).read())
    return destination / (item["name"] + "-" + item["version"])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["validate", "stage", "package", "unpack", "compare", "readback"])
    parser.add_argument("--root", type=Path, default=Path("."))
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--tag")
    selection.add_argument("--crate")
    parser.add_argument("--mode", choices=["trusted", "bootstrap"], default="trusted")
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--expected", type=Path)
    parser.add_argument("--unpack-dir", type=Path)
    parser.add_argument("--stage-dir", type=Path)
    parser.add_argument("--target-dir", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    plan = load_plan(root)
    if args.crate:
        candidates = [item for item in plan["packages"] if item["name"] == args.crate]
        require(len(candidates) == 1, "Crate is not in the approved plan")
        args.tag = candidates[0]["name"] + "-v" + candidates[0]["version"]
    item, head = validate_identity(root, plan, args.tag, os.environ)
    validate_manifests(root, plan, head)
    receipt = {"schema": 1, "status": "PASS", "repository": plan["repository"], "sourceCommit": head,
               "tag": args.tag, "publisherCommit": git(root, "rev-parse", "HEAD"), "package": item, "planSha256": digest(root / "release/crates.json")}
    if args.command == "validate":
        validate_registry(plan, item, args.mode)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as output:
                output.write("crate=" + item["name"] + "\nversion=" + item["version"] + "\ntag=" + args.tag + "\ntoolchain=" + plan["toolchain"] + "\n")
    elif args.command == "stage":
        require(args.stage_dir is not None and args.target_dir is not None, "Missing staging directories")
        require(not args.target_dir.exists() and not args.target_dir.resolve().is_relative_to(root), "Build target must be new and external")
        require(os.environ.get("CARGO_HOME"), "Explicit clean Cargo home is required")
        require(not any(k.startswith(("CARGO_SOURCE_", "CARGO_REGISTRIES_")) or k == "CARGO_REGISTRY_DEFAULT" for k in os.environ), "Cargo registry environment overrides are forbidden")
        receipt["staging"] = stage_source(root, plan, item, head, args.stage_dir, Path(os.environ["CARGO_HOME"]))
        args.target_dir.mkdir(parents=True)
    else:
        require(args.stage_dir is not None and args.target_dir is not None, "Missing staging directories")
        check_staged_source(root, plan, head, args.stage_dir)
        require(not args.target_dir.resolve().is_relative_to(root) and not args.target_dir.is_symlink(), "Build target must be external")
        archive = args.target_dir / "package" / (item["name"] + "-" + item["version"] + ".crate")
        receipt["archive"] = inspect_package(archive, item, head)
        if args.command == "unpack":
            require(args.unpack_dir is not None, "Missing isolated unpack directory")
            unpack_package(archive, item, head, args.unpack_dir, root)
        if args.command in {"compare", "readback"}:
            require(args.expected is not None and read_json(args.expected) == receipt, "Qualified package bytes/source changed")
        if args.command == "readback":
            status, data = registry_get("/api/v1/crates/" + item["name"] + "/" + item["version"])
            version = data.get("version", {})
            require(status == 200 and version.get("checksum") == receipt["archive"]["sha256"] and version.get("yanked") is False, "Published checksum/readback differs")
            receipt["registryReadback"] = {"checksum": version["checksum"], "yanked": False}
            receipt["downloadReadback"] = download_readback(item, receipt["archive"])
    args.receipt.write_text(json.dumps(receipt, indent=2) + "\n")


if __name__ == "__main__":
    main()
