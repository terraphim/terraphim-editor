# Authored alternative synonym registration — verified 11 October 2026

Implemented in the existing shared Terraphim alternatives crate, browser document projections and acknowledged host sessions. An alternative creates a document/span-scoped KG concept using the existing `terraphim_automata::parse_markdown_directives_str`, `terraphim_types::{Thesaurus, NormalizedTerm, NormalizedTermValue}` and `terraphim_automata::ConceptIndex`. For example, actual reopened output contains `synonyms:: Decision, Choice` and both normalized keys reference concept ID 1 in `alternative:s1`.

Annotations remain the durable authority. Queries derive registrations from committed source; unacknowledged proposals cannot publish aliases. Separate spans have separate thesauri and must not be merged by their local numeric IDs. Comma-containing and multiline alternatives use the same typed thesaurus directly because the existing Markdown parser cannot represent them losslessly. Exact ordered display text and original/human/AI provenance remain in annotations. Blank normalized terms do not create searchable aliases.

## Requirement evidence

| Requirement | Actual verification |
| --- | --- |
| Standard `synonyms::` syntax and existing KG types | Six shared-crate tests exercise the real directive parser, typed thesaurus and reverse concept index, including human/AI provenance, Unicode, complex text and blank canonical regressions. |
| Actual KG registration | Four integration tests activate a real RoleGraph, match every synonym to the same concept and restore its validated checkpoint. |
| Save/reopen | Actual VS Code native document save is compared with filesystem bytes; reopening reads those bytes into a fresh shared WASM engine and queries the reconstructed typed group. |
| Undo/redo | Actual VS Code `undo` removes membership; `redo` restores exact annotated source and membership. |
| Committed outputs only | Host tests exercise rejection, wrong/stale acknowledgements, external edits, removal and isolation between spans/documents. |
| Browser/host engine | 15 tests run the actual generated WASM, including proposal-before-ACK, exact ACK, fresh-engine reopen and source-history synchronization. |

[Actual native outputs](synonym-registration-20261011/vscode-native-actual-output.json) record all five passing native checks, full resulting source, query results and reopened thesaurus. The test coordinates the real VS Code TextDocument API and shared WASM directly; it does not certify custom-webview interaction or provider UI acceptance.

## Verification and review

A clean source tree was assembled from editor HEAD `2d802a8b89e99a2a24149e251fd4ed99da57b05c` plus only the 11 implementation/test files for this change. [481 tests passed](synonym-registration-20261011/clean-source-tests.log), with zero failed or ignored; [strict all-target Clippy](synonym-registration-20261011/clean-source-clippy.log) and formatting checks passed. The working tree also passed 507 tests, but that larger count includes earlier unrelated uncommitted tests and is not the clean-source claim.

Focused logs: [shared crate](synonym-registration-20261011/shared-crate-tests.log), [real RoleGraph/host](synonym-registration-20261011/actual-rolegraph-host-tests.log), [actual WASM](synonym-registration-20261011/actual-wasm-tests.log). Initial missing-registration and blank-canonical failures are retained alongside passing outputs. Independent review caught the blank canonical defect; a failing regression was reproduced, fixed and retested. Final review reported no further definite blocker in the scoped registration/ACK/history implementation. UBS reported zero critical findings and 1,017 warnings; it is not represented as a warning-free static scan.

RoleGraph concept matching/checkpoint restoration is proved. Reverse document retrieval for a single-concept graph without edges is not claimed: the initial attempt failed because upstream reverse-document indexing depends on edges. Registration does not silently mutate a shared active role.

## Existing VS Code extension package

The existing extension was rebuilt with the tested engine. [Package receipt](synonym-registration-20261011/package-receipt.json) records SHA-256 `d73e282b382a2ab6d906e683742e24433e427bf9ab6270b6c2c5091310929813`. ZIP integrity and all 122 imported asset hashes passed; native-tested JavaScript/WASM bytes match the packaged bytes. The asset fingerprint covers 246 source files including earlier uncommitted editor runtime work, so this package is not presented as a clean reproducible release.

This closes scoped authored-synonym registration. It does not change acceptance of the original 41 visual requirements, produce new demo videos, rebuild the LSP binary or modify the Zed fork.
