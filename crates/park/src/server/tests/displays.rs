use super::*;
use lince_interface::host::display::{
    DisplayAction, DisplayConfiguration, DisplayOutcome, DisplayRequest,
};
use std::time::Instant;

fn server() -> Server {
    let mut server = Server::new([640, 480]).unwrap();
    server.registered_output_catalog().unwrap();
    server
}

fn submit(server: &mut Server, action: DisplayAction) {
    let id = server.display_snapshot().last_request + 1;
    server.display_request(DisplayRequest { id, action }, |_| {
        panic!("Only Keep may persist monitor settings")
    });
}

fn proposed(server: &Server, scale: f64) -> DisplayConfiguration {
    let mut heads = server.display_snapshot().configuration();
    heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().scale = scale;
    heads
}

fn apply_next(server: &mut Server) -> OutputRequest {
    let mut requests = server.output_requests();
    assert_eq!(requests.len(), 1);
    let request = requests.pop().unwrap();
    assert_eq!(
        server.output_configuration_preview(&request),
        !request.test && !server.output_configuration_recovery(&request)
    );
    assert!(server.output_configuration_current(&request));
    assert!(
        server
            .apply_virtual_output_configuration(&request, [640, 480])
            .unwrap()
    );
    server.output_configuration_finished(request.token, true);
    request
}

fn preview(server: &mut Server) -> u64 {
    let snapshot = server.display_snapshot();
    let heads = proposed(server, 2.0);
    submit(
        server,
        DisplayAction::Preview {
            revision: snapshot.revision,
            heads,
        },
    );
    assert!(apply_next(server).test);
    assert!(!apply_next(server).test);
    server.display_snapshot().preview.unwrap().token
}

#[test]
fn rotated_preview_timeout_restores_the_complete_configuration_and_status_updates_do_not_retire_its_token() {
    let mut server = server();
    let before = server.display_snapshot();
    let mut heads = proposed(&server, 2.0);
    let settings = heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap();
    settings.rotation = lince_interface::host::mapping::QuarterTurn::Clockwise90;
    settings.position = [20, 10];
    submit(&mut server, DisplayAction::Preview { revision: before.revision, heads });
    assert!(apply_next(&mut server).test);
    assert!(!apply_next(&mut server).test);
    let preview = server.display_snapshot().preview.unwrap();
    let revision = server.display_snapshot().revision;
    server.output_adaptive_sync(OutputId(1), lince_interface::host::display::AdaptiveSyncStatus {
        support: lince_interface::host::display::AdaptiveSyncSupport::Supported,
        prepared: Some(false), effective: Some(false), pending: false, reason: None });
    server.advance_display(Instant::now());
    assert_eq!(server.display_snapshot().revision, revision);
    assert_eq!(server.display_preview_token(), Some(preview.token));
    assert!(server.output_requests().is_empty());
    server.advance_display(preview.deadline + Duration::from_millis(1));
    let request = apply_next(&mut server);
    assert!(server.output_configuration_recovery(&request) || server.display_preview_token().is_none());
    assert_eq!(server.display_snapshot().configuration(), before.configuration());
    assert_eq!(server.display_snapshot().outputs, before.outputs);
    assert!(server.display_snapshot().preview.is_none());
}

#[test]
fn native_display_test_uses_the_backend_without_a_wayland_configuration_or_layout_change() {
    let mut server = server();
    let before = server.display_snapshot();
    let heads = proposed(&server, 2.0);
    submit(
        &mut server,
        DisplayAction::Test {
            revision: before.revision,
            heads,
        },
    );
    assert_eq!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Pending
    );
    assert!(apply_next(&mut server).test);
    let after = server.display_snapshot();
    assert_eq!(before.heads, after.heads);
    assert_eq!(before.outputs, after.outputs);
    assert_eq!(before.revision, after.revision);
    assert!(after.preview.is_none());
    assert_eq!(after.response.unwrap().outcome, DisplayOutcome::Tested);
    assert!(server.output_requests().is_empty());
}

