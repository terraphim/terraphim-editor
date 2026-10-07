#!/bin/bash

# Runs one CI command in the job-local environment (scripts/ci/env.sh):
#
#   bash scripts/ci/run.sh cargo test --locked --workspace
#   bash scripts/ci/run.sh --network cargo fetch --locked
#   bash scripts/ci/run.sh --tools trunk build --release --locked
#
# --network gives Cargo network access (the fetch step only); --tools puts
# the tools from scripts/ci/tools.sh on PATH.
# The runner's command policy classifies a step by its first word, so steps
# name `bash` and this script rather than the tool itself.
set -euo pipefail
while [ "$#" -gt 0 ]; do
  case "$1" in
    --network) export CI_NETWORK=1 ;;
    --tools) export CI_TOOLS_PATH=1 ;;
    *) break ;;
  esac
  shift
done
source "$(dirname "$0")/env.sh"
cd "$CI_ROOT"
echo "+ $*"
exec "$@"
