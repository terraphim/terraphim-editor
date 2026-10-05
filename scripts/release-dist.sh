#!/bin/bash
# Regenerates the committed release copy in ./dist on purpose.
#
# Development builds (trunk build / trunk serve) write to target/trunk-dist and
# never touch ./dist. Run this script, review `git status dist`, then commit.
set -euo pipefail
cd "$(dirname "$0")/.."

trunk build --release --dist dist

# Fail loudly if the release copy is incomplete.
for pattern in 'index.html' 'terraphim-editor-*_bg.wasm' 'terraphim-editor-*.js' \
               'terraphim-editor.min.js' 'terraphim-editor.min.css' 'example.html'; do
  if ! compgen -G "dist/$pattern" > /dev/null; then
    echo "release-dist: dist/$pattern missing" >&2
    exit 1
  fi
done

echo "release-dist: dist/ regenerated; review with: git status dist"
