#!/usr/bin/env python3
"""Read-only benchmark metadata; NEVER launches a build, browser or benchmark.

Print a JSON snapshot before/after the parent-approved official invocation.
--log requires an exact declarative approved path AND immutable expected hash.
Snapshots are inspection records, not retrospective execution manifests.
"""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import subprocess
import sys
import time


ROOT = Path(__file__).resolve().parents[2]
# Exact public implementation inputs, not extension/directory trust. No KG
# corpus, credential, registry, .git, generated output or fixture JSON bodies.
SOURCE_FILES = frozenset("""
Cargo.toml Cargo.lock webdriver.json templates/editor.html
tests/web_bench.rs tests/web_bench_logged.rs
tests/support/mod.rs tests/support/bench.rs tests/support/alt_panel.rs
tests/support/files.rs tests/support/fixtures.rs tests/support/indicators.rs
tests/support/kg.rs tests/support/overflow.rs tests/support/trim.rs
src/lib.rs src/document.rs src/kg.rs src/lab.rs src/providers.rs
public/js/alert.js public/js/alternatives-panel.js public/js/blocks.js
public/js/chrome.js public/js/config.js public/js/editor.js public/js/indicators.js
public/js/lab.js public/js/overflow.js public/js/persistence.js
public/js/selection-menu.js public/js/terraphim-editor.js public/js/trim.js
public/css/alternatives-panel.css public/css/blocks.css public/css/lab.css
public/css/overflow.css public/css/persistence.css public/css/selection-menu.css
public/css/tokens.css public/css/trim.css public/css/write-on.css
crates/terraphim_alternatives/Cargo.toml
crates/terraphim_alternatives/src/article.rs crates/terraphim_alternatives/src/block.rs
crates/terraphim_alternatives/src/context.rs crates/terraphim_alternatives/src/document.rs
crates/terraphim_alternatives/src/lib.rs crates/terraphim_alternatives/src/model.rs
crates/terraphim_alternatives/src/offset.rs crates/terraphim_alternatives/src/reanchor.rs
crates/terraphim_alternatives/src/words.rs
crates/terraphim_lab/Cargo.toml crates/terraphim_lab/src/actions.rs
crates/terraphim_lab/src/cuts.rs crates/terraphim_lab/src/lib.rs
crates/terraphim_lab/src/lists.rs crates/terraphim_lab/src/offset.rs
crates/terraphim_lab/src/rolegraph.rs crates/terraphim_lab/src/text.rs
crates/terraphim_lab/src/trim.rs crates/terraphim_lab/src/weak.rs
scripts/browser-tests.sh scripts/ci/typing-benchmark-diagnostics.py
tests/browser/typing_benchmark_diagnostics_test.py
""".split())
LOG_BASE = "docs/validation/evidence/rolegraph-browser/20261009/production/"
# Declarative retained-evidence approval. New logs need parent review and an
# exact path/hash manifest edit BEFORE use; no CLI allow-path/hash override.
APPROVED_LOGS = {
    LOG_BASE + "rolegraph-production-editor-browser-full.log":
        "775e145409761b43b0139adeb5eac8a694022817b3420027464808aba038f5e0",
    LOG_BASE + "rolegraph-production-editor-browser-bench-retry.log":
        "a70b69a6f1eef6904e23131ba06a750cebc566d0dc855bb034d55bb98b8e4c4c",
    LOG_BASE + "rolegraph-production-editor-browser-bench-controlled-rerun.log":
        "ac4a0f2364d9edcfa1d523cff5f4c99f5af1944121a95686ca5da6eb0ed90223",
}
MAX_LOG_BYTES = 1024 * 1024
MAX_SOURCE_BYTES = 16 * 1024 * 1024


class SafetyError(ValueError):
    """Only fixed safe reason codes: never paths or rejected contents."""


class SafeArgumentParser(argparse.ArgumentParser):
    def error(self, message):
        raise SafetyError("arguments_rejected")


def relative_input(value):
    path = Path(value)
    if path.is_absolute():
        try:
            path = path.relative_to(ROOT)
        except ValueError:
            raise SafetyError("path_rejected") from None
    if not path.parts or any(part in {"..", ".git"} for part in path.parts):
        raise SafetyError("path_rejected")
    return path.as_posix()


