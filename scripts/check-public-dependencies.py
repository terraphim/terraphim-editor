#!/usr/bin/env python3
"""Reject private dependencies and temporary path-patched release lockfiles."""
import argparse
from pathlib import Path
import re
import sys
import tomllib

CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
WORKSPACE = {"terraphim-editor", "terraphim_alternatives", "terraphim_lab", "terraphim_rolegraph_browser"}
REQUIRED = {
    "terraphim_types": "1.22.3",
    "terraphim-markdown-parser": "1.20.5",
    "terraphim_automata": "2.1.0",
    "terraphim_rolegraph_core": "0.1.1",
    "terraphim_rolegraph": "2.0.1",
    "terraphim_lsp_core": "1.23.1",
}


def problems(lock):
    """Return failures for package source, archive identity and required versions."""
    issues = []
    found = set()
    for package in lock.get("package", []):
        name = package["name"]
        version = package["version"]
        source = package.get("source")
        if source is None:
            if name not in WORKSPACE:
                issues.append(f"{name}: unpublished local dependency (operator path patch)")
        elif source != CRATES_IO:
            issues.append(f"{name}: source is not the public crates.io registry")
        elif re.fullmatch(r"[0-9a-f]{64}", package.get("checksum", "")) is None:
            issues.append(f"{name}: missing or invalid archive checksum")
        if name in REQUIRED:
            if version != REQUIRED[name]:
                issues.append(f"{name}: unexpected version {version}; expected {REQUIRED[name]}")
            elif source == CRATES_IO:
                found.add(name)
    for name in sorted(REQUIRED.keys() - found):
        issues.append(f"{name} {REQUIRED[name]}: missing published registry identity")
    return issues


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lock", type=Path, default=Path(__file__).resolve().parents[1] / "Cargo.lock")
    args = parser.parse_args()
    try:
        issues = problems(tomllib.loads(args.lock.read_text()))
    except (OSError, tomllib.TOMLDecodeError, KeyError) as error:
        print(f"public dependencies: cannot validate lockfile: {error}", file=sys.stderr)
        return 1
    if issues:
        print("public dependencies: NOT QUALIFIED\n" + "\n".join(issues), file=sys.stderr)
        return 1
    print("public dependency lockfile admitted; Cargo must fetch and verify archives")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