#[test]
fn late_preview_presentation_failure_uses_one_existing_rollback_and_retains_the_original_error() {
    let mut server = server();
    let before = server.display_snapshot().configuration();
    preview(&mut server);
    assert!(server.recover_display_preview("fixture scanout failure".into()));
    assert!(server.recover_display_preview("a repeated observation must not restart recovery".into()));
    apply_next(&mut server);
    assert_eq!(server.display_snapshot().configuration(), before);
    assert!(server.display_snapshot().preview.is_none());
    let DisplayOutcome::Failed(reason) = server.display_snapshot().response.unwrap().outcome else { panic!("A recovered failure must not be reported as a successful preview"); };
    assert!(reason.contains("fixture scanout failure"));
    assert!(reason.contains("restored"));
    assert!(server.output_requests().is_empty());
}

#[test]
fn native_preview_tests_before_applying_and_has_a_compositor_owned_deadline() {
    let mut server = server();
    let before = server.display_snapshot();
    let heads = proposed(&server, 2.0);
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: before.revision,
            heads,
        },
    );
    assert!(server.display_snapshot().preview.is_none());
    assert!(apply_next(&mut server).test);
    assert_eq!(before.heads, server.display_snapshot().heads);
    assert!(!apply_next(&mut server).test);
    let after = server.display_snapshot();
    assert_ne!(before.revision, after.revision);
    assert_eq!(after.heads[0].scale, 2.0);
    assert_eq!(after.outputs[0].bounds.size(), [320.0, 240.0]);
    let confirmation = after.preview.unwrap();
    assert!(!confirmation.reverting);
    assert!(
        confirmation
            .deadline
            .saturating_duration_since(Instant::now())
            > Duration::from_secs(14)
    );
    assert!(
        server
            .display_timeout()
            .is_some_and(|timeout| timeout <= Duration::from_secs(15))
    );
}

#[test]
fn keep_persists_once_and_a_save_failure_keeps_the_live_confirmation_available() {
    let mut server = server();
    let token = preview(&mut server);
    assert_eq!(server.display_preview_token(), Some(token));
    assert_eq!(server.kept_display_token(), None);
    server.display_request(
        DisplayRequest {
            id: 2,
            action: DisplayAction::Keep { token },
        },
        |_| Err(io::Error::other("injected write failure")),
    );
    let snapshot = server.display_snapshot();
    assert!(snapshot.preview.is_some());
    assert!(
        matches!(snapshot.response.unwrap().outcome, DisplayOutcome::Failed(reason) if reason.contains("injected write failure"))
    );
    let mut writes = 0;
    server.display_request(
        DisplayRequest {
            id: 3,
            action: DisplayAction::Keep { token },
        },
        |heads| {
            assert_eq!(heads[0].scale, 2.0);
            writes += 1;
            Ok(())
        },
    );
    assert_eq!(writes, 1);
    assert_eq!(server.kept_display_token(), Some(token));
    assert!(server.display_snapshot().preview.is_none());
    assert_eq!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Kept
    );
    assert!(server.display_timeout().is_none());
    submit(
        &mut server,
        DisplayAction::Identify {
            output: OutputId(1),
        },
    );
    assert_eq!(server.kept_display_token(), Some(token));
    submit(&mut server, DisplayAction::Keep { token });
    assert!(matches!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Failed(_)
    ));
    server.advance_display(Instant::now() + Duration::from_secs(60));
    assert!(server.output_requests().is_empty());
    assert_eq!(server.display_snapshot().heads[0].scale, 2.0);
}

#[test]
fn expiry_reverts_without_ui_frames_and_does_not_require_a_live_protocol_object() {
    let mut server = server();
    let before = server.display_snapshot();
    preview(&mut server);
    let deadline = server.display_snapshot().preview.unwrap().deadline;
    server.advance_display(deadline);
    assert!(server.display_snapshot().preview.unwrap().reverting);
    assert!(!apply_next(&mut server).test);
    let after = server.display_snapshot();
    assert!(after.preview.is_none());
    assert_eq!(after.heads, before.heads);
    assert_eq!(after.outputs, before.outputs);
    assert_eq!(after.response.unwrap().outcome, DisplayOutcome::Reverted);
    assert!(server.display_timeout().is_none());
}

