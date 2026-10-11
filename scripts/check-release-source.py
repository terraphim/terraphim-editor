#!/usr/bin/env python3
"""Refuse release builds from source bytes that are not a clean Git commit."""
import argparse
from pathlib import Path
import subprocess


def check_source(repo, expected_head=None):
    """Return the clean source HEAD, rejecting changed or untracked release inputs."""
    repo = Path(repo).resolve()
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip()
    if expected_head is not None and head != expected_head:
        raise ValueError("source HEAD changed; rebuild from the admitted commit")
    status = subprocess.check_output(
        ["git", "status", "--porcelain", "--untracked-files=all"], cwd=repo
    )
    if status:
        raise ValueError("release source contains modified or untracked files; admit source first")
    required = ["public/roles/default.json", "scripts/build-dist.sh", "scripts/publish-editor-dist.py"]
    for name in required:
        subprocess.run(["git", "ls-files", "--error-unmatch", "--", name], cwd=repo,
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if (repo / name).is_symlink() or not (repo / name).is_file():
            raise ValueError(f"release input must be a tracked regular file: {name}")
    return head


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--expected-head")
    args = parser.parse_args()
    try:
        print(check_source(args.repo, args.expected_head))
    except (ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release source refused: {error}\n")


if __name__ == "__main__":
    main()
