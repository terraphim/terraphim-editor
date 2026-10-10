#!/usr/bin/env python3
"""Read-only runtime subset integrity and local import/font closure check."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
IMPORT = re.compile(r'(?:\bfrom\s*|\bimport\s*(?:\(\s*)?)[\"\']([^\"\']+)[\"\']')
CSS_URL = re.compile(r'url\(\s*[\"\']?([^\)\"\']+)[\"\']?\s*\)')


def check(vendor, canonical=None):
    canonical = canonical or ROOT / 'public/vendor/manifest.json'
    if vendor.is_symlink() or not vendor.is_dir():
        raise ValueError('vendor directory missing or symlinked')
    # Reject links before reading any member (including symlinked directories).
    for path in vendor.rglob('*'):
        if path.is_symlink():
            raise ValueError(f'symlink rejected: {path}')
    manifest = json.loads(canonical.read_text())
    if manifest.get('schema') != 1 or manifest.get('upstreamFileCount') != 98 or manifest.get('noticeFileCount') != 9 or len(manifest['files']) != 107:
        raise ValueError('unexpected manifest schema/counts')
    if (vendor / 'manifest.json').is_symlink() or (vendor / 'manifest.json').read_bytes() != canonical.read_bytes():
        raise ValueError('manifest differs from source canonical manifest')
    expected = {'manifest.json'}
    for item in manifest['files']:
        path = PurePosixPath(item['path'])
        if path.is_absolute() or '..' in path.parts or str(path) in expected:
            raise ValueError('unsafe/duplicate manifest member')
        expected.add(str(path))
        target = vendor / path
        if not target.is_file() or target.is_symlink():
            raise ValueError(f'missing/unsafe member: {path}')
        data = target.read_bytes()
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise ValueError(f'hash mismatch: {path}')
    actual = set()
    for path in vendor.rglob('*'):
        if path.is_symlink():
            raise ValueError(f'symlink rejected: {path}')
        if path.is_file():
            actual.add(path.relative_to(vendor).as_posix())
    if actual != expected:
        raise ValueError(f'unexpected files: {sorted(actual - expected)}')
    for path in vendor.rglob('*'):
        if path.suffix not in ('.js', '.css'):
            continue
        text = path.read_text()
        references = IMPORT.findall(text) if path.suffix == '.js' else CSS_URL.findall(text)
        if path.suffix == '.css' and re.search(r'@import\b', text):
            raise ValueError(f'CSS import not allowed: {path}')
        for reference in references:
            if not reference.startswith(('./', '../')):
                raise ValueError(f'non-local reference: {path}: {reference}')
            target = (path.parent / reference).resolve()
            if not target.is_relative_to(vendor.resolve()) or not target.is_file():
                raise ValueError(f'unclosed reference: {path}: {reference}')
    return len(manifest['files'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', type=Path, required=True, help='vendor directory, not distribution root')
    args = parser.parse_args()
    try:
        count = check(args.check)
        print(f'runtime-assets: {count} files verified; module/font closure local')
    except (ValueError, OSError, KeyError) as error:
        print(f'runtime-assets: FAIL: {error}', file=sys.stderr)
        sys.exit(1)
