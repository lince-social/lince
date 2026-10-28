use super::super::{Result, cargo_command};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{IsTerminal, Read, Write},
    net::TcpListener,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod browser;
mod diagnostics;
mod metrics;
mod observe;
mod automation;

const QUIET_INSPECTOR_TEST: &str = "runtime::tests::availability::record_loading_and_deletion_wake_a_quiet_worker_without_test_input";
const PERMISSION_INSPECTOR_TEST: &str = "runtime::tests::availability::permission::real_permission_refusal_and_refresh_keep_the_captured_source_and_native_draft";
const POINTER_DRAG_CANCEL_TEST: &str = "server::tests::cancelling_an_accepted_pointer_drag_cannot_perform_its_drop_and_retires_once";
const FILE_HOVER_SCROLL_TEST: &str = "runtime::tests::hover::scroll::stationary_file_edge_scroll_reaches_its_real_limit_and_cancels_without_late_wakes";
const FILE_HOVER_LIFETIME_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::hover_lifetime::a_real_drag_crosses_outputs_without_reusing_the_previous_hover_or_moving_its_source",
    "server::tests::transfers_tests::dispatch::hover_lifetime::a_hover_reply_retained_by_a_full_input_queue_cannot_outlive_its_modal_source_output_or_lock",
    "server::tests::transfers_tests::dispatch::hover_lifetime::escape_cancels_a_real_pointer_drag_without_a_drop_and_native_ime_and_application_input_recover",
];
const WORKSPACE_HISTORY_TEST: &str = "server::tests::transfers_tests::dispatch::workspace_history::rendered_workspace_history_save_cancel_cold_start_and_real_held_batched_provider_input_preserve_distinct_actions";
const WORKSPACE_HISTORY_LOWER_TESTS: &[&str] = &[
    "native::workspace::navigation::tests::numeric_reselection_is_optional_and_identity_activation_does_not_toggle",
    "native::workspace::navigation::tests::history_keeps_identity_after_reorder_and_retires_deleted_or_replaced_canvas_targets",
    "native::workspace::navigation::tests::failed_selection_keeps_history_and_a_fresh_session_starts_empty",
    "native::workspace::navigation::tests::output_histories_stay_independent_and_a_disconnected_view_cannot_recover_old_history",
    "native::workspace::navigation::tests::a_temporary_laboratory_destination_never_supplies_or_replaces_numeric_history",
];
const OUTPUT_AFFINITY_TEST: &str = "server::tests::transfers_tests::dispatch::affinity::two_real_output_affinities_reconnect_without_stealing_input_and_cold_native_focus_and_unfocus_restore_by_stable_identity";
const FOCUS_POLICY_SETTINGS_TEST: &str = "server::tests::transfers_tests::dispatch::focus_policy::real_focus_settings_controls_save_cancel_and_reload_requested_warp_without_claiming_a_nested_cursor_capability";
const FOCUS_POLICY_CLIENT_TEST: &str = "server::tests::transfers_tests::dispatch::focus_policy::real_client_focus_keeps_click_pointer_queue_popup_ime_and_lock_ownership_across_two_outputs";
const FOCUS_POLICY_MOTION_TEST: &str = "server::tests::transfers_tests::dispatch::focus_policy::overlapping_real_clients_keep_keyboard_focus_for_120_presentations_while_an_area_moves_native_content_under_a_stationary_pointer";
const OPENING_RULE_WORKFLOW_TEST: &str = "server::tests::transfers_tests::dispatch::opening::rendered_cell_rule_order_and_window_overrides_survive_startup_and_reapply_to_the_same_real_wayland_window";
const OPENING_RULE_READINESS_TEST: &str = "server::tests::transfers_tests::dispatch::opening::configured_rules_gate_presented_readiness_reject_late_snapshots_and_leave_a_quiet_worker_after_recovery";
const CONFIGURATION_RENDER_TESTS: &[&str] = &[
    "presentation::tests::configuration::every_configuration_owner_reaches_review_cancel_update_and_continue_without_implicitly_saving_at_scaled_sizes",
    "presentation::tests::configuration::rendered_review_of_real_external_revisions_preserves_the_draft_and_rejects_an_older_comparison",
    "presentation::tests::configuration::owners::eight_rendered_binding_conflicts_scroll_to_the_last_choice_preserve_parent_draft_and_reload_the_mixed_saved_chords",
    "presentation::tests::configuration::owners::rendered_layout_focus_gesture_and_overview_owners_merge_independent_file_fields_then_save_and_reopen_their_real_controls",
    "presentation::tests::configuration::owners::rendered_opening_order_review_selects_one_complete_list_preserves_real_rule_records_and_reloads_before_startup",
];
const CONFIGURATION_INPUT_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::configuration::real_client_ime_and_controlled_pending_paste_preserve_a_stale_settings_draft_through_file_review_cancel_save_and_input_recovery",
    "server::tests::transfers_tests::dispatch::configuration::queued_review_cannot_apply_after_output_or_lock_loss_and_a_real_blocked_file_read_cannot_wake_its_disposed_worker",
];
const CONFIGURATION_QUIET_TEST: &str = "runtime::tests::configuration::an_atomic_file_edit_wakes_a_quiet_worker_and_publishes_the_new_policy_without_injected_input";
const CONFIGURATION_ACTIVITY_TESTS: &[&str] = &[
    "host::input::activity::tests::an_inactive_or_returned_input_owner_cannot_reuse_an_earlier_activity_revision",
    "native::actions::tests::queued_ui_actions_refuse_lost_and_returned_activity_while_fresh_and_windowed_actions_still_run",
];
const CAPTURE_SETTINGS_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::privacy::settings::native_capture_settings_use_real_scaled_controls_cancel_apply_and_file_backed_cell_startup_without_spawning_missing_applications",
    "server::tests::transfers_tests::dispatch::privacy::settings::real_window_properties_exclusion_retains_a_failed_draft_retries_the_captured_uid_and_keeps_closed_metadata_private_after_cold_start",
];
const CAPTURE_RENDER_TESTS: &[&str] = &[
    "presentation::tests::capture::native_capture_omits_private_pixels_preserves_public_updates_and_runs_one_main_update",
    "presentation::tests::capture::capture_retains_public_frozen_shader_pixels_and_restores_private_targets_without_another_update",
    "presentation::tests::capture::removed_private_child_in_a_paused_cached_surface_is_refreshed_only_for_external_capture",
    "presentation::tests::capture::contributors::imported_glb_and_generated_wgsl_meshes_follow_their_real_sources_in_capture_and_a_replacement_keeps_its_public_pixels",
    "presentation::tests::capture::aliases::a_real_size_transition_alias_keeps_its_owner_pixels_and_gets_fresh_public_capture_without_another_update",
    "presentation::tests::capture::aliases::inactive_overview_nested_preview_and_companion_output_follow_the_real_retained_source_through_activation_and_replacement",
];
const CAPTURE_CLIENT_TEST: &str = "server::tests::transfers_tests::dispatch::privacy::real_output_capture_keeps_public_updates_and_excludes_private_windows_and_owned_popups";
const CAPTURE_LIFETIME_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::capture_lifetime::peers::disconnecting_an_independent_capturer_cannot_write_its_retained_readback_or_retire_the_ordinary_application",
    "server::tests::transfers_tests::dispatch::capture_lifetime::peers::twenty_actual_capture_session_resize_stop_and_disconnect_cycles_retire_gpu_resources_and_return_to_quiet_owner_input",
    "server::tests::transfers_tests::dispatch::capture_lifetime::peers::retained_cursor_and_drag_artwork_cannot_inherit_a_replacement_origin_without_a_fresh_authorized_request",
];
const CAPTURE_RETAINED_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::capture_lifetime::retained::released_shm_reuse_without_commit_preserves_direct_pixels_and_twenty_new_versions_retire_owned_exports",
    "server::tests::transfers_tests::dispatch::capture_lifetime::retained::retained_direct_readbacks_reject_privacy_stop_lock_unmap_and_original_deadline_then_recover_fresh_pixels_and_input",
    "server::tests::transfers_tests::dispatch::capture_lifetime::retained::disconnecting_a_direct_capturer_retires_retained_pixels_without_writing_shm_or_disrupting_the_application",
];
const CAPTURE_RETAINED_PROTOCOL_TESTS: &[&str] = &[
    "server::tests::capture_tests::output_capture_demand_tracks_retained_jobs_and_private_output_uses_a_filtered_frame",
    "server::tests::capture_tests::window_sources_capture_only_the_selected_toplevel_and_stop_on_unmap",
    "server::tests::capture_tests::resuming_capture_preserves_each_pending_requests_original_deadline_and_fresh_work_recovers",
];
const CAPTURE_ALIAS_POLICY_TESTS: &[&str] = &[
    "native::topology::presentation::tests::a_displayed_capture_alias_requires_its_current_producer_target_and_visual_owner",
    "native::capture::render::tests::retired_image_history_can_be_redacted_but_an_eligible_current_writer_still_needs_a_real_filtered_completion",
    "native::topology::surface_render::order::tests::filtered_preview_dependencies_precede_their_consumers_and_cycles_cannot_gain_a_completion",
];
const WINDOWED_FULLSCREEN_MODEL_TESTS: &[&str] = &[
    "layout::desktop::windowed_fullscreen::tests::bounded_protocol_presentation_retains_requests_homes_and_neighbour_geometry_for_twenty_cycles",
    "layout::desktop::windowed_fullscreen::tests::output_constraints_and_deliberate_edits_resolve_the_existing_request_without_an_entry_snapshot",
    "layout::desktop::windowed_fullscreen::tests::client_fullscreen_and_maximize_requests_preserve_bounded_policy_until_an_explicit_exit",
    "layout::desktop::windowed_fullscreen::tests::bounded_policy_does_not_clear_a_different_windows_fullscreen_intent",
    "layout::desktop::windowed_fullscreen::tests::close_transfer_and_cold_load_never_replay_a_retired_live_protocol_mode",
    "layout::desktop::windowed_fullscreen::tests::native_missing_and_invalid_targets_leave_the_complete_model_unchanged",
    "native::layout::desktop::windowed_fullscreen::tests::the_shared_command_toggles_protocol_intent_without_changing_camera_or_item_geometry",
    "native::layout::desktop::windowed_fullscreen::tests::a_properties_control_changes_its_captured_window_without_taking_focus_from_the_inspector",
    "native::layout::desktop::windowed_fullscreen::tests::captured_controls_refuse_replaced_hidden_transient_canvas_and_sandbox_targets",
    "native::layout::desktop::windowed_fullscreen::tests::properties_availability_rejects_missing_record_binding_and_inactive_seat_then_recovers_the_same_target",
];
const WINDOWED_FULLSCREEN_CONFIGURATION_TEST: &str = "native::shortcuts::configuration::tests::windowed_fullscreen_starts_unbound_and_uses_the_existing_atomic_configuration_and_startup";
const WINDOWED_FULLSCREEN_CLIENT_TESTS: &[&str] = &[
    "server::tests::transfers_tests::windowed_fullscreen::real_wayland_windowed_fullscreen_uses_operation_and_keeps_slot_bar_neighbour_and_requested_geometry",
    "server::tests::transfers_tests::windowed_fullscreen::a_delayed_fullscreen_ack_keeps_the_old_buffer_without_repeating_configures_and_close_retires_mode",
    "server::tests::transfers_tests::dispatch::windowed_fullscreen::real_properties_and_saved_rebound_input_keep_the_captured_record_slot_and_native_focus_then_cold_start_without_session_mode",
    "server::tests::transfers_tests::dispatch::windowed_fullscreen::bounded_fullscreen_keeps_actual_popup_pixels_and_recipients_requested_edits_and_explicit_mode_transitions_in_flat_and_spatial_slots",
    "server::tests::transfers_tests::dispatch::windowed_fullscreen::queued_properties_cannot_toggle_after_record_close_output_modal_or_lock_retirement_and_fresh_input_recovers",
];
const COMPACT_MODEL_TESTS: &[&str] = &[
    "layout::desktop::rows::compact::tests::compact_members_share_one_slot_and_ungroup_restores_individual_homes_and_sizes",
    "layout::desktop::rows::compact::tests::incompatible_member_constraints_are_clipped_inside_a_stable_slot_without_moving_neighbours",
    "layout::desktop::rows::compact::tests::free_mode_keeps_the_relation_dormant_and_returns_to_its_remembered_active_member",
    "layout::desktop::rows::compact::tests::close_detach_reorder_and_slot_transfer_keep_each_placement_once",
    "layout::desktop::rows::compact::tests::native_group_persistence_and_invalid_members_preserve_the_single_existing_owner",
    "layout::desktop::rows::compact::tests::a_hidden_members_fullscreen_request_cannot_blank_the_active_slot_or_take_over",
    "layout::desktop::rows::compact::tests::compact_resize_uses_slot_intent_instead_of_an_oversized_members_minimum",
    "native::layout::desktop::compact::tests::shared_commands_select_reorder_detach_and_ungroup_without_losing_native_or_application_identity",
    "native::layout::desktop::compact::tests::dragging_blocks_group_changes_and_explicit_activation_reveals_a_live_hidden_member",
    "native::layout::desktop::compact::tests::captured_properties_refuse_changed_surface_canvas_and_closed_record_associations",
    "native::layout::desktop::compact::tests::compact_properties_and_selection_refuse_inactive_seats_and_missing_or_replaced_local_record_bindings",
    "native::layout::desktop::compact::tests::strip_labels_keep_private_member_provenance_and_free_mode_retires_the_selector",
    "native::layout::desktop::compact::tests::compact_commands_are_registered_discoverable_and_unbound_without_changing_existing_defaults",
    "native::layout::desktop::compact::chooser::tests::chooser_cancel_preserves_the_model_and_confirm_performs_one_membership_commit",
    "native::layout::desktop::compact::chooser::tests::a_changed_selected_surface_or_drag_keeps_the_draft_and_cannot_commit_to_a_replacement",
    "native::layout::desktop::compact::chooser::tests::a_replacement_modal_retains_keyboard_focus_when_the_old_chooser_receives_a_late_action",
];
const COMPACT_CLIENT_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::compact::workflow::rendered_compact_properties_chooser_header_and_saved_rebound_actions_keep_the_captured_members_at_scaled_widths",
    "server::tests::transfers_tests::dispatch::compact::workflow::ordinary_native_group_save_and_actual_cell_restart_restore_content_identity_order_and_active_member_without_reseeding",
    "server::tests::transfers_tests::dispatch::compact::clients::actual_compact_clients_keep_popup_ime_drag_transfer_and_queued_capture_ownership_without_hidden_input_or_neighbour_configure_loops",
    "server::tests::transfers_tests::dispatch::compact::lifetime::queued_compact_confirmation_refuses_closed_replaced_modal_output_and_locked_owners_then_fresh_choices_recover",
];
const SCRATCH_MODEL_TESTS: &[&str] = &[
    "layout::desktop::scratch::tests::marking_does_not_move_or_hide_and_twenty_reveals_keep_one_item_and_restore_its_home",
    "layout::desktop::scratch::tests::scratch_and_groups_are_exclusive_and_failed_actions_preserve_the_model",
    "layout::desktop::scratch::tests::normalized_native_save_starts_hidden_at_home_and_never_persists_the_reveal_state",
    "layout::desktop::scratch::tests::hidden_client_requests_cannot_reveal_scratch_or_take_fullscreen_and_close_retires_it",
    "native::layout::desktop::scratch::tests::scratch_commands_move_one_live_item_and_hide_its_family_without_client_minimization",
    "native::layout::desktop::scratch::tests::captured_commands_reject_replacement_and_lock_and_drag_without_mutation",
    "native::layout::desktop::scratch::tests::captured_scratch_properties_require_the_same_local_record_and_active_input_before_revealing_or_moving",
    "native::layout::desktop::scratch::tests::lost_home_uses_the_current_workspace_without_recreating_the_home",
    "native::layout::desktop::scratch::tests::commands_are_discoverable_unbound_and_nonrepeating_for_shared_desktop_use",
    "native::layout::desktop::scratch::chooser::tests::chooser_cancel_keeps_membership_focus_and_content_and_confirm_moves_only_the_captured_item",
    "native::layout::desktop::scratch::chooser::tests::stale_choice_preserves_the_draft_and_session_loss_retires_confirmation",
    "native::layout::desktop::scratch::chooser::tests::replacing_the_keyboard_owner_keeps_the_new_dialog_and_private_labels_keep_their_source",
    "native::workspace::scratch::tests::ordinary_native_save_normalizes_scratch_home_and_startup_restores_hidden_content",
    "native::workspace::scratch::tests::a_real_scratch_snapshot_write_failure_keeps_live_content_and_last_good_files_then_retry_reopens_hidden_at_home",
];
const SCRATCH_WORKFLOW_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::scratch::workflow::real_scratch_properties_long_chooser_cancel_saved_chords_and_focus_return_keep_the_same_records_at_scaled_widths",
    "server::tests::transfers_tests::dispatch::scratch::workflow::actual_cell_restart_restores_native_scratch_hidden_at_its_home_with_owned_content_without_reseeding_or_launching",
    "server::tests::transfers_tests::dispatch::scratch::clients::real_scratch_clients_keep_popup_ime_drag_delayed_configure_and_capture_ownership_for_twenty_two_output_round_trips",
    "server::tests::transfers_tests::dispatch::scratch::lifetime::queued_scratch_properties_and_choices_cannot_reveal_closed_replaced_modal_output_or_locked_targets_and_fresh_input_recovers",
];
const CONFIGURATION_POLICY_TESTS: &[&str] = &[
    "native::shortcuts::editor::tests::reviewed_rebase_keeps_independent_binding_changes_in_draft_until_normal_save",
    "native::shortcuts::editor::tests::cancelling_binding_review_preserves_the_original_draft_and_can_resume_capture",
    "native::shortcuts::editor::tests::an_external_binding_change_during_a_held_capture_preserves_its_release_and_local_candidate",
    "native::shortcuts::configuration::reload::tests::debounce_does_not_allow_a_continuous_burst_to_postpone_reading_forever",
    "native::shortcuts::configuration::reload::tests::native_atomic_replacement_invalid_file_and_self_save_share_one_accepted_configuration",
    "native::shortcuts::configuration::reload::tests::a_dangling_selected_symlink_observes_creation_and_later_atomic_target_replacement",
    "native::shortcuts::configuration::reload::tests::a_read_started_before_a_local_save_cannot_restore_the_old_configuration",
    "native::shortcuts::configuration::reload::tests::continued_atomic_writes_do_not_starve_publication_and_settle_on_the_final_file",
    "native::shortcuts::configuration::reload::tests::disposal_aborts_the_deadline_and_discards_a_late_reader_result_without_a_wake",
    "native::shortcuts::configuration::review::tests::cancelling_review_preserves_the_editor_draft_active_values_and_parent_keyboard_owner",
    "native::shortcuts::configuration::review::tests::explicit_conflict_choice_updates_only_the_original_draft_and_retains_independent_file_changes",
    "native::shortcuts::configuration::review::tests::changed_file_and_old_queued_choice_require_a_new_comparison",
    "native::shortcuts::configuration::review::tests::a_closed_editor_cannot_be_replaced_by_another_owner",
    "native::shortcuts::configuration::review::tests::a_hidden_or_disabled_prior_field_is_not_given_focus_after_cancel",
    "native::shortcuts::configuration::review::tests::hiding_the_parent_or_changing_the_canvas_or_workspace_retires_review_without_applying",
    "native::shortcuts::configuration::review::tests::a_replacement_modal_retains_its_keyboard_and_focused_field",
    "native::shortcuts::configuration::review::tests::removing_the_captured_output_cannot_restore_its_original_editor_focus",
    "native::shortcuts::configuration::review::merge::tests::independent_bindings_and_scalar_preferences_merge_without_text_or_list_splicing",
    "native::shortcuts::configuration::review::merge::tests::removed_override_and_explicit_unbound_are_different_choices",
    "native::shortcuts::configuration::review::merge::tests::conflicting_preset_and_rule_order_lists_are_atomic_fields",
    "native::shortcuts::configuration::review::merge::tests::identical_edits_are_not_conflicts_and_review_work_is_bounded",
];
const AUTOMATION_MODEL_TESTS: &[&str] = &[
    "native::layout::desktop::control::tests::captured_selection_cannot_retarget_after_focus_or_canvas_changes",
    "native::layout::desktop::control::tests::dispatcher_access_cannot_grant_window_output_launch_or_capture_permissions",
    "native::layout::desktop::control::tests::qualified_workspace_selection_is_idempotent_and_reordering_invalidates_the_destination",
];
const AUTOMATION_PROTOCOL_TESTS: &[&str] = &[
    "server::control::flow::tests::the_rate_bucket_limits_bursts_and_refills_from_elapsed_time",
    "server::control::flow::tests::flow_cannot_consume_reserved_replies_or_accept_a_guessed_acknowledgement",
    "server::tests::control_tests::the_management_global_and_command_authority_are_connection_scoped",
    "server::tests::control_tests::request_admission_reserves_terminal_slots_and_coalesces_a_slow_observers_state_history",
    "server::tests::control_tests::malformed_and_oversized_invocations_do_not_reserve_or_execute_work",
    "server::tests::control_tests::locking_and_unlocking_cannot_revive_an_earlier_queued_request",
    "server::tests::control_tests::owner_revocation_and_resource_destruction_retire_unexecuted_work",
    "server::tests::control_tests::snapshot_pages_keep_one_revision_and_report_stale_or_invalid_cursors",
    "server::tests::control_tests::duplicate_in_flight_identifiers_disconnect_without_a_second_dispatch",
    "server::tests::control_tests::a_closed_wayland_connection_invalidates_an_already_delivered_work_item",
    "server::tests::control_tests::inactive_seat_round_trips_do_not_revive_a_previously_queued_request",
    "server::tests::control_tests::an_unfinished_worker_reply_has_a_finite_deadline_and_retires_its_reservation",
    "server::tests::control_tests::sdk::the_public_sdk_recovers_from_actual_socket_write_backpressure_without_resending_a_snapshot",
    "server::tests::control_tests::sdk::a_public_sdk_mutation_timeout_never_replays_the_command_and_a_late_reply_cannot_replace_a_fresh_snapshot",
    "server::tests::control_tests::sdk::the_public_sdk_consumes_resync_and_reads_the_final_revision_after_a_real_slow_observer_flood",
    "server::tests::control_tests::sdk::restarting_an_owner_selected_helper_uses_a_fresh_authorized_fd_and_only_its_new_connection_grants",
    "server::tests::control_tests::outputs::management_observation_tracks_real_mode_refresh_disable_reenable_hotplug_and_stale_output_pages",
];
const AUTOMATION_INPUT_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::control::typed_commands_reach_the_real_worker_and_report_actual_launch_failure_without_stopping_park",
    "server::tests::transfers_tests::dispatch::control::changing_focus_ahead_of_a_queued_protocol_move_rejects_it_instead_of_moving_the_new_target",
    "server::tests::transfers_tests::dispatch::control::ownership::queued_automation_refuses_real_popup_modal_preedit_and_ime_grab_owners_then_fresh_commands_and_client_input_recover",
    "server::tests::transfers_tests::dispatch::control::ownership::destroying_a_queued_commands_window_and_reusing_its_actual_wayland_object_number_cannot_move_or_close_the_replacement",
];
const AUTOMATION_RENDER_TEST: &str = "presentation::tests::control::the_park_tab_explains_availability_and_revokes_only_through_its_host_binding";
const AUTOMATION_REPORT_TESTS: &[&str] = &[
    "park::validation::tests::dependency_source_copy_preserves_content_identity_and_refuses_symlink_escape",
    "park::validation::automation::tests::report_acceptance_requires_every_named_observation_and_refuses_empty_failed_or_replayed_calls",
    "park::validation::automation::tests::report_acceptance_binds_payload_protocol_session_targets_permission_results_and_actual_native_cleanup",
    "park::validation::automation::tests::choosing_the_physical_automation_fixture_preserves_other_helper_choices_and_extends_its_watchdog",
];
const OPENING_LIFETIME_LOWER_TESTS: &[&str] = &[
    "native::layout::desktop::opening::ui::tests::lifetime::an_acknowledged_creation_retries_the_same_record_after_queue_denial_revision_conflict_permission_denial_or_timeout",
    "native::layout::desktop::opening::ui::tests::lifetime::a_real_late_save_cannot_link_rules_or_steal_focus_after_editor_view_cell_or_authentication_replacement",
    "native::layout::desktop::opening::ui::tests::lifetime::configured_startup_waits_for_a_real_snapshot_or_its_terminal_deadline_and_ignores_a_retired_late_reply",
    "native::layout::desktop::opening::tests::queued_reapply_rejects_changed_canvas_workspace_output_assignment_and_mapping_before_preparation",
];
const FOCUS_POLICY_LOWER_TESTS: &[&str] = &[
    "native::layout::desktop::focus::tests::pointer_focus_is_optional_preserves_unfocus_and_has_no_idle_retargeting",
    "native::layout::desktop::focus::tests::cursor_warp_requires_host_support_and_is_retired_by_new_pointer_input",
];
const OUTPUT_AFFINITY_LOWER_TESTS: &[&str] = &[
    "layout::outputs::tests::reconnecting_preferred_output_keeps_a_workspace_on_other_monitors",
    "layout::outputs::tests::returning_head_waits_for_the_input_workspace_then_restores_without_stealing_focus",
    "layout::outputs::tests::explicit_home_change_retires_an_older_return_request",
    "layout::outputs::tests::an_explicit_home_move_while_the_old_head_is_absent_survives_its_return",
    "layout::outputs::tests::initial_restore_resolves_provisional_assignments_before_selecting_the_saved_workspace",
    "layout::outputs::tests::active_preferences_reject_ambiguous_or_unnamed_homes",
    "native::layout::desktop::focus::tests::saved_native_focus_restores_follow_but_explicit_unfocus_stays_unfocused",
    "native::layout::desktop::focus::tests::saved_focus_waits_for_a_native_target_and_cannot_replace_another_editor_input",
    "native::layout::desktop::focus::tests::saved_application_focus_never_recreates_or_reactivates_a_protocol_target",
    "native::layout::desktop::focus::tests::following_on_another_spatial_output_never_changes_the_current_output_camera",
];
const TABLET_LIFETIME_TEST: &str = "server::tests::tablet_tests::lifetimes::twenty_tablet_lifetimes_release_every_held_edge_and_recover_with_independent_tool_identity";
const LOGOUT_RECOVERY_TEST: &str = "runtime::tests::logout::confirmed_logout_save_failure_is_recoverable_and_trusted_shutdown_saves_through_an_open_dialog";
const ABSOLUTE_ORDER_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::sequence::lifetimes::twenty_native_pen_device_lifetimes_settle_host_worker_contacts_and_pointer_entities_after_held_or_ended_removal",
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::settings::ordinary_calibration_controls_preserve_failed_drafts_and_saved_state_then_save_reload_and_reconnect_using_real_file_io",
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::sequence::successive_devices_type_into_their_own_native_client_and_layer_targets_across_outputs_despite_an_earlier_output_receipt_and_full_worker_queue",
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::sequence::retirement::retired_absolute_sources_controls_modals_outputs_locks_seats_and_save_failed_logout_cannot_replay_typing_or_keep_an_old_modifier",
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::touch_pen_and_mapped_tool_press_then_typing_keep_the_real_editor_under_worker_pressure",
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::rejected_absolute_press_cannot_type_or_fire_a_replacement_and_clears_an_old_modifier",
    "server::tests::transfers_tests::dispatch::native_pointer::absolute::full_absolute_deferred_input_cancels_without_stopping_park_and_fresh_input_recovers",
];
const NATIVE_POINTER_OWNERSHIP_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::native_pointer::captured_native_press_survives_queue_pressure_but_cannot_activate_a_replaced_action",
    "server::tests::transfers_tests::dispatch::native_pointer::output::actual_output_handoff_replays_a_valid_native_click_and_refuses_a_replacement_editor",
    "server::tests::transfers_tests::dispatch::native_pointer::output::real_output_handoff_keeps_deferred_wheel_with_its_presented_editor_and_retires_replacement_modal_output_and_lock_before_fresh_input",
    "server::tests::transfers_tests::dispatch::native_pointer::wheel::ordinary_native_wheel_uses_the_presented_recipient_under_real_queue_pressure_and_retires_changed_action_modal_and_lock_without_resetting_valid_ime",
];
const ACCESSIBILITY_CONTEXT_TESTS: &[&str] = &[
    "native::accessibility::actions::tests::accessibility_value_uses_normal_editor_limits_unicode_and_keyboard_ownership",
    "native::accessibility::actions::tests::activity_loss_and_return_retire_captured_accessibility_edits_before_a_delayed_host_status_and_fresh_edits_recover",
    "native::accessibility::actions::tests::changed_canvas_hidden_lock_and_replaced_modal_reject_queued_control_actions",
    "native::accessibility::actions::tests::disabled_controls_remain_describable_and_protected_fields_never_export_plain_text_runs",
    "native::accessibility::actions::tests::text_runs_and_selection_come_from_the_existing_editor_layout",
    "native::accessibility::actions::tests::keyboard_reveal_uses_one_captured_intent_and_retires_it_on_focus_canvas_or_lock_loss",
    "native::accessibility::actions::tests::accessibility_export_does_not_mark_text_changed_and_dirty_layout_retires_old_selection_actions",
    "native::accessibility::actions::tests::visible_companion_controls_keep_their_output_owner_and_do_not_capture_the_other_output_keyboard",
    "native::accessibility::actions::tests::world_camera_and_item_motion_do_not_retarget_semantic_actions_but_final_mapping_changes_retire_them",
    "native::accessibility::actions::tests::descriptions_of_presented_text_remain_valid_while_an_older_edit_request_is_rejected",
    "native::accessibility::actions::tests::protecting_an_existing_field_retires_its_older_plain_text_description",
];
const ACCESSIBILITY_PLATFORM_TESTS: &[&str] = &[
    "presentation::accessibility::platform::private_platform_adapter_discovers_controls_and_round_trips_actions_and_removal",
    "presentation::accessibility::platform::dispatcher::real_desktop_presentation_gates_platform_text_and_reader_callbacks_survive_worker_pressure",
    "presentation::accessibility::platform::dispatcher::outputs::actual_independent_output_draws_gate_bus_text_and_discarded_coalesced_late_and_hidden_owner_work_cannot_restore_stale_semantics",
];
const MAGNIFICATION_SETTINGS_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::rendered_keyboard_magnification_saves_through_the_dispatcher_retries_pressure_and_loads_at_fresh_startup";
const MAGNIFICATION_CLIENT_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::clients::actual_wayland_client_magnification_uses_independent_pixels_and_input_without_resizing_in_flat_spatial_free_and_tiled_views";
const ACCESSIBILITY_PROTECTION_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::platform::real_private_reader_protected_fields_and_queued_caret_requests_retire_on_composed_session_lock_and_fresh_input_recovers";
const ACCESSIBILITY_MIXED_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::platform::mixed::mixed_scale_nonzero_origin_heads_keep_independent_text_caret_ime_and_real_client_lens_input";
const MAGNIFICATION_INPUT_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::input::actual_client_constraints_relative_motion_and_virtual_host_warp_use_the_presented_lens_and_retire_stale_focus_work";
const MAGNIFICATION_REVEAL_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::input::keyboard_tab_reverse_tab_and_long_shortcut_list_reveal_stay_put_under_a_stationary_pointer_and_real_client_typing_recovers";
const MAGNIFICATION_DROP_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::drop::real_magnified_data_offer_retires_held_pipe_on_output_mapping_or_lock_loss_and_fresh_drop_imports_once_without_moving_its_source";
const MAGNIFICATION_CARET_TEST: &str = "server::tests::transfers_tests::dispatch::magnification::caret::real_application_and_layer_carets_anchor_their_mixed_scale_head_without_a_popup_and_cursor_pixels_capture_and_owner_loss_follow_the_same_mapping";
const MAGNIFICATION_TEXT_INPUT_TESTS: &[&str] = &[
    "server::tests::text_input_tests::native_client_caret_is_committed_owned_and_retired_without_an_ime_provider_or_popup",
    "server::tests::text_input_tests::provider_join_loss_and_unavailable_peer_destruction_preserve_the_real_text_input_owner_and_reenable_without_a_popup",
];
const OUTPUT_ROTATION_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::outputs::saved_rotation_loads_at_actual_worker_startup_survives_fitting_parent_resize_and_recovers_without_overwriting_saved_intent",
    "server::tests::transfers_tests::dispatch::outputs::invalid_saved_virtual_geometry_preserves_the_usable_default_and_other_startup_settings",
    "server::tests::transfers_tests::dispatch::outputs::rotation::real_monitor_controls_keep_four_rotations_pixel_input_popup_bar_and_capture_consistent_on_one_head_and_reload_the_saved_candidate",
    "server::tests::transfers_tests::dispatch::outputs::rotation::scaled_and_narrow_monitor_keyboard_controls_preserve_a_failed_save_and_revert_then_keep_and_recreate_the_accepted_rotation",
    "server::tests::transfers_tests::dispatch::outputs::rotation::queued_monitor_keep_cannot_save_after_real_deadline_composed_lock_or_last_head_replacement_and_fresh_input_and_capture_recover",
];
const OUTPUT_ROTATED_ACCESSIBILITY_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::magnification::platform::mixed::clockwise_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input",
    "server::tests::transfers_tests::dispatch::magnification::platform::mixed::inverted_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input",
    "server::tests::transfers_tests::dispatch::magnification::platform::mixed::counterclockwise_native_unicode_carets_ime_and_external_accessibility_bounds_match_rotated_capture_and_companion_input",
];
const OUTPUT_CONTROL_MODEL_TESTS: &[&str] = &[
    "server::tests::display_tests::modeled_hdr_confirmation_refuses_unobserved_mismatched_and_pending_keep_preserves_failed_save_and_reverts_or_reloads_confirmed_intent",
    "server::tests::display_tests::twenty_later_head_failures_reject_old_completions_and_restore_the_entire_multihead_configuration_once",
];
const OUTPUT_CONTROL_UI_TEST: &str = "native::layout::desktop::monitors::tests::hdr_and_vrr_controls_keep_requested_prepared_observed_and_saved_confirmation_separate_and_always_allow_revert";
const OUTPUT_CONTROL_CLIENT_TEST: &str = "server::tests::transfers_tests::dispatch::outputs::controls::ordinary_color_and_vrr_controls_preserve_rejected_drafts_without_saving_or_resizing_then_keep_and_reload_a_usable_sdr_desktop";
const OUTPUT_GENERATION_CLIENT_TEST: &str = "server::tests::transfers_tests::dispatch::outputs::lifetimes::twenty_color_generations_retire_output_and_cursor_readbacks_preserve_raw_window_sources_and_exclude_private_pixels_with_fresh_input";
const POWER_TRANSITION_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::power::a_real_power_peer_keeps_the_original_deadline_while_gpu_readback_and_ordinary_input_progress_then_a_fresh_authorized_peer_recovers",
    "server::tests::transfers_tests::dispatch::power::delayed_gpu_frame_owners_retire_across_lock_unlock_output_and_power_epochs_and_current_protected_pixels_and_input_recover",
    "server::tests::transfers_tests::dispatch::power::twenty_separately_authorized_power_peer_replacements_retire_requests_preserve_windows_and_bar_reservations_and_recover_capture_and_input",
    "server::tests::transfers_tests::dispatch::power::power_admission_during_scale_preview_and_color_change_keeps_lock_rollback_protected_pixels_original_deadlines_and_fresh_capture",
];
const POWER_PROTOCOL_TESTS: &[&str] = &[
    "server::tests::power_tests::output_power_requires_its_own_connection_authority_and_nested_outputs_report_unavailable",
    "server::tests::power_tests::output_power_reports_only_backend_observations_and_keeps_windows_and_output_configuration",
    "server::tests::power_tests::replacing_an_uncommitted_power_request_does_not_let_its_failure_poison_the_new_request",
    "server::tests::power_tests::destroying_the_manager_keeps_its_controls_valid_and_output_replacement_retires_the_old_target",
    "server::tests::power_tests::sleep_wake_and_helper_failure_cannot_release_the_session_lock",
    "server::tests::power_tests::disconnected_power_helpers_cannot_replay_a_captured_request_on_a_new_connection",
    "server::tests::power_tests::a_full_power_queue_fails_only_the_excess_control_and_keeps_requests_bounded",
    "server::tests::power_tests::malformed_power_modes_disconnect_only_the_offending_connection_and_fresh_clients_still_work",
    "server::tests::power_tests::control_object_pressure_has_a_bounded_connection_failure_and_does_not_starve_a_new_helper",
];
const SMITHAY_ATOMIC_TESTS: &[&str] = &[
    "backend::drm::surface::atomic::test::hdr_connector_values_and_the_matching_framebuffer_share_the_actual_atomic_request",
    "backend::drm::surface::atomic::test::removing_a_connector_does_not_reapply_its_pending_hdr_metadata",
];
const OUTPUT_COLOR_CLIENT_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::outputs::color::negotiated_float_and_ten_bit_wayland_clients_preserve_wide_gamut_precision_metadata_only_updates_alpha_and_companion_pixels_without_resizing",
    "server::tests::transfers_tests::dispatch::outputs::color::an_unimportable_advertised_precision_buffer_disconnects_only_its_client_and_fresh_pixels_input_and_resources_recover",
];
const OUTPUT_COLOR_NATIVE_TEST: &str = "presentation::color_tests::actual_native_camera_and_export_preserve_linear_values_above_sdr_white";
const OUTPUT_COLOR_GLES_TEST: &str = "desktop::renderer::tests::actual_linear_alpha_and_final_hdr_encoding_preserve_precision_and_use_the_declared_transfer_once";
const OUTPUT_COLOR_DMABUF_TESTS: &[&str] = &[
    "graphics::import::tests::half_float_export_and_reimport_preserve_values_above_sdr_white",
    "graphics::import::tests::packed_ten_bit_export_and_reimport_preserve_channel_positions_and_all_precision",
];
const APPEARANCE_LOWER_TESTS: &[&str] = &[
    "native::shader_effects::compositor::tests::appearance_fields_apply_independently_preserve_invalid_and_conflicting_drafts_and_cancel_explicitly",
    "native::shader_effects::compositor::tests::replaced_canvas_and_locked_controls_cannot_apply_an_older_appearance_action",
    "native::shader_effects::compositor::tests::reduced_motion_and_protection_remove_only_the_transient_fade_without_rewriting_preferences",
    "native::shader_effects::compositor::tests::appearance_status_reports_pending_and_limited_effects_without_rewriting_saved_settings",
    "native::shader_effects::compositor::tests::appearance_toggles_refresh_visible_and_accessible_state_without_replacing_numeric_drafts",
    "native::topology::compositor::tests::shadow_uses_the_actual_triangle_boundary_and_expands_it_without_mutating_source_geometry",
    "native::topology::compositor::tests::clipped_holes_keep_both_boundaries_while_nonfinite_and_excess_geometry_is_refused",
    "native::topology::compositor::tests::optional_shadow_obeys_existing_budget_and_owner_layers_then_releases_with_its_surface",
    "native::topology::compositor::blur::tests::backdrop_readiness_is_bound_to_the_current_target_viewport_and_request_lifetime",
    "native::topology::compositor::blur::draw::tests::unsupported_views_keep_every_original_item_and_restore_only_suppressed_quads_in_order",
    "native::topology::compositor::blur::draw::tests::admitted_phase_restores_draw_functions_and_retained_items_without_changing_another_view",
    "native::topology::stacking::tests::decorations_stay_with_their_actual_face_without_reordering_unrelated_transparent_content",
    "native::workspace::tests::compositor_appearance_reloads_from_the_workspace_and_invalid_saves_keep_the_last_good_document",
];
const APPEARANCE_RENDER_TESTS: &[&str] = &[
    "presentation::tests::appearance::rendered_appearance_fields_apply_cancel_and_reload_through_real_workspace_startup",
    "presentation::tests::appearance::actual_backdrop_blur_changes_translucent_pixels_preserves_sharp_content_and_excludes_private_sources",
    "presentation::tests::appearance::twenty_real_gpu_appearance_cycles_retire_closed_items_and_preserve_bounded_resources",
    "presentation::tests::appearance::reduced_motion_protection_and_last_output_loss_retire_effects_without_losing_companion_content_or_preferences",
    "presentation::tests::appearance::geometry::moving_follow_uses_the_current_rendered_target_for_120_single_frames_with_appearance_enabled",
    "presentation::tests::appearance::reference::backdrop_filter_matches_independent_linear_premultiplied_rgba_at_multiple_output_scales",
    "presentation::tests::appearance::geometry::workspace_hole_effects_keep_actual_clipped_pixels_and_clicks_at_fractional_scales",
    "presentation::tests::appearance::transmission::real_glass_transmission_keeps_its_texture_and_sharp_foreground_when_backdrop_blur_is_enabled",
    "presentation::tests::appearance::reference::shifted_render_viewport_clamps_backdrop_taps_and_replaces_its_previous_viewport_request",
    "presentation::tests::appearance::utilities::rendered_compact_choices_and_scratch_visibility_retire_effects_and_exclude_private_members",
];
const APPEARANCE_CLIENT_TEST: &str = "server::tests::transfers_tests::dispatch::appearance::real_wayland_client_keeps_sharp_content_popup_pixels_and_input_across_appearance_in_flat_spatial_free_and_tiled_scenes";
const APPEARANCE_COST_TEST: &str = "presentation::tests::appearance_cost::paired_appearance_cost_records_three_warmed_release_comparisons_on_one_and_two_outputs";
const APPEARANCE_CAPTURE_TESTS: &[&str] = &[
    "server::tests::transfers_tests::dispatch::capture_lifetime::queued_effect_capture_revalidates_privacy_source_revocation_lock_and_deadline_before_writing_shm",
    "server::tests::transfers_tests::dispatch::capture_lifetime::direct_window_capture_keeps_its_raw_alpha_and_retired_source_cannot_retarget_another_client",
    "server::tests::transfers_tests::dispatch::capture_lifetime::queued_group_and_scratch_privacy_changes_retire_old_pixels_and_keep_real_client_focus_and_public_updates",
    "server::tests::transfers_tests::dispatch::capture_lifetime::queued_capture_survives_only_matching_output_mappings_and_last_output_recovery_keeps_protection_and_clients",
    "server::tests::transfers_tests::dispatch::capture_lifetime::queued_readback_pause_keeps_shm_and_timers_quiet_then_resumes_or_rejects_changed_privacy",
];

