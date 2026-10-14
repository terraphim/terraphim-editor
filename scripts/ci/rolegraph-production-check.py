#!/usr/bin/env python3
"""Fail closed on production graph provenance or incomplete local assets."""
import json
from pathlib import Path
import re
import subprocess
import sys

REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
REPOSITORY = "https://github.com/terraphim/terraphim-core"
VERSIONS = {
    "terraphim_rolegraph": "2.0.1",
    "terraphim_types": "1.22.3",
    "terraphim_automata": "2.1.0",
    "terraphim_rolegraph_core": "0.1.1",
    "terraphim-markdown-parser": "1.20.5",
}


def metadata(data):
    for name, version in VERSIONS.items():
        matches = [p for p in data["packages"] if p["name"] == name]
        if len(matches) != 1 or matches[0]["version"] != version or matches[0]["source"] != REGISTRY or matches[0].get("repository") != REPOSITORY:
            raise ValueError(f"{name}: expected one published {version} from {REGISTRY} with public GitHub source")
    bridge = next(p for p in data["packages"] if p["name"] == "terraphim_rolegraph_browser")
    if bridge["id"] not in data["workspace_members"]:
        raise ValueError("bridge is not a root workspace member")


def require(root, relative):
    asset = root / relative
    if not asset.is_file() or asset.stat().st_size == 0:
        raise ValueError(f"missing or empty asset: {asset}")
    return asset


def graph_assets(root):
    glue = require(root, "terraphim_rolegraph_browser.js")
    require(root, "terraphim_rolegraph_browser_bg.wasm")
    # Bindgen emits local snippet imports; preserve/check every referenced file.
    for relative in re.findall(r'''["'](\./snippets/[^"']+)["']''', glue.read_text()):
        resolved = (root / relative).resolve()
        if not resolved.is_relative_to(root.resolve()):
            raise ValueError(f"snippet escapes asset root: {relative}")
        require(root, relative)


def assets(root):
    for relative in ("index.html", "terraphim_editor.js", "terraphim_editor_bg.wasm",
                     "terraphim-editor.min.js", "terraphim-editor.min.css", "example.html",
                     "js/rolegraph-worker.js"):
        require(root, relative)
    graph_assets(root / "rolegraph")
    checker = Path(__file__).with_name("runtime-assets-check.py")
    result = subprocess.run([sys.executable, str(checker), "--check", str(root / "vendor")])
    if result.returncode:
        raise ValueError("local runtime vendor assets failed integrity/closure check")


if __name__ == "__main__":
    try:
        if sys.argv[1] == "metadata":
            metadata(json.load(sys.stdin))
        elif sys.argv[1] == "assets":
            assets(Path(sys.argv[2]))
        elif sys.argv[1] == "graph-assets":
            graph_assets(Path(sys.argv[2]))
        else:
            raise ValueError("expected metadata, assets or graph-assets")
    except (ValueError, KeyError, StopIteration, IndexError) as error:
        sys.exit(f"rolegraph production check: {error}")
