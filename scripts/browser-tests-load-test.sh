#!/bin/bash
# Checks the load guard of scripts/browser-tests.sh (issue #110) against the
# real load average, without starting a browser: with a limit of 0, which
# no machine can be under (the comparison is strict), the script must say
# what it is waiting for, give up after the
# capped wait, run nothing, list the binaries as NOT RUN and exit 4; a
# benchmark must be SKIPPED (exit 3); the default limit must be 20 on Linux
# and three quarters of the logical CPUs (at least 4) on macOS; on CI the
# guard must only warn and run the binaries anyway (never exit 4), skip a
# benchmark above its limit (exit 3, mapped to 0 by the CI wrapper), and wait
# out the cap at most once per run.
#
#   bash scripts/browser-tests-load-test.sh
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

fail=0
check() {
  if ! printf '%s\n' "$out" | grep -qF -- "$2"; then
    echo "FAIL: $1: expected '$2' in:"
    printf '%s\n' "$out" | sed 's/^/    /'
    fail=1
  fi
}

# These checks are about local runs: clear the CI markers for them.
local_env=(env -u CI -u GITEA_ACTIONS -u BROWSER_TESTS_CI)

# 1. The load can never drop below 0: wait (capped at 6 s), say why, run
# nothing.
start=$(date +%s)
out="$("${local_env[@]}" BROWSER_TESTS_MAX_LOAD=0 BROWSER_TESTS_LOAD_WAIT=6 bash scripts/browser-tests.sh web web_chrome 2>&1)"
status=$?
secs=$(( $(date +%s) - start ))
[ "$status" -eq 4 ] || { echo "FAIL: guard: exit $status, expected 4"; fail=1; }
check "guard" "web: waiting for load"
check "guard" "busiest:"
check "guard" "web: NOT RUN"
check "guard" "web_chrome: NOT RUN"
check "guard" "Stopped: the load average stayed at or above 0 for 6s"
if printf '%s\n' "$out" | grep -qE ': (ok|FAILED) \('; then
  echo "FAIL: guard: a binary ran under load"; fail=1
fi
[ "$secs" -ge 6 ] && [ "$secs" -lt 30 ] || { echo "FAIL: guard: took ${secs}s, expected the 6 s cap"; fail=1; }

# 2. A benchmark above its stricter limit is skipped, not run (exit 3).
out="$("${local_env[@]}" BROWSER_TESTS_BENCH_MAX_LOAD=0 BROWSER_TESTS_LOAD_WAIT=0 bash scripts/browser-tests.sh web_bench 2>&1)"
status=$?
[ "$status" -eq 3 ] || { echo "FAIL: benchmark: exit $status, expected 3"; fail=1; }
check "benchmark" "web_bench: SKIPPED"

# 3. The default limit for this platform (read from the summary line).
if [ "$(uname -s)" = Darwin ]; then
  cpus="$(sysctl -n hw.logicalcpu)"
  want=$(( cpus * 3 / 4 )); [ "$want" -ge 4 ] || want=4
else
  want=20
fi
out="$("${local_env[@]}" -u BROWSER_TESTS_MAX_LOAD BROWSER_TESTS_BENCH_MAX_LOAD=0 BROWSER_TESTS_LOAD_WAIT=0 bash scripts/browser-tests.sh web_bench 2>&1)"
check "default limit" "(load limit $want,"

# 4. On CI the guard is warn-only: one capped wait, then the binaries run
# whatever the load, and the run never exits 4. The binary named here does
# not exist, so "running" it fails fast without a browser; what matters is
# that it was attempted (FAILED, exit 1), not listed as NOT RUN.
out="$(env -u CI -u GITEA_ACTIONS BROWSER_TESTS_CI=1 BROWSER_TESTS_MAX_LOAD=0 BROWSER_TESTS_LOAD_WAIT=2 bash scripts/browser-tests.sh no_such_browser_binary 2>&1)"
status=$?
[ "$status" -ne 4 ] || { echo "FAIL: CI: exit 4 on CI"; fail=1; }
check "CI" "CI: running anyway at load"
check "CI" "no_such_browser_binary: FAILED"
if printf '%s\n' "$out" | grep -qF "NOT RUN"; then
  echo "FAIL: CI: binaries were not run on CI"; fail=1
fi

# 5. On CI the benchmarks are still skipped above their limit (exit 3, which
# the CI wrapper maps to 0), and a run waits out the cap at most once: two
# benchmarks under an impossible limit give one capped wait, not two. No
# browser starts (both are skipped).
start=$(date +%s)
out="$(env -u CI -u GITEA_ACTIONS BROWSER_TESTS_CI=1 BROWSER_TESTS_BENCH_MAX_LOAD=0 BROWSER_TESTS_LOAD_WAIT=3 bash scripts/browser-tests.sh web_bench web_bench_logged 2>&1)"
status=$?
csecs=$(( $(date +%s) - start ))
[ "$status" -eq 3 ] || { echo "FAIL: CI benchmarks: exit $status, expected 3"; fail=1; }
check "CI benchmarks" "web_bench: SKIPPED"
check "CI benchmarks" "web_bench_logged: SKIPPED"
check "CI benchmarks" "CI already waited 3s once this run, not waiting again"
waits="$(printf '%s\n' "$out" | grep -c 'still >= 0 after 3s')"
[ "$waits" -eq 1 ] || { echo "FAIL: CI benchmarks: $waits capped waits, expected 1"; fail=1; }
[ "$csecs" -lt 6 ] || { echo "FAIL: CI benchmarks: took ${csecs}s, expected one 3 s wait"; fail=1; }
out="$(env -u CI -u GITEA_ACTIONS BROWSER_TESTS_BENCH_MAX_LOAD=0 BROWSER_TESTS_LOAD_WAIT=1 bash scripts/ci/browser-tests.sh web_bench 2>&1)"
status=$?
[ "$status" -eq 0 ] || { echo "FAIL: CI wrapper: exit $status, expected 0 for a benchmark skip"; fail=1; }
check "CI wrapper" "NOTICE: benchmark binaries skipped for host load"

if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "browser-tests load guard: ok (local: exit 4 after the ${secs}s cap; benchmark skipped; default limit $want; CI: warn only, binaries run, benchmarks skipped with one wait)"