static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn check_interruption() -> Result<()> {
    if INTERRUPTED.load(std::sync::atomic::Ordering::Relaxed) {
        Err("Validation interrupted; the session was not accepted as an orderly logout".into())
    } else {
        Ok(())
    }
}

fn interrupt_handler() -> Result<tokio::runtime::Runtime> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .map_err(error)?;
    runtime
        .block_on(async {
            let mut interrupt =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            tokio::spawn(async move {
                tokio::select! { _ = interrupt.recv() => {}, _ = terminate.recv() => {} }
                INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            Ok::<_, std::io::Error>(())
        })
        .map_err(error)?;
    Ok(runtime)
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Program {
    program: PathBuf,
    #[serde(default)]
    arguments: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Helper {
    target: Program,
    capabilities: Vec<String>,
}

impl Helper {
    fn valid(&self) -> bool {
        !self.capabilities.is_empty()
            && self.capabilities.len() <= 8
            && self
                .capabilities
                .iter()
                .enumerate()
                .all(|(index, capability)| {
                    matches!(
                        capability.as_str(),
                        "clipboard"
                            | "window-list"
                            | "window-control"
                            | "output-control"
                            | "capture"
                            | "input-method"
                            | "desktop-control"
                            | "launch"
                    ) && !self.capabilities[..index].contains(capability)
                })
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Configuration {
    terminal: Program,
    #[serde(default)]
    launcher: Option<Program>,
    #[serde(default)]
    lock: Option<Program>,
    #[serde(default)]
    toolkit_qml: Option<Program>,
    #[serde(default)]
    showoff: bool,
    #[serde(default)]
    clients: Vec<Program>,
    #[serde(default)]
    helper: Option<Helper>,
    #[serde(default)]
    automation: bool,
    #[serde(default = "one")]
    lifecycle_rounds: u32,
    #[serde(default = "observation_seconds")]
    observation_seconds: u64,
    #[serde(default)]
    soak_seconds: u64,
}

fn one() -> u32 {
    1
}
fn observation_seconds() -> u64 {
    90
}

impl Configuration {
    fn valid_duration(&self) -> bool {
        (1..=20).contains(&self.lifecycle_rounds)
            && (15..=300).contains(&self.observation_seconds)
            && self.soak_seconds <= 3600
    }

    fn session_seconds(&self) -> u64 {
        90 + u64::from(self.lifecycle_rounds) * 55 + self.observation_seconds + self.soak_seconds + if self.automation { 150 } else { 0 }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Run {
    repository: PathBuf,
    directory: PathBuf,
    executable_sha256: String,
    process_executable: PathBuf,
    process_sha256: String,
    build_info: serde_json::Value,
    installed: bool,
    probe_sha256: String,
    capture_sha256: String,
    automation: automation::Prepared,
    revision: String,
    port: u16,
    peer_port: u16,
    configuration: Configuration,
    recovery: String,
}

#[derive(Deserialize, Serialize)]
struct Phase {
    name: String,
    outcome: String,
    elapsed_ms: u128,
    detail: String,
}

#[derive(Serialize)]
struct Results {
    phases: Vec<Phase>,
    observations: BTreeMap<String, String>,
    resources: Vec<ResourceSample>,
}

#[derive(Serialize)]
struct ResourceSample {
    elapsed_ms: u128,
    resident_kib: Option<u64>,
    peak_resident_kib: Option<u64>,
    threads: Option<u64>,
    descriptors: Option<usize>,
    cpu_ticks: Option<u64>,
    drm_clients: Option<BTreeMap<String, BTreeMap<String, String>>>,
}

fn resource_sample(pid: rustix::process::Pid, elapsed_ms: u128) -> ResourceSample {
    let base = PathBuf::from(format!("/proc/{}", pid.as_raw_nonzero()));
    let status = fs::read_to_string(base.join("status")).unwrap_or_default();
    let field = |key: &str| {
        status.lines().find_map(|line| {
            line.strip_prefix(key)?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
    };
    let stat = fs::read_to_string(base.join("stat")).unwrap_or_default();
    let cpu_ticks = stat.rsplit_once(") ").and_then(|(_, fields)| {
        let values: Vec<_> = fields.split_whitespace().collect();
        values
            .get(11)?
            .parse::<u64>()
            .ok()?
            .checked_add(values.get(12)?.parse::<u64>().ok()?)
    });
    let mut drm_clients = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(base.join("fdinfo")) {
        for entry in entries.flatten() {
            let Ok(contents) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let fields: BTreeMap<String, String> = contents
                .lines()
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.starts_with("drm-")
                        .then(|| (name.to_owned(), value.trim().to_owned()))
                })
                .collect();
            let Some(client) = fields.get("drm-client-id") else {
                continue;
            };
            let device = fields
                .get("drm-pdev")
                .cloned()
                .or_else(|| {
                    fs::read_link(base.join("fd").join(entry.file_name()))
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
            drm_clients.insert(format!("{device}:{client}"), fields);
        }
    }
    ResourceSample {
        elapsed_ms,
        resident_kib: field("VmRSS:"),
        peak_resident_kib: field("VmHWM:"),
        threads: field("Threads:"),
        descriptors: fs::read_dir(base.join("fd"))
            .ok()
            .map(|entries| entries.filter_map(std::result::Result::ok).count()),
        cpu_ticks,
        drm_clients: (!drm_clients.is_empty()).then_some(drm_clients),
    }
}

fn resource_summary(samples: &[ResourceSample]) -> String {
    let Some(first) = samples.first() else {
        return "Resource samples: unavailable\n".into();
    };
    let Some(last) = samples.last() else {
        return "Resource samples: unavailable\n".into();
    };
    let difference = |first: Option<u64>, last: Option<u64>| {
        first
            .zip(last)
            .map(|(first, last)| (i128::from(last) - i128::from(first)).to_string())
            .unwrap_or_else(|| "unavailable".into())
    };
    let descriptors = first
        .descriptors
        .zip(last.descriptors)
        .map(|(first, last)| (last as i128 - first as i128).to_string())
        .unwrap_or_else(|| "unavailable".into());
    format!(
        "Resource samples: {} across {} ms; RSS delta {} KiB; descriptor delta {}; CPU tick delta {}; DRM client counters available in {} samples. Measurements require interpretation, not a stability pass.\n",
        samples.len(),
        last.elapsed_ms.saturating_sub(first.elapsed_ms),
        difference(first.resident_kib, last.resident_kib),
        descriptors,
        difference(first.cpu_ticks, last.cpu_ticks),
        samples
            .iter()
            .filter(|sample| sample.drm_clients.is_some())
            .count(),
    )
}

pub(super) fn dispatch(arguments: &[OsString]) -> Result<()> {
    match arguments.first().and_then(|argument| argument.to_str()) {
        Some("analyze") => metrics::analyze(&arguments[1..]),
        Some("browser") => browser::run(&arguments[1..]),
        Some("observe") => observe::run(&arguments[1..]),
        Some("prepare") if arguments.len() == 2 => prepare(Path::new(&arguments[1]), None),
        Some("prepare") if arguments.len() == 4 && arguments[1] == "--executable" => {
            prepare(Path::new(&arguments[3]), Some(Path::new(&arguments[2])))
        }
        Some("seat") if arguments.len() == 2 => seat(Path::new(&arguments[1])),
        Some("ready") if arguments.len() == 2 => ready(Path::new(&arguments[1])),
        Some("report") if arguments.len() == 2 => {
            let path = Path::new(&arguments[1]).join("summary.txt");
            print!("{}", fs::read_to_string(path).map_err(error)?);
            Ok(())
        }
        None | Some("help" | "--help") => {
            println!(
                "cargo xtask park validate prepare [--executable /installed/bin/lince] /absolute/path/to/fixtures.json"
            );
            println!("cargo xtask park validate report /absolute/path/to/run");
            println!(
                "cargo xtask park validate observe /installed/bin/lince PID [--seconds 5400] [--unit USER_UNIT]"
            );
            println!(
                "cargo xtask park validate browser /installed/bin/lince --geckodriver PATH [--plain] [--provider FIXTURE.json] [--host-close PROGRAM] [--host-prepare PROGRAM] [--ownership] [--save-failure] [--reopen-sandbox] [--lifecycle-rounds 0] [--fullscreen] [--seconds 75] [--outputs 1]"
            );
            println!(
                "cargo xtask park validate analyze LOG [--warmup 10] [--seconds 60] [--minimum-fps 57] [--surface ID]"
            );
            println!(
                "Fixtures: {{\"terminal\":{{\"program\":\"xdg-terminal-exec\",\"arguments\":[]}},\"showoff\":false,\"clients\":[]}}"
            );
            println!("Use automation:true in a separate private fixture for the authorized physical-session SDK probe; preserve a selected provider helper in its own fixture. Preparation always verifies the selected optimized candidate's nested automation path.");
            println!(
                "Preparation checks and compiles before emitting one physical-session command."
            );
            Ok(())
        }
        _ => Err("Use cargo xtask park validate help".into()),
    }
}

fn error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(error)?;
    bytes.push(b'\n');
    let temporary = path.with_extension("pending");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(error)?;
    file.write_all(&bytes).map_err(error)?;
    file.sync_all().map_err(error)?;
    fs::rename(temporary, path).map_err(error)
}

fn load<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = fs::symlink_metadata(path).map_err(error)?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(format!(
            "Invalid or oversized test configuration: {}",
            path.display()
        ));
    }
    serde_json::from_reader(File::open(path).map_err(error)?).map_err(error)
}

fn executable(program: &Path) -> Result<PathBuf> {
    let candidate = if program.components().count() > 1 {
        program.to_path_buf()
    } else {
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(program))
            .find(|path| path.is_file())
            .ok_or_else(|| format!("Missing executable: {}", program.display()))?
    };
    let name = candidate
        .file_name()
        .ok_or("Executable path has no file name")?;
    let parent = candidate
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let candidate = fs::canonicalize(parent).map_err(error)?.join(name);
    let metadata = fs::metadata(&candidate).map_err(error)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!("Not executable: {}", candidate.display()));
    }
    Ok(candidate)
}

fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(error)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(error)?;
        if count == 0 {
            return Ok(hash
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect());
        }
        hash.update(&buffer[..count]);
    }
}

fn bounded(command: &mut Command, deadline: Duration) -> Result<(ExitStatus, u128)> {
    let start = Instant::now();
    let mut child = command.spawn().map_err(error)?;
    loop {
        if let Err(error) = check_interruption() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        if let Some(status) = child.try_wait().map_err(error)? {
            return Ok((status, start.elapsed().as_millis()));
        }
        if start.elapsed() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("Phase exceeded {} seconds", deadline.as_secs()));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn logged(command: &mut Command, path: &Path) -> Result<()> {
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)
        .map_err(error)?;
    command
        .stdout(file.try_clone().map_err(error)?)
        .stderr(file);
    Ok(())
}

fn preparation_step(directory: &Path, name: &str, arguments: &[&str]) -> Result<()> {
    preparation_step_with_environment(directory, name, arguments, &[])
}

fn copy_dependency_source(source: &Path, destination: &Path, relative: &Path, hashes: &mut BTreeMap<String, String>) -> Result<()> {
    fs::create_dir(destination).map_err(error)?;
    let entries = fs::read_dir(source).map_err(error)?.collect::<std::io::Result<Vec<_>>>().map_err(error)?;
    for entry in entries {
        let name = entry.file_name();
        if matches!(name.to_str(), Some("target" | ".git" | "Cargo.lock")) { continue; }
        let kind = entry.file_type().map_err(error)?;
        let target = destination.join(&name);
        let relative = relative.join(&name);
        if kind.is_dir() {
            copy_dependency_source(&entry.path(), &target, &relative, hashes)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target).map_err(error)?;
            hashes.insert(relative.to_str().ok_or("Dependency source paths must be UTF-8")?.to_owned(), digest(&target)?);
        } else {
            return Err(format!("Refusing a nonregular dependency source: {}", entry.path().display()));
        }
    }
    Ok(())
}

