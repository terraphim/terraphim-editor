#!/bin/bash
# Retained entry point for existing CI: admit only public crates.io lockfiles.
# This never downloads private packages or reads registry credentials.
set -euo pipefail
source "$(dirname "$0")/env.sh"
cd "$CI_ROOT"
python3 scripts/check-public-dependencies.py
python3 scripts/ci/prepare-public-cargo-home.py