def confined_read(relative, limit):
    """No-follow every ancestor from /, then read only the pinned regular FD.

    Metadata resolution verifies strict real-root containment; fd-relative
    O_NOFOLLOW opens prevent link substitution after that check. If a directory
    is renamed after open, we read the pinned approved directory, not its new
    symlink replacement. All byte reads are bounded and source changes fail.
    """
    directory = None
    leaf = None
    try:
        path = ROOT / relative
        # lstat every lexical component BEFORE resolution or content access.
        current = Path(path.anchor)
        for component in path.parts[1:]:
            current = current / component
            if stat.S_ISLNK(current.lstat().st_mode):
                raise SafetyError("symlink_rejected")
        real_root = ROOT.resolve(strict=True)
        real_path = path.resolve(strict=True)
        if real_root not in real_path.parents:
            raise SafetyError("path_rejected")
        directory = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        for component in path.parts[1:-1]:
            next_fd = os.open(component, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                              dir_fd=directory)
            os.close(directory)
            directory = next_fd
        leaf = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        before = os.fstat(leaf)
        if not stat.S_ISREG(before.st_mode) or before.st_size > limit or before.st_nlink != 1:
            raise SafetyError("file_type_or_size_rejected")
        chunks = []
        total = 0
        while True:
            chunk = os.read(leaf, min(65536, limit + 1 - total))
            if not chunk:
                break
            chunks.append(chunk)
            total += len(chunk)
            if total > limit:
                raise SafetyError("file_size_rejected")
        after = os.fstat(leaf)
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns) != (
                after.st_size, after.st_mtime_ns, after.st_ctime_ns):
            raise SafetyError("file_changed_during_read")
        return b"".join(chunks)
    except (OSError, RuntimeError):
        raise SafetyError("file_access_rejected") from None
    finally:
        if leaf is not None:
            os.close(leaf)
        if directory is not None:
            os.close(directory)


def command(args):
    """Fixed read-only commands; no shell, no environment/stderr disclosure."""
    try:
        result = subprocess.run(args, cwd=ROOT, stdout=subprocess.PIPE,
                                stderr=subprocess.DEVNULL, timeout=5, check=False)
        return result.stdout.decode("utf-8", "replace") if result.returncode == 0 else None
    except (OSError, subprocess.TimeoutExpired):
        return None


