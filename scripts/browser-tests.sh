#!/bin/bash

# Runs the browser test binaries (every tests/*.rs that declares
# `run_in_browser`) one at a time in headless Chrome and prints one line per
# binary: result, wall time, the harness's `finished in` and the host load.
#
#   scripts/browser-tests.sh                  # every browser binary
#   scripts/browser-tests.sh web web_ghosts   # just these
#
# Unlike a bare `wasm-pack test --headless --chrome`, which stops at the
# first failing binary, every binary runs and the summary shows them all.
#
# Host load (issues #53, #110). wasm-bindgen-test-runner gives each binary a
# fixed 20 s budget (see the header of tests/web.rs); the runner is not given
# more time here: an unresponsive page is a failure the suite exists to catch.
# Instead, before EVERY binary (on CI: before the first only, warn-only; see
# ON_CI below) the script waits for the one-minute load
# average to drop below BROWSER_TESTS_MAX_LOAD, printing the load, the limit,
# how long it has waited and the busiest processes (what it is waiting for)
# every 30 s. The wait is capped at BROWSER_TESTS_LOAD_WAIT seconds (default
# 300) per binary: if the load is still too high, that binary and the rest
# are NOT run (locally; never on CI), the summary lists them as NOT RUN, and
# the script exits 4 with
# a message saying so; it never runs a binary under a load it was told to
# avoid. On CI the binaries finish in at most 3.3 s of their 20 s budget
# (25 runs, see CLAUDE.md "Browser tests"); the timeouts and SIGKILLs seen
# on developer machines came from other processes (several agents' headless
# Chrome sessions on a shared laptop), which this wait keeps the tests away
# from.
#
# BROWSER_TESTS_MAX_LOAD defaults to 20 on Linux (the CI runner) and, on
# macOS, to three quarters of the logical CPUs (at least 4), where the load
# average counts threads waiting on I/O as well and a loaded laptop slows
# headless Chrome far sooner. Before each benchmark the script waits the same
# way for the stricter BROWSER_TESTS_BENCH_MAX_LOAD (default 12 on Linux,
# half the logical CPUs on macOS) and skips the benchmark, reported as
# SKIPPED, if the load stays above it: web_bench asserts that the
# debounced surface is no slower than the textarea baseline (issue #28) and
# web_bench_logged types into a 5,000-word document on several paths; both
# came within a few seconds of the budget, and web_bench overran it once, at
# a load of 14. The load average lags short spikes, so this lowers the risk
# rather than removing it.
#
# BROWSER_TESTS_CARGO_ARGS adds arguments to each binary's `cargo test`
# (CI passes --locked).
#
# Exit status: 0 when every binary passed, 1 when any failed, 3 when none
# failed but a benchmark was skipped for load (the run is incomplete), 4 when
# the load did not drop in time and binaries were not run (also incomplete;
# local runs only, never on CI).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

if [ "$(uname -s)" = Darwin ]; then
  cpus="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 8)"
  default_max=$(( cpus * 3 / 4 ))
  [ "$default_max" -ge 4 ] || default_max=4
  default_bench=$(( cpus / 2 ))
  [ "$default_bench" -ge 3 ] || default_bench=3
else
  default_max=20
  default_bench=12
fi
MAX_LOAD="${BROWSER_TESTS_MAX_LOAD:-$default_max}"
BENCH_MAX_LOAD="${BROWSER_TESTS_BENCH_MAX_LOAD:-$default_bench}"
LOAD_WAIT="${BROWSER_TESTS_LOAD_WAIT:-300}"
# On CI (CI, GITEA_ACTIONS or BROWSER_TESTS_CI set; scripts/ci/browser-tests.sh
# sets the last) the guard keeps its earlier, warn-only behaviour: one capped
# wait before the first binary, then every binary runs whatever the load.
# The CI runner passes at loads up to 23 with every binary far inside its
# budget, so a hard stop there would only add a way for CI to fail.
# A CI run waits out the cap at most once: after one CI wait has timed out,
# no later wait (for a benchmark or not) waits again; the load is checked
# once and the run goes on (benchmarks above their limit are still skipped).
ON_CI=0
if [ -n "${CI:-}${GITEA_ACTIONS:-}${BROWSER_TESTS_CI:-}" ]; then
  ON_CI=1
fi
CI_WAITED_OUT=0
# Benchmarks: run only below the load limit.
BENCHMARKS=" web_bench web_bench_logged "
# The runner's budget is part of what is tested.
unset WASM_BINDGEN_TEST_TIMEOUT
read -r -a CARGO_ARGS <<< "${BROWSER_TESTS_CARGO_ARGS:-}"

load1() {
  if [ -r /proc/loadavg ]; then
    cut -d' ' -f1 /proc/loadavg
  else
    sysctl -n vm.loadavg | awk '{ print $2 }'
  fi
}

below_limit() {
  awk -v l="$(load1)" -v m="$1" 'BEGIN { exit !(l < m) }'
}

