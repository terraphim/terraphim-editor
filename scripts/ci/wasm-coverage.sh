#!/usr/bin/env bash
# Coverage of the Rust code as it runs in the browser tests (issue #100).
#
# `cargo llvm-cov` measures the native tests only. The `#[wasm_bindgen]`
# exports (src/lib.rs, and the document, KG, provider and Lab APIs) run only
# in the headless-Chrome suites, so their coverage was never measured. This
# script builds every browser test binary with wasm-bindgen-test's coverage
# support, runs it, and reports line, function and region coverage per file.
#
#   bash scripts/ci/wasm-coverage.sh              # every browser suite
#   bash scripts/ci/wasm-coverage.sh web web_kg   # just these
#
# It does not gate CI. It exits non-zero if a suite fails or writes no
# profile. Output, in $WASM_COV_OUT (default target/wasm-coverage):
#   summary.txt     llvm-cov report for src/ and crates/*/src
#   coverage.lcov   the same data as lcov
#   js-summary.txt  V8 line and function coverage of public/js
#   suites.txt      one line per suite: name, exit status, test result
#
# How it works (wasm-bindgen guide, "Coverage"):
#   1. Build with nightly `-Cinstrument-coverage -Zno-profiler-runtime
#      --emit=llvm-ir --cfg=wasm_bindgen_unstable_test_coverage`. minicov
#      (a wasm-bindgen-test dependency under that cfg) is the profiler
#      runtime; its C part needs a clang that targets wasm32.
#   2. wasm-bindgen-test-runner writes one .profraw per binary to
#      LLVM_PROFILE_FILE (%m and %p are expanded) after the tests ran.
#   3. llvm-cov needs an object file holding the coverage mapping. The
#      WebAssembly object writer puts `__llvm_prf_names` in a data segment,
#      so llvm-cov finds "no coverage data" in a wasm object compiled from
#      the .ll files. scripts/ci/covmap-ir.py keeps only the mapping globals
#      (plain constants with ELF section names), and clang compiles them as an
#      x86_64 ELF object, which llvm-cov reads.
#
# JavaScript: each suite's Chrome gets --remote-debugging-port through a
# temporary webdriver.json (the runner reads it from the working directory),
# and scripts/ci/js-coverage-cdp.py collects V8 precise coverage of the
# public/js files the tests inject. WASM_COV_JS=0 turns this off.
#
# A suite that fails is run once more, as CI does, because the instrumented
# build is slower and a loaded host can push a WebDriver call past its fixed
# 20 s; no timeout is raised. The failed attempt's profiles are discarded, so
# the coverage is that of the reported run.
#
# The cdylib build of the crate has no minicov, so `__llvm_profile_runtime`
# is undefined there; `-Clink-arg=--allow-undefined` lets that link. The test
# binaries do link minicov: a binary that did not would fail to instantiate
# and write no profile, which this script reports as a failure.
#
# The asserted latency benchmarks (web_bench, web_bench_logged) are skipped
# by default: instrumented code is slower and they measure time, not paths.
#
# Requirements, each overridable:
#   WASM_COV_TOOLCHAIN   nightly toolchain with the wasm32-unknown-unknown
#                        target and llvm-tools (default: nightly)
#   LLVM_BIN             directory with clang, llvm-profdata and llvm-cov of
#                        the toolchain's LLVM major version (default: found
#                        from Homebrew llvm@N or clang-N on PATH)
#   WASM_BINDGEN_TEST_RUNNER  runner of Cargo.lock's wasm-bindgen version
#                        (default: on PATH, or wasm-pack's cache)
#   CHROMEDRIVER         chromedriver for the installed Chrome (default: on
#                        PATH, or the newest in wasm-pack's cache that
#                        matches the Chrome major version)
# The private registry needs CARGO_REGISTRIES_TERRAPHIM_TOKEN as for any
# local build (CI serves it offline through scripts/ci/registry.sh).
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT" || exit 1