fn prepare_smithay_source(directory: &Path) -> Result<String> {
    let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).map_err(error)?;
    let destination = directory.join("smithay-dependency");
    let mut hashes = BTreeMap::new();
    copy_dependency_source(&root.join("crates/vendor/smithay"), &destination, Path::new(""), &mut hashes)?;
    isolate_dependency_manifest(&destination.join("Cargo.toml"))?;
    fs::copy(root.join("Cargo.lock"), destination.join("Cargo.lock")).map_err(error)?;
    write_json(&directory.join("smithay-source.json"), &serde_json::json!({
        "source": root.join("crates/vendor/smithay"), "copy": destination,
        "files_sha256": hashes, "seed_lock_sha256": digest(&root.join("Cargo.lock"))?,
        "prepared_manifest_sha256": digest(&destination.join("Cargo.toml"))?,
        "scope": "dependency atomic request unit fixtures; no kernel submission or physical acceptance",
    }))?;
    destination.join("Cargo.toml").to_str().map(str::to_owned).ok_or_else(|| "Dependency manifest must be UTF-8".into())
}

fn isolate_dependency_manifest(path: &Path) -> Result<()> {
    let mut manifest = fs::read(path).map_err(error)?;
    manifest.extend_from_slice(b"\n[workspace]\n");
    fs::write(path, manifest).map_err(error)
}

