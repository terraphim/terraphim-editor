# Traceability: Write_On demo → terraphim-editor → Zed

Date: 2026-10-07. Traces every requirement in [alternative-control.md](alternative-control.md) back to the source video and forward to evidence, first in terraphim-editor and then in the Zed integration (zed-terraphim plus the `terraphim-lsp` server in terraphim-ai).

## Method and sources

- **Video (source of truth).** Jason Fried's Write_On demo, x.com/jasonfried/status/2105403067793584590, 415.4 s (6:55), downloaded from the public media URL on 2026-10-07.
  - Transcript: a fresh Whisper (`ggml-small.en`) run with timestamps.
  - Frames: 24 frames extracted at the cited timestamps and inspected.
  - Neither the video nor its frames are stored in this repository; timestamps are quoted instead.
- **terraphim-editor.** main at `7f9307b`.
  - Every requirement was traced to code and to the body of the tests that assert it, not just the test names.
  - Full suite run on 2026-10-07: **371 native tests and 136 browser tests (40 `web_*` suites in headless Chrome), 0 failures**. Commands: `cargo test --workspace` and `wasm-pack test --headless --chrome`.
- **Zed.**
  - zed-terraphim main `7a467b7`, plus PR #31 (`task/28-trim-review-proof`, which pins `terraphim_lsp-v1.22.0-rc.2`).
  - terraphim-ai main `cdfb79e8b` (`crates/terraphim_lsp`, `crates/terraphim_lsp_core`).
  - The trim review was proved with `scripts/trim-review-proof.py`: 24/24 against the published rc.2 binary, and 1/18 against rc.1. It was also driven live in Zed 1.22 (zed-terraphim `docs/qualification.md`).
- **Verdict scale.**
  - **Done**: implemented, and a test asserts the behaviour.
  - **Untested**: implemented, but no test asserts it.
  - **Partial**: the missing part is named in Notes.
  - **Missing**: possible in the host, not done.
  - **Not possible**: the host cannot do it; the Notes give the approximation.
  - **Out of scope**: excluded by §1 or a §12 decision.

## Video confirms the requirements

Spot checks of the requirements' citations against the source:

- 0:00–0:11 "Starts out as a plain simple text editor… if you click the word count up here, it turns on" (R-2.1).
- 0:33–0:48 "a little squiggly underline… a couple dots at the end… mouse over it and use my arrow key" (R-2.4, R-3.1, R-3.2).
- 1:43 "the dot in the middle is highlighted… not the original word" (R-3.3).
- 1:57–2:08 "a thumbtack… an eraser. So it automatically detects that" (R-2.6).
- 2:33–2:52 "highlight some text, right click and say ghost it. It goes back to about 10%… revive" (R-5.1, R-5.2).
- 3:03–3:31 "AI alternatives for selection… this one has a bullet point… a bot point… I can just undo those" (R-8.6, R-4.4, R-4.6).
- 3:52–4:26 "the overflow panel… stash it in overflow… drag stuff over" (R-6.x).
- 4:29–4:44 save, open, post to X, LinkedIn blocked (R-7.2; out of scope by §1).
- 4:52–5:50 the Lab; "slightly trim, tighten it some more, even sharper, or cut it in half… it dims out… not rewriting anything… I can walk through the changes, I can make the cuts, or I can say, 'Eh, maybe not.'" (R-8.1 to R-8.5).

The frames confirm the visual claims:
- 0:03, the counter top-left;
- 1:44, the panel with Word / Sentence / Paragraph tabs and "return adds · esc closes";
- 2:38, the menu items and shortcuts (`Ctrl+Shift+A`, `Ctrl+Shift+G`, `Ctrl+/`, `Ctrl+Shift+X`);
- 3:20, bullet vs bot glyphs;
- 4:05, the Overflow panel and its `ctrl+⏎ or drag into the page to use` hint;
- 5:05, the Lab popover with six actions and five trim buttons;
- 5:40, the card "Slight trim 535 → 480 words · −10% … Make the cuts · Walk through · Done".

