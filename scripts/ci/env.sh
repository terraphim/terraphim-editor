# shellcheck shell=bash
# Job-local environment for the Gitea CI (.gitea/workflows/ci.yml). Sourced
# by the scripts in scripts/ci; every workflow step is a separate shell, so
# nothing here survives from one step to the next. Two switches, set before
# sourcing:
#
#   CI_NETWORK=1    let Cargo use the network (crates.io); only the
#                   `cargo fetch` step sets it
#   CI_TOOLS_PATH=1 put the tools installed by scripts/ci/tools.sh on PATH;
#                   only the steps after that one set it
#
# Build output. The terraphim-native runners on bigbox export a
# CARGO_TARGET_DIR and a CARGO_BUILD_BUILD_DIR shared by every repository a
# runner builds, filled through the host's `kache` compiler cache with
# read-only hard links. Both point into the checkout's own target/ instead.
# The runner reuses its checkout between jobs (`git checkout --force`), so
# target/ persists per runner and later builds are incremental. RUSTC_WRAPPER
# and CARGO_INCREMENTAL are inherited unchanged: only kache writes here.
#
# Secrets. The runner hands its own environment to every step, tokens
# included. Public crate downloads need no credentials and the runner
# reports status itself. So every variable
# whose name marks it as a credential is removed for every step, and Cargo
# runs offline except in the fetch step. This keeps the credentials out of
# what the build is given; it cannot keep them from a pull request that
# edits this file or the workflow, because the runner starts every step
# with them (see .gitea/workflows/ci.yml).
#
# Cargo home. target/ci-cargo-home keeps the registry cache job-local,
# without inheriting source replacements from the runner's ~/.cargo.
# Rustup and its toolchains are unaffected.
#
# Tools. wasm-pack, wasm-bindgen, trunk, chromedriver, terser and
# clean-css-cli are installed by scripts/ci/tools.sh under target/ci-tools,
# never globally, and reinstalled from checksummed archives on every run.
# HOME and XDG_CACHE_HOME are left alone (kache's daemon socket lives under
# ~/.cache).

CI_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export CARGO_TARGET_DIR="$CI_ROOT/target"
export CARGO_BUILD_BUILD_DIR="$CI_ROOT/target"
export CARGO_NET_RETRY=5

export CARGO_HOME="$CI_ROOT/target/ci-cargo-home"

# Names (upper-cased) that mark a credential: one of these words as a whole
# `_`-separated part of the name. scripts/ci/secrets-audit.sh checks that
# none survive, and scripts/ci/secrets-names-test.sh pins what matches.
CI_SECRET_RE='(^|_)(TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIALS?|API_?KEY|PRIVATE_KEY|ACCESS_KEY|ACCESS_KEY_ID|AUTH|BEARER|NETRC|PAT)($|_)'
ci_secret_names() {
  awk -v re="$CI_SECRET_RE" 'toupper($0) ~ re'
}
for ci_name in $(compgen -e | ci_secret_names); do
  unset "$ci_name"
done
unset ci_name
if [ "${CI_NETWORK:-0}" != 1 ]; then
  export CARGO_NET_OFFLINE=true
fi

# The runners export RUST_LOG=debug for themselves; inherited, it makes
# wasm-bindgen-test-runner log every webdriver call, which buries a failing
# test's panic message (scripts/browser-tests.sh keeps only the last lines).
unset RUST_LOG

CI_TOOLS="$CI_ROOT/target/ci-tools"
export WASM_PACK_CACHE="$CI_TOOLS/wasm-pack-cache"
if [ "${CI_TOOLS_PATH:-0}" = 1 ]; then
  export PATH="$CI_TOOLS/bin:$CI_TOOLS/node/node_modules/.bin:$PATH"
fi