fn preparation_step_with_environment(
    directory: &Path,
    name: &str,
    arguments: &[&str],
    environment: &[(&str, OsString)],
) -> Result<()> {
    let mut command = cargo_command();
    command
        .args(arguments)
        .envs(environment.iter().map(|(name, value)| (*name, value)))
        .env("RUST_MIN_STACK", "33554432")
        .env(
            "LINCE_PARK_TEST_ARTIFACTS",
            directory.join("native-captures"),
        );
    deny_warnings(&mut command);
    logged(&mut command, &directory.join(format!("{name}.log")))?;
    let (status, _) = bounded(&mut command, Duration::from_secs(3600))?;
    if !status.success() {
        return Err(format!(
            "{name} failed: {status}; see {}",
            directory.display()
        ));
    }
    let required: &[&str] = match name {
        "quiet-inspector-recovery" => &[QUIET_INSPECTOR_TEST],
        "record-permission-recovery" => &[PERMISSION_INSPECTOR_TEST],
        "native-input-protocol" => &[POINTER_DRAG_CANCEL_TEST],
        "file-hover-lifetime" => FILE_HOVER_LIFETIME_TESTS,
        "file-hover-scroll" => &[FILE_HOVER_SCROLL_TEST],
        "workspace-history" => &[WORKSPACE_HISTORY_TEST],
        "output-affinity-recovery" => &[OUTPUT_AFFINITY_TEST],
        "focus-policy-settings" => &[FOCUS_POLICY_SETTINGS_TEST],
        "focus-policy-clients" => &[FOCUS_POLICY_CLIENT_TEST],
        "focus-policy-motion" => &[FOCUS_POLICY_MOTION_TEST],
        "opening-rule-workflow" => &[OPENING_RULE_WORKFLOW_TEST],
        "opening-rule-readiness" => &[OPENING_RULE_READINESS_TEST],
        "configuration-review-rendering" => CONFIGURATION_RENDER_TESTS,
        "configuration-review-input" => CONFIGURATION_INPUT_TESTS,
        "capture-settings" => CAPTURE_SETTINGS_TESTS,
        "capture-rendering" => CAPTURE_RENDER_TESTS,
        "capture-client" => &[CAPTURE_CLIENT_TEST],
        "capture-lifetime" => CAPTURE_LIFETIME_TESTS,
        "capture-retained-pixels" => CAPTURE_RETAINED_TESTS,
        "capture-retained-protocol" => CAPTURE_RETAINED_PROTOCOL_TESTS,
        "output-rotated-accessibility" => OUTPUT_ROTATED_ACCESSIBILITY_TESTS,
        "output-control-model" => OUTPUT_CONTROL_MODEL_TESTS,
        "output-control-ui" => &[OUTPUT_CONTROL_UI_TEST],
        "output-control-clients" => &[OUTPUT_CONTROL_CLIENT_TEST],
        "output-generation-clients" => &[OUTPUT_GENERATION_CLIENT_TEST],
        "power-transition-workflows" => POWER_TRANSITION_TESTS,
        "power-protocol-lifetimes" => POWER_PROTOCOL_TESTS,
        "smithay-atomic-debug" | "smithay-atomic-release" => SMITHAY_ATOMIC_TESTS,
        "windowed-fullscreen-clients" => WINDOWED_FULLSCREEN_CLIENT_TESTS,
        "compact-group-clients" => COMPACT_CLIENT_TESTS,
        "scratch-workflows" => SCRATCH_WORKFLOW_TESTS,
        "configuration-quiet-reload" => &[CONFIGURATION_QUIET_TEST],
        "keyboard-shortcuts" => CONFIGURATION_POLICY_TESTS,
        "input-settings-policy" => CONFIGURATION_ACTIVITY_TESTS,
        "automation-protocol" => AUTOMATION_PROTOCOL_TESTS,
        "automation-input" => AUTOMATION_INPUT_TESTS,
        "automation-settings" => &[AUTOMATION_RENDER_TEST],
        "validation-tooling" => AUTOMATION_REPORT_TESTS,
        "absolute-input-ordering" => ABSOLUTE_ORDER_TESTS,
        "tablet-protocol-lifetimes" => &[TABLET_LIFETIME_TEST],
        "logout-recovery" => &[LOGOUT_RECOVERY_TEST],
        "native-pointer-ownership" => NATIVE_POINTER_OWNERSHIP_TESTS,
        "accessibility-platform" => ACCESSIBILITY_PLATFORM_TESTS,
        "magnification-settings" => &[MAGNIFICATION_SETTINGS_TEST],
        "magnification-client" => &[MAGNIFICATION_CLIENT_TEST],
        "accessibility-protection" => &[ACCESSIBILITY_PROTECTION_TEST],
        "accessibility-mixed" => &[ACCESSIBILITY_MIXED_TEST],
        "magnification-input" => &[MAGNIFICATION_INPUT_TEST],
        "magnification-reveal" => &[MAGNIFICATION_REVEAL_TEST],
        "magnification-drop" => &[MAGNIFICATION_DROP_TEST],
        "magnification-caret" => &[MAGNIFICATION_CARET_TEST],
        "magnification-text-input" => MAGNIFICATION_TEXT_INPUT_TESTS,
        "output-rotation-workflows" => OUTPUT_ROTATION_TESTS,
        "output-color-client-workflows" => OUTPUT_COLOR_CLIENT_TESTS,
        "output-color-native-pixels" => &[OUTPUT_COLOR_NATIVE_TEST],
        "output-color-gles-pixels" => &[OUTPUT_COLOR_GLES_TEST],
        "output-color-dmabuf-pixels" => OUTPUT_COLOR_DMABUF_TESTS,
        "window-layout-policy" => APPEARANCE_LOWER_TESTS,
        "compositor-appearance" => APPEARANCE_RENDER_TESTS,
        "compositor-appearance-client" => &[APPEARANCE_CLIENT_TEST],
        "compositor-appearance-capture" => APPEARANCE_CAPTURE_TESTS,
        "compositor-appearance-cost" => &[APPEARANCE_COST_TEST],
        _ => &[],
    };
    let groups = [required,
        if name == "window-layout-policy" { WORKSPACE_HISTORY_LOWER_TESTS } else { &[] },
        if name == "window-layout-policy" { OUTPUT_AFFINITY_LOWER_TESTS } else { &[] },
        if name == "window-layout-policy" { FOCUS_POLICY_LOWER_TESTS } else { &[] },
        if name == "window-layout-policy" { OPENING_LIFETIME_LOWER_TESTS } else { &[] },
        if name == "window-layout-policy" { AUTOMATION_MODEL_TESTS } else { &[] },
        if name == "window-layout-policy" { CAPTURE_ALIAS_POLICY_TESTS } else { &[] },
        if name == "window-layout-policy" { WINDOWED_FULLSCREEN_MODEL_TESTS } else { &[] },
        if name == "window-layout-policy" { COMPACT_MODEL_TESTS } else { &[] },
        if name == "window-layout-policy" { SCRATCH_MODEL_TESTS } else { &[] },
        if name == "window-layout-policy" { ACCESSIBILITY_CONTEXT_TESTS } else { &[] },
        if name == "keyboard-shortcuts" { &[WINDOWED_FULLSCREEN_CONFIGURATION_TEST] } else { &[] }];
    if groups.iter().any(|group| !group.is_empty()) {
        let log = fs::read_to_string(directory.join(format!("{name}.log"))).map_err(error)?;
        for required in groups.into_iter().flatten() { inspector_result(&log, required)?; }
    }
    Ok(())
}

