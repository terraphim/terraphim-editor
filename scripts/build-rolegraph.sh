#!/bin/bash
# Build the separate Worker WASM module; never writes committed dist/ by default.
# Uses the published crates.io packages and the admitted public workspace lock.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
OUT="${1:-target/rolegraph-dist}"
if [ -n "${TE_ROLEGRAPH_SOURCE:-}" ]; then
  echo "rolegraph: source overrides are forbidden in production builds" >&2
  exit 1
fi

python3 scripts/check-public-dependencies.py
METADATA="$(cargo metadata --locked --format-version 1 --manifest-path "$ROOT/Cargo.toml")"
python3 scripts/ci/rolegraph-production-check.py metadata <<< "$METADATA"
VERSION="$(python3 -c 'import json,sys; m=json.load(sys.stdin); print(next(p["version"] for p in m["packages"] if p["name"]=="wasm-bindgen"))' <<< "$METADATA")"
TARGET="$(python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<< "$METADATA")"
BINDGEN="${WASM_BINDGEN:-wasm-bindgen}"
ACTUAL="$("$BINDGEN" --version | awk '{print $2}')"
if [ "$VERSION" != "$ACTUAL" ]; then
  echo "rolegraph: wasm-bindgen CLI $ACTUAL does not match locked crate $VERSION; set WASM_BINDGEN to a matching executable" >&2
  exit 1
fi
cargo build --locked --release --target wasm32-unknown-unknown -p terraphim_rolegraph_browser
mkdir -p "$OUT"
"$BINDGEN" --target web --out-dir "$OUT" --out-name terraphim_rolegraph_browser \
  "$TARGET/wasm32-unknown-unknown/release/terraphim_rolegraph_browser.wasm"
cp public/js/rolegraph-worker.js "$OUT/rolegraph-worker.js"
python3 scripts/ci/rolegraph-production-check.py graph-assets "$OUT"
echo "rolegraph: Worker WASM and glue written to $OUT"
