#!/bin/bash

# The first CI step: reports how many credential variables the runner hands
# to a step (names counted, never values) and fails if any survive
# scripts/ci/env.sh, which every other step sources. The count before is
# what any code in the job, the workflow included, could read; see
# .gitea/workflows/ci.yml.
set -euo pipefail
inherited="$(compgen -e)"
source "$(dirname "$0")/env.sh"
before="$(printf '%s\n' "$inherited" | ci_secret_names | wc -l | tr -d ' ')"
left="$(compgen -e | ci_secret_names || true)"
echo "secrets-audit: $before credential variable(s) inherited from the runner"
if [ -n "$left" ]; then
  echo "secrets-audit: still set after scripts/ci/env.sh: $(tr "\n" " " <<<"$left")" >&2
  exit 1
fi
echo "secrets-audit: 0 left after scripts/ci/env.sh"
