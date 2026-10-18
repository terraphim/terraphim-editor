#!/usr/bin/env python3
"""Fail-closed actual Chrome/Gecko clipboard admission for one immutable candidate."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'terraphim/terraphim-editor'
HEX = re.compile(r'[a-f0-9]{64}')
SOURCE = re.compile(r'[a-f0-9]{40}')
FAMILIES = ('chrome', 'firefox')
REQUIRED_ASSETS = {'terraphim-editor.min.js', 'terraphim_editor.js', 'terraphim_editor_bg.wasm'}
HARNESSES = ('tests/browser/webdriver-clipboard-user-flow.cjs', 'tests/browser/run-synonym-sharing.cjs',
             'scripts/qualify-trusted-clipboard.py')


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load_json(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError('Missing, linked or oversized proof file')
    return json.loads(path.read_text())


def read_output(path):
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 4096:
        raise ValueError('Actual clipboard output missing, linked or oversized')
    return path.read_bytes()


def release_module():
    spec = importlib.util.spec_from_file_location('editor_release_clipboard', ROOT / 'scripts/publish-editor-release.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def candidate(folder, source):
    folder = Path(folder)
    names = [('host-assets.tar.gz', 'host-assets.json'), ('editor-candidate.tar.gz', 'assets-manifest.json')]
    found = [(folder / archive, folder / manifest) for archive, manifest in names
             if (folder / archive).is_file() and (folder / manifest).is_file()]
    if len(found) != 1 or not SOURCE.fullmatch(source):
        raise ValueError('One explicit exact-source candidate required')
    archive, manifest_path = found[0]
    manifest = load_json(manifest_path)
    if manifest.get('sourceHead') != source:
        raise ValueError('Candidate source differs')
    if archive.is_symlink():
        raise ValueError('Linked candidate archive refused')
    assets = manifest.get('assets')
    if not isinstance(assets, dict) or not REQUIRED_ASSETS <= assets.keys():
        raise ValueError('Candidate lacks required editor runtime assets')
    files = release_module().safe_members(archive)
    if set(files) != set(assets) or any(not isinstance(value, str) or not HEX.fullmatch(value)
                                     or hashlib.sha256(files[name]).hexdigest() != value
                                     for name, value in assets.items()):
        raise ValueError('Candidate archive/member hashes differ')
    return manifest, files, archive, manifest_path


def validate_browser(folder, family, manifest, manifest_sha):
    """Validate actual saved output bytes; this alone never admits a CI run."""
    folder = Path(folder)
    receipt = load_json(folder / 'clipboard-receipt.json')
    runtime = load_json(folder / 'runtime-assets.json')
    browser_path = receipt.get('browserPath', receipt.get('firefoxPath', ''))
    browser_sha = receipt.get('browserSha256', receipt.get('firefoxSha256', ''))
    declared = receipt.get('browserFamily', 'firefox' if 'Firefox.app/' in browser_path else None)
    if family not in FAMILIES or declared != family:
        raise ValueError('Browser family differs; no relabelling another engine')
    if (receipt.get('status') != 'PASS' or receipt.get('priorClipboardRead') is not False
            or receipt.get('priorClipboardRestored') is not False
            or not isinstance(receipt.get('browser'), str) or not re.fullmatch(r'\d+(?:\.\d+)+', receipt['browser'])
            or not isinstance(browser_path, str) or not browser_path.startswith('/')
            or not isinstance(browser_sha, str) or not HEX.fullmatch(browser_sha)
            or not isinstance(receipt.get('driverPath'), str) or not receipt['driverPath'].startswith('/')
            or not isinstance(receipt.get('driverSha256'), str) or not HEX.fullmatch(receipt['driverSha256'])
            or not isinstance(receipt.get('driverVersion'), str) or not receipt['driverVersion']):
        raise ValueError('Incomplete actual browser/clipboard proof')
    proof = receipt.get('pasteProof', {})
    if (proof.get('caret') != 8 or proof.get('richNodes') != 0
            or not any(event.get('isTrusted') is True and {'text/plain', 'text/html'} <= set(event.get('types', []))
                       for event in proof.get('events', []))):
        raise ValueError('Trusted mixed-format paste/caret/plain-text proof missing')
    if (receipt.get('pastedText') != 'p1\np2\np3Hello' or receipt.get('copiedText') != 'p1\np2'
            or read_output(folder / 'actual-pasted.txt') != b'p1\np2\np3Hello'
            or read_output(folder / 'actual-copied.txt') != b'p1\np2'):
        raise ValueError('Actual complete paste/copy outputs differ')
    served = runtime.get('served')
    if (runtime.get('status') != 'PASS' or runtime.get('exitCode') != 0 or runtime.get('mismatches') != []
            or runtime.get('manifestSha256') != manifest_sha or not isinstance(served, dict)
            or not REQUIRED_ASSETS <= served.keys()
            or any(manifest['assets'].get(name) != value for name, value in served.items())):
        raise ValueError('Actual served runtime differs from the exact candidate')
    return {'family': family, 'browser': receipt['browser'], 'browserSha256': browser_sha,
            'driverSha256': receipt['driverSha256'], 'pastedSha256': digest(folder / 'actual-pasted.txt'),
            'copiedSha256': digest(folder / 'actual-copied.txt')}


def context(environment):
    if environment.get('GITHUB_ACTIONS') != 'true' or environment.get('GITHUB_REPOSITORY') != REPOSITORY:
        raise ValueError('Fresh same-run GitHub workflow context required')
    workflow = environment.get('GITHUB_WORKFLOW_REF', '')
    if not any(workflow.startswith(REPOSITORY + '/.github/workflows/' + name + '@')
               for name in ('public-root.yml', 'release.yml')):
        raise ValueError('Unrecognized clipboard qualification workflow')
    run_id, attempt = environment.get('GITHUB_RUN_ID', ''), environment.get('GITHUB_RUN_ATTEMPT', '')
    if not run_id.isdigit() or int(run_id) < 1 or not attempt.isdigit() or int(attempt) < 1:
        raise ValueError('Missing actual run/attempt identity')
    return {'runId': run_id, 'runAttempt': attempt, 'workflowRef': workflow,
            'repository': REPOSITORY, 'sourceCommit': environment.get('GITHUB_SHA')}


def proof_inventory(folder):
    folder = Path(folder)
    files = {}
    for family in FAMILIES:
        for path in sorted((folder / family).rglob('*')):
            if path.is_symlink():
                raise ValueError('Linked proof file refused')
            if path.is_file():
                files[path.relative_to(folder).as_posix()] = digest(path)
    for family in FAMILIES:
        log = folder / (family + '.log')
        if log.is_symlink() or not log.is_file():
            raise ValueError('Actual browser execution log missing')
        files[log.name] = digest(log)
    if len(files) > 512 or sum((folder / name).stat().st_size for name in files) > 32 * 1024 * 1024:
        raise ValueError('Proof inventory exceeds bounds')
    return files


def validate_binding(report, expected, source):
    """Pure binding validation; browser output verification remains separate."""
    if (report.get('schema') != 1 or report.get('status') != 'PASS'
            or expected.get('context', {}).get('sourceCommit') != source
            or not SOURCE.fullmatch(source)
            or any(report.get(key) != value for key, value in expected.items())):
        raise ValueError('Current run/source/candidate/harness/output binding differs')


def verify_admission(folder, candidate_folder, source, expected_context, root=ROOT):
    """Refuse stale, paste-only, untrusted or mismatched release admission."""
    folder = Path(folder)
    report = load_json(folder / 'clipboard-admission.json')
    manifest, _, archive, manifest_path = candidate(candidate_folder, source)
    expected = {'context': expected_context, 'archiveSha256': digest(archive),
                'manifestSha256': digest(manifest_path), 'cargoLockSha256': digest(root / 'Cargo.lock'),
                'harnesses': {name: digest(root / name) for name in HARNESSES},
                'outputs': proof_inventory(folder)}
    validate_binding(report, expected, source)
    verified = []
    for family in FAMILIES:
        receipt = load_json(folder / family / 'clipboard-receipt.json')
        if (receipt.get('platform') != 'darwin' or receipt.get('headless') is not False
                or receipt.get('inputTransport') != 'W3C native clicks/keys' or receipt.get('browserFamily') != family):
            raise ValueError('Actual headed Mac engine qualification required')
        verified.append(validate_browser(folder / family, family, manifest, digest(manifest_path)))
    if report.get('browsers') != verified:
        raise ValueError('Browser admission differs from actual outputs')
    return report


def package_host(assets, output, source):
    assets, output = Path(assets), Path(output)
    manifest = load_json(assets / 'host-assets.json')
    if manifest.get('sourceHead') != source or output.exists():
        raise ValueError('Fresh exact-source candidate output required')
    output.mkdir(parents=True)
    shutil.copyfile(assets / 'host-assets.json', output / 'host-assets.json')
    with tarfile.open(output / 'host-assets.tar.gz', 'w:gz') as archive:
        for name, sha in sorted(manifest['assets'].items()):
            if not isinstance(name, str) or any(part in ('', '.', '..') for part in name.split('/')) or name.startswith('/') or '\\' in name:
                raise ValueError('Unsafe source candidate path')
            path = assets / name
            if path.is_symlink() or not path.is_file() or digest(path) != sha:
                raise ValueError('Actual source candidate assets differ')
            archive.add(path, arcname=name, recursive=False)
    candidate(output, source)


def cleanup_group(group):
    """Terminate only a process group created by this verification invocation."""
    if not isinstance(group, int) or group <= 1 or group == os.getpgrp():
        raise ValueError('Refusing an invalid owned process group')
    try:
        os.killpg(group, signal.SIGTERM)
    except ProcessLookupError:
        return
    end = time.monotonic() + 0.5
    while time.monotonic() < end:
        try:
            os.killpg(group, 0)
        except ProcessLookupError:
            return
        time.sleep(0.025)
    try:
        os.killpg(group, signal.SIGKILL)
    except ProcessLookupError:
        pass


def run_owned(command, cwd, environment, log, evidence, driver_path=None, timeout=120):
    """Own the wrapper group and the harness's separately owned driver group."""
    evidence = Path(evidence)
    evidence.mkdir(parents=True, exist_ok=True)
    process = subprocess.Popen(command, cwd=cwd, env=environment, stdout=log,
                               stderr=subprocess.STDOUT, start_new_session=True)
    original_error = None
    try:
        (evidence / 'owned-wrapper.json').write_text(json.dumps({'processGroup': process.pid,
            'command': command, 'scope': 'Owned verifier wrapper process group only'}, indent=2) + '\n')
        return process.wait(timeout=timeout)
    except BaseException as error:
        original_error = error
        raise
    finally:
        errors = []
        try:
            cleanup_group(process.pid)
        except (OSError, ValueError, subprocess.SubprocessError) as error:
            errors.append('wrapper group cleanup: ' + str(error))
        # Reap the actual child even if a platform denies probing a retired
        # group. Otherwise Popen.__del__ warns despite the child having exited.
        try:
            process.wait(timeout=2)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=2)
        except (OSError, subprocess.SubprocessError) as error:
            errors.append('wrapper reap: ' + str(error))
        owned_path = evidence / 'owned-process.json'
        # Startup can time out before the driver records its identity. Preserve
        # that original error rather than replacing it with missing metadata.
        if driver_path is not None and owned_path.exists():
            try:
                owned = load_json(owned_path)
                if owned.get('driverPath') != str(Path(driver_path).resolve()):
                    raise ValueError('Owned driver identity differs')
                cleanup_group(owned.get('processGroup'))
            except (OSError, ValueError, KeyError, TypeError) as error:
                errors.append('driver cleanup: ' + str(error))
        if errors:
            log.write('\n' + '\n'.join(errors) + '\n')
            log.flush()
            if original_error is None:
                raise ValueError('Owned process cleanup failed: ' + '; '.join(errors))


