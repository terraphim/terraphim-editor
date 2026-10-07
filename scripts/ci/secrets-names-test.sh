#!/bin/bash

# Pins which variable names scripts/ci/env.sh treats as credentials (and so
# removes from every CI step), with names the runners do or could pass in.
set -euo pipefail
source "$(dirname "$0")/env.sh"

secret=(GITEA_TOKEN CARGO_REGISTRIES_TERRAPHIM_TOKEN RUNNER_TOKEN
  RUNNER_STATUS_TOKEN FIRECRACKER_AUTH_TOKEN AGENT_HMAC_SECRET
  AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN SSH_AUTH_SOCK
  DOCKER_AUTH_CONFIG NETRC GH_PAT OPENAI_API_KEY ANTHROPIC_APIKEY
  DB_PASSWORD GOOGLE_APPLICATION_CREDENTIALS npm_config_auth)
plain=(GIT_AUTHOR_NAME GITEA_URL RUNNER_LABELS CARGO_TARGET_DIR PATH HOME
  RUSTC_WRAPPER KEYBOARD PATCH_LEVEL TOKENIZERS_PARALLELISM)

failed=0
for name in "${secret[@]}"; do
  [ -n "$(printf '%s\n' "$name" | ci_secret_names)" ] ||
    { echo "secrets-names-test: $name should count as a credential" >&2; failed=1; }
done
for name in "${plain[@]}"; do
  [ -z "$(printf '%s\n' "$name" | ci_secret_names)" ] ||
    { echo "secrets-names-test: $name should not count as a credential" >&2; failed=1; }
done
[ "$failed" = 0 ] && echo "secrets-names-test: ${#secret[@]} credential and ${#plain[@]} plain names classified"
exit "$failed"
