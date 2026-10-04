# Is the Zed plugin a better fit for "Alternative Control"?

Companion to `alternative-control.md` (the Write_On requirements). Written 2026-10-02.

## Answer

**No.** The Zed plan (`terraphim/zed-terraphim#1`) is a thin `wasm32-wasip2` adapter over LSP, MCP and ACP. Zed's extension API has no way to draw span decorations, add panels, bind hover-plus-arrow keys or set per-span opacity, so most of the spec (§3–§7) cannot be built there. `terraphim-editor`, which owns its DOM, is the only existing target where the spec can be built as written.

Zed can still get a reduced version through `terraphim_lsp`: AI alternatives as code actions, Lab "mark" results as diagnostics, and an approximate trim preview. That only works if both targets share the `terraphim_lsp` core and the annotation format (see "Shared model" below).

## The plan that was compared

- **`terraphim/zed-terraphim#1`**, "[EPIC] Zed integration via thin native adapter and shared Terraphim releases" (opened 2026-08-14, open, no comments). The repo contains only a README.
- It depends on **`terraphim/terraphim-ai#3224`** (shared platform releases for Desktop, Zed and VS Code). #3224 covers release and BOM work. It has no editor features.
- The parts of the boundary that decide the question:
  - "Use a thin Rust extension compiled to `wasm32-wasip2` for Zed host integration."
  - "Use Zed-native LSP, MCP, and ACP lanes and released `terraphim_app_server`/TACP capabilities."
  - Non-goals: "Recreating Terraphim Desktop inside Zed", "Shipping Svelte, Lit, portal-framework, or Webview application code", "Hosting a separate KG/search engine inside the extension WASM".

Because of the last non-goal, even the thesaurus-backed alternatives provider (R-8.7) cannot run inside the extension. It would have to live in `terraphim_lsp`, which the extension starts as a language server.

## What a Zed extension can do (checked against source)

`zed_extension_api` 0.7.0 (latest on crates.io), `wit/since_v0.6.0/extension.wit`, exports:

| Hook | wit line |
|---|---|
| `language-server-command`, `-initialization-options`, `-workspace-configuration` (+ `additional-*`) | 92–106 |
| `labels-for-completions`, `labels-for-symbols` | 134–135 |
| `complete-slash-command-argument`, `run-slash-command` | 139–142 |
| `context-server-command`, `context-server-configuration` | 145–148 |
| `suggest-docs-packages`, `index-docs` | 155–158 |
| DAP: `get-dap-binary`, `dap-request-kind`, `dap-config-to-scenario`, `dap-locator-*` | 161–166 |

There are no hooks for editor decorations, gutter rendering, panels or views, keymaps or input handling, context-menu items, or text opacity. Inside Zed, the extension can only add UI through an LSP server, using whatever LSP features Zed itself renders.

Zed settings checked in the `terraphim/zed` mirror (`assets/settings/default.json`, main):
- `"unnecessary_code_fade": 0.3` ("How much to fade out unused code") exists. This is how Zed renders LSP `DiagnosticTag::Unnecessary`. Confirm that wiring in the Z0 spike before relying on it.
- `"inlay_hints": { "enabled": false }`: inlay hints are **off by default**, so any feature that depends on them needs the user to turn them on.

## Requirement-by-requirement fit

Key: **Full** = implementable as specified; **Partial** = an approximation through LSP; **None** = not possible in a Zed extension.