TC="${WASM_COV_TOOLCHAIN:-nightly}"
OUT="${WASM_COV_OUT:-$ROOT/target/wasm-coverage}"
TARGET=wasm32-unknown-unknown
WASM_PACK_CACHE="${WASM_PACK_CACHE:-$HOME/Library/Caches/.wasm-pack}"
[ -d "$WASM_PACK_CACHE" ] || WASM_PACK_CACHE="$HOME/.cache/.wasm-pack"

die() { echo "wasm-coverage: $*" >&2; exit 2; }

# --- tools -----------------------------------------------------------------

rustc +"$TC" -V >/dev/null 2>&1 || die "toolchain $TC not installed (rustup toolchain install $TC)"
llvm_major="$(rustc +"$TC" -vV | awk '/^LLVM version/ { split($3, v, "."); print v[1] }')"
[ -n "$llvm_major" ] || die "cannot read the LLVM version of $TC"

if [ -z "${LLVM_BIN:-}" ]; then
  for dir in "/opt/homebrew/opt/llvm@$llvm_major/bin" "/usr/local/opt/llvm@$llvm_major/bin" "/usr/lib/llvm-$llvm_major/bin"; do
    if [ -x "$dir/clang" ]; then LLVM_BIN="$dir"; break; fi
  done
fi
[ -n "${LLVM_BIN:-}" ] && [ -x "$LLVM_BIN/clang" ] \
  || die "no clang for LLVM $llvm_major; install it (brew install llvm@$llvm_major) or set LLVM_BIN"
for tool in clang llvm-ar llvm-profdata llvm-cov; do
  [ -x "$LLVM_BIN/$tool" ] || die "$LLVM_BIN/$tool missing"
done
clang_major="$("$LLVM_BIN/clang" --version | awk 'NR == 1 { for (i = 1; i <= NF; i++) if ($i == "version") { split($(i + 1), v, "."); print v[1] } }')"
[ "$clang_major" = "$llvm_major" ] || die "clang is LLVM $clang_major, $TC is LLVM $llvm_major"

locked_bindgen="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)"
bindgen_version() { "$(dirname "$1")/wasm-bindgen" --version 2>/dev/null | awk '{ print $2 }'; }
if [ -z "${WASM_BINDGEN_TEST_RUNNER:-}" ]; then
  candidates=()
  path_runner="$(command -v wasm-bindgen-test-runner || true)"
  [ -n "$path_runner" ] && candidates+=("$path_runner")
  for r in "$WASM_PACK_CACHE"/wasm-bindgen-*/wasm-bindgen-test-runner; do
    [ -x "$r" ] && candidates+=("$r")
  done
  for r in "${candidates[@]+"${candidates[@]}"}"; do
    if [ "$(bindgen_version "$r")" = "$locked_bindgen" ]; then WASM_BINDGEN_TEST_RUNNER="$r"; break; fi
  done
fi
[ -n "${WASM_BINDGEN_TEST_RUNNER:-}" ] \
  || die "no wasm-bindgen-test-runner $locked_bindgen; run wasm-pack test once or set WASM_BINDGEN_TEST_RUNNER"

if [ -z "${CHROMEDRIVER:-}" ]; then
  chrome_major=""
  for chrome in "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" google-chrome google-chrome-stable chromium; do
    if v="$("$chrome" --version 2>/dev/null)"; then
      chrome_major="$(echo "$v" | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+\./) { split($i, p, "."); print p[1]; exit } }')"
      break
    fi
  done
  for d in "$(command -v chromedriver || true)" "$WASM_PACK_CACHE"/chromedriver-*/chromedriver; do
    [ -n "$d" ] && [ -x "$d" ] || continue
    major="$("$d" --version 2>/dev/null | awk '{ split($2, p, "."); print p[1] }')"
    if [ -z "$chrome_major" ] || [ "$major" = "$chrome_major" ]; then CHROMEDRIVER="$d"; break; fi
  done
fi
[ -n "${CHROMEDRIVER:-}" ] || die "no chromedriver for Chrome ${chrome_major:-?}; set CHROMEDRIVER"
export CHROMEDRIVER

# --- build -----------------------------------------------------------------

if [ "$#" -gt 0 ]; then
  SUITES=("$@")