def identity(path):
    relative = relative_input(path)
    if relative not in SOURCE_FILES:
        raise SafetyError("source_not_approved")
    data = confined_read(relative, MAX_SOURCE_BYTES)
    return {"sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def snapshot():
    sources = {p: identity(ROOT / p) for p in sorted(SOURCE_FILES)}
    return sources, telemetry()


def available(metrics):
    return {"status": "AVAILABLE", "metrics": metrics, "reason": None}


def unavailable(reason):
    return {"status": "NOT_AVAILABLE", "metrics": None, "reason": reason}


def telemetry():
    result = {"logical_cpus": available(os.cpu_count()) if os.cpu_count() else unavailable("cpu_count_unavailable")}
    try:
        result["load_1_5_15"] = available(list(os.getloadavg()))
    except OSError:
        result["load_1_5_15"] = unavailable("load_unavailable")
    # Aggregate only; do not disclose PIDs, command lines or user processes.
    rows = command(["ps", "-A", "-o", "pcpu=,rss="])
    values = []
    valid = bool(rows and rows.strip())
    for row in (rows or "").splitlines():
        match = re.fullmatch(r"\s*(\d+(?:\.\d+)?)\s+(\d+)\s*", row)
        if not match:
            valid = False
            break
        values.append((float(match[1]), int(match[2])))
    result["process_aggregate"] = available({
        "count": len(values), "pcpu_sum": sum(cpu for cpu, _ in values),
        "rss_kib_sum": sum(rss for _, rss in values),
    }) if valid else unavailable("command_unavailable" if rows is None else "empty_or_invalid_format")
    result["process_aggregate"]["caveat"] = "ps CPU is not interval CPU; RSS sum double-counts shared memory"
    if platform.system() == "Darwin":
        raw = command(["vm_stat"])
        result["vm_stat"] = parse_vm_stat(raw)
        memory = command(["sysctl", "-n", "hw.memsize"])
        result["hw_memsize"] = available(int(memory.strip())) if memory and re.fullmatch(r"\d+\s*", memory) else unavailable("command_unavailable" if memory is None else "empty_or_invalid_format")
    else:
        result["vm_stat"] = unavailable("unsupported_platform")
        result["hw_memsize"] = unavailable("unsupported_platform")
    result["complete"] = all(value["status"] == "AVAILABLE" for value in result.values() if isinstance(value, dict))
    return result


def parse_vm_stat(raw):
    if raw is None:
        return unavailable("command_unavailable")
    lines = raw.splitlines()
    header = re.fullmatch(r"Mach Virtual Memory Statistics: \(page size of (\d+) bytes\)", lines[0]) if lines else None
    if not header:
        return unavailable("empty_or_invalid_format")
    metrics = {"page_size_bytes": int(header[1])}
    # Ignore no unknown labels: output only these known numeric OS counters.
    labels = {
        "Pages free", "Pages active", "Pages inactive", "Pages speculative",
        "Pages throttled", "Pages wired down", "Pages purgeable", '"Translation faults"',
        "Pages copy-on-write", "Pages zero filled", "Pages reactivated", "Pages purged",
        "File-backed pages", "Anonymous pages", "Pages stored in compressor",
        "Pages occupied by compressor", "Decompressions", "Compressions", "Pageins",
        "Pageouts", "Swapins", "Swapouts",
    }
    for line in lines[1:]:
        match = re.fullmatch(r'([^:]+):\s+(\d+)\.', line)
        if not match or match[1] not in labels:
            return unavailable("empty_or_invalid_format")
        key = match[1].strip('"').lower().replace(' ', '_').replace('-', '_')
        if key in metrics:
            return unavailable("duplicate_metric")
        metrics[key] = int(match[2])
    return available(metrics) if "pages_free" in metrics and "pages_active" in metrics else unavailable("missing_metrics")


NUMBER = r"\d+(?:\.\d+)?"
OK_LINE = re.compile(
    rf"(?P<binary>web_bench(?:_logged)?): ok \((?P<wall>\d+)s wall, finished in (?P<harness>{NUMBER})s, load (?P<load>{NUMBER})\) "
    r"test result: ok\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; (?P<ignored>\d+) ignored; (?P<filtered>\d+) filtered out")
FAIL_LINE = re.compile(rf"(?P<binary>web_bench(?:_logged)?): FAILED \((?P<wall>\d+)s wall, finished in (?P<harness>{NUMBER})s, load (?P<load>{NUMBER})\)")
SKIP_LINE = re.compile(rf"(?P<binary>web_bench(?:_logged)?): SKIPPED \(load (?P<load>{NUMBER}) >= (?P<limit>{NUMBER}); benchmark\)")
# Retained guard-wait noise is validated, then discarded (never emitted).
WAIT_LINE = re.compile(
    rf"web_bench(?:_logged)?: waiting for load {NUMBER} to drop below {NUMBER} \(\d+s of \d+s\); busiest: "
    rf"[A-Za-z0-9 _()\-]+ {NUMBER}%(?:, [A-Za-z0-9 _()\-]+ {NUMBER}%)*")
ASSERTED_LINE = re.compile(
    r"Typing latency \(asserted\) on 5043 words \(30464 chars\), median of 3 rounds x 20 keystrokes "
    r"after 1 warm-up round\(s\) x 5 keystrokes and 5 untimed pre-roll keystrokes per round, ms per keystroke incl\. Rust conversion: "
    rf"textarea baseline \(sync conversion\) (?P<textarea>{NUMBER}); surface with 120 ms debounce (?P<surface>{NUMBER}); "
    rf"debounced trailing render \(once per burst\) (?P<flush>{NUMBER}) ms; debounced surface/textarea ratio (?P<ratio>{NUMBER}); "
    r"per-round totals textarea (?P<a>\d+/\d+/\d+) ms, debounced (?P<b>\d+/\d+/\d+) ms")


def parse_metric_lines(text):
    records = []
    for line in text.splitlines():
        line = line.strip()
        # Only candidate benchmark lines are parsed. All other historic output
        # (stack/process/commands) is discarded and is NEVER echoed.
        if not (line.startswith("web_bench") or line.startswith("Typing latency")):
            continue
        if WAIT_LINE.fullmatch(line):
            continue
        match = OK_LINE.fullmatch(line)
        if match:
            records.append({"kind": "runner_result", "binary": match["binary"], "status": "PASS",
                            "wall_seconds": int(match["wall"]), "harness_seconds": float(match["harness"]),
                            "start_load": float(match["load"]),
                            **{key: int(match[key]) for key in ("passed", "failed", "ignored", "filtered")}})
            continue
        match = FAIL_LINE.fullmatch(line)
        if match:
            records.append({"kind": "runner_result", "binary": match["binary"], "status": "FAIL",
                            "wall_seconds": int(match["wall"]), "harness_seconds": float(match["harness"]), "start_load": float(match["load"])})
            continue
        match = SKIP_LINE.fullmatch(line)
        if match:
            records.append({"kind": "runner_result", "binary": match["binary"], "status": "SKIPPED",
                            "load": float(match["load"]), "limit": float(match["limit"])})
            continue
        match = ASSERTED_LINE.fullmatch(line)
        if match:
            records.append({"kind": "asserted_latency", "words": 5043, "characters": 30464,
                            **{key: float(match[key]) for key in ("textarea", "surface", "flush", "ratio")},
                            "textarea_rounds_ms": [int(n) for n in match["a"].split("/")],
                            "surface_rounds_ms": [int(n) for n in match["b"].split("/")]})
            continue
        raise SafetyError("benchmark_metric_grammar_rejected")
    if not records:
        raise SafetyError("benchmark_metrics_missing")
    return records


def retained_log(value):
    relative = relative_input(value)
    if relative not in APPROVED_LOGS:
        raise SafetyError("log_not_approved")
    data = confined_read(relative, MAX_LOG_BYTES)
    digest = hashlib.sha256(data).hexdigest()
    if digest != APPROVED_LOGS[relative]:
        raise SafetyError("log_hash_rejected")
    try:
        metrics = parse_metric_lines(data.decode("utf-8", "strict"))
    except UnicodeDecodeError:
        raise SafetyError("log_encoding_rejected") from None
    return {"path": relative, "identity": {"sha256": digest, "bytes": len(data)},
            "metrics": metrics,
            "note": "success output suppression can leave raw timings unavailable"}


def main():
    parser = SafeArgumentParser(description=__doc__)
    parser.add_argument("--phase", choices=["inspection", "before", "after"], default="inspection")
    parser.add_argument("--run-id", required=True)
    parser.add_argument("--exit-status", type=int, help="parent-recorded official runner exit; not inferred")
    parser.add_argument("--log")
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,95}", args.run_id):
        raise SafetyError("run_id_rejected")
    # Reject unapproved logs before source/telemetry collection and any output.
    log = retained_log(args.log) if args.log else None
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    sources, telemetry = snapshot()
    result = {
        "schema": 1, "kind": "inspection-only", "run_id": args.run_id, "phase": args.phase,
        "utc_started": started, "utc_completed": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "monotonic_completed_ns": time.monotonic_ns(),
        "head": (command(["git", "rev-parse", "HEAD"]) or "unavailable").strip(),
        "os": {"system": platform.system(), "release": platform.release(), "machine": platform.machine()},
        "sources": sources, "telemetry": telemetry,
        "official_exit_status_parent_recorded": args.exit_status,
        "collection_status": "COMPLETE" if telemetry["complete"] else "METADATA_INCOMPLETE",
        "limitations": "No engine profile, during-key CPU, tool versions, served-WASM hash or source-freeze proof",
    }
    if log is not None:
        result["retained_log"] = log
    print(json.dumps(result, indent=2, sort_keys=True))
    # Collection status is independent of the parent-recorded benchmark exit.
    return 0 if telemetry["complete"] else 3


if __name__ == "__main__":
    try:
        sys.exit(main())
    except SafetyError as error:
        print(json.dumps({"status": "REJECTED", "reason": str(error)}), file=sys.stderr)
        sys.exit(2)
