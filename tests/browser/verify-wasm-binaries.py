#!/usr/bin/env python3
"""Run every real browser test binary, retaining failures rather than stopping early."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--browser', choices=['chrome', 'firefox'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--only-failures-from', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binaries = sorted(p.stem for p in (root / 'tests').glob('*.rs')
                      if 'run_in_browser' in p.read_text())
    parent = None
    if args.only_failures_from:
        raw = args.only_failures_from.read_bytes()
        prior = json.loads(raw)
        selected = [run['binary'] for run in prior['runs'] if run['status'] != 'PASS']
        if prior['browser'] != args.browser or any(name not in binaries for name in selected):
            raise ValueError('Diagnostic selection must use failures from the same browser')
        binaries = selected
        parent = {'path':str(args.only_failures_from.resolve()),
                  'sha256':hashlib.sha256(raw).hexdigest()}
    receipt = {'scope': 'Real source-level WASM browser tests; not native extension or Safari qualification',
               'browser': args.browser, 'status': 'RUNNING', 'binaries': binaries, 'runs': [],
               'startedAt': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    if parent:
        receipt['scope'] = 'Selected failed-binary diagnostic; does not replace the unfiltered audit'
        receipt['parentAudit'] = parent
    (output / 'executed-runner.py').write_bytes(Path(__file__).read_bytes())
    env = dict(os.environ)
    env.pop('WASM_BINDGEN_TEST_TIMEOUT', None)
    for binary in binaries:
        command = ['wasm-pack', 'test', '--headless', '--' + args.browser,
                   '--', '--locked', '--test', binary]
        log = output / (binary + '.log')
        started = time.monotonic()
        with log.open('w') as stream:
            child = subprocess.Popen(command, cwd=root, env=env, stdout=stream,
                                     stderr=subprocess.STDOUT, start_new_session=True)
            try:
                code = child.wait(timeout=180)
                status = 'PASS' if code == 0 else 'FAIL'
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGTERM)
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait()
                code = child.returncode
                status = 'TIMEOUT'
            finally:
                # Stop only this owned test process group, including browser
                # children left behind when a driver exits or times out.
                try:
                    os.killpg(child.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        text = log.read_text()
        counts = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', text)
        if not counts and status == 'PASS':
            status = 'NO_RESULT'
        run = {'binary': binary, 'command': command, 'processGroup':child.pid,
               'status': status, 'exitCode': code,
               'seconds': round(time.monotonic() - started, 3), 'reportedCounts': counts,
               'logSha256': hashlib.sha256(log.read_bytes()).hexdigest()}
        receipt['runs'].append(run)
        (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
        print(json.dumps(run), flush=True)
    receipt['status'] = 'PASS' if all(r['status'] == 'PASS' for r in receipt['runs']) else 'FAIL'
    receipt['finishedAt'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    (output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
