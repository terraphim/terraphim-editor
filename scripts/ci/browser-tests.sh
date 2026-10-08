#!/bin/bash

# The CI browser-test step: every run_in_browser binary in headless Chrome
# through scripts/browser-tests.sh, which runs all of them (a bare
# `wasm-pack test --headless --chrome` stops at the first failing binary)
# and prints a per-binary summary.
#
# BROWSER_TESTS_CI=1 keeps the load guard warn-only here (it never stops a
# CI run; exit 4 is for local runs). scripts/browser-tests.sh exits 3 when
# nothing failed but a benchmark
# (web_bench, web_bench_logged) was skipped because the host load stayed
# above its limit. The runner host is shared, so in CI that counts as a
# pass: the skip is reported in the summary and below, and every functional
# browser test still ran. Any failure (exit 1) fails the step.
#
# The tools from scripts/ci/tools.sh go on PATH, and each binary's
# `cargo test` gets --locked (offline: scripts/ci/env.sh).
set -uo pipefail
export CI_TOOLS_PATH=1
# shellcheck disable=SC1091  # env.sh is checked on its own; not followed here
source "$(dirname "$0")/env.sh"
cd "$CI_ROOT" || exit 1

BROWSER_TESTS_CI=1 BROWSER_TESTS_CARGO_ARGS=--locked bash scripts/browser-tests.sh "$@"
status=$?
if [ "$status" -eq 3 ]; then
  echo "NOTICE: benchmark binaries skipped for host load; all other browser tests passed"
  exit 0
fi
exit "$status"
