#!/bin/bash

# Makes the Terraphim registry crates in Cargo.lock available to Cargo
# without a registry token.
#
# The Terraphim registry serves its index entries and crate downloads to
# anyone on the tailnet; only its config.json asks for a token, and Cargo
# reads config.json before anything else. So this step downloads, without
# credentials, the index entry and the .crate of every package Cargo.lock
# takes from that registry, checks each crate against Cargo.lock's SHA-256,
# and lays them out as a Cargo local registry in target/ci-registry. The
# Cargo home of scripts/ci/env.sh (target/ci-cargo-home) replaces the
# registry with that directory, so no Cargo command in CI talks to the
# registry or needs its token. The token is removed from every step
# (scripts/ci/env.sh).
#
# target/ persists between jobs on a runner, so the local registry is
# rebuilt from scratch on every run. A new or bumped Terraphim crate in
# Cargo.lock is picked up with no change here.
set -euo pipefail
source "$(dirname "$0")/env.sh"
cd "$CI_ROOT"

die() {
  echo "registry: $*" >&2
  exit 1
}

# One "name version checksum" line per Cargo.lock package from the registry.
locked="$(awk -v src="source = \"sparse+$CI_TERRAPHIM_INDEX\"" '
  /^\[\[package\]\]$/ { name = ""; version = ""; source = "" }
  /^name = /     { name = $3 }
  /^version = /  { version = $3 }
  $0 == src      { source = 1 }
  /^checksum = / { if (source) print name, version, $3 }
' Cargo.lock | tr -d '"')"
[ -n "$locked" ] || die "Cargo.lock takes nothing from $CI_TERRAPHIM_INDEX"

work="$CI_REGISTRY_DIR.new"
rm -rf "$work"
mkdir -p "$work/index"
while read -r name version checksum; do
  lower="$(printf '%s' "$name" | tr '[:upper:]' '[:lower:]')"
  case "${#lower}" in
    1) prefix=1 ;;
    2) prefix=2 ;;
    3) prefix="3/${lower:0:1}" ;;
    *) prefix="${lower:0:2}/${lower:2:2}" ;;
  esac
  mkdir -p "$work/index/$prefix"
  # Only the locked version's index line, so Cargo can resolve nothing else.
  curl -sSf --retry 3 "$CI_TERRAPHIM_INDEX$prefix/$lower" |
    awk -v want="\"vers\":\"$version\"" 'index($0, want)' >"$work/index/$prefix/$lower"
  [ -s "$work/index/$prefix/$lower" ] || die "$name $version is not in the registry index"
  crate="$work/$name-$version.crate"
  curl -sSf --retry 3 -o "$crate" "${CI_TERRAPHIM_INDEX}api/v1/crates/$name/$version/download"
  actual="$(sha256sum "$crate" | cut -d' ' -f1)"
  [ "$actual" = "$checksum" ] || die "$name $version: SHA-256 $actual, Cargo.lock has $checksum"
  echo "registry: $name $version"
done <<<"$locked"

rm -rf "$CI_REGISTRY_DIR"
mv "$work" "$CI_REGISTRY_DIR"

mkdir -p "$CARGO_HOME"
cat >"$CARGO_HOME/config.toml" <<EOF
# Written by scripts/ci/registry.sh: the Terraphim registry is replaced by
# the token-free local copy of the crates Cargo.lock takes from it.
[source.terraphim-remote]
registry = "sparse+$CI_TERRAPHIM_INDEX"
replace-with = "terraphim-local"

[source.terraphim-local]
local-registry = "$CI_REGISTRY_DIR"
EOF
