# RoleGraph browser bridge

Single-owner `GraphSession::dispatch(&mut self, &str) -> Result<String, BridgeError>`
and wasm-bindgen `BrowserRoleGraph` constructor/dispatch. Uses actual RoleGraph with
`kg-integration`, no medical feature. Initial generation/revision are `"0"`.

Uses exact Terraphim-registry pins: RoleGraph **2.0.1**, automata **2.1.0**, types
**1.22.3**. Both new releases have independently verified registry/archive/source
identity. The bridge is a root-workspace member, remains `publish = false`, and is
built as a separate Worker WASM module, not a main-editor Rust dependency. The
root lock is authoritative; no sibling graph patch or standalone lock is used.

Current build and stage evidence: `../../docs/verification/rolegraph-production-integration.md`
and `../../docs/verification/rolegraph-production-runtime.md`. The fresh desktop
functional matrix passes; final full-editor browser regression is pending. These
results do not authorise editor commits/distribution or full-UI cold offline release.

## Current workspace build

```sh
cargo test --locked -p terraphim_rolegraph_browser
cargo clippy --locked -p terraphim_rolegraph_browser --all-targets -- -D warnings
WASM_BINDGEN=/absolute/path/to/matching/wasm-bindgen \
  bash scripts/build-rolegraph.sh target/rolegraph-dist
```

Current wasm-bindgen lock/CLI pairing is **0.2.129**. A normal
`trunk build --release --dist target/rolegraph-production-check` includes the graph
assets automatically or fails; no `TE_ROLEGRAPH_BUILD` flag is required.
For Trunk, put the matching CLI directory on `PATH` as well as selecting that
executable with `WASM_BINDGEN` for the graph hook; see the complete build example
in `../../docs/design/rolegraph-embedding.md`.
`TE_ROLEGRAPH_SOURCE` is rejected. Do not regenerate committed `dist/` or `package/`
without separate authority. See `../../docs/design/rolegraph-embedding.md` for host
API, deployment and real desktop fixture commands.

## Wire contract

See `../../docs/contracts/schemas/rolegraph-browser.schema.json`. All u64 identities,
ranks, generations and revisions are canonical decimal strings, never JS numbers.
Thesaurus text and `graphJson` stay opaque on JS boundaries. Rust bounds them and
validates pairing before constructing a graph. Malformed/oversized envelopes throw a
transport error; valid envelopes return `ok:false` with stable code/message, no raw
user text or panic detail. Generation and optional expected revision are checked on
every command (including reads and preparations). The main engine owns the next
generation: synchronization accepts any canonical u64, including equal generations
and an initial `clear_role` at `"0"`. Host-created role loads normally choose +1.

Preparations return `{token,thesaurusText}` without replacing active state. Only the
latest successful preparation is retained. Mutation makes its base revision stale.
Activation consumes the token, publishes the caller's next generation and resets
revision to `"0"`; clear does the same without a graph. Candidate mutations clone,
validate complete corpus and graph, then publish once and increment revision once.
An exact counting serializer checks the **escaped complete snapshot wrapper**
before preparation/publication/activation (including the target generation and
revision), so every acknowledged active revision stays within the 16 MiB export
and import budget. It stops counting at the limit without building an oversized
encoded snapshot. Source and graph size limits are not independent allowances.
Errors leave active state/revision unchanged. Removal of a missing document is an
acknowledged no-op mutation with `removed:false` and increments revision.

Results: `match` string ID array; `connected` boolean; `query` and
`learning_documents` indexed-document DTO arrays; `documents_for_term` sorted ID
array; `document` indexed DTO plus `sourceAvailable` and optional original `source`;
`nodes`/`edges` deterministic numeric-ID-ordered pages; `stats` upstream statistics;
`export_snapshot` `{snapshotText}`. Mutation results contain `indexed`, `removed`,
`updated`, or `adjusted`. Indexed DTO fields: `id`, decimal `rank`, decimal `nodes`,
`tags`, `matched_edges`; edges contain decimal `id`, `rank` and `doc_hash` values.
Ranked query order is preserved, only unordered edge lists are sorted.

## Explicit supported boundaries

- All 19 approved operations dispatch to real upstream methods.
- `query.skipDirect:true` is refused: upstream fallback does not expose that option.
  Fallback includes pinned concepts. Operators require `operator:and|or`; `terms`
  defaults to an empty list (which yields no results).
- Learning `matched_edges` are preserved as historical result metadata, as in the
  native `IndexedDocument` API. Their checked-pair endpoints must reference known
  thesaurus concepts; provenance IDs must be nonempty/bounded, counts positive,
  totals bounded. Their ranks remain exact u64 strings. Provenance IDs describe
  historical sources, not foreign keys into the current corpus. Removing/replacing
  such sources does not invent graph contributions or rewrite learning metadata.
  Actual graph edges still require current document references and exact accounting.
- Learning adjustment rejects overflow/underflow instead of allowing saturation.
- Query/pattern/document bounds are UTF-8 bytes (JSON Schema lengths are only an
  additional character-level ceiling). `match`, `documents_for_term` and
  `learning_documents` now accept `{text,offset?,limit?}` (default 0/100), as an
  additive wire extension. Matches retain occurrence order; document collections
  use deterministic ID order. Nodes/edges/query also paginate. All offsets are
  u32 integers (0–4294967295) on native and WASM. Connectivity/index/query internals
  continue to consume the complete matcher output, never the paged DTO.