fn inspector_result(log: &str, required: &str) -> Result<()> {
    let expected = format!("test {required} ... ok");
    if log.lines().any(|line| line == expected) {
        Ok(())
    } else {
        Err(format!(
            "The required test did not report a pass: {required}"
        ))
    }
}

fn prepare_toolkit(directory: &Path, program: Option<&Program>, installed: bool) -> Result<()> {
    let report = directory.join("toolkit-interaction.json");
    let reason = if installed {
        Some("The source toolkit fixture does not validate the selected installed executable.")
    } else if program.is_none() {
        Some("No external Qt 6.8+ qml runner was selected in toolkit_qml.")
    } else {
        None
    };
    if let Some(reason) = reason {
        return write_json(
            &report,
            &serde_json::json!({ "status": "unrun", "reason": reason }),
        );
    }
    let program = program.ok_or("Missing toolkit runner")?;
    let result = preparation_step_with_environment(
        directory,
        "toolkit-interaction-rendering",
        &[
            "test",
            "--release",
            "--locked",
            "-p",
            "lince-park",
            "--features",
            super::INPUT_FEATURES,
            "--lib",
            "server::tests::manipulation_tests::tiled::toolkit::",
            "--",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ],
        &[
            ("LINCE_PARK_TEST_QML", program.program.as_os_str().into()),
            (
                "LINCE_PARK_TEST_QML_ARGUMENTS",
                serde_json::to_string(&program.arguments)
                    .map_err(error)?
                    .into(),
            ),
        ],
    )
    .and_then(|()| {
        let log = fs::read_to_string(directory.join("toolkit-interaction-rendering.log"))
            .map_err(error)?;
        toolkit_results(&log)
    });
    write_json(
        &report,
        &serde_json::json!({
            "status": if result.is_ok() { "passed" } else { "failed" },
            "scope": "source-fixture",
            "runner": program.program,
            "error": result.as_ref().err(),
        }),
    )?;
    result
}

fn toolkit_results(log: &str) -> Result<()> {
    for case in [
        "flat_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
        "spatial_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
        "properties::flat_qt_properties_keep_the_original_record_through_focus_and_close",
        "properties::spatial_qt_properties_keep_the_original_record_through_focus_and_close",
        "properties::flat_free_qt_properties_keep_the_original_record_through_focus_and_close",
        "properties::spatial_free_qt_properties_keep_the_original_record_through_focus_and_close",
        "properties::unavailable::flat_free_qt_deleted_record_keeps_its_draft_and_refresh_target",
        "properties::unavailable::spatial_tiled_qt_deleted_record_keeps_its_draft_and_refresh_target",
        "properties::creation::flat_free_qt_creation_retry_keeps_its_pending_inspector_after_workspace_change_and_close",
    ] {
        let expected =
            format!("test server::tests::manipulation_tests::tiled::toolkit::{case} ... ok");
        if !log.lines().any(|line| line.trim_end() == expected.as_str()) {
            return Err(format!("The {case} Qt toolkit case did not report a pass"));
        }
    }
    Ok(())
}

fn prepare_capture_probe(directory: &Path) -> Result<String> {
    let mut command = cargo_command();
    command.args([
        "run",
        "--release",
        "--locked",
        "--message-format=json",
        "-p",
        "lince-park",
        "--features",
        "server-tests",
        "--example",
        "capture_probe",
        "--",
        "--help",
    ]);
    deny_warnings(&mut command);
    let output = command.output().map_err(error)?;
    fs::write(directory.join("prepare-capture-probe.log"), &output.stderr).map_err(error)?;
    if !output.status.success() {
        return Err("Preparing the capture probe failed".into());
    }
    let binary = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|value| {
            value["reason"] == "compiler-artifact" && value["target"]["name"] == "capture_probe"
        })
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
        .last()
        .ok_or("Cargo did not report the capture probe executable")?;
    let target = directory.join("bin/capture-probe");
    fs::copy(binary, &target).map_err(error)?;
    digest(&target)
}

fn deny_warnings(command: &mut Command) {
    command.env("RUSTFLAGS", super::warning_flags());
}

pub(super) fn inspect_build(executable: &Path) -> Result<serde_json::Value> {
    let output = Command::new("timeout")
        .args(["--kill-after=1s", "10s"])
        .arg(executable)
        .arg("--build-info")
        .output()
        .map_err(error)?;
    if !output.status.success() {
        return Err("The selected executable cannot report its build information".into());
    }
    let information: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(error)?;
    validate_build(&information)?;
    Ok(information)
}

fn validate_build(information: &serde_json::Value) -> Result<()> {
    if information["features"]["park"] != true
        || information["features"]["ui"] != true
        || information["optimized"] != true
        || !information["target"]
            .as_str()
            .is_some_and(|target| target.ends_with("-linux-gnu"))
        || !["version", "revision"].iter().all(|key| {
            information[key]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        })
    {
        return Err("The selected executable must report an optimized Linux build with UI and Park support, a version and revision".into());
    }
    Ok(())
}

fn process_executable(executable: &Path) -> Result<PathBuf> {
    let executable = fs::canonicalize(executable).map_err(error)?;
    let payload = executable
        .parent()
        .ok_or("Executable has no parent")?
        .join(".lince-wrapped");
    let executable = if payload.is_file() {
        fs::canonicalize(payload).map_err(error)?
    } else {
        executable
    };
    let mut magic = [0; 4];
    File::open(&executable)
        .map_err(error)?
        .read_exact(&mut magic)
        .map_err(error)?;
    if magic == *b"\x7fELF" {
        return Ok(executable);
    }
    Err("Cannot identify the selected wrapper's compositor payload".into())
}