#[test]
fn explicit_revert_rejects_stale_tokens_and_keeps_the_original_plan_until_completion() {
    let mut server = server();
    let before = server.display_snapshot();
    let token = preview(&mut server);
    submit(&mut server, DisplayAction::Revert { token: token + 1 });
    assert!(!server.display_snapshot().preview.unwrap().reverting);
    assert!(server.output_requests().is_empty());
    submit(&mut server, DisplayAction::Revert { token });
    assert_eq!(server.display_snapshot().heads[0].scale, 2.0);
    apply_next(&mut server);
    assert_eq!(server.display_snapshot().heads, before.heads);
    assert_eq!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Reverted
    );
}

#[test]
fn a_failed_preview_test_never_applies_and_failed_apply_restores_the_baseline() {
    let mut server = server();
    let before = server.display_snapshot();
    let heads = proposed(&server, 2.0);
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: before.revision,
            heads: heads.clone(),
        },
    );
    let request = server.output_requests().pop().unwrap();
    assert!(request.test);
    server.output_configuration_finished(request.token, false);
    assert!(server.output_requests().is_empty());
    assert_eq!(server.display_snapshot().heads, before.heads);
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: before.revision,
            heads,
        },
    );
    apply_next(&mut server);
    let request = server.output_requests().pop().unwrap();
    assert!(!request.test);
    server.output_configuration_finished(request.token, false);
    apply_next(&mut server);
    assert_eq!(server.display_snapshot().heads, before.heads);
    assert!(
        matches!(server.display_snapshot().response.unwrap().outcome, DisplayOutcome::Failed(reason) if reason.contains("restored"))
    );
}

#[test]
fn stale_modes_ids_layouts_and_all_disabled_drafts_are_rejected_before_backend_work() {
    let mut server = server();
    let snapshot = server.display_snapshot();
    let mut drafts = Vec::new();
    drafts.push(BTreeMap::from([(OutputId(1), None)]));
    drafts.push(BTreeMap::from([(
        OutputId(99),
        snapshot.heads[0].settings(),
    )]));
    for change in [0, 1, 2, 3, 4] {
        let mut heads = snapshot.configuration();
        let settings = heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap();
        match change {
            0 => settings.mode.size = [999, 777],
            1 => settings.mode.refresh_mhz += 1,
            2 => settings.scale = 1.234,
            3 => settings.position = [-1, 0],
            _ => settings.position = [16384, 0],
        }
        drafts.push(heads);
    }
    for heads in drafts {
        submit(
            &mut server,
            DisplayAction::Preview {
                revision: snapshot.revision,
                heads,
            },
        );
        assert!(matches!(
            server.display_snapshot().response.unwrap().outcome,
            DisplayOutcome::Failed(_)
        ));
        assert!(server.output_requests().is_empty());
    }
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: snapshot.revision + 1,
            heads: snapshot.configuration(),
        },
    );
    assert!(server.output_requests().is_empty());
    assert_eq!(server.display_snapshot().heads, snapshot.heads);
}

#[test]
fn parent_window_bounds_reject_a_native_preview_without_touching_virtual_heads() {
    let mut server = server();
    let snapshot = server.display_snapshot();
    let mut heads = snapshot.configuration();
    heads
        .get_mut(&OutputId(1))
        .unwrap()
        .as_mut()
        .unwrap()
        .position = [1, 0];
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: snapshot.revision,
            heads,
        },
    );
    let request = server.output_requests().pop().unwrap();
    assert!(
        !server
            .apply_virtual_output_configuration(&request, [640, 480])
            .unwrap()
    );
    server.output_configuration_finished(request.token, false);
    assert!(server.output_requests().is_empty());
    assert_eq!(server.display_snapshot().heads, snapshot.heads);
}