- Snapshot `triggerThreshold` is an additive wrapper field: upstream serialization
  omits it; absent field defaults to 0.3. Snapshots preserve source-unavailable
  documents, validate node/edge counts and references, exact endpoint contributions,
  automata metadata, and compare available source contributions without rewriting
  imported ranks. Upstream edge rank is exactly 1, not the contribution total.
- Import wrapper <=16 MiB; corpus <=16 MiB/1000 documents/1 MiB per document;
  complete encoded snapshot <=16 MiB; patterns/nodes <=10,000; edges <=100,000;
  query <=4 KiB; page <=100; tags <=1000/document and <=10,000/graph;
  node memberships plus graph nodes <=100,000, learning metadata edges <=100,000,
  container members <=1,000,000 (conservatively counted on publication).
  Borrowed raw payloads receive streaming collection preflight before typed
  deserialization or active-state cloning. Snapshot/thesaurus internals receive the
  same preflight before constructing maps/vectors. Distinct edge
  growth is preflighted before document insertion. The synchronous engine cannot
  interrupt a command: worker watchdog/queue/cancellation belong to the host.

Native tests use the real graph. Browser runtime parity and worker/storage gates
remain separate from compilation and must not be inferred from native passing tests.

## Historical Step 3 verification (2026-10-08, pre-publication)

The following records initial development evidence, not current build instructions
or current release gates. Current native workspace420, JS61 and registry-pinned
release delivery evidence is in the reports above; obsolete lint/standalone-lock
limitations below have since been resolved and reviewed.

23 native tests pass, including every operation, shared exact-rank fixture,
generation/revision/token guards, corrupt references/ranks, Unicode text, source
unavailability, unsafe concept IDs and >100,000-edge atomic refusal. Shared
33-command fixture passes in real headless Chrome 155.0.8059.40 and native Rust,
including exact edge `10000000100000002`, learning rank `9007199254740995`, both
Boolean operators, threshold roundtrip, corruption and resource/stale refusals.
The desktop browser tool was disconnected; an isolated temporary Chrome profile
was used, never the user's signed-in profile. Firefox/Safari are not qualified.

`cargo fmt --check`, package `cargo clippy --all-targets -- -D warnings`, and
`cargo build --release --target wasm32-unknown-unknown` pass. Upstream types still
emit two deprecated `std::f64::EPSILON` warnings; this is not a clean upstream lint
gate. Python jsonschema 4.25.1 validates the schema, 27 successful scenario requests
and 5 deliberately invalid payloads, all 33 emitted native response envelopes and
the actual exported wrapper; numeric DTO rank and out-of-u32-offset negatives are
also checked. No global tool installation was performed.

Review regressions were test-first: five initial tests failed before fixes. P1
tests now reject nine 1 MiB backslash bodies atomically, restore a valid >14 MiB
near-boundary wrapper, and reject oversized graph-plus-corpus trigger/learning
mutations without losing the previous snapshot. The original 33-command shared
fixture is unchanged. An additional 123-command native/Chrome fixture covers 101
document/learning collections across pages, u32 offset boundaries, nonempty
learning edge metadata import/export, and the escaped-corpus refusal.

In fresh WASM, rejecting **five million empty tags in raw JSON** leaves revision
unchanged and grows linear memory from 1,245,184 to 16,252,928 bytes (delta
15,007,744), rather than allocating a 120 MB typed tag vector. This is bounded
evidence for that adversarial case, not peak-memory qualification for all supported
corpora, nor JS heap/process memory measurement.

Those initial build/test/lint runs used **command-line-only** Cargo overrides:

```sh
--config 'patch.crates-io.terraphim_rolegraph.path="/Users/alex/projects/terraphim/terraphim-core-rel/crates/terraphim_rolegraph"'
--config 'patch.crates-io.terraphim_automata.path="/Users/alex/projects/terraphim/terraphim-core-rel/crates/terraphim_automata"'
--config 'patch.crates-io.terraphim_types.path="/Users/alex/projects/terraphim/terraphim-core-rel/crates/terraphim_types"'
--config 'patch.crates-io.terraphim_rolegraph_core.path="/Users/alex/projects/terraphim/terraphim-core-rel/crates/terraphim_rolegraph_core"'
```

They were appended to the initial standalone build commands. **Do not use them
for current builds.** The development lock was archived and removed during approved
root integration; current published registry dependencies require no override.

Temporary evidence directory:
`/private/var/folders/rb/l4_6j2nx0lnfhysk974ldpg40000gn/T/opencode/`:
`rolegraph-bridge-native.log`, `rolegraph-bridge-clippy-final.log`,
`rolegraph-bridge-wasm-build.log`, `rolegraph-bridge-chrome.html` and
`rolegraph-bridge-chrome.log`. Matching bindgen 0.2.129 generated real web glue in
`rolegraph-bridge-web/`. Serve that directory, copied `tests/web_rolegraph.html`
and `tests/fixtures/scenario.json` to reproduce the browser fixture. For review
coverage also copy `tests/web_review.html` and `tests/fixtures/review_scenario.json`.
Review evidence uses `rolegraph-bridge-review-{red.log,native.log,wasm.log,clippy.log,
chrome.html,chrome.log}` in the same temporary directory. The owned port-8893
server is stopped after verification; headless Chrome exits after each run.
Final rebuilt-WASM runs are captured in
`rolegraph-bridge-final-web_rolegraph.{html,log}` and
`rolegraph-bridge-final-web_review.{html,log}`. No owned Chrome-profile process
or port-8893 test server remains running.
Worker/storage/editor qualification is not part of these bridge results.
