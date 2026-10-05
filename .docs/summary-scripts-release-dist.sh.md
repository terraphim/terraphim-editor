# Summary: scripts/release-dist.sh

## Purpose
Deliberate release step that regenerates the committed `dist/` copy (#38).

## Key points
- Runs `trunk build --release --dist dist` and fails if any expected release file is missing.
- The CI release workflow builds the same way.
