#!/usr/bin/env python3
"""Read-only package integrity and full-parity attestation gate; never publishes.

A successful integrity check verifies bytes, not product behavior. Full parity
additionally requires complete, hash-bound requirement attestations. It cannot
replace independent output review, installed-host tests or stakeholder acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re

REQUIREMENTS = tuple(f'R-{section}.{number}' for section, count in [(2, 7), (3, 6), (4, 6), (5, 3), (6, 5), (7, 4), (8, 7), (9, 3)] for number in range(1, count + 1))

SUPPLEMENTAL = ('synonym-registration', 'synonym-save-reopen', 'synonym-undo-redo', 'host-history', 'host-multiview', 'install', 'upgrade', 'rollback')

def _keys(item, required, optional=()):
    if not isinstance(item, dict) or not set(required) <= item.keys() or item.keys() - set(required) - set(optional):
        raise ValueError('missing or unknown contract fields')


def _string(value):
    if not isinstance(value, str) or not value.strip():
        raise ValueError('nonempty string required')


def _hash(value, digits=64):
    if not isinstance(value, str) or not re.fullmatch('[0-9a-f]{' + str(digits) + '}', value):
        raise ValueError('invalid immutable hash')


def _read(root, item):
    name = item['path']
    _string(name)
    path = PurePosixPath(name)
    if path.is_absolute() or '..' in path.parts or '\\' in name or ':' in name or str(path) != name:
        raise ValueError('unsafe or noncanonical relative path')
    target = root
    for part in path.parts:
        target = target / part
        if target.is_symlink():
            raise ValueError('symlink release paths refused')
    if not target.is_file():
        raise ValueError('missing regular release file')
    _hash(item['sha256'])
    if type(item['bytes']) is not int or item['bytes'] < 0:
        raise ValueError('invalid byte count')
    raw = target.read_bytes()
    if len(raw) != item['bytes'] or hashlib.sha256(raw).hexdigest() != item['sha256']:
        raise ValueError('release file size or hash mismatch')
    return raw


def validate_release(manifest_path, artifact_root):
    """Verify relative regular files and evidence bindings; return all admission errors."""
    report = {'valid': False, 'errors': [], 'manifest': {}, 'ledgers': {}}
    try:
        manifest = json.loads(Path(manifest_path).read_bytes())
        _keys(manifest, ['schema', 'releaseId', 'channel', 'artifacts', 'evidence', 'limitations'])
        if type(manifest['schema']) is not int or manifest['schema'] != 1 or manifest['channel'] not in ['candidate', 'stable']:
            raise ValueError('unsupported release schema or channel')
        _string(manifest['releaseId'])
        if not isinstance(manifest['artifacts'], list) or not manifest['artifacts']:
            raise ValueError('at least one artifact required')
        if not isinstance(manifest['evidence'], list) or not isinstance(manifest['limitations'], list):
            raise ValueError('evidence and limitations must be lists')
        root = Path(artifact_root).resolve(strict=True)
        seen = set()
        for artifact in manifest['artifacts']:
            _keys(artifact, ['host', 'version', 'sourceCommit', 'path', 'bytes', 'sha256'])
            for key in ['host', 'version']:
                _string(artifact[key])
            _hash(artifact['sourceCommit'], 40)
            if artifact['host'] in seen:
                raise ValueError('duplicate host artifact')
            seen.add(artifact['host'])
            _read(root, artifact)
        evidence_paths = set()
        for evidence in manifest['evidence']:
            _keys(evidence, ['host', 'kind', 'artifactSha256', 'path', 'bytes', 'sha256'])
            if evidence['kind'] not in ['requirements', 'video', 'receipt', 'test-log']:
                raise ValueError('unknown evidence kind')
            artifact = next((a for a in manifest['artifacts'] if a['host'] == evidence['host']), None)
            if not artifact or evidence['artifactSha256'] != artifact['sha256']:
                raise ValueError('evidence is not bound to its host artifact')
            if evidence['path'] in evidence_paths:
                raise ValueError('duplicate evidence path')
            evidence_paths.add(evidence['path'])
            raw = _read(root, evidence)
            if evidence['kind'] == 'requirements':
                if evidence['host'] in report['ledgers']:
                    raise ValueError('duplicate host requirement ledger')
                ledger = json.loads(raw)
                if not isinstance(ledger, dict):
                    raise ValueError('requirement ledger must be an object')
                report['ledgers'][evidence['host']] = ledger
        for limitation in manifest['limitations']:
            _keys(limitation, ['host', 'requirement', 'status', 'issue'])
            if limitation['host'] not in seen or limitation['status'] not in ['OPEN', 'RESOLVED']:
                raise ValueError('invalid limitation host or status')
            for key in ['requirement', 'issue']:
                _string(limitation[key])
        report['manifest'] = manifest
        report['valid'] = True
    except (OSError, ValueError, TypeError, KeyError, StopIteration) as error:
        report['errors'].append(str(error))
    return report


def evaluate_claim(report, host, claim):
    """Deny incomplete full-parity attestations even when all package bytes match."""
    reasons = list(report['errors'])
    if claim not in ['integrity', 'full-parity']:
        reasons.append('unknown claim')
    if report['valid'] and host is not None and host not in [a['host'] for a in report['manifest']['artifacts']]:
        reasons.append('requested host has no admitted artifact')
    if report['valid'] and claim == 'full-parity':
        rows = report['ledgers'].get(host, {}).get('requirements', [])
        if not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows):
            reasons.append('invalid requirement rows')
        else:
            ids = [row.get('id') for row in rows]
            if any(not isinstance(ident, str) for ident in ids) or len(ids) != len(REQUIREMENTS) or set(ids) != set(REQUIREMENTS):
                reasons.append('exactly the original 41 unique requirement IDs required')
            for row in rows:
                checks = row.get('checks')
                if (row.get('accepted') is not True or row.get('status') != 'PASS' or not isinstance(row.get('remainingCriteria', ''), str) or row.get('remainingCriteria', '').strip()
                        or not isinstance(checks, list) or not checks or any(not isinstance(c, dict) or c.get('status') != 'PASS' for c in checks)):
                    reasons.append(f"incomplete requirement: {row.get('id')}")
        supplements = report['ledgers'].get(host, {}).get('supplemental', [])
        if not isinstance(supplements, list) or any(not isinstance(item, dict) or not isinstance(item.get('id'), str) for item in supplements):
            reasons.append('invalid supplemental gates')
        elif len(supplements) != len(SUPPLEMENTAL) or {item['id'] for item in supplements} != set(SUPPLEMENTAL):
            reasons.append('all eight supplemental gates required')
        else:
            for item in supplements:
                checks = item.get('checks')
                if item.get('status') != 'PASS' or not isinstance(checks, list) or not checks or any(not isinstance(c, dict) or c.get('status') != 'PASS' for c in checks):
                    reasons.append(f"incomplete supplemental gate: {item['id']}")
        if any(item['host'] == host and item['status'] == 'OPEN' for item in report['manifest']['limitations']):
            reasons.append('host has unresolved limitations')
    return {'allowed': report['valid'] and not reasons, 'host': host, 'claim': claim, 'reasons': reasons}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--claim', choices=['integrity', 'full-parity'], default='integrity')
    parser.add_argument('--host')
    args = parser.parse_args()
    if args.claim == 'full-parity' and not args.host:
        parser.error('--host required for full-parity')
    report = validate_release(args.manifest, args.root)
    decision = evaluate_claim(report, args.host, args.claim)
    print(json.dumps(decision, indent=2))
    return 0 if decision['allowed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
