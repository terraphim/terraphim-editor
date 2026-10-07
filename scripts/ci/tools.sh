#!/bin/bash

# Installs the browser-test and bundle tools for the Gitea CI into
# target/ci-tools (see scripts/ci/env.sh), x86_64 Linux only:
#
#   wasm-pack       pinned release archive, SHA-256 checked
#   wasm-bindgen    the CLI and test runner, pinned to Cargo.lock's
#                   wasm-bindgen; wasm-pack and Trunk use a matching one on
#                   PATH instead of downloading their own
#   trunk           pinned release archive, SHA-256 checked
#   chromedriver    pinned Chrome for Testing archive, SHA-256 checked; it
#                   must match the installed google-chrome (see below)
#   terser, clean-css-cli
#                   scripts/ci/node/package-lock.json, `npm ci
#                   --ignore-scripts`; Trunk's post_build hook
#                   (scripts/build-dist.sh) uses them
#
# target/ persists between jobs on a runner, so nothing installed by an
# earlier job is trusted: bin/, node/ and wasm-pack's cache are removed and
# rebuilt on every run. Only the downloaded archives are kept, and each is
# checked against its pinned SHA-256 before use.
#
# chromedriver has to match the browser's major version. wasm-pack only
# downloads one when none is on PATH, and then takes the newest stable
# release whatever Chrome is installed. Chrome for Testing publishes no
# checksums, so the archive is pinned here: when google-chrome on the runner
# is upgraded, this step fails and says which pins to update.
set -euo pipefail
source "$(dirname "$0")/env.sh"
cd "$CI_ROOT"

WASM_PACK_VERSION=0.13.1
WASM_PACK_SHA256=c539d91ccab2591a7e975bcf82c82e1911b03335c80aa83d67ad25ed2ad06539
WASM_BINDGEN_VERSION=0.2.129
WASM_BINDGEN_SHA256=82d12bb940e2d4e72e0d5605387fc1b8ca179044e012b620f0ce4e7440e8320e
TRUNK_VERSION=0.21.14
TRUNK_SHA256=f2b4680cd239693a646a2795e4633c625328d7b2a044fbe749fa3a2fe9e7036b
CHROMEDRIVER_VERSION=145.0.7632.75
CHROMEDRIVER_SHA256=20ebc33ae9306b1c840cbdfa2d4f3f06f8894d8fee0cfaf9b40ff06e719c008e
CHROME="${CHROME:-google-chrome}"

die() {
  echo "tools: $*" >&2
  exit 1
}

# The wasm-bindgen CLI must be the version the crate links against.
locked_bindgen="$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/[^0-9.]/, ""); print; exit }' Cargo.lock)"
[ "$locked_bindgen" = "$WASM_BINDGEN_VERSION" ] ||
  die "Cargo.lock has wasm-bindgen $locked_bindgen; update WASM_BINDGEN_VERSION and WASM_BINDGEN_SHA256 in $0"

# The pinned chromedriver must drive the installed Chrome (same build).
chrome_version="$("$CHROME" --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+' | head -n 1)"
[ "${chrome_version%.*}" = "${CHROMEDRIVER_VERSION%.*}" ] ||
  die "$CHROME is $chrome_version but chromedriver is pinned to $CHROMEDRIVER_VERSION; in $0 set CHROMEDRIVER_VERSION to $chrome_version and CHROMEDRIVER_SHA256 to the sha256sum of https://storage.googleapis.com/chrome-for-testing-public/$chrome_version/linux64/chromedriver-linux64.zip"

rm -rf "${CI_TOOLS:?}/bin" "${CI_TOOLS:?}/node" "${WASM_PACK_CACHE:?}" "${CI_TOOLS:?}/unpack"
mkdir -p "$CI_TOOLS/bin" "$CI_TOOLS/download" "$CI_TOOLS/unpack"

# fetch URL SHA256: the archive, from the download cache when its checksum
# matches, otherwise downloaded and checked; prints its path.
fetch() {
  local url="$1" sum="$2" file partial
  file="$CI_TOOLS/download/$sum-$(basename "$url")"
  if [ -f "$file" ] && echo "$sum  $file" | sha256sum -c --quiet - >/dev/null 2>&1; then
    echo "$file"
    return
  fi
  partial="$file.partial"
  curl -fsSL --retry 3 -o "$partial" "$url"
  echo "$sum  $partial" | sha256sum -c --quiet - >&2 || die "checksum mismatch for $url"
  mv "$partial" "$file"
  echo "$file"
}

unpack() {
  local dir
  dir="$(mktemp -d "$CI_TOOLS/unpack/XXXXXX")"
  case "$1" in
    *.zip) unzip -q "$1" -d "$dir" ;;
    *) tar -xzf "$1" -C "$dir" ;;
  esac
  echo "$dir"
}

dir="$(unpack "$(fetch "https://github.com/rustwasm/wasm-pack/releases/download/v$WASM_PACK_VERSION/wasm-pack-v$WASM_PACK_VERSION-x86_64-unknown-linux-musl.tar.gz" "$WASM_PACK_SHA256")")"
install -m 755 "$dir/wasm-pack-v$WASM_PACK_VERSION-x86_64-unknown-linux-musl/wasm-pack" "$CI_TOOLS/bin/"

dir="$(unpack "$(fetch "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$WASM_BINDGEN_VERSION/wasm-bindgen-$WASM_BINDGEN_VERSION-x86_64-unknown-linux-musl.tar.gz" "$WASM_BINDGEN_SHA256")")"
for tool in wasm-bindgen wasm-bindgen-test-runner wasm2es6js; do
  install -m 755 "$dir/wasm-bindgen-$WASM_BINDGEN_VERSION-x86_64-unknown-linux-musl/$tool" "$CI_TOOLS/bin/"
done

dir="$(unpack "$(fetch "https://github.com/trunk-rs/trunk/releases/download/v$TRUNK_VERSION/trunk-x86_64-unknown-linux-gnu.tar.gz" "$TRUNK_SHA256")")"
install -m 755 "$dir/trunk" "$CI_TOOLS/bin/"

dir="$(unpack "$(fetch "https://storage.googleapis.com/chrome-for-testing-public/$CHROMEDRIVER_VERSION/linux64/chromedriver-linux64.zip" "$CHROMEDRIVER_SHA256")")"
install -m 755 "$dir/chromedriver-linux64/chromedriver" "$CI_TOOLS/bin/"

rm -rf "$CI_TOOLS/unpack"

mkdir -p "$CI_TOOLS/node"
cp scripts/ci/node/package.json scripts/ci/node/package-lock.json "$CI_TOOLS/node/"
npm ci --prefix "$CI_TOOLS/node" --ignore-scripts --no-audit --no-fund

export PATH="$CI_TOOLS/bin:$CI_TOOLS/node/node_modules/.bin:$PATH"
echo "tools:"
for tool in wasm-pack wasm-bindgen wasm-bindgen-test-runner trunk chromedriver terser cleancss; do
  echo "  $(command -v "$tool"): $("$tool" --version | head -n 1)"
done
echo "  $(command -v "$CHROME"): $("$CHROME" --version)"