#[test]
fn hotplug_discards_stale_native_work_and_recovers_on_an_available_connector() {
    let mut server = server();
    preview(&mut server);
    server
        .configure_outputs(&[Monitor {
            id: OutputId(8),
            name: "DP-2".into(),
            origin: [0, 0],
            size: [640, 480],
        }])
        .unwrap();
    server.registered_output_catalog().unwrap();
    server.advance_display(Instant::now());
    let recovery = apply_next(&mut server);
    assert_eq!(
        recovery.heads.keys().copied().collect::<Vec<_>>(),
        vec![OutputId(8)]
    );
    let after = server.display_snapshot();
    assert!(after.preview.is_none());
    assert_eq!(after.heads[0].id, OutputId(8));
    assert!(after.heads[0].current.is_some());
    assert_eq!(after.response.unwrap().outcome, DisplayOutcome::Reverted);
}

#[cfg(feature = "session")]
#[test]
fn output_suspension_expires_confirmation_without_ui_and_cannot_keep_a_stale_preview() {
    let mut server = server();
    let token = preview(&mut server);
    let deadline = server.display_snapshot().preview.unwrap().deadline;
    server.disconnect_outputs();
    server.output_catalog(Vec::new()).unwrap();
    server.advance_display(deadline);
    for _ in 0..3 {
        let requests = server.output_requests();
        if requests.is_empty() { break; }
        for request in requests { server.output_configuration_finished(request.token, false); }
    }
    let headless = server.display_snapshot();
    assert!(headless.heads.is_empty());
    assert!(headless.outputs.is_empty());
    assert!(headless.preview.is_none());
    assert!(server.display_timeout().is_none());
    assert!(server.output_requests().is_empty());
    assert!(matches!(headless.response.unwrap().outcome, DisplayOutcome::Failed(_)));
    server.configure_outputs(&[Monitor {
        id: OutputId(8),
        name: "Recovery-8".into(),
        origin: [0, 0],
        size: [640, 480],
    }]).unwrap();
    server.registered_output_catalog().unwrap();
    server.advance_display(deadline + Duration::from_secs(60));
    let restored = server.display_snapshot();
    assert_eq!(restored.heads.len(), 1);
    assert_eq!(restored.heads[0].id, OutputId(8));
    assert_eq!(restored.heads[0].scale, 1.0);
    assert!(restored.preview.is_none());
    submit(&mut server, DisplayAction::Keep { token });
    let after = server.display_snapshot();
    assert_eq!(after.heads, restored.heads);
    assert!(matches!(after.response.unwrap().outcome, DisplayOutcome::Failed(_)));
    assert!(server.output_requests().is_empty());
    assert!(server.display_timeout().is_none());
}

#[test]
fn a_revision_change_between_test_and_apply_cannot_apply_the_old_native_draft() {
    let mut server = server();
    let snapshot = server.display_snapshot();
    let heads = proposed(&server, 2.0);
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: snapshot.revision,
            heads,
        },
    );
    apply_next(&mut server);
    let request = server.output_requests().pop().unwrap();
    let mut heads = snapshot.heads;
    heads[0].modes.push(OutputMode {
        size: [320, 240],
        refresh_mhz: 75000,
        preferred: false,
    });
    server.output_catalog(heads).unwrap();
    assert!(!server.output_configuration_current(&request));
    assert!(
        !server
            .apply_virtual_output_configuration(&request, [640, 480])
            .unwrap()
    );
    server.output_configuration_finished(request.token, false);
    apply_next(&mut server);
    assert_eq!(server.display_snapshot().heads[0].scale, 1.0);
}

#[test]
fn a_forged_backend_token_cannot_bypass_either_native_or_protocol_request_ownership() {
    let mut server = server();
    let snapshot = server.display_snapshot();
    submit(
        &mut server,
        DisplayAction::Test {
            revision: snapshot.revision,
            heads: snapshot.configuration(),
        },
    );
    let mut request = server.output_requests().pop().unwrap();
    let token = request.token;
    request.token += 1;
    assert!(!server.output_configuration_current(&request));
    assert!(
        !server
            .apply_virtual_output_configuration(&request, [640, 480])
            .unwrap()
    );
    server.output_configuration_finished(request.token, true);
    request.token = token;
    assert!(server.output_configuration_current(&request));
    server.output_configuration_finished(token, true);
}