| Req | Feature | Zed (via `terraphim_lsp`) | How / why not |
|---|---|---|---|
| R-2.1 | Plain mode / Write_On toggle | None | No chrome. The closest equivalent is a workspace setting that turns the LSP features on or off. |
| R-2.2, R-2.3 | Span-anchored alternatives, 3 granularities | Partial | The LSP server can hold the span model and annotation block. Zed has no idea what a span is. |
| R-2.4 | Hover + ↑/↓ cycling in place | None | Extensions cannot handle keys. Closest equivalent: code-action menu "Replace with *X*", one entry per alternative. |
| R-2.5 | Persistence | Full | The server reads and writes the trailing annotation block (R-9.1, decided: embedded). |
| R-2.6 | a/an fix-up on swap | Full | The code action's `WorkspaceEdit` can include the article edit. |
| R-2.7 | Emptying removes indicator | Partial | Follows from whatever indicator is used (diagnostic or inlay hint). |
| R-3.1–R-3.6 | Underline, dot row, lit dot, paragraph gutter rule | None | No decoration API. Closest: a Hint-severity diagnostic underline (Zed's styling, not 1px lavender), or an inlay hint such as `[2/5]` after the span (off by default). No dots, no gutter rule. |
| R-4.1–R-4.6 | Alternatives side panel, tabs, provenance markers, live add | None / Partial | No panels. Hover markdown can list the alternatives with human/bot markers (R-4.4), but it is read-only. Adding an alternative would need a slash command or `executeCommand`. |
| R-5.1–R-5.3 | Ghost it / Revive | Partial | Only as server-driven `Unnecessary` diagnostics, faded at a single global opacity (0.3, not ≈0.1). There is no context-menu item, so it would be a code action "Ghost this span". Ghosted spans can be stored in the annotation block. |
| R-6.1–R-6.5 | Overflow panel, stash, pull back | None | No panel. Closest: a code action that moves the selection into a `*.overflow.md` file the user opens in another pane. |
| R-7.1–R-7.4 | Chrome, corner controls, context menu, shortcuts | None | Zed's own chrome. Code actions appear in Zed's code-action menu, not a custom context menu. Shortcuts can only be bound by the user to built-in actions. |
| R-8.1, R-8.3 | Lab popover, trim buttons | Partial | Slash commands (`/lab-trim 10`) in the assistant panel, or one code action per trim level. No popover. |
| R-8.2 | Mark actions (weak, long, convoluted, tone, hedges) | **Full in substance** | This is what diagnostics are for: Hint/Information severity with a code per mark type. Hedges and filler can come from a KG list, the same as in the editor. |
| R-8.4, R-8.5 | Trim preview, keep-by-click, Make the cuts | Partial | Candidate cuts can be shown as `Unnecessary` diagnostics. "Keep" and "Make the cuts" become code actions. No status card. |
| R-8.6 | AI alternatives for selection | **Full in substance** | Code action on the selection appends bot-tagged alternatives to the annotation block. Completion items can offer the same list while typing. |
| R-8.7 | Provider abstraction (thesaurus first, LLM optional) | Full | Must live in `terraphim_lsp`, not the extension (epic non-goal). |
| R-9.1–R-9.3 | Markdown + annotations, schema, export | Full | Server-side. The embedded block is visible as raw text in Zed (see Decisions). |
| R-10 | Visual tokens | None | Zed theme only. |

Summary: of the 12 spec sections, **§8.2 and §8.6 carry over almost intact**, persistence and the data model (§2.5, §9) carry over, with the embedded block visible as raw text, ghosting and trim carry over only approximately, and **§3, §4, §6, §7 and §10, the parts that make Write_On what it is, do not carry over.**

## What `terraphim_lsp` would need

`crates/terraphim_lsp` currently supports hover, completion, and push/pull diagnostics (`server.rs`). For the Partial/Full rows above, Z2 would need to add:

- `textDocument/codeAction` with `WorkspaceEdit` (alternative swap, a/an fix-up, ghost/revive, stash, keep/cut).
- `workspace/executeCommand` (add alternative, run Lab mark, run trim level).
- `textDocument/inlayHint` (optional `[i/n]` indicator).
- `DiagnosticTag::Unnecessary` on diagnostics for ghosted spans and trim candidates.
- Annotation-block parse/write plus re-anchoring on `didChange`, excluding the block itself.

None of this is in the current Z0–Z4 phases of `zed-terraphim#1`. Z2 only says "Integrate the released LSP binary/interface for language and knowledge features."

## The other routes

- **`terraphim/terraphim-gpui-editor`** (standalone GPUI editor): could build the whole spec, but means owning a native editor. It has its own plan in `.docs/implementation-plan.md` that is unrelated to Write_On.
- **`terraphim/zed`** (Gitea mirror of Zed, not a Gitea fork; `fork: false`). Last updated 2026-03-06, carrying `auto_preview_markdown` (PR #48733) work. A fork could build everything in GPUI, but that means maintaining a Zed fork, which is the most expensive option and contradicts the "thin adapter" boundary of `zed-terraphim#1`. Not recommended.
- **`terraphim-editor`** (Rust→WASM, own DOM): the full spec can be built as written (see `alternative-control.md` §11). **Recommended primary target.**

## Shared model across the WASM editor and LSP clients

The span model should **not** go into `EngineEvent`. That contract (`crates/terraphim_engine_events`) covers agent evolution and approvals (`EvolutionProposed/Approved/Rejected/Applied`, `AllowOnce/AllowAlways/Reject/RejectAlways`), not document state.

**Plan (updated 2026-10-04): KG synonyms are the alternatives.** An earlier plan proposed a shared crate, `terraphim_alternatives`, owning the whole span model for both clients (with a later extraction when `terraphim_lsp` adopted it, `terraphim/terraphim-editor#16`). That plan is superseded: #16 is closed. The shared model is now the knowledge graph plus `terraphim_lsp`:

- **Concept id to synonyms index** ([terraphim/terraphim-core#75](https://git.terraphim.cloud/terraphim/terraphim-core/issues/75)). Match positions already exist through `find_matches(.., true)`, as validated on 2026-10-02. Only the reverse index (concept id to synonyms) is new.
- **`terraphim_lsp` split into a WASM-buildable core plus a server** ([terraphim/terraphim-ai#3409](https://git.terraphim.cloud/terraphim/terraphim-ai/issues/3409)). The core is the main engine for every client: spans, alternatives from the index, re-anchoring and diagnostics. The server wraps it for Zed and other LSP clients.
- **Browser editor** ([terraphim/terraphim-editor#13](https://git.terraphim.cloud/terraphim/terraphim-editor/issues/13)) consumes the `terraphim_lsp` core compiled to WASM. No alternatives provider is written in this repo.
- **`terraphim_alternatives`** ([terraphim/terraphim-editor#2](https://git.terraphim.cloud/terraphim/terraphim-editor/issues/2)) keeps its name but is scoped to non-KG state only: human-written alternatives, ghost flags, overflow and the annotation block (R-9.2 schema, serialised as the trailing fenced block). It is not the shared cross-client model, and it lives in this repo as `crates/terraphim_alternatives`.

**Sequencing (cross-repo):**

1. [terraphim-core#75](https://git.terraphim.cloud/terraphim/terraphim-core/issues/75): concept id to synonyms index.
2. [terraphim-ai#3409](https://git.terraphim.cloud/terraphim/terraphim-ai/issues/3409): `terraphim_lsp` core/server split, using the index.
3. [terraphim-editor#13](https://git.terraphim.cloud/terraphim/terraphim-editor/issues/13): browser editor consumes the core.

[terraphim-editor#2](https://git.terraphim.cloud/terraphim/terraphim-editor/issues/2) is independent of this order, because it holds only state the KG cannot derive. Zed's Z2 remains blocked behind #3224, and terraphim-ai main CI is red (#3325).

Fields that must stay portable across all clients. "Source" marks where each field comes from: **KG** means derived from the knowledge graph on demand and never persisted; **Block** means persisted in the annotation block (#2).

| Field | Source | Purpose |
|---|---|---|
| `version` | Block | Schema version, for N/N-1 negotiation |
| `spans[].id`, `kind`, `anchor{text,start,end}` | KG (matches and positions), Block (spans with human or ghost state) | Identity and re-anchoring |
| `spans[].alts[]{text, source: original\|kg\|human\|ai, model?}` | KG (`kg` synonyms), Block (`human`, `ai`, and `original`) | List and provenance (R-4.4) |
| `spans[].active` | Block | Active index (R-2.2, R-3.3) |
| `spans[].ghost` | Block | Ghost attribute (R-5.3) |
| `overflow` | Block | Stash (R-6) |

The `kg` value for `alts[].source` is new under this plan: KG synonyms are recomputed from the index, so they are not written to the block.

TACP/`EngineEvent` only needs an event if alternatives can come from an *agent* rather than the editor (e.g. `AlternativesProposed { doc, span_id, alts, source: ai, model }`). That is optional and can wait until an agent actually produces them.

## Effect of the decisions (`alternative-control.md` §12, answered 2026-10-02)

- **Embedded trailing block (not a sidecar).** `terraphim_alternatives` (#2) owns the schema for non-KG state, and parses and writes the trailing fenced block, splitting the body from the annotations. In Zed and other LSP clients the block will be visible as raw text, and edits to it are user edits. `terraphim_lsp` must therefore (a) exclude the block from diagnostics, marks and re-anchoring, and (b) treat a hand-edited or malformed block as recoverable: keep the body, report one diagnostic, and do not drop annotations silently. An LSP `foldingRange` for the block would hide it in clients that honour folding; whether Zed collapses LSP folding ranges by default needs checking in the Z0 spike.
- **Thesaurus first.** This fits both targets, because `terraphim_lsp` already loads KG/thesaurus data for hover, completion and diagnostics.
- **The Lab in v1 includes marks and trim.** The marks carry over to Zed as diagnostics. Trim carries over only approximately, through `Unnecessary` fading.

## Evidence

- Gitea: `terraphim/zed-terraphim#1`, `terraphim/terraphim-ai#3224`, `#3338`, `#3328`, repo metadata for `terraphim/zed` and `terraphim/zed-terraphim` (read 2026-10-02 via API).
- Tracking: epic `terraphim/terraphim-editor#1` (children #2-#16; #16 closed as superseded); `terraphim/terraphim-ai#3409` (LSP); Z0 evidence comment on `terraphim/zed-terraphim#1`.
- `zed_extension_api` 0.7.0 crate source, `wit/since_v0.6.0/extension.wit`.
- `terraphim/zed` mirror, `assets/settings/default.json` at `main`.
- `terraphim-ai/crates/terraphim_lsp/src/server.rs`, `crates/terraphim_engine_events/src/`.
