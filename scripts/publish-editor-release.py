#!/usr/bin/env python3
"""Qualify fixed editor artifacts and publish the same bytes as an immutable prerelease."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'terraphim/terraphim-editor'
SOURCE_URL = 'https://github.com/' + REPOSITORY
SHA_RE = re.compile(r'[0-9a-f]{40}')
SHA256_RE = re.compile(r'[0-9a-f]{64}')
FILES = ('editor-candidate.tar.gz', 'assets-manifest.json', 'qualification.json',
         'saved-outputs.tar.gz', 'release-manifest.json', 'SHA256SUMS')
GATES = ('wasm-host', 'chrome-synonyms', 'chrome-offline')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_module(name, relative):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git(root, *args):
    return subprocess.check_output(['git', *args], cwd=root, text=True, timeout=30).strip()


def admit_source(tag, root=ROOT):
    """Require an immutable RC of the root package and a commit reviewed on main."""
    version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
    if not re.fullmatch('v' + re.escape(version) + r'-rc\.[1-9][0-9]*', tag):
        raise ValueError('Expected an editor prerelease tag matching the root package version')
    source = load_module('release_source', 'scripts/check-release-source.py').check_source(root)
    if git(root, 'rev-parse', f'refs/tags/{tag}^{{commit}}') != source:
        raise ValueError('Tag does not identify this exact clean source')
    if subprocess.run(['git', 'merge-base', '--is-ancestor', source, 'refs/remotes/origin/main'],
                      cwd=root, capture_output=True, timeout=30).returncode:
        raise ValueError('Tagged source must already belong to public main')
    return source


def safe_members(archive):
    """Read bounded regular archive members, rejecting traversal, links and duplicates."""
    result = {}
    total = 0
    with tarfile.open(archive, 'r:gz') as handle:
        for member in handle:
            path = PurePosixPath(member.name)
            if (not member.isfile() or path.is_absolute() or str(path) != member.name
                    or any(p in ('', '.', '..') for p in member.name.split('/'))
                    or '\\' in member.name or member.name in result):
                raise ValueError('Unsafe or duplicate archive member')
            total += member.size
            if total > 128 * 1024 * 1024 or len(result) >= 2048:
                raise ValueError('Archive exceeds size or member bound')
            result[member.name] = handle.extractfile(member).read()
    if not result:
        raise ValueError('Empty archive')
    return result


def check_assets(folder, source, lock_sha):
    manifest = json.loads((folder / 'assets-manifest.json').read_text())
    if (manifest.get('schema') != 1 or manifest.get('sourceHead') != source
            or manifest.get('sourceRepository') != SOURCE_URL
            or manifest.get('cargoLockSha256') != lock_sha):
        raise ValueError('Asset provenance differs from exact public source and lock')
    files = safe_members(folder / 'editor-candidate.tar.gz')
    assets = manifest['assets']
    if not isinstance(assets, dict) or not assets or set(files) != set(assets):
        raise ValueError('Archive and asset manifest members differ')
    if 'host-session.js' in files:
        raise ValueError('Host bridge remains a separately pinned source asset')
    for name, raw in files.items():
        if hashlib.sha256(raw).hexdigest() != assets[name]:
            raise ValueError('Archive member checksum differs')
    for required in ('index.html', 'terraphim_editor.js', 'terraphim_editor_bg.wasm', 'roles/default.json'):
        if required not in files:
            raise ValueError('Missing editor delivery member')
    return files


def checksums(text):
    result = {}
    for line in text.splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  ([A-Za-z0-9][A-Za-z0-9_.-]*)', line)
        if not match or match[2] in result:
            raise ValueError('Malformed or duplicate checksum entry')
        result[match[2]] = match[1]
    return result


def check_qualification(folder, source, lock_sha, tag):
    report = json.loads((folder / 'qualification.json').read_text())
    if (report.get('status') != 'PASS' or report.get('tag') != tag
            or report.get('sourceCommit') != source or report.get('sourceRepository') != SOURCE_URL
            or report.get('cargoLockSha256') != lock_sha
            or report.get('archiveSha256') != digest(folder / 'editor-candidate.tar.gz')
            or report.get('manifestSha256') != digest(folder / 'assets-manifest.json')
            or report.get('savedOutputsSha256') != digest(folder / 'saved-outputs.tar.gz')
            or report.get('fullParityAccepted') is not False):
        raise ValueError('Qualification differs from exact candidate/source or overstates parity')
    gates = report.get('gates', [])
    if (len(gates) != len(GATES) or {g.get('name') for g in gates} != set(GATES)
            or any(g.get('status') != 'PASS' for g in gates)):
        raise ValueError('All actual targeted qualification gates must pass')
    outputs = safe_members(folder / 'saved-outputs.tar.gz')
    expected = report.get('outputs')
    if not expected or set(outputs) != set(expected):
        raise ValueError('Saved output inventory differs')
    for name, raw in outputs.items():
        if hashlib.sha256(raw).hexdigest() != expected[name]:
            raise ValueError('Saved output checksum differs')
    for gate in gates:
        name = gate['name'] + '.log'
        if name not in outputs or hashlib.sha256(outputs[name]).hexdigest() != gate.get('logSha256'):
            raise ValueError('Actual gate log is missing or differs')
    tap = outputs['wasm-host.log'].decode()
    if not re.search(r'(?m)^# pass 15\s*$', tap) or not re.search(r'(?m)^# fail 0\s*$', tap):
        raise ValueError('Actual WASM test output is incomplete')
    if not outputs.get('story/annotated.md'):
        raise ValueError('Saved annotated source is missing')
    browser = json.loads(outputs['story/browser-receipt.json'])
    graph = json.loads(outputs['story/knowledge-graph.json'])
    clean = outputs['story/corpus/project-brief.md'].decode()
    if (browser.get('status') != 'PASS' or not all(browser.get(k) is True for k in
            ('nativeKeyboardUndoRedo', 'saveReopen', 'actualDownloads'))
            or browser.get('graph') != graph or browser.get('clean') != clean
            or 'concern' in clean.lower() or 'Risk:' not in clean or 'Decision:' not in clean
            or graph['data']['risk']['id'] != graph['data']['concern']['id']):
        raise ValueError('Actual saved synonym graph/document outputs failed')
    offline = json.loads(outputs['offline.json'])
    if (offline.get('status') != 'PASS' or offline.get('engine') != 'chrome'
            or len(offline.get('scenarios', [])) != 2
            or any(s.get('result', {}).get('saveReopenExport') is not True or s.get('externalRequests') != []
                   or s.get('freshContext') is not True for s in offline['scenarios'])):
        raise ValueError('Actual Chrome full-page/embed delivery did not pass')
    return report


def verify_package(folder, tag, root=ROOT):
    folder = Path(folder)
    files = [folder / name for name in FILES]
    if set(p.name for p in folder.iterdir()) != set(FILES) or any(p.is_symlink() or not p.is_file() for p in files):
        raise ValueError('Expected only the complete regular release file set')
    source = admit_source(tag, root)
    lock_sha = digest(root / 'Cargo.lock')
    check_assets(folder, source, lock_sha)
    check_qualification(folder, source, lock_sha, tag)
    expected = {p.name: digest(p) for p in files if p.name != 'SHA256SUMS'}
    if checksums((folder / 'SHA256SUMS').read_text()) != expected:
        raise ValueError('Release checksums differ')
    readiness = load_module('release_readiness', 'scripts/check-release-readiness.py')
    report = readiness.validate_release(folder / 'release-manifest.json', folder)
    if not readiness.evaluate_claim(report, 'browser', 'integrity')['allowed']:
        raise ValueError('Release integrity/evidence binding failed')
    artifact = report['manifest']['artifacts']
    if (len(artifact) != 1 or artifact[0]['sourceCommit'] != source or artifact[0]['version'] != tag
            or report['manifest']['channel'] != 'candidate'
            or not any(item['host'] == 'browser' and item['requirement'] == 'all41' and item['status'] == 'OPEN'
                       for item in report['manifest']['limitations'])):
        raise ValueError('Prerelease identity or explicit limitations missing')
    return source, files


def package(stage, output, tag):
    source = admit_source(tag)
    if output.exists():
        raise ValueError('Use a new candidate directory')
    publisher = load_module('dist_inventory', 'scripts/publish-editor-dist.py')
    assets = publisher.inventory(stage)
    output.mkdir(parents=True)
    with tarfile.open(output / 'editor-candidate.tar.gz', 'w:gz') as handle:
        for name in sorted(assets):
            handle.add(stage / name, arcname=name, recursive=False)
    manifest = {'schema': 1, 'sourceHead': source, 'sourceRepository': SOURCE_URL,
                'cargoLockSha256': digest(ROOT / 'Cargo.lock'), 'assets': assets}
    (output / 'assets-manifest.json').write_text(json.dumps(manifest, indent=2, sort_keys=True) + '\n')
    check_assets(output, source, manifest['cargoLockSha256'])
def gh(*args):
    return subprocess.check_output(["gh", *args], text=True, timeout=120)


def api(path):
    return json.loads(gh("api", "--hostname", "github.com", path))


def select_release(pages, tag):
    found = [item for page in pages for item in page if item["tag_name"] == tag]
    if len(found) > 1:
        raise ValueError("Ambiguous release tag")
    return found[0] if found else None


def release(repo, tag):
    # Listing includes drafts; the public tag endpoint returns 404 for drafts.
    pages = json.loads(gh("api", "--hostname", "github.com", "--paginate", "--slurp",
                          f"repos/{repo}/releases?per_page=100"))
    return select_release(pages, tag)


def verify_remote_tag(repo, tag, source):
    value = api(f"repos/{repo}/git/ref/tags/{tag}")["object"]
    for _ in range(5):
        if value["type"] == "commit" and value["sha"] == source:
            return
        if value["type"] != "tag" or not SHA_RE.fullmatch(value["sha"]):
            break
        value = api(f"repos/{repo}/git/tags/{value['sha']}")["object"]
    raise ValueError("Remote tag does not resolve to the qualified source commit")


def resume_plan(current, tag, files):
    if current.get("tag_name") != tag or current.get("prerelease") is not True:
        raise ValueError("Release tag or preview channel differs")
    if type(current.get("draft")) is not bool:
        raise ValueError("Release draft state is missing")
    assets = current["assets"]
    names = [item["name"] for item in assets]
    wanted = {p.name: p for p in files}
    if len(names) != len(set(names)) or not set(names).issubset(wanted):
        raise ValueError("Unexpected or duplicate remote release assets")
    for item in assets:
        if item.get("state") != "uploaded" or item.get("size") != wanted[item["name"]].stat().st_size:
            raise ValueError("Existing asset is incomplete or differs in size; refusing replacement")
        if item.get("digest") not in (None, "sha256:" + digest(wanted[item["name"]])):
            raise ValueError("Existing remote asset digest differs; refusing replacement")
    missing = [p for p in files if p.name not in names]
    if missing and not current["draft"]:
        raise ValueError("Published release has missing assets; refusing repair in place")
    return missing


def anonymous_download(url, destination):
    """Anonymous HTTPS GET, without curl config, auth headers, cookies or netrc."""
    parsed = urlparse(url)
    if (parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password
            or parsed.query or parsed.fragment):
        raise ValueError("Anonymous download requires an ordinary HTTPS URL")
    if destination.exists() or destination.is_symlink():
        raise ValueError("Refusing to overwrite a download")
    result = subprocess.run([
        "curl", "--disable", "--fail", "--silent", "--show-error", "--location",
        "--proto", "=https", "--proto-redir", "=https", "--max-redirs", "5",
        "--connect-timeout", "15", "--max-time", "300", "--max-filesize", str(128 * 1024 * 1024),
        "--retry", "2", "--output", str(destination), "--write-out", "%{http_code}", url,
    ], text=True, capture_output=True)
    if result.returncode or result.stdout != "200":
        raise ValueError(f"Anonymous GET failed: HTTP {result.stdout}; curl {result.returncode}")
    return destination.read_bytes()


def publish(repo, tag, files, source):
    if repo != REPOSITORY:
        raise ValueError("Invalid GitHub repository")
    repository = api(f"repos/{repo}")
    if repository.get("private") is not False or repository.get("full_name", "").lower() != repo.lower():
        raise ValueError("Publication requires the exact public GitHub repository")
    verify_remote_tag(repo, tag, source)
    current = release(repo, tag)
    with tempfile.TemporaryDirectory(prefix="terraphim-publish-") as folder:
        temp = Path(folder)
        if current is None:
            notes = temp / "notes.md"
            notes.write_text(f"Terraphim editor prerelease {tag}; source `{source}`.\n\n"
                             "The attached browser archive passed real WASM host-session tests and "
                             "Chrome synonym creation, swap, undo/redo, save/reopen and graph/text downloads. "
                             "Offline full-page and embed delivery were checked. Saved outputs and receipts are attached. "
                             "This prerelease does not qualify all 41 requirements or native Safari, Firefox and Zed.\n")
            gh("release", "create", tag, "--repo", f"github.com/{repo}", "--verify-tag",
               "--draft", "--prerelease", "--latest=false", "--title", tag, "--notes-file", str(notes))
            current = release(repo, tag)
        if current is None:
            raise ValueError("Draft release could not be recovered")
        missing = resume_plan(current, tag, files)
        # Verify every existing asset before mutating an incomplete draft.
        for path in files:
            if path not in missing:
                gh("release", "download", tag, "--repo", f"github.com/{repo}",
                   "--pattern", path.name, "--dir", str(temp))
                if (temp / path.name).read_bytes() != path.read_bytes():
                    raise ValueError(f"Existing {path.name} differs; refusing replacement")
        for path in missing:
            gh("release", "upload", tag, str(path), "--repo", f"github.com/{repo}")
            gh("release", "download", tag, "--repo", f"github.com/{repo}",
               "--pattern", path.name, "--dir", str(temp))
            if (temp / path.name).read_bytes() != path.read_bytes():
                raise ValueError("Uploaded asset readback differs")
        current = release(repo, tag)
        if current is None or resume_plan(current, tag, files):
            raise ValueError("Release asset set is incomplete")
        verify_remote_tag(repo, tag, source)
        if current["draft"]:
            gh("release", "edit", tag, "--repo", f"github.com/{repo}", "--verify-tag", "--draft=false")
        published = release(repo, tag)
        if published is None or published["draft"] or resume_plan(published, tag, files):
            raise ValueError("Release is not publicly published with the exact asset set")
        verify_remote_tag(repo, tag, source)
        for path in files:
            url = f"https://github.com/{repo}/releases/download/{tag}/{path.name}"
            if anonymous_download(url, temp / ("public-" + path.name)) != path.read_bytes():
                raise ValueError("Anonymous release readback differs")
    return {"schema": 1, "status": "PUBLISHED_PUBLIC_VERIFIED", "repository": repo,
            "tag": tag, "sourceCommit": source, "releaseId": published["id"],
            "assets": [{"name": p.name, "sha256": digest(p), "bytes": p.stat().st_size} for p in files]}



def qualify(folder, evidence, tag):
    """Run actual built WASM and Chrome; archive returned files, never proposed inputs."""
    import functools
    import http.server
    import os
    import threading
    source = admit_source(tag)
    lock_sha = digest(ROOT / 'Cargo.lock')
    payload = check_assets(folder, source, lock_sha)
    if evidence.exists() or (folder / 'qualification.json').exists():
        raise ValueError('Use new evidence paths; retain previous output')
    evidence.mkdir(parents=True)
    gates = []
    with tempfile.TemporaryDirectory(prefix='editor-release-qualify-') as temporary:
        staged = Path(temporary) / 'dist'
        staged.mkdir()
        for name, raw in payload.items():
            target = staged / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        # Node needs an explicit ESM suffix; only a byte-identical test copy is renamed.
        (staged / 'terraphim_editor.mjs').write_bytes(payload['terraphim_editor.js'])
        (staged / 'package.json').write_text('{"type":"module"}\n')
        env = dict(os.environ)
        env['TERRAPHIM_HOST_WASM'] = str(staged)
        env['TERRAPHIM_PLAYWRIGHT_MODULE'] = str(ROOT / 'scripts/ci/release-node/node_modules/playwright')
        env['TERRAPHIM_STORY_OUT'] = str(evidence / 'story')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(staged)))
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        env['TERRAPHIM_STORY_URL'] = f'http://127.0.0.1:{server.server_port}/'
        commands = [
            ('wasm-host', ['node', '--test', '--test-reporter=tap', 'tests/browser/host-session-conformance.cjs', 'tests/browser/authored-synonyms.cjs']),
            ('chrome-synonyms', ['node', 'tests/browser/synonym-sharing.cjs']),
            ('chrome-offline', [sys.executable, 'scripts/ci/editor-offline-check.py', str(staged), '--engine', 'chrome', '--report', str(evidence / 'offline.json')]),
        ]
        try:
            for name, command in commands:
                result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, text=True, timeout=240)
                log = evidence / (name + '.log')
                log.write_text(result.stdout + result.stderr)
                if result.returncode:
                    raise ValueError(f'{name} failed; retained {log.name}')
                if name == 'wasm-host' and not re.search(r'(?m)^# pass 15\s*$', result.stdout):
                    raise ValueError('Expected all 15 actual WASM host tests')
                gates.append({'name': name, 'status': 'PASS', 'command': command, 'logSha256': digest(log)})
        finally:
            server.shutdown()
            server.server_close()
            thread.join(timeout=10)
    outputs = {}
    with tarfile.open(folder / 'saved-outputs.tar.gz', 'w:gz') as handle:
        for path in sorted(evidence.rglob('*')):
            if path.is_symlink():
                raise ValueError('Symlink evidence forbidden')
            if path.is_file():
                name = path.relative_to(evidence).as_posix()
                outputs[name] = digest(path)
                handle.add(path, arcname=name, recursive=False)
    report = {'schema': 1, 'status': 'PASS', 'tag': tag, 'sourceRepository': SOURCE_URL,
              'sourceCommit': source, 'cargoLockSha256': lock_sha,
              'archiveSha256': digest(folder / 'editor-candidate.tar.gz'),
              'manifestSha256': digest(folder / 'assets-manifest.json'),
              'savedOutputsSha256': digest(folder / 'saved-outputs.tar.gz'),
              'gates': gates, 'outputs': outputs, 'fullParityAccepted': False,
              'limitations': 'Targeted Chrome and WASM checks only; all41 and native Safari/Firefox/Zed are not qualified by this run.'}
    (folder / 'qualification.json').write_text(json.dumps(report, indent=2) + '\n')
    check_qualification(folder, source, lock_sha, tag)
    def file(name):
        path = folder / name
        return {'path': name, 'bytes': path.stat().st_size, 'sha256': digest(path)}
    archive = file('editor-candidate.tar.gz')
    manifest = {'schema': 1, 'releaseId': 'editor-' + source, 'channel': 'candidate',
                'artifacts': [{**archive, 'host': 'browser', 'version': tag, 'sourceCommit': source}],
                'evidence': [{**file(name), 'host': 'browser', 'kind': 'receipt', 'artifactSha256': archive['sha256']}
                             for name in ('qualification.json', 'saved-outputs.tar.gz')],
                'limitations': [{'host': 'browser', 'requirement': 'all41', 'status': 'OPEN',
                                 'issue': SOURCE_URL + '/blob/' + source + '/docs/requirements/traceability.md'}]}
    (folder / 'release-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
    (folder / 'SHA256SUMS').write_text(''.join(digest(folder / name) + '  ' + name + '\n' for name in FILES if name != 'SHA256SUMS'))
    verify_package(folder, tag)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    admit = sub.add_parser('admit'); admit.add_argument('--tag', required=True)
    build = sub.add_parser('package'); build.add_argument('--tag', required=True); build.add_argument('--stage', type=Path, required=True); build.add_argument('--output', type=Path, required=True)
    test = sub.add_parser('qualify'); test.add_argument('--tag', required=True); test.add_argument('--folder', type=Path, required=True); test.add_argument('--evidence', type=Path, required=True)
    verify = sub.add_parser('verify'); verify.add_argument('--tag', required=True); verify.add_argument('--folder', type=Path, required=True)
    release = sub.add_parser('publish'); release.add_argument('--tag', required=True); release.add_argument('--folder', type=Path, required=True); release.add_argument('--receipt', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'admit':
        print(admit_source(args.tag))
    elif args.command == 'package':
        package(args.stage, args.output, args.tag)
    elif args.command == 'qualify':
        qualify(args.folder, args.evidence, args.tag)
    elif args.command == 'verify':
        source, files = verify_package(args.folder, args.tag)
        print(json.dumps({'status': 'VERIFIED', 'source': source, 'files': [p.name for p in files]}))
    else:
        if args.receipt.exists() or args.receipt.is_symlink():
            raise ValueError('Use a new publication receipt path')
        source, files = verify_package(args.folder, args.tag)
        with tempfile.TemporaryDirectory(prefix='editor-publication-snapshot-') as temporary:
            snapshot = Path(temporary)
            for path in files:
                (snapshot / path.name).write_bytes(path.read_bytes())
            source, snapshots = verify_package(snapshot, args.tag)
            result = publish(REPOSITORY, args.tag, snapshots, source)
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        with args.receipt.open('x') as output:
            output.write(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.CalledProcessError, subprocess.TimeoutExpired, tarfile.TarError) as error:
        sys.exit(f'editor release refused: {error}')
