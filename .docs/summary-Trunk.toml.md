# Summary: Trunk.toml

## Purpose
Trunk build configuration.

## Key points
- Development output goes to `target/trunk-dist` so `trunk build` / `trunk serve` never touch the tracked release copy in `dist/` (#38).
- Build hook runs `scripts/build-dist.sh`, which writes its bundle into Trunk's staging directory.
- Serve on 127.0.0.1:8080 with explicit JS/CSS MIME types.
