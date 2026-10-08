# Traceability citation check at `84cf119`

Produced by `trace_check.py` over the 41 editor rows of `docs/requirements/traceability.md`, against the native and both browser logs of the final run.

One citation is reported as not found: `document::tests::pull_back_*` (R-6.4). The checker does not expand a wildcard after a `tests::` module. Checked by hand: `src/document.rs` has 11 tests named `pull_back_*`, and all 11 ran and passed in the final native run.

| Row | Cited test | File | Found | Passed | Note |
|---|---|---|---|---|---|
| R-2.1 | `web_chrome::test_plain_mode_shows_only_the_counter` | tests/web_chrome.rs | y | y |  |
| R-2.1 | `web_chrome::test_counter_click_toggles_write_on_and_every_corner_control` | tests/web_chrome.rs | y | y |  |
| R-2.1 | `web_chrome::test_plain_mode_keeps_formatting_shortcuts_and_the_palette` | tests/web_chrome.rs | y | y |  |
| R-2.1 | `web_chrome::test_tools_toggle_reveals_toolbar_preview_and_export` | tests/web_chrome.rs | y | y |  |
| R-2.1 | `web_embed::test_embed_plain_mode_hides_the_tools_until_asked` | tests/web_embed.rs | y | y |  |
| R-2.1 | `web_chrome::test_plain_mode_has_the_video_palette_and_column` | tests/web_chrome.rs | y | y |  |
| R-2.2 | `lifecycle::add_alternative_validates_and_deduplicates` | crates/terraphim_alternatives/tests/lifecycle.rs | y | y |  |
| R-2.2 | `web_alt_panel::test_selection_add_up_down_edit_and_undo` | tests/web_alt_panel.rs | y | y |  |
| R-2.3 | `web_indicators::test_word_headline_sentence_and_paragraph_geometry` | tests/web_indicators.rs | y | y |  |
| R-2.4 | `web_cycling::test_hover_and_arrows_cycle_with_wrap_and_one_undo_step_each` | tests/web_cycling.rs | y | y |  |
| R-2.5 | `web_cycling::test_save_reopen_keeps_the_active_alternative…` | tests/web_cycling.rs | y | y |  |
| R-2.5 | `block_round_trip::parse_of_write_is_identity_for_fixture_documents` | crates/terraphim_alternatives/tests/block_round_trip.rs | y | y |  |
| R-2.6 | `article_swap::paperclip_eraser_thumbtack_and_back` | crates/terraphim_alternatives/tests/article_swap.rs | y | y |  |
| R-2.6 | `web_cycling::test_article_fix_up_sequence_with_undo_redo_and_typing` | tests/web_cycling.rs | y | y |  |
| R-2.7 | `lifecycle::removing_all_non_original_alternatives_removes_the_span` | crates/terraphim_alternatives/tests/lifecycle.rs | y | y |  |
| R-2.7 | `web_indicators::test_spans_with_only_the_original_get_no_indicator` | tests/web_indicators.rs | y | y |  |
| R-3.1 | `web_indicators::test_word_headline…` | tests/web_indicators.rs | y | y |  |
| R-3.2 | `web_indicators::test_word_headline…` | tests/web_indicators.rs | y | y |  |
| R-3.3 | `web_indicators::test_word_headline…` | tests/web_indicators.rs | y | y |  |
| R-3.4 | `web_indicators::test_word_headline…` | tests/web_indicators.rs | y | y |  |
| R-3.4 | `web_indicators_registry::test_paragraph_rule_follows_the_column_when_a_panel_moves_it` | tests/web_indicators_registry.rs | y | y |  |
| R-3.5 | `web_headings::*` | tests/web_headings.rs | y | y | 5 tests in file |
| R-3.5 | `web_indicators::test_word_headline…` | tests/web_indicators.rs | y | y |  |
| R-3.6 | `web_cycling::test_dot_click_jumps_and_alt_arrows_cycle_from_the_caret` | tests/web_cycling.rs | y | y |  |
| R-4.1 | `web_alt_panel_tabs::test_chrome_dot_provenance_tabs_and_save` | tests/web_alt_panel_tabs.rs | y | y |  |
| R-4.1 | `web_alt_panel_width::test_panel_width_matches_the_video` | tests/web_alt_panel_width.rs | y | y |  |
| R-4.2 | `web_alt_panel_tabs::test_chrome_dot_provenance_tabs_and_save` | tests/web_alt_panel_tabs.rs | y | y |  |
| R-4.2 | `web_alt_panel_tabs::test_tabs_one_line_quiet_count_and_video_layout` | tests/web_alt_panel_tabs.rs | y | y |  |
| R-4.3 | `web_alt_panel_live::test_new_alternative_is_live_while_typed` | tests/web_alt_panel_live.rs | y | y |  |
| R-4.3 | `web_alt_panel_live_undo::test_live_alternative_undo_leave_and_composition` | tests/web_alt_panel_live_undo.rs | y | y |  |
| R-4.3 | `web_alt_panel_live_refused::test_live_alternative_duplicate_is_refused` | tests/web_alt_panel_live_refused.rs | y | y |  |
| R-4.3 | `web_alt_panel_live_limit::test_live_alternative_and_the_history_limit` | tests/web_alt_panel_live_limit.rs | y | y |  |
| R-4.3 | `web_alt_panel_live_capacity::test_live_alternative_with_the_history_at_capacity` | tests/web_alt_panel_live_capacity.rs | y | y |  |
| R-4.3 | `web_alt_panel::test_selection_add_up_down_edit_and_undo` | tests/web_alt_panel.rs | y | y |  |
| R-4.4 | `web_alt_panel_tabs::test_chrome_dot_provenance…` | tests/web_alt_panel_tabs.rs | y | y |  |
| R-4.5 | `web_alt_panel::test_selection_add_up_down_edit_and_undo` | tests/web_alt_panel.rs | y | y |  |
| R-4.6 | `web_kg_menu::test_ai_alternatives_for_selection_append_bot_marked_lines_as_one_step` | tests/web_kg_menu.rs | y | y |  |
| R-5.1 | `web_ghosts::test_ghost_and_revive_via_menu_and_shortcut` | tests/web_ghosts.rs | y | y |  |
| R-5.2 | `web_ghosts::test_ghost_and_revive_via_menu_and_shortcut` | tests/web_ghosts.rs | y | y |  |
| R-5.3 | `ghosts::*` | crates/terraphim_alternatives/tests/ghosts.rs | y | y | 24 tests in file |
| R-5.3 | `web_ghosts::test_ghost_follows_typing_elsewhere_and_inside` | tests/web_ghosts.rs | y | y |  |
| R-6.1 | `web_overflow::test_xyz_toggles_the_panel_with_aria_escape_and_mode_gating` | tests/web_overflow.rs | y | y |  |
| R-6.1 | `web_overflow_layout::test_ctrl_shift_d_toggles_the_panel_shift_exact` | tests/web_overflow_layout.rs | y | y |  |
| R-6.1 | `web_shortcuts::test_configured_and_write_on_shortcuts_are_disjoint` | tests/web_shortcuts.rs | y | y |  |
| R-6.1 | `web_overflow::test_panel_width_matches_the_video` | tests/web_overflow.rs | y | y |  |
| R-6.1 | `web_overflow_layout::test_panel_is_full_height_and_flush_right` | tests/web_overflow_layout.rs | y | y |  |
| R-6.2 | `web_overflow::…mode_gating` | tests/web_overflow.rs | y | y |  |
| R-6.3 | `web_overflow::test_stash_is_a_move_and_one_undo_step` | tests/web_overflow.rs | y | y |  |
| R-6.4 | `web_overflow::test_ctrl_enter_and_drop_insert_at_the_caret` | tests/web_overflow.rs | y | y |  |
| R-6.4 | `document::pull_back_*` | src/document.rs, crates/terraphim_alternatives/src/document.rs | n | - |  |
| R-6.5 | `web_overflow::test_panel_text_persists_is_not_exported…` | tests/web_overflow.rs | y | y |  |
| R-7.1 | `web_chrome::test_counter_click_toggles_write_on…` | tests/web_chrome.rs | y | y |  |
| R-7.1 | `web_chrome::test_plain_mode_shows_only_the_counter` | tests/web_chrome.rs | y | y |  |
| R-7.1 | `web_chrome::test_plain_mode_has_the_video_palette_and_column` | tests/web_chrome.rs | y | y |  |
| R-7.2 | `web_chrome::test_counter_click_toggles_write_on_and_every_corner_control` | tests/web_chrome.rs | y | y |  |
| R-7.3 | `web_ghosts::test_menu_keyboard_navigation_and_plugins` | tests/web_ghosts.rs | y | y |  |
| R-7.3 | `web_overflow::test_stash…` | tests/web_overflow.rs | y | y |  |
| R-7.4 | `web_shortcuts::test_configured_and_write_on_shortcuts_are_disjoint` | tests/web_shortcuts.rs | y | y |  |
| R-7.4 | `web_shortcuts::test_config_shortcut_does_not_fire_on_write_on_shift_chord` | tests/web_shortcuts.rs | y | y |  |
| R-7.4 | `web_shortcuts::test_config_shortcuts_need_their_exact_modifiers` | tests/web_shortcuts.rs | y | y |  |
| R-7.4 | `web_shortcuts::test_tools_toggle_needs_exactly_ctrl_shift_l` | tests/web_shortcuts.rs | y | y |  |
| R-8.1 | `web_lab::test_each_action_marks_exactly_the_engine_spans_and_clear_works` | tests/web_lab.rs | y | y |  |
| R-8.1 | `web_lab::test_popover_trim_row_and_hidden_texts_match_the_video` | tests/web_lab.rs | y | y |  |
| R-8.2 | `invariants::labels_and_kinds_cover_all_six_actions` | crates/terraphim_lab/tests/invariants.rs | y | y |  |
| R-8.2 | `invariants::no_action_mutates_the_input` | crates/terraphim_lab/tests/invariants.rs | y | y |  |
| R-8.2 | `web_lab::test_each_action_marks…` | tests/web_lab.rs | y | y |  |
| R-8.3 | `web_trim::test_each_level_card_and_preview_equal_the_engine` | tests/web_trim.rs | y | y |  |
| R-8.3 | `trim::levels_are_nested` | crates/terraphim_lab/tests/trim.rs | y | y |  |
| R-8.3 | `web_lab::test_popover_trim_row_and_hidden_texts_match_the_video` | tests/web_lab.rs | y | y |  |
| R-8.4 | `web_trim::test_each_level_card…` | tests/web_trim.rs | y | y |  |
| R-8.5 | `web_trim::test_click_keeps_the_outermost_cut…` | tests/web_trim.rs | y | y |  |
| R-8.5 | `web_trim_cuts::*` | tests/web_trim_cuts.rs | y | y | 2 tests in file |
| R-8.5 | `web_trim_done::*` | tests/web_trim_done.rs | y | y | 1 tests in file |
| R-8.6 | `tests/kg.rs::ai_alternatives_for_selection_append_after_the_writers_own` | tests/kg.rs | y | y |  |
| R-8.6 | `web_kg_menu::test_ai_alternatives_for_selection_append_bot_marked_lines_as_one_step` | tests/web_kg_menu.rs | y | y |  |
| R-8.7 | `tests/providers.rs::*` | tests/providers.rs | y | y | 11 tests in file |
| R-8.7 | `web_providers::*` | tests/web_providers.rs | y | y | 5 tests in file |
| R-9.1 | `persistence::preview_and_export_match_golden_files` | tests/persistence.rs | y | y |  |
| R-9.1 | `block_round_trip::*` | crates/terraphim_alternatives/tests/block_round_trip.rs | y | y | 21 tests in file |
| R-9.2 | `context_reanchor::*` | crates/terraphim_alternatives/tests/context_reanchor.rs | y | y | 16 tests in file |
| R-9.2 | `reanchoring::*` | crates/terraphim_alternatives/tests/reanchoring.rs | y | y | 14 tests in file |
| R-9.3 | `lifecycle::export_drops_ghosted_text_and_omits_overflow_and_block` | crates/terraphim_alternatives/tests/lifecycle.rs | y | y |  |

citations: 82; not found: 1; not passed: 0
rows covered: 41