#[test]
fn recovery_retries_are_bounded_and_do_not_leave_a_busy_confirmation_timer() {
    let mut server = server();
    let token = preview(&mut server);
    submit(&mut server, DisplayAction::Revert { token });
    for _ in 0..3 {
        let request = server.output_requests().pop().unwrap();
        server.output_configuration_finished(request.token, false);
    }
    let after = server.display_snapshot();
    assert!(after.preview.is_none());
    assert!(matches!(
        after.response.unwrap().outcome,
        DisplayOutcome::Failed(_)
    ));
    assert!(server.output_requests().is_empty());
    assert!(server.display_timeout().is_none());
}

#[test]
fn identification_uses_the_named_output_and_expires_in_the_compositor() {
    let mut server = server();
    submit(
        &mut server,
        DisplayAction::Identify {
            output: OutputId(99),
        },
    );
    assert!(server.display_snapshot().identification.is_none());
    submit(
        &mut server,
        DisplayAction::Identify {
            output: OutputId(1),
        },
    );
    let identification = server.display_snapshot().identification.unwrap();
    assert_eq!(identification.output, OutputId(1));
    server.advance_display(identification.deadline);
    assert!(server.display_snapshot().identification.is_none());
}

#[test]
fn closing_the_editor_cancels_pending_test_and_invalidates_an_already_dequeued_apply() {
    let mut server = server();
    let before = server.display_snapshot();
    let heads = proposed(&server, 2.0);
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: before.revision,
            heads: heads.clone(),
        },
    );
    submit(&mut server, DisplayAction::Cancel { request: 1 });
    assert!(server.output_requests().is_empty());
    assert_eq!(server.display_snapshot().heads, before.heads);
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: before.revision,
            heads,
        },
    );
    apply_next(&mut server);
    let abandoned = server.output_requests().pop().unwrap();
    submit(&mut server, DisplayAction::Cancel { request: 3 });
    assert!(!server.output_configuration_current(&abandoned));
    assert!(
        !server
            .apply_virtual_output_configuration(&abandoned, [640, 480])
            .unwrap()
    );
    server.output_configuration_finished(abandoned.token, false);
    apply_next(&mut server);
    assert_eq!(server.display_snapshot().heads, before.heads);
    assert!(server.display_snapshot().preview.is_none());
}

#[test]
fn closing_after_apply_uses_the_same_rollback_as_confirmation_expiry() {
    let mut server = server();
    let before = server.display_snapshot();
    preview(&mut server);
    submit(&mut server, DisplayAction::Cancel { request: 1 });
    apply_next(&mut server);
    assert_eq!(server.display_snapshot().heads, before.heads);
    assert_eq!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Reverted
    );
}

#[test]
fn repeated_request_ids_cannot_restart_or_replace_an_active_native_transaction() {
    let mut server = server();
    let snapshot = server.display_snapshot();
    let action = DisplayAction::Preview {
        revision: snapshot.revision,
        heads: proposed(&server, 2.0),
    };
    server.display_request(
        DisplayRequest {
            id: 1,
            action: action.clone(),
        },
        |_| unreachable!(),
    );
    server.display_request(DisplayRequest { id: 1, action }, |_| unreachable!());
    assert!(matches!(
        server.display_snapshot().response.unwrap().outcome,
        DisplayOutcome::Failed(_)
    ));
    assert!(apply_next(&mut server).test);
    apply_next(&mut server);
    assert!(server.display_snapshot().preview.is_some());
    assert_eq!(server.display_snapshot().heads[0].scale, 2.0);
}

#[test]
fn a_selected_virtual_refresh_rate_reaches_the_public_wayland_output_metadata() {
    let mut server = server();
    let mut heads = server.display_snapshot().heads;
    heads[0].modes.push(OutputMode {
        size: [320, 240],
        refresh_mhz: 75000,
        preferred: false,
    });
    server.output_catalog(heads).unwrap();
    let snapshot = server.display_snapshot();
    let mut desired = snapshot.configuration();
    desired
        .get_mut(&OutputId(1))
        .unwrap()
        .as_mut()
        .unwrap()
        .mode = snapshot.heads[0].modes[1].clone();
    submit(
        &mut server,
        DisplayAction::Preview {
            revision: snapshot.revision,
            heads: desired,
        },
    );
    apply_next(&mut server);
    apply_next(&mut server);
    let mode = server.output(OutputId(1)).unwrap().current_mode().unwrap();
    assert_eq!([mode.size.w, mode.size.h], [320, 240]);
    assert_eq!(mode.refresh, 75000);
}