No requirement was found that the video contradicts; items the video does not show are already marked *inferred* in the requirements.

## Matrix

Editor paths are relative to this repository. "crate" is `crates/terraphim_alternatives`. Zed: S = `terraphim-ai/crates/terraphim_lsp/src/server.rs`, T = `crates/terraphim_lsp/tests`, CT = `crates/terraphim_lsp_core/tests`.

| Req | Video | Editor: implementation → test | Editor | Zed: surface → test | Zed |
|---|---|---|---|---|---|
| R-2.1 Plain mode; counter toggles Write_On | 0:00–0:11 | `chrome.js` WriteOnChrome toggle → `web_chrome::test_counter_click_toggles_write_on_and_every_corner_control` | **Partial**: plain mode keeps the old toolbar and preview (`docs/design/write-on-chrome.md:10`; `web_chrome::test_plain_mode_shows_only_the_counter` asserts the toolbar is visible) | none; settings only | Not possible |
| R-2.2 Span-anchored alternatives, original first, one active | 0:30–0:53 | crate `model.rs` Span, `document.rs` add_span / set_active → `lifecycle::add_alternative_validates_and_deduplicates`, `web_alt_panel::test_selection_add_up_down_edit_and_undo` | Done | "Replace with X" code actions on KG terms → T `code_action_tests::offers_every_other_synonym_on_the_fixture_doc` | Partial: KG synonyms only; stored spans and the active index are never surfaced |
| R-2.3 Word / sentence / paragraph | 0:22, 2:08–2:15 | `SpanKind`, `indicators.js` → `web_indicators::test_word_headline_sentence_and_paragraph_geometry` | Done (paragraph spans tested from fixtures, not created through the UI) | word/phrase KG terms only | Partial |
| R-2.4 Hover + Up/Down cycles in place | 0:40–0:48 | `indicators.js` onCycleKey → `web_cycling::test_hover_and_arrows_cycle_with_wrap_and_one_undo_step_each` | Done | code-action menu picks | Not possible (no key handling) |
| R-2.5 List and active choice persisted | inferred | crate `block.rs` → `web_cycling::test_save_reopen_keeps_the_active_alternative…`, `block_round_trip::parse_of_write_is_identity_for_fixture_documents` | Done (decision 12.2: trailing block) | `terraphim.alternative.add` → T `command_tests::add_alternative_returns_a_versioned_block_rewrite` | Partial: tested but no code action carries the command |
| R-2.6 a/an fix-up | 1:57–2:08 | crate `article.rs` → `article_swap::paperclip_eraser_thumbtack_and_back`, `web_cycling::test_article_fix_up_sequence_with_undo_redo_and_typing` | Done | same WorkspaceEdit as "Replace with" → T `code_action_tests::article_is_fixed_in_the_same_workspace_edit` | Done |
| R-2.7 Emptying removes the indicator | 3:31–3:35 | `remove_alternative` / prune → `lifecycle::removing_all_non_original_alternatives_removes_the_span`, `web_indicators::test_spans_with_only_the_original_get_no_indicator` | Done | none | Missing |
| R-3.1 Thin muted underline | 0:33 | `write-on.css` `.te-ind-word` → `web_indicators::test_word_headline…` (1px, rgba(140,134,230,0.4)) | Done (no test re-checks on hover) | no decoration API | Not possible |
| R-3.2 Dot row, one per alternative | 0:33, 0:55 | `indicators.js` buildSpan → `web_indicators::test_word_headline…` | Done | inlay hint `[i/n]` → T `command_tests::inlay_hints_on_through_initialization_options` | Not possible (approximated) |
| R-3.3 One lit dot | 1:43 | `te-ind-dot--active` → `web_indicators::test_word_headline…` | Done | `i` of `[i/n]` (concept order, not "original") | Not possible |
| R-3.4 Paragraph gutter rule + dot column | 2:08 (frames) | `.te-ind-para` → `web_indicators::test_word_headline…` | Done | none | Not possible |
| R-3.5 Headline bold, larger | 0:30 | `.te-heading`, `.te-heading-marker` (`TeHeadingLayer`, #84) and `.te-ind-headline` → `web_headings::*` (5: plain `#` styling and hidden marker, marker shown on the caret line, typing, plain mode, headline span with alternatives), `web_indicators::test_word_headline…` | Done (#84): every `#` line in Write_On is bold, same face and colour, 1.15em; the marker is hidden but kept in the text and export, and shown dimmed while the caret is on the line | Zed theme | Not possible |
| R-3.6 Hover target; dot click jumps (inferred) | 0:40 | `onDotClick`, te:dot → `web_cycling::test_dot_click_jumps_and_alt_arrows_cycle_from_the_caret` | Done | hover shows the concept → T `lsp_integration_tests::test_hover_returns_description_for_matched_term` | Not possible (no dots; hover does not list alternatives) |
| R-4.1 Left panel, toggle, text dims | 1:11–1:22 | `alternatives-panel.js` → `web_alt_panel::test_chrome_dot_provenance_tabs_and_save` | Done (width and dimming not asserted) | none | Not possible |
| R-4.2 Word / Sentence / Paragraph tabs | 1:44 (frame) | `setTab` → `web_alt_panel::test_chrome_dot_provenance_tabs_and_save` | Done | none | Not possible |
| R-4.3 Editable list, Enter adds | 1:17–1:28 | `commitNewLine` → `web_alt_panel::test_selection_add_up_down_edit_and_undo` | Done | no LSP text input | Not possible (server command exists, unreachable) |
| R-4.4 Provenance glyphs | 3:14–3:24 | `renderLine` (dot / fa-robot) → `web_alt_panel::test_chrome_dot_provenance…` | Done | none | Missing (a hover with provenance is possible) |
| R-4.5 Up/Down in panel, live | 1:28–1:37 | `setActiveIndex` → `web_alt_panel::test_selection_add_up_down_edit_and_undo` | Done | menu Up/Down + Enter | Not possible (not live) |
| R-4.6 Undo removes AI alternatives | 3:27–3:35 | `alternativeOp` one undo step → `web_kg_menu::test_ai_alternatives_for_selection_append_bot_marked_lines_as_one_step` | Done | native undo of a Replace only | Missing |
| R-5.1 Ghost it, ~10%, counted | 2:33–2:45 | `selection-menu.js` TeGhostLayer → `web_ghosts::test_ghost_and_revive_via_menu_and_shortcut` (alpha 0.1±0.02) | Done | `ghosted` Hint, Unnecessary → T `command_tests::ghosts_are_published_as_unnecessary_hints` | Partial: renders stored ghosts only (fade 0.3); nothing creates one |
| R-5.2 Revive | 2:48–2:52 | `revive` → `web_ghosts::test_ghost_and_revive_via_menu_and_shortcut` | Done | none | Missing |
| R-5.3 Independent, elastic, persisted ghosts | decision 2026-10-04 | crate ghost / revive / apply_edit → `ghosts::*` (7 tests), `web_ghosts::test_ghost_follows_typing_elsewhere_and_inside` | Done (a fully deleted ghost is set aside for undo rather than dropped) | read-only render → CT `block_features::a_ghost_moved_by_an_outside_edit_is_found_again` | Partial: never written, not elastic in Zed |
| R-6.1 Overflow panel, XYZ toggle | 3:48–3:57 | `overflow.js` → `web_overflow::test_xyz_toggles_the_panel_with_aria_escape_and_mode_gating` | Done | none | Not possible |
| R-6.2 Free-form text | 3:57 | textarea → `web_overflow::…mode_gating` (mono) | Done | raw JSON field | Not possible |
| R-6.3 Stash selection | 3:59–4:06 | `stashRange` → `web_overflow::test_stash_is_a_move_and_one_undo_step` | Done | none | Missing (possible as a code action) |
| R-6.4 Pull back (Ctrl+Enter, drag) | 4:22–4:26 | `pullBack` → `web_overflow::test_ctrl_enter_and_drop_insert_at_the_caret` | Done | none | Missing / drag Not possible |
| R-6.5 Overflow persisted | inferred | `Annotations.overflow` → `web_overflow::test_panel_text_persists_is_not_exported…` | Done | preserved on block rewrite | Untested |
| R-7.1 Dark centred 70ch layout | frames | `tokens.css`, `write-on.css` → `web_chrome::test_counter_click_toggles_write_on…` | Done in Write_On (70ch not asserted; see R-2.1) | Zed centred layout | Not possible |
| R-7.2 Corner controls | 0:03, 4:29 | `chrome.js` build → `web_chrome::test_counter_click_toggles_write_on_and_every_corner_control` | Done; X/LinkedIn Out of scope (§1) | Zed's own save and open | Not possible |
| R-7.3 Context menu, 4 items + shortcuts | 2:38 | `TeSelectionMenu` + plug-ins → `web_ghosts::test_menu_keyboard_navigation_and_plugins`, `web_overflow::test_stash…` | Done (the AI item only on KG terms) | code-action menu | Not possible |
| R-7.4 No shortcut collisions | inferred | `config.js`, `editor_config.toml`, `selection-menu.js` (Ctrl+/, Ctrl+Shift+A/G/X), `teConfigShortcutMatches` in `editor.js` (exact modifiers, Shift included) → `web_shortcuts::test_configured_and_write_on_shortcuts_are_disjoint`, `web_shortcuts::test_config_shortcut_does_not_fire_on_write_on_shift_chord`, `web_shortcuts::test_config_shortcuts_need_their_exact_modifiers` | Done (#85): configured keys from both files are disjoint from the live menu keys, the shortcut reference and the panel, cycling and pull-back keys, normalised with Shift; a config `ctrl+x` no longer fires on Ctrl+Shift+X | extension binds no keys | N/A |
| R-8.1 Lab popover | 4:52–4:59 | `lab.js` TeLabPopover → `web_lab::test_each_action_marks_exactly_the_engine_spans_and_clear_works` | Done | trim rows in the code-action menu; mark actions not offered | Partial |
| R-8.2 Six mark actions, never rewrite | 5:03–5:15 | `terraphim_lab` → `invariants::labels_and_kinds_cover_all_six_actions`, `invariants::no_action_mutates_the_input`, `web_lab::test_each_action_marks…` | Done | Lab diagnostics + "Apply fix: X" → CT `lab::one_code_per_mark_kind_with_the_engine_reason`, T `command_tests::lab_mark_publishes_one_action_with_fixes` | Partial: only via settings and a save; off-tone untested |
| R-8.3 Five trim levels | 5:19–5:58 | `trim.js`, `trim.rs` → `web_trim::test_each_level_card_and_preview_equal_the_engine`, `trim::levels_are_nested` | Done | "Trim: Original … Cut in half ~50%" → T `trim_review_tests::the_menu_offers_five_levels_and_the_review_actions`, `trim-review-proof.py` | Done (in Zed once PR #31 pins rc.2) |
| R-8.4 Faded cuts + status card | 5:27–5:46, 5:40 frame | `TeLabTrim` + `card_text` → `web_trim::test_each_level_card…` (exact hint and buttons) | Done (local deterministic engine, decisions 12.4/12.5) | `trim-candidate` Unnecessary + showMessageRequest card → T `trim_review_tests::a_preview_returns_first_then_sends_the_card_with_three_actions`; live Zed | Done (pending rc.2 pin; card buttons not keyboard-reachable) |
| R-8.5 Keep, Make the cuts, Walk through, Done | 5:41–5:46 | `trim.js` → `web_trim::test_click_keeps_the_outermost_cut…`, `web_trim_cuts::*`, `web_trim_done::*` | Done | Keep / Make the cuts / Walk through actions → T `trim_review_tests::keep_unfades_exactly_that_cut…`, `walk_through_selects_each_cut_in_order_and_wraps`; live Zed | Done (pending rc.2 pin) |
| R-8.6 AI alternatives appended, bot-marked | 3:03–3:24 | `src/kg.rs` append_alternatives → `tests/kg.rs::ai_alternatives_for_selection_append_after_the_writers_own`, `web_kg_menu::…` | Done (thesaurus as "AI", decision 12.4) | "Replace with X" from the thesaurus | Partial: nothing appended or bot-marked |
| R-8.7 Pluggable provider | inferred | thesaurus wired directly (`src/kg.rs`) → `tests/kg.rs::loading_reports_the_thesaurus…` | **Partial**: no provider abstraction or LLM seam | thesaurus setting → T `thesaurus_tests::*` | Partial |
| R-9.1 Markdown + trailing block | decision 12.2 | crate `block.rs` → `persistence::preview_and_export_match_golden_files`, `block_round_trip::*` | Done | block excluded from analysis; one diagnostic if malformed → CT `alternatives::malformed_block_yields_one_diagnostic…` | Done (no folding) |
| R-9.2 Schema + context re-anchoring | decisions 2026-10-04/05 | crate `model.rs`, `context.rs`, `reanchor.rs` → `context_reanchor::*` (16), `reanchoring::*` | Done | round-trips the editor's writer → CT `block_features::committed_fixture_is_the_editor_writer_output` | Partial: context never refreshed by the server |
| R-9.3 Export: active text, no ghosts, no overflow | decision 12.3 | `Document::export` → `lifecycle::export_drops_ghosted_text_and_omits_overflow_and_block` | Done | none | Missing |

## Totals

| | Done | Partial | Missing | Not possible | Untested | Out of scope / N/A |
|---|---|---|---|---|---|---|
| terraphim-editor (41) | 39 | 2 (R-2.1, R-8.7) | 0 | n/a | 0 | X/LinkedIn per §1 (inside R-7.2) |
| Zed (41) | 5 (R-2.6, R-8.3, R-8.4, R-8.5, R-9.1) | 10 (R-8.2 among them) | 7 | 17 | 1 (R-6.5) | 1 (R-7.4) |

## Gaps and the smallest fixes

**terraphim-editor**
1. **R-8.7:** add a provider seam (trait or JS interface) for alternatives, with the thesaurus as the first implementation. Decision 12.4 already expects it.
2. ~~**R-3.5:** style plain `#` headings in Write_On, and hide the marker.~~ Done in #84.
3. ~~**R-7.4:** add a test asserting the configured and Write_On shortcuts are disjoint, with Shift taken into account.~~ Done in #85.
4. **R-2.1 / R-7.1:** decide whether plain mode should drop the legacy toolbar and preview (a recorded deviation today).

There are also unasserted details: panel widths and placement, the 70ch measure, the cursive tab face, several colours, the underline staying unchanged on hover, a dot hover not enabling cycling, and paragraph spans created through the UI.

**Zed**, ranked by user value; every item is possible with LSP and Zed's existing surfaces:
1. Lab mark actions as code actions ("Lab: Mark hedges and filler" and the rest, plus "Lab: Clear marks"). The server commands `terraphim.lab.mark` and `terraphim.lab.clear` exist; no code action carries them today (S `code_action` builds only fix, replacement and trim actions).
2. "Ghost it" and "Revive" code actions, each rewriting the block in one WorkspaceEdit.
3. A hover that lists the alternatives with provenance, and "Replace with" for stored human alternatives, updating `active`.
4. A two-step bridge for adding a typed alternative, which LSP has no text input for.
5. Stash to overflow and pull back as code actions.
6. Refresh the anchor context on save; a folding range for the block; export clean Markdown.
7. Tests for `lab-off-tone` and for overflow surviving a block rewrite.

The 17 "Not possible" items are limits of the Zed extension API: no panels, palette commands, key bindings, decorations, gutter or text opacity beyond `DiagnosticTag::Unnecessary`. Their approximations are listed in the Notes column above and in zed-terraphim's README.
