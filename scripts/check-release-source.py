#!/usr/bin/env python3
"""Refuse release builds from source bytes that are not a clean Git commit."""
import argparse
import importlib.util
import tomllib
from urllib.parse import urlsplit
from pathlib import Path
import subprocess


PUBLIC_REPOSITORY = "https://github.com/terraphim/terraphim-editor"
SPEC = importlib.util.spec_from_file_location("public_dependencies", Path(__file__).with_name("check-public-dependencies.py"))
PUBLIC_DEPENDENCIES = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PUBLIC_DEPENDENCIES)


def check_public_origin(repo):
    """Read the origin internally; never echo a credential-bearing remote URL."""
    raw = subprocess.check_output(["git", "remote", "get-url", "origin"], cwd=repo, text=True, stderr=subprocess.DEVNULL).strip()
    if raw == "git@github.com:terraphim/terraphim-editor.git":
        return PUBLIC_REPOSITORY
    try:
        parsed = urlsplit(raw)
        port = parsed.port
    except ValueError:
        raise ValueError("release origin must identify the public Terraphim editor repository without embedded credentials") from None
    allowed_user = parsed.username is None if parsed.scheme == "https" else parsed.username == "git"
    if (parsed.scheme not in {"https", "ssh"} or parsed.hostname != "github.com"
            or not allowed_user or parsed.password or port is not None
            or parsed.path not in {"/terraphim/terraphim-editor", "/terraphim/terraphim-editor.git"}
            or parsed.query or parsed.fragment):
        raise ValueError("release origin must identify the public Terraphim editor repository without embedded credentials")
    return PUBLIC_REPOSITORY


def check_public_lock(repo):
    lock = tomllib.loads((repo / "Cargo.lock").read_text())
    if PUBLIC_DEPENDENCIES.problems(lock):
        raise ValueError("release requires the qualified public crates.io dependency lock")
    config = tomllib.loads((repo / ".cargo/config.toml").read_text())
    if config.get("source") or config.get("registries") or config.get("registry", {}).get("default", "crates-io") != "crates-io":
        raise ValueError("release Cargo configuration must not replace public dependency sources")


def check_source(repo, expected_head=None):
    """Return the clean source HEAD, rejecting changed or untracked release inputs."""
    repo = Path(repo).resolve()
    check_public_origin(repo)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    if expected_head is not None and head != expected_head:
        raise ValueError("source HEAD changed; rebuild from the admitted commit")
    status = subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=repo
    )
    if status:
        raise ValueError("release source contains modified or untracked files; admit source first")
    required = ["public/roles/default.json", "scripts/build-dist.sh", "scripts/publish-editor-dist.py",
                "Cargo.toml", "Cargo.lock", ".cargo/config.toml", "scripts/check-public-dependencies.py"]
    for name in required:
        subprocess.run(["git", "ls-files", "--error-unmatch", "--", name], cwd=repo,
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if (repo / name).is_symlink() or not (repo / name).is_file():
            raise ValueError(f"release input must be a tracked regular file: {name}")
    check_public_lock(repo)
    return head


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--expected-head")
    args = parser.parse_args()
    try:
        print(check_source(args.repo, args.expected_head))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release source refused: {error}\n")


if __name__ == "__main__":
    main()