#[test]
#[cfg(feature = "nested")]
fn modeled_hdr_confirmation_refuses_unobserved_mismatched_and_pending_keep_preserves_failed_save_and_reverts_or_reloads_confirmed_intent() {
    use lince_interface::host::{color::{ColorMode, OutputColor}, display::{AdaptiveSyncStatus, AdaptiveSyncSupport, ColorStatus}};
    let hdr = OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 };
    let directory = tempfile::tempdir().unwrap();
    for (index, (effective, pending, ready)) in [
        (None, true, false), (Some(OutputColor::default()), false, false),
        (Some(OutputColor { peak_nits: 900, ..hdr }), false, false),
        (Some(hdr), true, false), (Some(hdr), false, true),
    ].into_iter().enumerate() {
        let path = directory.path().join(format!("color-{index}.json"));
        let initial = br#"{"keyboard":{"layout":"br"},"outputs":{"disconnected":{"enabled":false,"color":"hdr10","adaptive_sync":true}}}"#;
        std::fs::write(&path, initial).unwrap();
        let mut server = server();
        let before = server.display_snapshot();
        let name = before.heads[0].name.clone();
        server.output_color(OutputId(1), ColorStatus { hdr10: true, prepared: Some(OutputColor::default()),
            effective: Some(OutputColor::default()), revision: 1, ..Default::default() });
        let mut heads = before.configuration();
        let candidate = heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap();
        candidate.color = ColorMode::Hdr10; candidate.adaptive_sync = true;
        submit(&mut server, DisplayAction::Preview { revision: before.revision, heads });
        let test = server.output_requests().pop().unwrap();
        assert!(test.test);
        server.output_configuration_finished(test.token, true);
        let apply = server.output_requests().pop().unwrap();
        assert!(!apply.test);
        let mut modeled = before.heads.clone();
        modeled[0].color = ColorMode::Hdr10; modeled[0].adaptive_sync = true;
        server.output_catalog(modeled).unwrap();
        server.output_color(OutputId(1), ColorStatus { hdr10: true, prepared: Some(hdr), effective, pending, revision: 2, ..Default::default() });
        server.output_adaptive_sync(OutputId(1), AdaptiveSyncStatus { support: AdaptiveSyncSupport::Supported,
            prepared: Some(true), effective: Some(false), pending: true, reason: None });
        server.output_configuration_finished(apply.token, true);
        let preview = server.display_snapshot().preview.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), initial);
        let mut writes = 0;
        let id = server.display_snapshot().last_request + 1;
        server.display_request(DisplayRequest { id, action: DisplayAction::Keep { token: preview.token } }, |_| {
            writes += 1;
            Err(io::Error::other("controlled color settings write failure"))
        });
        assert_eq!(writes, usize::from(ready));
        assert_eq!(server.display_preview_token(), Some(preview.token));
        assert_eq!(server.display_snapshot().preview.unwrap().deadline, preview.deadline);
        assert_eq!(std::fs::read(&path).unwrap(), initial);
        let outcome = server.display_snapshot().response.unwrap().outcome;
        assert!(matches!(outcome, DisplayOutcome::Failed(ref reason) if if ready { reason.contains("write failure") } else { reason.contains("HDR output") }));
        if ready {
            let id = server.display_snapshot().last_request + 1;
            server.display_request(DisplayRequest { id, action: DisplayAction::Keep { token: preview.token } }, |heads| {
                writes += 1;
                crate::commands::Commands::save_outputs(&path, heads).map(|_| ())
            });
            assert_eq!(writes, 2);
            assert_eq!(server.display_snapshot().response.unwrap().outcome, DisplayOutcome::Kept);
            assert_eq!(server.kept_display_token(), Some(preview.token));
            let loaded = crate::commands::Commands::load(&path).unwrap();
            assert_eq!(loaded.outputs[&name].color, ColorMode::Hdr10);
            assert!(loaded.outputs[&name].adaptive_sync);
            assert_eq!(loaded.keyboard.layout, "br");
            assert_eq!(loaded.outputs["disconnected"].color, ColorMode::Hdr10);
            assert!(!loaded.outputs["disconnected"].enabled);
            submit(&mut server, DisplayAction::Keep { token: preview.token });
            assert!(matches!(server.display_snapshot().response.unwrap().outcome, DisplayOutcome::Failed(_)));
        } else {
            submit(&mut server, DisplayAction::Revert { token: preview.token });
            let recovery = server.output_requests().pop().unwrap();
            assert!(server.output_configuration_recovery(&recovery));
            assert!(server.apply_virtual_output_configuration(&recovery, [640, 480]).unwrap());
            server.output_configuration_finished(recovery.token, true);
            assert_eq!(server.display_snapshot().configuration(), before.configuration());
            assert!(server.display_snapshot().preview.is_none());
            assert_eq!(std::fs::read(&path).unwrap(), initial);
        }
    }
}

