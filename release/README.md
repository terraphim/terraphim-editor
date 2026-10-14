# Publishing the editor crates

`publish-crates.yml` runs only by dispatch on `main`. Select one of the two
crate names and an authentication mode. The workflow derives the version and
source tag from `release/crates.json`; it accepts no arbitrary source ref.

The initial plan deliberately has `source: null`. It cannot publish until a
reviewed main-branch change supplies the exact source commit, package Git tree
and standalone `Cargo.lock` SHA-256. The package tag must resolve to that exact
commit. Moving a tag or changing a source binding causes qualification to fail.

## Source and dependency order

The editor root depends on `terraphim_lsp_core`, which depends on the editor's
alternatives and Lab crates. Publishing the prerequisites must therefore happen
before merging the root's public dependency migration:

1. Merge the publisher and contract tests independently, preserving main's current
   source, manifest and lockfile.
2. Publish and verify the five core prerequisite versions in the release plan.
3. Prepare the public alternatives/Lab source on its reviewed source branch. Use
   a separate standalone Cargo workspace to generate the alternatives lock from
   crates.io, without local patches or private registries. Commit that lock and
   review the source tree. Add its exact commit/tree/lock binding to the main plan.
4. Create `terraphim_alternatives-v0.1.1` at the approved source commit, then dispatch
   its first publication from main using `bootstrap`.
5. After actual crates.io readback, generate and commit Lab's standalone public
   lock. The alternatives tree must remain byte-identical to its earlier approved
   tree. Review and bind Lab's source on main; create `terraphim_lab-v0.1.1` and
   dispatch its first publication.
6. Publish and verify the downstream LSP core. Finish the editor root migration,
   generate its actual public lock and qualify the full editor before merging it.

A sparse Git checkout retains the approved commit and Cargo's real
`.cargo_vcs_info.json`, while excluding the root manifest and Cargo configuration.
Both package directories are staged so Lab can use its reviewed path dependency;
Cargo normalizes that dependency to crates.io in the published archive. The
publisher checks the archive has no path, Git or private registry dependencies.

## Qualification and authentication

Every job starts with a fresh external `CARGO_HOME` and target directory. No
previous registry credentials, source-replacement configuration or compiled
artifacts are inherited. The package is formatted, linted, tested, checked for
Wasm, packaged with Cargo's verification enabled, then extracted outside the
workspace and tested again. A separate job reproduces the same archive checksum
before obtaining publishing credentials.

- `trusted`: environment `crates-io`; pinned `rust-lang/crates-io-auth-action`
  exchanges GitHub OIDC for a temporary token. Configure the trusted publisher
  for `terraphim/terraphim-editor`, `publish-crates.yml`, environment `crates-io`.
  The workflow reference is main. Existing crate ownership is checked first.
- `bootstrap`: environment `crates-io-bootstrap`; secret `CARGO_REGISTRY_TOKEN`.
  This explicit mode is accepted only when the entire crate name returns 404.
  It is never an authentication fallback for an existing crate. Remove the
  temporary environment binding after both new names have been published, then
  configure and use trusted publishing for subsequent versions.

Restrict both environments to main. Neither source-branch nor tag workflow code
receives credentials. Qualification has no publishing credentials, and all jobs
execute the same main publisher commit.

Success requires the public version metadata checksum and an anonymous download
from `static.crates.io` to match the qualified package exactly. A successful upload
alone is insufficient. An existing version stops a retry for manual readback;
versions and tags must remain immutable.