fn prepare(configuration: &Path, installed: Option<&Path>) -> Result<()> {
    let selected = installed
        .map(|path| -> Result<PathBuf> {
            if !path.is_absolute() {
                return Err("--executable requires an absolute installed path".into());
            }
            let path = executable(path)?;
            inspect_build(&path)?;
            Ok(path)
        })
        .transpose()?;
    if !configuration.is_absolute() {
        return Err("Use an absolute fixtures path".into());
    }
    let mut configuration: Configuration = load(configuration)?;
    automation::configuration(&configuration)?;
    if !configuration.valid_duration() {
        return Err(
            "Use 1–20 lifecycle rounds, 15–300 observation seconds and at most 3600 soak seconds"
                .into(),
        );
    }
    if configuration
        .helper
        .as_ref()
        .is_some_and(|helper| !helper.valid())
    {
        return Err("Choose explicit, distinct supported helper capabilities".into());
    }
    if configuration.clients.len() > 16 {
        return Err("At most sixteen test clients are supported".into());
    }
    for program in std::iter::once(&mut configuration.terminal)
        .chain(&mut configuration.clients)
        .chain(
            configuration
                .helper
                .iter_mut()
                .map(|helper| &mut helper.target),
        )
        .chain(configuration.launcher.iter_mut())
        .chain(configuration.lock.iter_mut())
        .chain(configuration.toolkit_qml.iter_mut())
    {
        program.program = executable(&program.program)?;
        if program.arguments.len() > 128
            || program
                .arguments
                .iter()
                .any(|argument| argument.len() > 8192 || argument.contains('\0'))
        {
            return Err("Invalid fixture arguments".into());
        }
    }
    let nix = executable(Path::new("nix"))?;
    let timeout = executable(Path::new("timeout"))?;
    executable(Path::new("loginctl"))?;
    executable(Path::new("dbus-run-session"))?;
    executable(Path::new("dbus-update-activation-environment"))?;
    let repository = env::current_dir().map_err(error)?;
    let base = repository.join("target/park-validation");
    fs::create_dir_all(&base).map_err(error)?;
    let metadata = fs::symlink_metadata(&base).map_err(error)?;
    if !metadata.is_dir() || metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err("Validation state must be an owned directory, not a symlink".into());
    }
    fs::set_permissions(&base, fs::Permissions::from_mode(0o700)).map_err(error)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(error)?
        .as_nanos();
    let directory = base.join(format!("run-{stamp}-{}", std::process::id()));
    fs::create_dir(&directory).map_err(error)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).map_err(error)?;
    for name in ["data", "bin"] {
        fs::create_dir(directory.join(name)).map_err(error)?;
    }
    let profiles = directory.join("profiles");
    fs::create_dir(&profiles).map_err(error)?;
    for (index, program) in std::iter::once(&mut configuration.terminal)
        .chain(&mut configuration.clients)
        .chain(
            configuration
                .helper
                .iter_mut()
                .map(|helper| &mut helper.target),
        )
        .chain(configuration.launcher.iter_mut())
        .chain(configuration.lock.iter_mut())
        .chain(configuration.toolkit_qml.iter_mut())
        .enumerate()
    {
        let profile = profiles.join(format!("client-{index}"));
        fs::create_dir(&profile).map_err(error)?;
        let run_path = directory.to_str().ok_or("Test directory must be UTF-8")?;
        let profile_path = profile.to_str().ok_or("Test profile must be UTF-8")?;
        for argument in &mut program.arguments {
            *argument = argument
                .replace("{run}", run_path)
                .replace("{profile}", profile_path);
        }
    }
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(error)?;
    if !revision.status.success() {
        return Err("Cannot record source revision".into());
    }
    fs::write(directory.join("source-revision.txt"), revision.stdout).map_err(error)?;
    let mut session = Command::new("loginctl");
    session.args(["list-sessions", "--no-pager"]);
    logged(&mut session, &directory.join("sessions-before.log"))?;
    let _ = bounded(&mut session, Duration::from_secs(10))?;
    if selected.is_none() {
        let smithay_manifest = prepare_smithay_source(&directory)?;
        for (name, arguments) in [
            (
                "smithay-atomic-lock",
                vec!["metadata", "--format-version", "1", "--manifest-path", smithay_manifest.as_str(), "--no-default-features",
                    "--features", "backend_drm,renderer_gl,wayland_frontend,desktop"],
            ),
            (
                "smithay-atomic-debug",
                vec!["test", "--locked", "--manifest-path", smithay_manifest.as_str(), "--no-default-features",
                    "--features", "backend_drm,renderer_gl,wayland_frontend,desktop", "--lib", "backend::drm::surface::atomic::test::", "--", "--test-threads=1"],
            ),
            (
                "smithay-atomic-release",
                vec!["test", "--locked", "--release", "--manifest-path", smithay_manifest.as_str(), "--no-default-features",
                    "--features", "backend_drm,renderer_gl,wayland_frontend,desktop", "--lib", "backend::drm::surface::atomic::test::", "--", "--test-threads=1"],
            ),
            (
                "validation-tooling",
                vec!["test", "--locked", "-p", "xtask", "park::validation"],
            ),
            (
                "check-session",
                vec!["check", "--locked", "-p", "lince", "--features", "park"],
            ),
            ("check-sandbox", vec!["check", "--locked", "-p", "lince"]),
            (
                "protocol",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "server-tests",
                    "--lib",
                ],
            ),
            (
                "native-input-protocol",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "server::tests",
                ],
            ),
            (
                "native-input-editor",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "native_input::",
                ],
            ),
            (
                "capture-buffer-layout",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "desktop::renderer::capture::tests",
                ],
            ),
            (
                "capture-gpu-export",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "capture_gpu_window_export",
                    "--",
                    "--ignored",
                ],
            ),
            (
                "connected-cell-startup",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "connected_cell_is_attached_before_render_plugins_finish",
                    "--",
                    "--ignored",
                ],
            ),
            (
                "quiet-inspector-recovery",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    QUIET_INSPECTOR_TEST,
                    "--",
                    "--ignored",
                    "--exact",
                    "--test-threads=1",
                ],
            ),
            (
                "record-permission-recovery",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    PERMISSION_INSPECTOR_TEST,
                    "--",
                    "--ignored",
                    "--exact",
                    "--test-threads=1",
                ],
            ),
            (
                "file-hover-lifetime",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::hover_lifetime::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "file-hover-scroll",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", FILE_HOVER_SCROLL_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "workspace-history",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", WORKSPACE_HISTORY_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "output-affinity-recovery",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", OUTPUT_AFFINITY_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "focus-policy-settings",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", FOCUS_POLICY_SETTINGS_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "focus-policy-clients",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", FOCUS_POLICY_CLIENT_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "focus-policy-motion",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", FOCUS_POLICY_MOTION_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "opening-rule-workflow",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", OPENING_RULE_WORKFLOW_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "opening-rule-readiness",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", OPENING_RULE_READINESS_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "automation-protocol",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", "server-tests", "--lib", "--",
                    "server::control::flow::tests::", "server::tests::control_tests::", "--test-threads=1",
                ],
            ),
            (
                "automation-input",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::control::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "automation-settings",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", AUTOMATION_RENDER_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "native-pointer-ownership",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::native_pointer::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "accessibility-platform",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", "nested,server-tests,accessibility-gate",
                    "--lib", "presentation::accessibility::platform::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "magnification-settings",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", MAGNIFICATION_SETTINGS_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "magnification-client",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", MAGNIFICATION_CLIENT_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "accessibility-protection",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", "nested,server-tests,accessibility-gate",
                    "--lib", ACCESSIBILITY_PROTECTION_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "accessibility-mixed",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", "nested,server-tests,accessibility-gate",
                    "--lib", ACCESSIBILITY_MIXED_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "output-rotated-accessibility",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", "nested,server-tests,accessibility-gate",
                    "--lib", "server::tests::transfers_tests::dispatch::magnification::platform::mixed::", "--", "--ignored",
                    "--skip", ACCESSIBILITY_MIXED_TEST, "--test-threads=1",
                ],
            ),
            (
                "magnification-input",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", MAGNIFICATION_INPUT_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "magnification-reveal",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", MAGNIFICATION_REVEAL_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "magnification-drop",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", MAGNIFICATION_DROP_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "magnification-caret",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", MAGNIFICATION_CARET_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "magnification-text-input",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::text_input_tests::", "--", "--test-threads=1",
                ],
            ),
            (
                "output-rotation-workflows",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::outputs::", "--", "--ignored", "--test-threads=1",
                    "--skip", "server::tests::transfers_tests::dispatch::outputs::color::",
                    "--skip", "server::tests::transfers_tests::dispatch::outputs::controls::",
                    "--skip", "server::tests::transfers_tests::dispatch::outputs::lifetimes::",
                ],
            ),
            (
                "output-color-client-workflows",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::outputs::color::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "output-control-ui",
                vec!["test", "--locked", "-p", "lince-interface", "--features", "native-presentation", "--lib",
                    OUTPUT_CONTROL_UI_TEST, "--", "--exact", "--test-threads=1"],
            ),
            (
                "output-control-model",
                vec!["test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES, "--lib",
                    "server::tests::display_tests::", "--", "--test-threads=1"],
            ),
            (
                "output-control-clients",
                vec!["test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES, "--lib",
                    OUTPUT_CONTROL_CLIENT_TEST, "--", "--ignored", "--exact", "--test-threads=1"],
            ),
            (
                "output-generation-clients",
                vec!["test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES, "--lib",
                    OUTPUT_GENERATION_CLIENT_TEST, "--", "--ignored", "--exact", "--test-threads=1"],
            ),
            (
                "output-color-native-pixels",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", OUTPUT_COLOR_NATIVE_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "output-color-gles-pixels",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", OUTPUT_COLOR_GLES_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "output-color-dmabuf-pixels",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "graphics::import::tests::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "power-protocol-lifetimes",
                vec!["test", "--locked", "-p", "lince-park", "--features", "server-tests", "--lib",
                    "server::tests::power_tests::", "--", "--test-threads=1"],
            ),
            (
                "power-transition-workflows",
                vec!["test", "--locked", "--release", "-p", "lince-park", "--features", "session,server-tests", "--lib",
                    "server::tests::transfers_tests::dispatch::power::", "--", "--ignored", "--test-threads=1"],
            ),
            (
                "tablet-protocol-lifetimes",
                vec!["test", "--locked", "-p", "lince-park", "--features", "server-tests", "--lib",
                    TABLET_LIFETIME_TEST, "--", "--exact", "--test-threads=1"],
            ),
            (
                "logout-recovery",
                vec!["test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES, "--lib",
                    LOGOUT_RECOVERY_TEST, "--", "--ignored", "--exact", "--test-threads=1"],
            ),
            (
                "absolute-input-ordering",
                vec![
                    "test",
                    "--locked",
                    "--release",
                    "-p",
                    "lince-park",
                    "--features",
                    "session,server-tests",
                    "--lib",
                    "server::tests::transfers_tests::dispatch::native_pointer::absolute::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "physical-output-planning",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "session,server-tests",
                    "--lib",
                    "session::display::tests",
                ],
            ),
            (
                "compositor-appearance",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::appearance::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "compositor-appearance-client",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", APPEARANCE_CLIENT_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "capture-rendering",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "presentation::tests::capture::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "capture-client",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", CAPTURE_CLIENT_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "capture-lifetime",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::capture_lifetime::peers::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "capture-retained-protocol",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", "server-tests", "--lib",
                    "server::tests::capture_tests::", "--", "--test-threads=1",
                ],
            ),
            (
                "capture-retained-pixels",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::capture_lifetime::retained::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "capture-settings",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::privacy::settings::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "windowed-fullscreen-clients",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "--", "--ignored", "server::tests::transfers_tests::windowed_fullscreen::",
                    "server::tests::transfers_tests::dispatch::windowed_fullscreen::", "--test-threads=1",
                ],
            ),
            (
                "compact-group-clients",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::compact::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "scratch-workflows",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::scratch::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "compositor-appearance-capture",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::capture_lifetime::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "compositor-appearance-cost",
                vec![
                    "test", "--locked", "--release", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", APPEARANCE_COST_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "physical-input-settings",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "session,server-tests",
                    "--lib",
                    "session::devices::tests::",
                    "--",
                    "--test-threads=1",
                ],
            ),
            (
                "window-layout-policy",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "--",
                    "layout::desktop::",
                    "layout::outputs::",
                    "native::canvas_controls::",
                    "native::workspace::navigation::tests::",
                    "native::workspace::scratch::tests::",
                    "native::accessibility::",
                    "host::mapping::",
                    "host::raster::",
                    "native::topology::surface_budget::",
                    "native::topology::output_views::",
                    "native::topology::view::",
                    "native::topology::compositor::",
                    "native::topology::stacking::",
                    "native::capture::",
                    "host::capture::",
                    "native::topology::surface_render::",
                    CAPTURE_ALIAS_POLICY_TESTS[0],
                    "native::shader_effects::compositor::",
                    "native::workspace::tests::compositor_appearance_reloads_from_the_workspace_and_invalid_saves_keep_the_last_good_document",
                    "native::canvas_background::",
                    "native::output_views::",
                    "--test-threads=1",
                ],
            ),
            (
                "keyboard-shortcuts",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "--",
                    "native::shortcuts::",
                    "native::keyboard::",
                    "native::actions::tests::",
                    "--test-threads=1",
                ],
            ),
            (
                "input-settings-policy",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "--",
                    "host::input::",
                    "host::external::",
                    "controls::editing::",
                    "native::layout::desktop::input_settings::",
                    "native::layout::desktop::pointer_settings::",
                    "native::layout::desktop::external_settings::",
                    "native::actions::tests::",
                    "--test-threads=1",
                ],
            ),
            (
                "input-settings-persistence",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "--",
                    "commands::tests::",
                    "external_commands::tests::",
                    "input_settings::tests::",
                    "desktop::input::tests::",
                    "--test-threads=1",
                ],
            ),
            (
                "shortcut-modal-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::shortcuts::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "configuration-review-rendering",
                vec![
                    "test", "--release", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "presentation::tests::configuration::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "configuration-review-input",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", "server::tests::transfers_tests::dispatch::configuration::", "--", "--ignored", "--test-threads=1",
                ],
            ),
            (
                "configuration-quiet-reload",
                vec![
                    "test", "--locked", "-p", "lince-park", "--features", super::INPUT_FEATURES,
                    "--lib", CONFIGURATION_QUIET_TEST, "--", "--ignored", "--exact", "--test-threads=1",
                ],
            ),
            (
                "tiling-controls-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::tiling::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "session-logout-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "logout",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "recent-window-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::recent::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "window-opening-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::opening::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "pointer-binding-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::pointer_bindings::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "window-manipulation-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::manipulation::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "window-manipulation-input",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "server::tests::manipulation_tests::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                    "--skip",
                    "::toolkit::",
                ],
            ),
            (
                "window-manipulation-physics",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--lib",
                    "native::physics::manipulation::",
                    "--",
                    "--test-threads=1",
                ],
            ),
            (
                "window-layout-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::layout_preferences::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "window-widths-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::widths::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "camera-follow-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "server::tests::focus_tests::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "monitor-navigation-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::monitors::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "workspace-navigation-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "workspace_navigation",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "workspace-transfer-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "transfers",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "overview-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::overview::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "monitor-settings-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::displays::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "output-recovery",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "session,server-tests",
                    "--lib",
                    "suspension",
                    "--",
                    "--include-ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "output-raster-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "server::tests::raster_tests::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "effect-snapshot-policy",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "--",
                    "native::description::shader::snapshot::",
                    "native::description::shader::tests::frozen_snapshots_stop_cameras_and_inputs_but_can_be_disabled_and_resumed",
                    "native::description::shader::tests::appearance_lighting_inputs_follow_working_mappings_freeze_and_disable",
                    "--test-threads=1",
                ],
            ),
            (
                "preview-layer-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::layers::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "preview-lifetime-policy",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "--",
                    "native::fiote::character::residency::tests::",
                    "native::topology::surface_budget::residency::tests::",
                    "native::render_layers::",
                    "--test-threads=1",
                ],
            ),
            (
                "effect-output-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::shaders::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "item-stacking-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::stacking::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "command-discovery-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::commands::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "keyboard-settings-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::keyboard::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "pointer-settings-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::pointer_settings::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "external-command-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::INPUT_FEATURES,
                    "--lib",
                    "presentation::tests::external_commands::",
                    "--",
                    "--ignored",
                    "--test-threads=1",
                ],
            ),
            (
                "effect-source-rendering",
                vec![
                    "test",
                    "--release",
                    "--locked",
                    "-p",
                    "lince-interface",
                    "--features",
                    "native-presentation",
                    "--lib",
                    "native::description::shader::",
                    "--",
                    "--test-threads=1",
                ],
            ),
            (
                "capture-helper-lifecycle",
                vec![
                    "test",
                    "--locked",
                    "-p",
                    "lince-smithay-capture",
                    "--features",
                    "backend_drm",
                    "--lib",
                    "wayland::image_copy_capture::tests",
                ],
            ),
            (
                "policy",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::NATIVE_FEATURES,
                    "--example",
                    "native_policy_probe",
                ],
            ),
            (
                "clipboard",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "clipboard-gate",
                    "--example",
                    "clipboard_probe",
                ],
            ),
            (
                "gpu-synchronization",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    "graphics-probe",
                    "--example",
                    "synchronization_probe",
                ],
            ),
            (
                "nested",
                vec![
                    "run",
                    "--locked",
                    "-p",
                    "lince-park",
                    "--features",
                    super::NATIVE_FEATURES,
                    "--example",
                    "nested",
                    "--",
                    "--directory",
                ],
            ),
        ] {
            if name == "nested" {
                let data = directory.join("nested-data");
                let data = data.to_str().ok_or("Test path must be UTF-8")?;
                let mut arguments = arguments;
                arguments.extend([data, "--outputs", "2", "--smoke"]);
                preparation_step(&directory, name, &arguments)?;
            } else {
                preparation_step(&directory, name, &arguments)?;
            }
        }
    }
    prepare_toolkit(
        &directory,
        configuration.toolkit_qml.as_ref(),
        selected.is_some(),
    )?;
    let target = directory.join("bin/lince");
    if let Some(selected) = &selected {
        std::os::unix::fs::symlink(selected, &target).map_err(error)?;
        let resolved = fs::canonicalize(selected).map_err(error)?;
        if let Ok(relative) = resolved.strip_prefix("/nix/store") {
            let package = Path::new("/nix/store").join(
                relative
                    .components()
                    .next()
                    .ok_or("Invalid package closure")?,
            );
            let status = Command::new("nix-store")
                .arg("--add-root")
                .arg(directory.join("package-closure"))
                .arg("--realise")
                .arg(package)
                .status()
                .map_err(error)?;
            if !status.success() {
                return Err("Could not pin the selected package closure".into());
            }
        }
    } else {
        let mut command = cargo_command();
        command.args([
            "run",
            "--release",
            "--locked",
            "--message-format=json",
            "-p",
            "lince",
            "--features",
            "instinct,park",
            "--",
            "--build-info",
        ]);
        deny_warnings(&mut command);
        let output = command.output().map_err(error)?;
        fs::write(directory.join("prepare-executable.log"), &output.stderr).map_err(error)?;
        if !output.status.success() {
            return Err("Preparing the executable failed; inspect prepare-executable.log".into());
        }
        let binary = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
            .filter(|value| {
                value["reason"] == "compiler-artifact" && value["target"]["name"] == "lince"
            })
            .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
            .last()
            .ok_or("Cargo did not report the Lince executable")?;
        fs::copy(binary, &target).map_err(error)?;
    }
    let build_info = inspect_build(&target)?;
    let process_executable = process_executable(&target)?;
    let mut probe = cargo_command();
    probe.args([
        "run",
        "--release",
        "--locked",
        "--message-format=json",
        "-p",
        "lince-park",
        "--features",
        "server-tests",
        "--example",
        "session_probe",
        "--",
        "--help",
    ]);
    deny_warnings(&mut probe);
    let output = probe.output().map_err(error)?;
    fs::write(directory.join("prepare-probe.log"), &output.stderr).map_err(error)?;
    if !output.status.success() {
        return Err("Preparing the native session probe failed".into());
    }
    let probe = output
        .stdout
        .split(|byte| *byte == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter(|value| {
            value["reason"] == "compiler-artifact" && value["target"]["name"] == "session_probe"
        })
        .filter_map(|value| value["executable"].as_str().map(PathBuf::from))
        .last()
        .ok_or("Cargo did not report the session probe executable")?;
    fs::copy(probe, directory.join("bin/session-probe")).map_err(error)?;
    let capture_sha256 = prepare_capture_probe(&directory)?;
    let capture_data = directory.join("nested-capture-data");
    fs::create_dir(&capture_data).map_err(error)?;
    let capture_report = directory.join("nested-capture.json");
    write_json(
        &capture_data.join("park.json"),
        &serde_json::json!({"ready": {"program": directory.join("bin/capture-probe"),
        "arguments": [capture_report], "capabilities": ["capture"]}}),
    )?;
    if selected.is_some() {
        let mut command = Command::new(&target);
        command
            .args([
                "--park-nested",
                "--park-duration",
                "20",
                "--park-outputs",
                "2",
                "--directory",
            ])
            .arg(&capture_data)
            .args(["--port", "0", "--peer-port", "0"]);
        if configuration.showoff {
            command.arg("--showoff");
        }
        logged(
            &mut command,
            &directory.join("nested-installed-capture.log"),
        )?;
        let (status, _) = bounded(&mut command, Duration::from_secs(45))?;
        if !status.success() {
            return Err(format!("Installed nested capture failed: {status}"));
        }
    } else {
        preparation_step(
            &directory,
            "nested-capture-export",
            &[
                "run",
                "--locked",
                "-p",
                "lince-park",
                "--features",
                super::NATIVE_FEATURES,
                "--example",
                "nested",
                "--",
                "--directory",
                capture_data
                    .to_str()
                    .ok_or("Capture test path must be UTF-8")?,
                "--outputs",
                "2",
                "--ready-hook",
                "--smoke",
            ],
        )?;
    }
    let captured: serde_json::Value = load(&capture_report)?;
    if captured["status"] != "passed" {
        return Err("Nested output capture did not pass".into());
    }
    let automation = automation::prepare(&directory, &target, &process_executable)?;
    fs::copy(
        env::current_exe().map_err(error)?,
        directory.join("bin/xtask"),
    )
    .map_err(error)?;
    let http = TcpListener::bind("127.0.0.1:0").map_err(error)?;
    let peer = TcpListener::bind("127.0.0.1:0").map_err(error)?;
    let run = Run {
        repository: repository.clone(), directory: directory.clone(),
        executable_sha256: digest(&target)?, revision: build_info["revision"].as_str().unwrap().to_owned(),
        process_sha256: digest(&process_executable)?, process_executable, build_info,
        installed: selected.is_some(),
        probe_sha256: digest(&directory.join("bin/session-probe"))?,
        capture_sha256,
        automation,
        port: http.local_addr().map_err(error)?.port(),
        peer_port: peer.local_addr().map_err(error)?.port(), configuration,
        recovery: "Return to the Niri VT recorded in sessions-before.log; use the independent timeout if the test hangs.".into(),
    };
    write_json(&directory.join("run.json"), &run)?;
    write_json(
        &directory.join("data/park.json"),
        &serde_json::json!({
            "terminal": run.configuration.terminal,
            "launcher": run.configuration.launcher,
            "lock": run.configuration.lock,
            "ready": {"program": directory.join("bin/xtask"), "arguments": ["park", "validate", "ready", directory],
                "capabilities": if run.configuration.automation { automation::grants() } else { run.configuration.helper.as_ref().map(|helper| helper.capabilities.clone()).unwrap_or_else(|| vec!["capture".into()]) }}
        }),
    )?;
    let script = format!(
        "#!/bin/sh\n{} --signal=TERM --kill-after=10s {}s {} develop {} -c {} park validate seat {}\npark_status=$?\nprintf '%s\\n' \"$park_status\" > {}\nexit \"$park_status\"\n",
        quote(&timeout),
        run.configuration.session_seconds() + 30,
        quote(&nix),
        quote(&PathBuf::from(format!(
            "{}#interface",
            repository.display()
        ))),
        quote(&directory.join("bin/xtask")),
        quote(&directory),
        quote(&directory.join("watchdog-exit.txt")),
    );
    let wrapper = directory.join("run-seat");
    fs::write(&wrapper, script).map_err(error)?;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).map_err(error)?;
    println!(
        "Prepared {}\nOn a spare foreground TTY, run:\n{}\n{}",
        directory.display(),
        quote(&wrapper),
        run.recovery
    );
    Ok(())
}

