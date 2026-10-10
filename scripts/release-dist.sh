#!/bin/bash
# Authorised release only: build in isolation, validate, then publish curated bytes.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -n "${TE_ROLEGRAPH_SOURCE:-}" ]; then
  echo 'release-dist: TE_ROLEGRAPH_SOURCE is forbidden for releases' >&2
  exit 1
fi
unset TRUNK_STAGING_DIR TRUNK_DIST_DIR
SOURCE_HEAD="$(git rev-parse HEAD)"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --allow-replace|--prior-owned)
      [ "$#" -ge 2 ] || { echo "release-dist: $1 requires a manifest" >&2; exit 2; }
      ARGS+=("$1" "$2"); shift 2 ;;
    --dry-run) ARGS+=("$1"); shift ;;
    *) echo "release-dist: unsupported argument $1" >&2; exit 2 ;;
  esac
done
# Reject a diverted target before mktemp or Trunk can write through it.
python3 -c 'from pathlib import Path; p=Path.cwd()/"target"; assert not p.is_symlink(), "target symlink forbidden"'
mkdir -p target
STAGE="$(mktemp -d "$PWD/target/release-staging-XXXXXXXX")"
MANIFEST="target/release-manifests/${STAGE##*/}.json"
echo "release-dist: isolated stage $STAGE; metadata $MANIFEST"
trunk build --release --dist "$STAGE"
python3 scripts/publish-editor-dist.py --stage "$STAGE" --manifest "$MANIFEST" --source-head "$SOURCE_HEAD" ${ARGS[@]+"${ARGS[@]}"}