def observe_families(observe):
    """Collect both real family outcomes; any failure still denies admission."""
    failures = []
    for family in FAMILIES:
        try:
            observe(family)
        except (ValueError, OSError, subprocess.SubprocessError) as error:
            failures.append({'family': family, 'error': str(error)})
    return failures


def run(folder, output, source):
    folder, output = Path(folder).resolve(), Path(output).resolve()
    actual_context = context(os.environ)
    if actual_context['sourceCommit'] != source or output.exists():
        raise ValueError('Fresh matching source/run output required')
    manifest, files, archive, manifest_path = candidate(folder, source)
    output.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix='terraphim-clipboard-assets-') as temporary:
        assets = Path(temporary)
        for name, raw in files.items():
            target = assets / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        def observe(family):
            evidence = output / family
            env = dict(os.environ)
            executable = '/Applications/Firefox.app/Contents/MacOS/firefox' if family == 'firefox' else '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
            driver_name = 'geckodriver' if family == 'firefox' else 'chromedriver'
            driver = shutil.which(driver_name)
            if not driver:
                driver_dir = os.environ.get('GECKOWEBDRIVER' if family == 'firefox' else 'CHROMEWEBDRIVER', '')
                if driver_dir and (Path(driver_dir) / driver_name).is_file():
                    driver = str(Path(driver_dir) / driver_name)
            if not Path(executable).is_file() or not driver:
                raise ValueError('Actual hosted browser/driver absent; no fallback')
            env.update(TERRAPHIM_BROWSER_FAMILY=family, TERRAPHIM_BROWSER_EXECUTABLE=executable,
                       TERRAPHIM_DRIVER_EXECUTABLE=driver, TERRAPHIM_AUTHORITY_ASSETS=str(assets),
                       TERRAPHIM_STORY_OUT=str(evidence), TERRAPHIM_STORY_SCRIPT='webdriver-clipboard-user-flow.cjs',
                       TERRAPHIM_ASSET_MANIFEST=str(manifest_path))
            with (output / (family + '.log')).open('x') as log:
                subprocess.run(['sw_vers'], stdout=log, stderr=log, check=True)
                subprocess.run(['uname', '-a'], stdout=log, stderr=log, check=True)
                subprocess.run(['node', '--version'], stdout=log, stderr=log, check=True)
                exit_code = run_owned(['node', 'tests/browser/run-synonym-sharing.cjs'], ROOT, env,
                                      log, evidence, driver, timeout=120)
            if exit_code:
                raise ValueError(f'Actual {family} clipboard failed; retained outputs')
            validate_browser(evidence, family, manifest, digest(manifest_path))
        failures = observe_families(observe)
        if failures:
            (output / 'family-failures.json').write_text(json.dumps({'status': 'FAIL', 'failures': failures}, indent=2) + '\n')
            raise ValueError('Actual clipboard qualification failed: ' + ', '.join(item['family'] for item in failures))
    browsers = [validate_browser(output / family, family, manifest, digest(manifest_path)) for family in FAMILIES]
    report = {'schema': 1, 'status': 'PASS', 'context': actual_context,
              'archiveSha256': digest(archive), 'manifestSha256': digest(manifest_path),
              'cargoLockSha256': digest(ROOT / 'Cargo.lock'),
              'harnesses': {name: digest(ROOT / name) for name in HARNESSES},
              'browsers': browsers, 'outputs': proof_inventory(output),
              'runner': {**{name: os.environ.get(name) for name in ('RUNNER_OS', 'RUNNER_ARCH', 'ImageOS', 'ImageVersion')},
                         'nodeVersion': subprocess.check_output(['node', '--version'], text=True).strip()}}
    (output / 'clipboard-admission.json').write_text(json.dumps(report, indent=2) + '\n')
    verify_admission(output, folder, source, actual_context)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    package = commands.add_parser('package-host')
    package.add_argument('--assets', type=Path, required=True)
    package.add_argument('--output', type=Path, required=True)
    package.add_argument('--source', required=True)
    for name in ('run', 'verify'):
        command = commands.add_parser(name)
        command.add_argument('--candidate', type=Path, required=True)
        command.add_argument('--evidence', type=Path, required=True)
        command.add_argument('--source', required=True)
    args = parser.parse_args()
    if args.command == 'package-host':
        package_host(args.assets, args.output, args.source)
    elif args.command == 'run':
        run(args.candidate, args.evidence, args.source)
    else:
        verify_admission(args.evidence, args.candidate, args.source, context(os.environ))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError, tarfile.TarError) as error:
        raise SystemExit(f'trusted clipboard admission refused: {error}')