fn quote(path: &Path) -> String {
    format!(
        "'{}'",
        path.as_os_str().to_string_lossy().replace('\'', "'\\''")
    )
}

fn native_environment(command: &mut Command) -> Result<()> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    for name in [
        "LD_LIBRARY_PATH",
        "GBM_BACKENDS_PATH",
        "__EGL_VENDOR_LIBRARY_DIRS",
    ] {
        let saved = format!("LINCE_HOST_{name}");
        if let Some(original) = env::var_os(&saved) {
            match original.as_bytes().split_first() {
                Some((b'0', [])) => {
                    command.env_remove(name);
                }
                Some((b'1', value)) => {
                    command.env(name, OsString::from_vec(value.to_vec()));
                }
                _ => return Err(format!("Invalid saved environment for {name}")),
            }
        }
        command.env_remove(saved);
    }
    for name in [
        "NIRI_SOCKET",
        "SWAYSOCK",
        "I3SOCK",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "WAYFIRE_SOCKET",
    ] {
        command.env_remove(name);
    }
    Ok(())
}

fn ready(directory: &Path) -> Result<()> {
    let run: Run = load(&directory.join("run.json"))?;
    if run.directory != directory || !directory.is_absolute() {
        return Err("Invalid prepared run directory".into());
    }
    automation::configuration(&run.configuration)?;
    let socket =
        PathBuf::from(env::var_os("WAYLAND_DISPLAY").ok_or("Park did not supply its display")?);
    let socket = if socket.is_absolute() {
        socket
    } else {
        PathBuf::from(
            env::var_os("XDG_RUNTIME_DIR").ok_or("Park did not supply its runtime directory")?,
        )
        .join(socket)
    };
    if !socket.is_absolute() || !socket.exists() {
        return Err("Park readiness did not supply a live absolute display path".into());
    }
    let pid = env::var("LINCE_PARK_BOOTSTRAP_PID").map_err(error)?;
    let pid = pid.parse::<i32>().map_err(error)?;
    if pid <= 1
        || socket.file_name().and_then(|name| name.to_str())
            != Some(format!("wayland-park-{pid}").as_str())
    {
        return Err("Readiness process identity does not match the Park socket".into());
    }
    let bus = env::var("DBUS_SESSION_BUS_ADDRESS").map_err(error)?;
    let status = Command::new("dbus-update-activation-environment")
        .args([
            "WAYLAND_DISPLAY",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_TYPE",
            "XDG_SESSION_DESKTOP",
        ])
        .env_remove("WAYLAND_SOCKET")
        .status()
        .map_err(error)?;
    if !status.success() {
        return Err(
            "Could not configure the validation session's private D-Bus activation environment"
                .into(),
        );
    }
    if run
        .configuration
        .helper
        .as_ref()
        .is_some_and(|helper| !helper.valid())
    {
        return Err("Invalid helper authorization in the prepared fixture".into());
    }
    write_json(
        &directory.join("ready.json"),
        &serde_json::json!({"socket": socket, "pid": pid, "bus": bus}),
    )?;
    if run.configuration.automation {
        automation::verify_artifacts(&run)?;
        let failure = Command::new(directory.join("bin/control-probe"))
            .arg(directory.join("physical-automation.json"))
            .arg(directory.join("bin/session-probe"))
            .exec();
        return Err(format!("The authorized automation probe could not start: {failure}"));
    }
    if let Some(helper) = &run.configuration.helper {
        let descriptor = env::var("WAYLAND_SOCKET")
            .map_err(error)?
            .parse::<i32>()
            .map_err(error)?;
        if descriptor < 0 {
            return Err(
                "The configured helper did not receive its authorized Wayland connection".into(),
            );
        }
        let failure = Command::new(&helper.target.program)
            .args(&helper.target.arguments)
            .exec();
        return Err(format!("The authorized helper could not start: {failure}"));
    }
    let failure = Command::new(directory.join("bin/capture-probe"))
        .arg(directory.join("physical-capture.json"))
        .exec();
    Err(format!(
        "The output capture probe could not start: {failure}"
    ))
}

struct ManagedChild {
    child: Child,
    group: rustix::process::Pid,
}

impl ManagedChild {
    fn spawn(command: &mut Command) -> Result<Self> {
        let child = command.process_group(0).spawn().map_err(error)?;
        let group = rustix::process::Pid::from_raw(child.id() as i32)
            .ok_or("Invalid child process group")?;
        Ok(Self { child, group })
    }
}

struct Children(Vec<ManagedChild>);

impl Drop for Children {
    fn drop(&mut self) {
        for process in &mut self.0 {
            let _ =
                rustix::process::kill_process_group(process.group, rustix::process::Signal::KILL);
            let _ = process.child.wait();
        }
    }
}

fn physical_terminal(metadata: &str, input: &Path) -> bool {
    let terminal = metadata.lines().find_map(|line| line.strip_prefix("TTY="));
    metadata.lines().any(|line| line == "Active=yes")
        && metadata
            .lines()
            .any(|line| line.starts_with("Seat=") && line.len() > 5)
        && terminal.is_some_and(|terminal| {
            terminal
                .strip_prefix("tty")
                .is_some_and(|number| number.parse::<u32>().is_ok_and(|number| number > 0))
                && input == Path::new("/dev").join(terminal)
        })
}

fn seat(directory: &Path) -> Result<()> {
    let invalid_terminal =
        "Run from a logged-in, active physical TTY; the agent pseudo-terminal is insufficient";
    if !std::io::stdin().is_terminal() {
        return Err(invalid_terminal.into());
    }
    let input = fs::read_link("/proc/self/fd/0").map_err(error)?;
    let run: Run = load(&directory.join("run.json"))?;
    automation::configuration(&run.configuration)?;
    automation::verify_artifacts(&run)?;
    if !run.configuration.valid_duration()
        || run
            .configuration
            .helper
            .as_ref()
            .is_some_and(|helper| !helper.valid())
        || !directory.is_absolute()
        || run.directory != directory
        || digest(&directory.join("bin/lince"))? != run.executable_sha256
        || digest(&directory.join("bin/session-probe"))? != run.probe_sha256
        || digest(&directory.join("bin/capture-probe"))? != run.capture_sha256
        || digest(&run.process_executable)? != run.process_sha256
        || inspect_build(&directory.join("bin/lince"))? != run.build_info
    {
        return Err("Prepared executable or directory identity changed".into());
    }
    write_json(
        &directory.join("hardware-before.json"),
        &diagnostics::hardware(),
    )?;
    let session = Command::new("loginctl")
        .args([
            "show-session",
            "self",
            "--property=Active",
            "--property=TTY",
            "--property=Seat",
            "--property=VTNr",
        ])
        .output()
        .map_err(error)?;
    let metadata = String::from_utf8(session.stdout).map_err(error)?;
    fs::write(directory.join("physical-session.log"), &metadata).map_err(error)?;
    if !session.status.success() || !physical_terminal(&metadata, &input) {
        return Err(invalid_terminal.into());
    }
    let http = TcpListener::bind(("127.0.0.1", run.port)).map_err(error)?;
    let peer = TcpListener::bind(("127.0.0.1", run.peer_port)).map_err(error)?;
    drop((http, peer));
    let claim = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("seat-started"))
        .map_err(error)?;
    drop(claim);
    let _signals = interrupt_handler()?;
    let mut results = Results {
        phases: Vec::new(),
        resources: Vec::new(),
        observations: [
            "physical-input",
            "visible-scanout",
            "vt-return",
            "hotplug",
            "suspend-resume",
            "lock-protection",
            "clean-login",
            "long-session",
        ]
        .into_iter()
        .map(|name| (name.to_owned(), "awaiting human observation".into()))
        .collect(),
    };
    let mut children = Children(Vec::new());
    let outcome = seat_inner(&run, &mut children, &mut results);
    if let Err(detail) = &outcome {
        results.phases.push(Phase {
            name: "physical-smoke".into(),
            outcome: "failed".into(),
            elapsed_ms: 0,
            detail: detail.clone(),
        });
    }
    drop(children);
    write_json(
        &directory.join("hardware-after.json"),
        &diagnostics::hardware(),
    )?;
    write_json(&directory.join("results.json"), &results)?;
    let mut summary = format!(
        "Park validation {}\nExecutable SHA-256: {}\n",
        run.revision, run.executable_sha256
    );
    for phase in &results.phases {
        summary.push_str(&format!(
            "{}: {} ({} ms) {}\n",
            phase.name, phase.outcome, phase.elapsed_ms, phase.detail
        ));
    }
    for (name, outcome) in &results.observations {
        summary.push_str(&format!("{name}: {outcome}\n"));
    }
    summary.push_str(&resource_summary(&results.resources));
    summary.push_str(&format!("{}\n", run.recovery));
    fs::write(directory.join("summary.txt"), &summary).map_err(error)?;
    print!("{summary}");
    outcome
}