#[test]
fn twenty_later_head_failures_reject_old_completions_and_restore_the_entire_multihead_configuration_once() {
    let bounds = [1440u32, 960];
    let mut server = Server::new(bounds.map(|value| value as i32)).unwrap();
    server.configure_scaled_outputs(&[
        Monitor { id: OutputId(1), name: "Rollback-A".into(), origin: [0, 0], size: [640, 480] },
        Monitor { id: OutputId(2), name: "Rollback-B".into(), origin: [640, 0], size: [640, 480] },
    ], &BTreeMap::from([(OutputId(1), 1.0), (OutputId(2), 1.0)])).unwrap();
    server.registered_output_catalog().unwrap();
    let original = server.display_snapshot();
    for _ in 0..20 {
        let before = server.display_snapshot();
        let mut proposed = before.configuration();
        let first = proposed.get_mut(&OutputId(1)).unwrap().as_mut().unwrap();
        first.rotation = lince_interface::host::mapping::QuarterTurn::Clockwise90;
        first.scale = 1.25;
        proposed.get_mut(&OutputId(2)).unwrap().as_mut().unwrap().position = [700, 20];
        submit(&mut server, DisplayAction::Preview { revision: before.revision, heads: proposed.clone() });
        let test = server.output_requests().pop().unwrap();
        assert!(test.test);
        assert!(server.apply_virtual_output_configuration(&test, bounds).unwrap());
        server.output_configuration_finished(test.token, true);
        let apply = server.output_requests().pop().unwrap();
        assert!(!apply.test);
        let mut partial = before.configuration();
        partial.insert(OutputId(1), proposed[&OutputId(1)].clone());
        assert!(server.install_virtual_outputs(before.heads.clone(), &partial, bounds, false).unwrap());
        assert_ne!(server.display_snapshot().configuration(), before.configuration());
        server.output_configuration_finished(apply.token, false);
        let recovery = server.output_requests().pop().unwrap();
        assert!(server.output_configuration_recovery(&recovery));
        assert!(!server.output_configuration_current(&apply));
        server.output_configuration_finished(apply.token, true);
        assert!(server.output_configuration_current(&recovery));
        assert!(server.apply_virtual_output_configuration(&recovery, bounds).unwrap());
        server.output_configuration_finished(recovery.token, true);
        let after = server.display_snapshot();
        assert_eq!(after.configuration(), original.configuration());
        assert_eq!(after.outputs, original.outputs);
        assert!(after.preview.is_none());
        assert!(matches!(after.response.unwrap().outcome, DisplayOutcome::Failed(_)));
        server.output_configuration_finished(apply.token, true);
        server.output_configuration_finished(recovery.token, true);
        assert_eq!(server.display_snapshot().configuration(), original.configuration());
        assert!(server.output_requests().is_empty());
        assert!(server.display_timeout().is_none());
    }
}
