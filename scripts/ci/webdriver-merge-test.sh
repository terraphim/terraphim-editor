#!/bin/bash

# Pins how wasm-coverage.sh combines the repository's webdriver.json with the
# per-suite debugging port (issue #116): the window size stays, the port is
# added once, other capabilities survive, and no base means the port alone.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

merge() { python3 -I "$here/webdriver-merge.py" "$@"; }
args_of() { python3 -I -c 'import json,sys; print(" ".join(json.load(sys.stdin)["goog:chromeOptions"]["args"]))'; }

failed=0
check() { # name expected actual
  [ "$2" = "$3" ] || { echo "webdriver-merge-test: $1: expected '$2', got '$3'" >&2; failed=1; }
}

# The repository's own file keeps its window size and gains the port.
got="$(merge "$here/../../webdriver.json" --remote-debugging-port=9333 | args_of)"
check "repository file" "--window-size=1140,900 --remote-debugging-port=9333" "$got"

# No base, or an empty path: just the port.
check "missing base" "--remote-debugging-port=1" "$(merge "$tmp/none.json" --remote-debugging-port=1 | args_of)"
check "empty path" "--remote-debugging-port=1" "$(merge "" --remote-debugging-port=1 | args_of)"

# Other capabilities survive; a repeated argument is not duplicated.
printf '{"goog:chromeOptions": {"args": ["--a"], "binary": "/x"}, "other": 1}\n' > "$tmp/base.json"
out="$(merge "$tmp/base.json" --a --b)"
check "dedup" "--a --b" "$(printf '%s' "$out" | args_of)"
check "kept" "/x 1" "$(printf '%s' "$out" | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print(d["goog:chromeOptions"]["binary"], d["other"])')"

# A base that is not an object is refused.
printf '[]\n' > "$tmp/bad.json"
if merge "$tmp/bad.json" --a > /dev/null 2>&1; then
  echo "webdriver-merge-test: a non-object base should be refused" >&2; failed=1
fi

[ "$failed" = 0 ] && echo "webdriver-merge-test: ok"
exit "$failed"
