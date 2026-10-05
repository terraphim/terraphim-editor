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
# Host load (issue #53). wasm-bindgen-test-runner gives each binary a fixed
# 20 s budget (see the header of tests/web.rs); the runner is not given more
# time here: an unresponsive page is a failure the suite exists to catch.
# Instead, before the first binary the script waits (up to
# BROWSER_TESTS_LOAD_WAIT seconds, default 300) for the one-minute load
# average to drop below BROWSER_TESTS_MAX_LOAD (default 20), so that the
# browser tests do not run alongside a heavy build; the binaries then run
# whatever the load, and each line records the load it ran at. Before each
# benchmark it waits the same way for the stricter
# BROWSER_TESTS_BENCH_MAX_LOAD (default 12) and skips the benchmark, reported
# as SKIPPED, if the load stays above it: web_bench asserts that the
# debounced surface is no slower than the textarea baseline (issue #28) and
# web_bench_logged types into a 5,000-word document on several paths; both
# came within a few seconds of the budget, and web_bench overran it once, at
# a load of 14. The load average lags short spikes, so this lowers the risk
# rather than removing it.
#
# Exit status: 0 when every binary passed, 1 when any failed, 3 when none
# failed but a benchmark was skipped for load (the run is incomplete).
set -uo pipefail
cd "$(dirname "$0")/.."

MAX_LOAD="${BROWSER_TESTS_MAX_LOAD:-20}"
BENCH_MAX_LOAD="${BROWSER_TESTS_BENCH_MAX_LOAD:-12}"
LOAD_WAIT="${BROWSER_TESTS_LOAD_WAIT:-300}"
# Benchmarks: run only below the load limit.
BENCHMARKS=" web_bench web_bench_logged "
# The runner's budget is part of what is tested.
unset WASM_BINDGEN_TEST_TIMEOUT

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

# Wait for the load to drop below the limit $1; returns 1 if it did not.
wait_for_load() {
  local limit="$1" waited=0
  while ! below_limit "$limit"; do
    if [ "$waited" -ge "$LOAD_WAIT" ]; then
      return 1
    fi
    if [ "$waited" -eq 0 ]; then
      echo "  waiting for load $(load1) to drop below $limit (up to ${LOAD_WAIT}s)"
    fi
    sleep 10
    waited=$((waited + 10))
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
summary=()
first=1
for bin in "${BINARIES[@]}"; do
  if [[ "$BENCHMARKS" == *" $bin "* ]]; then
    if ! wait_for_load "$BENCH_MAX_LOAD"; then
      line="$bin: SKIPPED (load $(load1) >= $BENCH_MAX_LOAD; benchmark)"
      echo "$line"
      summary+=("$line")
      skipped=$((skipped + 1))
      continue
    fi
  elif [ "$first" -eq 1 ]; then
    wait_for_load "$MAX_LOAD" || true
  fi
  first=0
  load="$(load1)"
  start=$(date +%s)
  out="$(wasm-pack test --headless --chrome -- --test "$bin" 2>&1)"
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
printf '  %s\n' "${summary[@]}"
if [ "$failed" -gt 0 ]; then
  exit 1
fi
if [ "$skipped" -gt 0 ]; then
  exit 3
fi
exit 0
