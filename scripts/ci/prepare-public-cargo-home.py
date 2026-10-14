#!/usr/bin/env python3
"""Reset only the checkout-owned CI cache before this run uses public Cargo."""
from pathlib import Path
import shutil


def prepare(root):
    """Create an empty CI Cargo home without following filesystem aliases."""
    root = Path(root).resolve()
    target = root / "target"
    home = target / "ci-cargo-home"
    if target.is_symlink() or home.is_symlink():
        raise ValueError("CI Cargo storage must not be a symlink")
    if target.exists() and not target.is_dir():
        raise ValueError("CI target must be a directory")
    if home.exists() and not home.is_dir():
        raise ValueError("CI Cargo home must be a directory")
    target.mkdir(exist_ok=True)
    if home.exists():
        shutil.rmtree(home)
    home.mkdir(mode=0o700)
    return home


if __name__ == "__main__":
    prepare(Path(__file__).resolve().parents[2])
    print("Fresh checkout-owned CI Cargo home prepared")