else
  SUITES=()
  BENCHMARKS=" web_bench web_bench_logged "
  for file in tests/*.rs; do
    name="$(basename "$file" .rs)"
    case "$BENCHMARKS" in *" $name "*) continue ;; esac
    if awk '/run_in_browser/ { found = 1; exit } END { exit !found }' "$file"; then
      SUITES+=("$name")
    fi
  done
fi

rm -rf "$OUT/prof" "$OUT/obj"
mkdir -p "$OUT/prof" "$OUT/obj"
export CARGO_TARGET_DIR="$OUT/target"
export CARGO_BUILD_BUILD_DIR="$OUT/target"
export RUSTFLAGS="-Cinstrument-coverage -Zno-profiler-runtime --emit=llvm-ir --cfg=wasm_bindgen_unstable_test_coverage -Clink-arg=--allow-undefined"
export CARGO_HOST_RUSTFLAGS="--cfg=wasm_bindgen_unstable_test_coverage"
export CC_wasm32_unknown_unknown="$LLVM_BIN/clang"
export AR_wasm32_unknown_unknown="$LLVM_BIN/llvm-ar"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER="$WASM_BINDGEN_TEST_RUNNER"
unset WASM_BINDGEN_TEST_TIMEOUT

test_args=()
for s in "${SUITES[@]}"; do test_args+=(--test "$s"); done

echo "wasm-coverage: $TC (LLVM $llvm_major), clang $LLVM_BIN, runner $locked_bindgen, ${#SUITES[@]} suites"
if ! cargo +"$TC" test --target "$TARGET" "${test_args[@]}" --no-run --message-format=json-render-diagnostics \
    > "$OUT/build.json" 2> "$OUT/build.log"; then
  tail -30 "$OUT/build.log" >&2
  die "instrumented build failed (log: $OUT/build.log)"
fi

# The .ll beside each workspace artefact: the test binaries and the libraries.
python3 -I - "$OUT/build.json" "$ROOT" > "$OUT/ll-files.txt" <<'PY'
import json, os, sys
build, root = sys.argv[1], sys.argv[2]
seen = []
for line in open(build, encoding="utf-8"):
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    if msg.get("reason") != "compiler-artifact":
        continue
    if not msg.get("manifest_path", "").startswith(root):
        continue  # registry and crates.io dependencies
    for f in msg.get("filenames", []):
        stem, ext = os.path.splitext(os.path.basename(f))
        if ext == ".rlib":
            stem = stem[3:] if stem.startswith("lib") else stem
        elif ext != ".wasm":
            continue
        ll = os.path.join(os.path.dirname(f), stem + ".ll")
        if os.path.exists(ll) and ll not in seen:
            seen.append(ll)
print("\n".join(seen))
PY
[ -s "$OUT/ll-files.txt" ] || die "no .ll files found for the workspace artefacts"

# --- run -------------------------------------------------------------------

: > "$OUT/suites.txt"
JS="${WASM_COV_JS:-1}"
if [ "$JS" = 1 ] && [ -e "$ROOT/webdriver.json" ]; then
  die "$ROOT/webdriver.json exists; move it away or set WASM_COV_JS=0"
fi
trap 'rm -f "$ROOT/webdriver.json"' EXIT
trap 'exit 130' INT TERM
rm -f "$OUT"/js-*.json

count_profiles() { local n=0 f; for f in "$OUT"/prof/*.profraw; do [ -e "$f" ] && n=$((n + 1)); done; echo "$n"; }

# One run of suite $1, attempt $2; JavaScript coverage to js-$1-$2.json.
run_suite() {
  local s="$1" attempt="$2" port collector=""
  if [ "$JS" = 1 ]; then
    port="$(python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
    printf '{"goog:chromeOptions": {"args": ["--remote-debugging-port=%s"]}}\n' "$port" > "$ROOT/webdriver.json"
    python3 -I "$ROOT/scripts/ci/js-coverage-cdp.py" collect "$port" "$OUT/js-$s-$attempt.json" "$ROOT/public/js" &
    collector=$!
  fi
  LLVM_PROFILE_FILE="$OUT/prof/${s}_${attempt}_%m_%p.profraw" \
    cargo +"$TC" test --target "$TARGET" --test "$s" > "$OUT/run-$s.log" 2>&1
  local status=$?
  rm -f "$ROOT/webdriver.json"
  if [ -n "$collector" ]; then
    # The collector stops when the browser goes away; give it 10 s.
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$collector" 2>/dev/null || break
      sleep 1
    done
    kill -TERM "$collector" 2>/dev/null
    wait "$collector" 2>/dev/null
  fi
  return "$status"
}

failed=0
for s in "${SUITES[@]}"; do
  note=""
  for attempt in 1 2; do
    before="$(count_profiles)"
    run_suite "$s" "$attempt"
    status=$?
    after="$(count_profiles)"
    result="$(awk '/^test result:/ { sub(/^test result: /, ""); r = $0 } END { print r }' "$OUT/run-$s.log")"
    if [ "$after" -le "$before" ]; then
      result="$result (no profile written)"
      [ "$status" -ne 0 ] || status=1
    fi
    [ "$status" -ne 0 ] || break
    cp "$OUT/run-$s.log" "$OUT/run-$s-attempt$attempt.log"
    note=" (attempt $attempt failed: run-$s-attempt$attempt.log)"
    if [ "$attempt" = 1 ]; then
      # Only the attempt that is reported counts towards the coverage.
      rm -f "$OUT/prof/${s}_1_"*.profraw "$OUT/js-$s-1.json"
    fi
  done
  echo "$s exit=$status ${result:-no test result}$note" | tee -a "$OUT/suites.txt"
  [ "$status" -eq 0 ] || failed=1
done

# --- report ----------------------------------------------------------------

objects=()
while IFS= read -r ll; do
  [ -n "$ll" ] || continue
  name="$(basename "$ll" .ll)"
  python3 -I "$ROOT/scripts/ci/covmap-ir.py" "$ll" "$OUT/obj/$name.ll" || continue
  "$LLVM_BIN/clang" --target=x86_64-unknown-linux-gnu -Wno-override-module -c "$OUT/obj/$name.ll" -o "$OUT/obj/$name.o" \
    || die "clang could not compile the coverage mapping of $name"
  objects+=(-object "$OUT/obj/$name.o")
done < "$OUT/ll-files.txt"
[ "${#objects[@]}" -gt 0 ] || die "no coverage objects"

# Merged after the objects are built, so llvm-cov does not warn that the
# objects are newer than the profile.
profraws=("$OUT"/prof/*.profraw)
[ -e "${profraws[0]}" ] || die "no .profraw files written"
"$LLVM_BIN/llvm-profdata" merge -sparse "${profraws[@]}" -o "$OUT/merged.profdata" || die "llvm-profdata merge failed"

ignore='(/\.cargo/|/rustlib/|/rustc/|/tests/|/benches/)'
"$LLVM_BIN/llvm-cov" report --instr-profile="$OUT/merged.profdata" "${objects[@]}" \
  --ignore-filename-regex="$ignore" 2> "$OUT/llvm-cov.log" \
  | sed "s#$ROOT/##" > "$OUT/summary.txt"
"$LLVM_BIN/llvm-cov" export --format=lcov --instr-profile="$OUT/merged.profdata" "${objects[@]}" \
  --ignore-filename-regex="$ignore" > "$OUT/coverage.lcov" 2>> "$OUT/llvm-cov.log"
[ -s "$OUT/summary.txt" ] || { cat "$OUT/llvm-cov.log" >&2; die "llvm-cov report failed"; }

cat "$OUT/summary.txt"
echo "wasm-coverage: summary in $OUT/summary.txt, lcov in $OUT/coverage.lcov"
if [ "$JS" = 1 ]; then
  js_runs=("$OUT"/js-*.json)
  if [ -e "${js_runs[0]}" ]; then
    python3 -I "$ROOT/scripts/ci/js-coverage-cdp.py" report "$ROOT/public/js" "${js_runs[@]}" > "$OUT/js-summary.txt"
    cat "$OUT/js-summary.txt"
    echo "wasm-coverage: JavaScript summary in $OUT/js-summary.txt"
  else
    echo "wasm-coverage: no JavaScript coverage was collected" >&2
    failed=1
  fi
fi
exit "$failed"
