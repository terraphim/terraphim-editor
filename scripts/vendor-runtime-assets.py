#!/usr/bin/env python3
"""Materialize the reviewed subset from already downloaded archives; never fetch.

Explicit maintenance tool, not a build hook. Refuses an existing destination.
The prerequisite closure and notice provenance are supplied from the reviewed
asset inventory. Archive and member hashes are checked before writing anything.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import tarfile

PACKAGES = {
    'shoelace': ('2.12.0', 'https://registry.npmjs.org/@shoelace-style/shoelace/-/shoelace-2.12.0.tgz', '76ae69764177f9fac0628bc59caa243b6897ef1536e265a381b3c31de1996f52'),
    'fontawesome': ('6.5.2', 'https://registry.npmjs.org/@fortawesome/fontawesome-free/-/fontawesome-free-6.5.2.tgz', '1622d514686c299db3ffe9405c4f124778ce812658f66282d4524247e66fff9c'),
}


def members(archive, expected):
    if hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise ValueError(f'archive hash mismatch: {archive.name}')
    result = {}
    with tarfile.open(archive, 'r:gz') as tar:
        for member in tar:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not path.parts or path.parts[0] != 'package':
                raise ValueError(f'unsafe archive path: {member.name}')
            if member.isdir():
                continue
            if not member.isfile() or member.name in result:
                raise ValueError(f'unsafe/duplicate archive member: {member.name}')
            result[member.name] = tar.extractfile(member).read()
    return result


def materialize(scratch, destination):
    if destination.exists():
        raise ValueError('destination exists; refuse to overwrite')
    closure = json.loads((scratch / 'closure.json').read_text())
    notices = json.loads((scratch / 'notice-provenance.json').read_text())
    archives = {key: members(scratch / f'{key}.tgz', spec[2]) for key, spec in PACKAGES.items()}
    entries, payload = [], {}
    for item in closure['files']:
        package = item['package']
        version, source, archive_hash = PACKAGES[package]
        member = 'package/' + item['path']
        data = archives[package][member]
        path = f'{package}/{version}/{item["path"]}'
        entries.append(dict(path=path, bytes=item['bytes'], sha256=item['sha256'], source=source,
                            archiveSha256=archive_hash, member=member, package=package, version=version))
        payload[path] = data
    for item in notices:
        notice_path = PurePosixPath(item['path'])
        archive = scratch / (notice_path.parts[1] + '.tgz')
        member = 'package/' + notice_path.name
        data = members(archive, item['archiveSha256'])[member]
        path = 'shoelace/2.12.0/' + item['path']
        entries.append(dict(item, path=path, member=member))
        payload[path] = data
    if len(payload) != len(entries):
        raise ValueError('duplicate destination')
    for item in entries:
        path = PurePosixPath(item['path'])
        if path.is_absolute() or '..' in path.parts:
            raise ValueError('unsafe destination')
        data = payload[item['path']]
        if len(data) != item['bytes'] or hashlib.sha256(data).hexdigest() != item['sha256']:
            raise ValueError(f'member hash mismatch: {item["path"]}')
    for path, data in payload.items():
        target = destination / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    manifest = dict(schema=1, upstreamFileCount=len(closure['files']), noticeFileCount=len(notices),
                    files=sorted(entries, key=lambda item: item['path']))
    (destination / 'manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(f'vendored {len(entries)} verified upstream files; authored manifest separate')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archives', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=Path('public/vendor'))
    args = parser.parse_args()
    materialize(args.archives, args.output)