fn seat_inner(run: &Run, children: &mut Children, results: &mut Results) -> Result<()> {
    let directory = &run.directory;
    let mut command = Command::new(executable(Path::new("timeout"))?);
    command
        .args(["--signal=TERM", "--kill-after=10s"])
        .arg(format!("{}s", run.configuration.session_seconds()))
        .args(["dbus-run-session", "--"])
        .arg(directory.join("bin/lince"))
        .args(["--park", "--no-tray", "--directory"])
        .arg(directory.join("data"))
        .arg("--port")
        .arg(run.port.to_string())
        .arg("--peer-port")
        .arg(run.peer_port.to_string())
        .env_remove("LINCE_DATA_DIR")
        .env_remove("LINCE_PORT")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("LINCE_PARK_BOOTSTRAP_PID")
        .env("LINCE_PARK_TRACE_FRAMES", "1")
        .env("RUST_MIN_STACK", "33554432");
    if run.configuration.showoff {
        command.arg("--showoff");
    }
    logged(&mut command, &directory.join("session.log"))?;
    children.0.push(ManagedChild::spawn(&mut command)?);
    let start = Instant::now();
    let (socket, pid, bus) = loop {
        check_interruption()?;
        if let Some(status) = children.0[0].child.try_wait().map_err(error)? {
            return Err(format!("Park exited before readiness: {status}"));
        }
        if directory.join("ready.json").exists() {
            let ready: serde_json::Value = load(&directory.join("ready.json"))?;
            let pid = ready["pid"]
                .as_i64()
                .and_then(|pid| i32::try_from(pid).ok())
                .and_then(rustix::process::Pid::from_raw)
                .ok_or("Invalid readiness process")?;
            let bus = ready["bus"]
                .as_str()
                .ok_or("Missing isolated session bus")?
                .to_owned();
            break (
                PathBuf::from(ready["socket"].as_str().ok_or("Invalid readiness result")?),
                pid,
                bus,
            );
        }
        if start.elapsed() > Duration::from_secs(40) {
            return Err("No first-frame readiness within 40 seconds".into());
        }
        thread::sleep(Duration::from_millis(50));
    };
    results.phases.push(Phase { name: "first-frame-readiness".into(), outcome: "passed".into(), elapsed_ms: start.elapsed().as_millis(), detail: "Readiness hook ran after the initial scene submission; physical scanout still needs observation".into() });
    automation::collect(run, &socket, pid.as_raw_nonzero().get() as u32, children, results)?;
    for round in 0..run.configuration.lifecycle_rounds {
        results
            .resources
            .push(resource_sample(pid, start.elapsed().as_millis()));
        let mut probe = Command::new(directory.join("bin/session-probe"));
        probe
            .env("WAYLAND_DISPLAY", &socket)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus)
            .env_remove("WAYLAND_SOCKET");
        let log = format!("native-session-probe-{round}.log");
        logged(&mut probe, &directory.join(&log))?;
        let (status, elapsed_ms) = bounded(&mut probe, Duration::from_secs(55))?;
        results.phases.push(Phase {
            name: format!("native-window-lifecycle-{round}"),
            outcome: if status.success() { "passed" } else { "failed" }.into(),
            elapsed_ms,
            detail: format!("{status}; {log}"),
        });
        if !status.success() {
            return Err("Native window lifecycle acceptance failed".into());
        }
        results
            .resources
            .push(resource_sample(pid, start.elapsed().as_millis()));
        write_json(&directory.join("progress.json"), results)?;
    }
    if run.configuration.helper.is_none() && !run.configuration.automation {
        let capture_started = Instant::now();
        let report = directory.join("physical-capture.json");
        while !report.exists() {
            check_interruption()?;
            if capture_started.elapsed() >= Duration::from_secs(12) {
                return Err("The authorized physical output capture probe did not complete".into());
            }
            if children.0[0].child.try_wait().map_err(error)?.is_some() {
                return Err("Park exited before capture acceptance".into());
            }
            thread::sleep(Duration::from_millis(50));
        }
        let captured: serde_json::Value = load(&report)?;
        if captured["status"] != "passed" {
            return Err("Physical output capture did not pass".into());
        }
        results.phases.push(Phase { name: "authorized-output-capture".into(), outcome: "passed".into(), elapsed_ms: capture_started.elapsed().as_millis(),
            detail: "Source/protocol/export/pixel checks in physical-capture.json; visual fidelity, cursor and portal acceptance remain separate".into() });
    }
    if children.0[0].child.try_wait().map_err(error)?.is_some() {
        return Err("Park exited after the probe closed its last windows".into());
    }
    let programs = std::iter::once(&run.configuration.terminal).chain(&run.configuration.clients);
    for (index, program) in programs.enumerate() {
        let mut command = Command::new(executable(Path::new("timeout"))?);
        command
            .args(["--signal=TERM", "--kill-after=5s"])
            .arg(format!(
                "{}s",
                run.configuration.observation_seconds + run.configuration.soak_seconds + 20
            ))
            .arg(&program.program)
            .args(&program.arguments)
            .env("WAYLAND_DISPLAY", &socket)
            .env("DBUS_SESSION_BUS_ADDRESS", &bus)
            .env("XDG_CURRENT_DESKTOP", "Lince")
            .env("XDG_SESSION_TYPE", "wayland")
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("SDL_VIDEODRIVER", "wayland")
            .env("SDL_VIDEO_DRIVER", "wayland")
            .env_remove("WAYLAND_SOCKET")
            .env_remove("DISPLAY")
            .env_remove("XAUTHORITY")
            .env_remove("XDG_ACTIVATION_TOKEN");
        native_environment(&mut command)?;
        logged(&mut command, &directory.join(format!("client-{index}.log")))?;
        children.0.push(ManagedChild::spawn(&mut command)?);
    }
    results.phases.push(Phase {
        name: "external-clients".into(),
        outcome: "awaiting human observation".into(),
        elapsed_ms: 0,
        detail: "Processes launched; this alone does not establish mapped windows or rendering"
            .into(),
    });
    write_json(&directory.join("progress.json"), results)?;
    let observation = Instant::now();
    let hold =
        Duration::from_secs(run.configuration.observation_seconds + run.configuration.soak_seconds);
    println!(
        "Observe client windows and exercise the shortcut checklist. Super+Shift+E opens logout confirmation; otherwise the runner requests saved logout after {} seconds. {}",
        hold.as_secs(),
        run.recovery
    );
    let mut requested_logout = false;
    let mut sample_at = Instant::now();
    loop {
        check_interruption()?;
        if let Some(status) = children.0[0].child.try_wait().map_err(error)? {
            let orderly = status.success()
                && fs::read_to_string(directory.join("session.log"))
                    .map_err(error)?
                    .lines()
                    .any(|line| line == "Park orderly logout completed");
            results.phases.push(Phase {
                name: "session-exit".into(),
                outcome: if orderly { "passed" } else { "failed" }.into(),
                elapsed_ms: start.elapsed().as_millis(),
                detail: format!("{status}; orderly logout marker: {orderly}"),
            });
            return if orderly {
                Ok(())
            } else {
                Err(format!(
                    "Session ended without confirmed orderly logout: {status}"
                ))
            };
        }
        if Instant::now() >= sample_at {
            results
                .resources
                .push(resource_sample(pid, start.elapsed().as_millis()));
            write_json(&directory.join("progress.json"), results)?;
            sample_at = Instant::now() + Duration::from_secs(1);
        }
        if !requested_logout && observation.elapsed() > hold {
            let live =
                fs::read_link(format!("/proc/{}/exe", pid.as_raw_nonzero())).map_err(error)?;
            if live != run.process_executable || digest(&live)? != run.process_sha256 {
                return Err("Compositor process identity changed before logout".into());
            }
            rustix::process::kill_process(pid, rustix::process::Signal::TERM).map_err(error)?;
            requested_logout = true;
        }
        if observation.elapsed() > hold + Duration::from_secs(30) {
            return Err("Interactive smoke ended without confirmed logout; remaining automation and observations are not passed".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependency_source_copy_preserves_content_identity_and_refuses_symlink_escape() {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/park-validation")
            .join(format!("dependency-copy-{stamp}-{}", std::process::id()));
        let source = directory.join("source");
        fs::create_dir_all(source.join("src")).unwrap();
        fs::create_dir(source.join("target")).unwrap();
        fs::write(source.join("Cargo.toml"), "fixture manifest").unwrap();
        fs::write(source.join("src/lib.rs"), "fixture source").unwrap();
        fs::write(source.join("Cargo.lock"), "untrusted seed").unwrap();
        fs::write(source.join("target/generated"), "excluded artifact").unwrap();
        let destination = directory.join("copy");
        let mut hashes = BTreeMap::new();
        copy_dependency_source(&source, &destination, Path::new(""), &mut hashes).unwrap();
        assert_eq!(fs::read(destination.join("src/lib.rs")).unwrap(), b"fixture source");
        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes["src/lib.rs"], digest(&destination.join("src/lib.rs")).unwrap());
        assert!(!destination.join("Cargo.lock").exists());
        assert!(!destination.join("target").exists());
        isolate_dependency_manifest(&destination.join("Cargo.toml")).unwrap();
        assert_eq!(fs::read(destination.join("Cargo.toml")).unwrap(), b"fixture manifest\n[workspace]\n");
        assert_eq!(fs::read(source.join("Cargo.toml")).unwrap(), b"fixture manifest");
        assert_ne!(hashes["Cargo.toml"], digest(&destination.join("Cargo.toml")).unwrap());
        let linked = directory.join("linked");
        fs::create_dir(&linked).unwrap();
        fs::write(directory.join("outside"), "must not copy").unwrap();
        std::os::unix::fs::symlink(directory.join("outside"), linked.join("escape")).unwrap();
        let rejected = directory.join("rejected");
        assert!(copy_dependency_source(&linked, &rejected, Path::new(""), &mut BTreeMap::new()).is_err());
        assert!(!rejected.join("escape").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn interrupted_probe_is_reaped_without_interrupting_other_tests() {
        const MARKER: &str = "LINCE_PARK_INTERRUPT_TEST";
        if let Some(path) = env::var_os(MARKER) {
            let _signals = interrupt_handler().unwrap();
            fs::write(path, "ready").unwrap();
            let started = Instant::now();
            let result = bounded(Command::new("sleep").arg("20"), Duration::from_secs(25));
            assert!(result.unwrap_err().contains("interrupted"));
            assert!(started.elapsed() < Duration::from_secs(5));
            return;
        }
        let directory =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/park-validation");
        fs::create_dir_all(&directory).unwrap();
        let marker = directory.join(format!("interrupt-test-{}", std::process::id()));
        let mut child = Command::new(env::current_exe().unwrap())
            .args(["--exact", "park::validation::tests::interrupted_probe_is_reaped_without_interrupting_other_tests"])
            .env(MARKER, &marker).spawn().unwrap();
        let started = Instant::now();
        while !marker.exists() && started.elapsed() < Duration::from_secs(5) {
            assert!(child.try_wait().unwrap().is_none());
            thread::sleep(Duration::from_millis(10));
        }
        assert!(marker.exists());
        let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if started.elapsed() > Duration::from_secs(6) {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Interrupted validation did not exit promptly");
            }
            thread::sleep(Duration::from_millis(10));
        }
        fs::remove_file(marker).unwrap();
    }

    #[test]
    fn installed_identity_tracks_the_payload_of_shell_and_elf_wrappers() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../target/park-validation/wrapper-{stamp}"));
        fs::create_dir_all(&directory).unwrap();
        let executable = directory.join("lince");
        let payload = directory.join(".lince-wrapped");
        fs::write(&payload, b"\x7fELFpayload").unwrap();
        for header in [b"#!/bin/sh".as_slice(), b"\x7fELFwrapper".as_slice()] {
            fs::write(&executable, header).unwrap();
            assert_eq!(
                process_executable(&executable).unwrap(),
                payload.canonicalize().unwrap()
            );
        }
        fs::remove_file(&payload).unwrap();
        assert_eq!(
            process_executable(&executable).unwrap(),
            executable.canonicalize().unwrap()
        );
        fs::write(&executable, b"#!/bin/sh").unwrap();
        assert!(process_executable(&executable).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn build_capabilities_reject_help_output_and_non_park_executables() {
        let information = serde_json::json!({"version":"1.0", "revision":"test", "optimized":true,
            "target":"x86_64-unknown-linux-gnu", "features":{"ui":true,"park":true}});
        assert!(validate_build(&information).is_ok());
        for field in ["ui", "park"] {
            let mut missing = information.clone();
            missing["features"][field] = false.into();
            assert!(validate_build(&missing).is_err());
        }
        let mut missing = information.clone();
        missing["optimized"] = false.into();
        assert!(validate_build(&missing).is_err());
        let mut missing = information.clone();
        missing["target"] = "aarch64-apple-darwin".into();
        assert!(validate_build(&missing).is_err());
        assert!(validate_build(&serde_json::json!({"help":"--park"})).is_err());
    }

    #[test]
    fn executable_resolution_preserves_the_invoked_symlink_name() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("park-executable-{stamp}-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        let target = directory.join("dispatch");
        fs::write(&target, b"#!/bin/sh\n[ \"${0##*/}\" = selected-command ]\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
        let alias = directory.join("selected-command");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        let resolved = executable(&alias).unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(resolved.file_name(), alias.file_name());
        assert!(Command::new(resolved).status().unwrap().success());
        assert!(executable(&directory).is_err());
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(executable(&alias).is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn shell_paths_preserve_literal_metacharacters() {
        assert_eq!(
            quote(Path::new("/tmp/a'b $(touch x)")),
            "'/tmp/a'\\''b $(touch x)'"
        );
    }

    #[test]
    fn physical_validation_requires_the_actual_active_session_terminal() {
        let active = "Active=yes\nTTY=tty3\nSeat=seat0\nVTNr=3\n";
        assert!(physical_terminal(active, Path::new("/dev/tty3")));
        assert!(!physical_terminal(active, Path::new("/dev/pts/3")));
        assert!(!physical_terminal(active, Path::new("/dev/tty2")));
        assert!(!physical_terminal(
            &active.replace("Active=yes", "Active=no"),
            Path::new("/dev/tty3")
        ));
        assert!(!physical_terminal(
            &active.replace("Seat=seat0", "Seat="),
            Path::new("/dev/tty3")
        ));
        assert!(!physical_terminal(
            "Active=yes\nTTY=pts/3\nSeat=seat0\n",
            Path::new("/dev/pts/3")
        ));
        assert!(!physical_terminal(
            "Active=yes\nTTY=tty0\nSeat=seat0\n",
            Path::new("/dev/tty0")
        ));
    }

    #[test]
    fn fixture_schema_rejects_shell_and_unknown_fields() {
        assert!(
            serde_json::from_str::<Configuration>(
                r#"{"terminal":{"program":"terminal","shell":true}}"#
            )
            .is_err()
        );
        let config: Configuration = serde_json::from_str(
            r#"{"terminal":{"program":"terminal","arguments":["$(touch x)"]}}"#,
        )
        .unwrap();
        assert_eq!(config.terminal.arguments, ["$(touch x)"]);
        assert!(!config.showoff);
        assert!(config.launcher.is_none());
        assert!(config.lock.is_none());
        assert!(config.toolkit_qml.is_none());
        let config: Configuration = serde_json::from_str(
            r#"{"terminal":{"program":"terminal"},"launcher":{"program":"chosen-launcher","arguments":["two words"]},"lock":{"program":"chosen-locker","arguments":["$(touch x)"]}}"#,
        ).unwrap();
        assert_eq!(config.launcher.unwrap().arguments, ["two words"]);
        assert_eq!(config.lock.unwrap().arguments, ["$(touch x)"]);
        let config: Configuration =
            serde_json::from_str(r#"{"terminal":{"program":"terminal"},"showoff":true}"#).unwrap();
        assert!(config.showoff);
        let config: Configuration = serde_json::from_str(
            r#"{"terminal":{"program":"terminal"},"toolkit_qml":{"program":"chosen-qml","arguments":["two words","$(touch x)"]}}"#,
        ).unwrap();
        assert_eq!(
            config.toolkit_qml.unwrap().arguments,
            ["two words", "$(touch x)"]
        );
    }

    #[test]
    fn missing_toolkit_and_installed_selection_are_reported_unrun_without_launching_a_runner() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory =
            env::temp_dir().join(format!("park-toolkit-{stamp}-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        prepare_toolkit(&directory, None, false).unwrap();
        let report: serde_json::Value = load(&directory.join("toolkit-interaction.json")).unwrap();
        assert_eq!(report["status"], "unrun");
        assert!(report["reason"].as_str().unwrap().contains("toolkit_qml"));
        let program = Program {
            program: directory.join("must-not-execute"),
            arguments: Vec::new(),
        };
        prepare_toolkit(&directory, Some(&program), true).unwrap();
        let report: serde_json::Value = load(&directory.join("toolkit-interaction.json")).unwrap();
        assert_eq!(report["status"], "unrun");
        assert!(
            report["reason"]
                .as_str()
                .unwrap()
                .contains("installed executable")
        );
        assert!(!directory.join("toolkit-interaction-rendering.log").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn inspector_preparation_rejects_empty_wrong_and_failed_test_selections() {
        for required in [QUIET_INSPECTOR_TEST, PERMISSION_INSPECTOR_TEST] {
            assert!(inspector_result(&format!("test {required} ... ok"), required).is_ok());
            for log in [
                String::new(),
                "running 0 tests\ntest result: ok. 0 passed".into(),
                "test another::case ... ok".into(),
                format!("test {required} ... FAILED"),
            ] {
                assert!(inspector_result(&log, required).is_err());
            }
        }
    }

    #[test]
    fn toolkit_acceptance_requires_every_named_case_instead_of_an_empty_or_partial_selection() {
        let cases = [
            "flat_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
            "spatial_qt_controls_popups_owned_dialogs_and_client_moves_keep_normal_input",
            "properties::flat_qt_properties_keep_the_original_record_through_focus_and_close",
            "properties::spatial_qt_properties_keep_the_original_record_through_focus_and_close",
            "properties::flat_free_qt_properties_keep_the_original_record_through_focus_and_close",
            "properties::spatial_free_qt_properties_keep_the_original_record_through_focus_and_close",
            "properties::unavailable::flat_free_qt_deleted_record_keeps_its_draft_and_refresh_target",
            "properties::unavailable::spatial_tiled_qt_deleted_record_keeps_its_draft_and_refresh_target",
            "properties::creation::flat_free_qt_creation_retry_keeps_its_pending_inspector_after_workspace_change_and_close",
        ].map(|case| format!("test server::tests::manipulation_tests::tiled::toolkit::{case} ... ok"));
        assert!(toolkit_results(&cases.join("\n")).is_ok());
        assert!(toolkit_results("running 0 tests\ntest result: ok. 0 passed").is_err());
        assert!(toolkit_results(&cases[..2].join("\n")).is_err());
        assert!(toolkit_results(&cases[..6].join("\n")).is_err());
        assert!(toolkit_results(&cases[..8].join("\n")).is_err());
        for index in 0..cases.len() {
            let mut incomplete = cases.to_vec();
            incomplete.remove(index);
            assert!(toolkit_results(&incomplete.join("\n")).is_err());
            let mut failed = cases.clone();
            failed[index] = failed[index].replace(" ... ok", " ... FAILED");
            assert!(toolkit_results(&failed.join("\n")).is_err());
        }
    }

    #[test]
    fn validation_durations_are_bounded_and_watchdogs_include_every_round() {
        let mut config: Configuration =
            serde_json::from_str(r#"{"terminal":{"program":"terminal"}}"#).unwrap();
        assert!(config.valid_duration());
        assert_eq!(config.lifecycle_rounds, 1);
        assert_eq!(config.observation_seconds, 90);
        assert_eq!(config.session_seconds(), 235);
        config.lifecycle_rounds = 20;
        config.observation_seconds = 300;
        config.soak_seconds = 3600;
        assert!(config.valid_duration());
        assert_eq!(config.session_seconds(), 5090);
        config.lifecycle_rounds = 21;
        assert!(!config.valid_duration());
        config.lifecycle_rounds = 1;
        config.soak_seconds = 3601;
        assert!(!config.valid_duration());
        config.soak_seconds = 0;
        config.observation_seconds = 0;
        assert!(!config.valid_duration());
    }

    #[test]
    fn resource_reports_preserve_negative_deltas_and_missing_measurements() {
        let sample = |elapsed_ms, resident_kib, descriptors, cpu_ticks| ResourceSample {
            elapsed_ms,
            resident_kib,
            descriptors,
            cpu_ticks,
            peak_resident_kib: None,
            threads: None,
            drm_clients: None,
        };
        let summary = resource_summary(&[
            sample(100, Some(2048), Some(20), Some(10)),
            sample(1100, Some(1024), Some(18), Some(14)),
        ]);
        assert!(summary.contains("across 1000 ms"));
        assert!(summary.contains("RSS delta -1024 KiB"));
        assert!(summary.contains("descriptor delta -2"));
        assert!(summary.contains("CPU tick delta 4"));
        let summary = resource_summary(&[sample(0, None, None, None)]);
        assert!(summary.contains("RSS delta unavailable"));
        assert!(!summary.contains("passed"));
    }

    #[test]
    fn helper_authorization_is_explicit_bounded_and_independent_of_client_arguments() {
        let parse = |capabilities: &str| {
            serde_json::from_str::<Helper>(&format!(r#"{{"target":{{"program":"chosen-helper","arguments":["two words"]}},"capabilities":{capabilities}}}"#)).unwrap()
        };
        let helper = parse(r#"["capture","window-list"]"#);
        assert!(helper.valid());
        assert_eq!(helper.target.arguments, ["two words"]);
        assert!(parse(r#"["clipboard","window-list","window-control","output-control","capture","input-method","desktop-control","launch"]"#).valid());
        for capabilities in [
            "[]",
            r#"["all"]"#,
            r#"["capture","capture"]"#,
            r#"["clipboard","window-list","window-control","output-control","capture","input-method","capture"]"#,
        ] {
            assert!(!parse(capabilities).valid());
        }
        let config: Configuration = serde_json::from_str(r#"{"terminal":{"program":"terminal"},"helper":{"target":{"program":"provider"},"capabilities":["input-method"]}}"#).unwrap();
        assert!(config.helper.unwrap().valid());
    }
}