# The busiest processes, so a wait says what it is waiting for.
busiest() {
  ps -A -o pcpu=,comm= 2>/dev/null | sort -rn | head -3 \
    | awk '{ cpu = $1; $1 = ""; sub(/^ /, ""); n = split($0, p, "/"); printf "%s%s %s%%", (NR > 1 ? ", " : ""), p[n], cpu }'
}

# Wait for the load to drop below the limit $1 before binary $2, at most
# LOAD_WAIT seconds, saying what it waits for every 30 s; returns 1 if the
# load did not drop in time.
wait_for_load() {
  local limit="$1" what="$2" waited=0 step=5
  if [ "$ON_CI" -eq 1 ] && [ "$CI_WAITED_OUT" -eq 1 ]; then
    if below_limit "$limit"; then
      return 0
    fi
    echo "  $what: load $(load1) >= $limit; CI already waited ${LOAD_WAIT}s once this run, not waiting again"
    return 1
  fi
  while ! below_limit "$limit"; do
    if [ "$waited" -ge "$LOAD_WAIT" ]; then
      echo "  load $(load1) still >= $limit after ${LOAD_WAIT}s; busiest: $(busiest)"
      [ "$ON_CI" -eq 0 ] || CI_WAITED_OUT=1
      return 1
    fi
    if [ $(( waited % 30 )) -eq 0 ]; then
      echo "  $what: waiting for load $(load1) to drop below $limit (${waited}s of ${LOAD_WAIT}s); busiest: $(busiest)"
    fi
    local nap=$(( LOAD_WAIT - waited < step ? LOAD_WAIT - waited : step ))
    sleep "$nap"
    waited=$((waited + nap))
  done
  return 0
}

if [ "$#" -gt 0 ]; then
  BINARIES=("$@")
else
  BINARIES=()
  for file in tests/*.rs; do
    if grep -q 'run_in_browser' "$file"; then
      name="${file#tests/}"
      BINARIES+=("${name%.rs}")
    fi
  done
fi

failed=0
skipped=0
not_run=()
summary=()
first=1
for bin in "${BINARIES[@]}"; do
  if [ "${#not_run[@]}" -gt 0 ]; then
    not_run+=("$bin")
    continue
  fi
  if [[ "$BENCHMARKS" == *" $bin "* ]]; then
    if ! wait_for_load "$BENCH_MAX_LOAD" "$bin"; then
      line="$bin: SKIPPED (load $(load1) >= $BENCH_MAX_LOAD; benchmark)"
      echo "$line"
      summary+=("$line")
      skipped=$((skipped + 1))
      continue
    fi
  elif [ "$ON_CI" -eq 1 ]; then
    if [ "$first" -eq 1 ] && ! wait_for_load "$MAX_LOAD" "$bin"; then
      echo "  CI: running anyway at load $(load1) (warn only; local runs stop here)"
    fi
  elif ! wait_for_load "$MAX_LOAD" "$bin"; then
    not_run+=("$bin")
    continue
  fi
  first=0
  load="$(load1)"
  start=$(date +%s)
  out="$(wasm-pack test --headless --chrome -- ${CARGO_ARGS[@]+"${CARGO_ARGS[@]}"} --test "$bin" 2>&1)"
  status=$?
  secs=$(( $(date +%s) - start ))
  finished="$(printf '%s\n' "$out" | grep -o 'finished in [0-9.]*s' | tail -1)"
  if [ "$status" -eq 0 ]; then
    result="$(printf '%s\n' "$out" | grep '^test result' | tail -1 | sed 's/; finished in.*//')"
    line="$bin: ok (${secs}s wall, ${finished:-no timing}, load $load) $result"
  else
    failed=$((failed + 1))
    line="$bin: FAILED (${secs}s wall, ${finished:-no result}, load $load)"
    printf '%s\n' "$out" | grep -v -E '^\s*(Compiling|Checking|Finished|Running|Starting|Only local|Please see|ChromeDriver was)' | tail -30
  fi
  echo "$line"
  summary+=("$line")
done

echo
echo "Browser test summary (load limit $MAX_LOAD, benchmarks $BENCH_MAX_LOAD):"
[ "${#summary[@]}" -eq 0 ] || printf '  %s\n' "${summary[@]}"
for bin in "${not_run[@]+"${not_run[@]}"}"; do
  echo "  $bin: NOT RUN (load did not drop below $MAX_LOAD within ${LOAD_WAIT}s)"
done
if [ "$failed" -gt 0 ]; then
  exit 1
fi
if [ "${#not_run[@]}" -gt 0 ]; then
  echo "Stopped: the load average stayed at or above $MAX_LOAD for ${LOAD_WAIT}s, so ${#not_run[@]} binaries were not run rather than run under load. Wait for the busy processes above to finish, or set BROWSER_TESTS_MAX_LOAD / BROWSER_TESTS_LOAD_WAIT." >&2
  exit 4
fi
if [ "$skipped" -gt 0 ]; then
  exit 3
fi
exit 0
