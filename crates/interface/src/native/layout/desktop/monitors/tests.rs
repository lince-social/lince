use super::*;
use crate::host::{
    HostRequest,
    display::{DisplayPreview, DisplayResponse, OutputHead, OutputMode},
};
use std::sync::mpsc::{Receiver, sync_channel};

fn snapshot() -> DisplaySnapshot {
    DisplaySnapshot {
        revision: 7,
        heads: [1, 2]
            .map(|id| OutputHead { color: Default::default(), adaptive_sync: Default::default(), rotation: Default::default(),
                id: OutputId(id),
                name: format!("DP-{id}"),
                modes: vec![
                    OutputMode {
                        size: [1920, 1080],
                        refresh_mhz: 60000,
                        preferred: true,
                    },
                    OutputMode {
                        size: [1920, 1080],
                        refresh_mhz: 144000,
                        preferred: false,
                    },
                ],
                current: Some(0),
                position: [(id as i32 - 1) * 1920, 0],
                scale: 1.0,
            })
            .into(),
        ..default()
    }
}

fn fixture() -> (App, Entity, Receiver<HostRequest>) {
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        bevy::input::InputPlugin,
        crate::native::actions::ActionsPlugin,
        crate::native::shortcuts::EditorPlugin,
    ))
    .init_resource::<Assets<Font>>()
    .init_resource::<crate::native::theme::Typography>()
    .insert_resource(snapshot());
    let (sender, receiver) = sync_channel(16);
    app.insert_resource(HostRequests(sender));
    let root = app.world_mut().spawn(Workspaces::default()).id();
    panel(app.world_mut(), root, root);
    app.update();
    (app, root, receiver)
}

fn refresh(app: &mut App) {
    update(app.world_mut());
}

fn input(app: &mut App, index: usize, value: &str) {
    let entity = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .fields[index];
    app.world_mut()
        .get_mut::<EditableText>(entity)
        .unwrap()
        .editor
        .set_text(value);
}

fn request(receiver: &Receiver<HostRequest>) -> DisplayRequest {
    let HostRequest::Display(request) = receiver.try_recv().unwrap() else {
        panic!("Expected a typed display request");
    };
    request
}

#[test]
fn rotation_and_adaptive_sync_share_the_captured_monitor_transaction_and_cancel_does_not_apply() {
    let (mut app, root, receiver) = fixture();
    app.world_mut().resource_mut::<DisplaySnapshot>().adaptive_sync.insert(OutputId(1), crate::host::display::AdaptiveSyncStatus {
        support: crate::host::display::AdaptiveSyncSupport::RequiresModeset, prepared: Some(false), effective: Some(false), pending: false, reason: None });
    Change::Open.apply(app.world_mut(), root);
    Change::Rotate.apply(app.world_mut(), root);
    Change::AdaptiveSync.apply(app.world_mut(), root);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.draft.size(OutputId(1)), [1080.0, 1920.0]);
    assert_eq!(app.world().get::<bevy::a11y::AccessibilityNode>(modal.rotation.0).unwrap().label(), Some("Rotation: 90° clockwise · change"));
    assert_eq!(app.world().get::<bevy::a11y::AccessibilityNode>(modal.adaptive_sync.0).unwrap().label(), Some("Adaptive sync: On · change"));
    assert!(app.world().get::<Text>(modal.adaptive_sync_status).unwrap().0.contains("active: off"));
    assert!(receiver.try_recv().is_err());
    Change::Apply.apply(app.world_mut(), root);
    let submitted = request(&receiver);
    let DisplayAction::Preview { heads, .. } = submitted.action else { panic!("Expected a monitor preview"); };
    assert_eq!(heads[&OutputId(1)].as_ref().unwrap().rotation, crate::host::mapping::QuarterTurn::Clockwise90);
    assert!(heads[&OutputId(1)].as_ref().unwrap().adaptive_sync);
    assert!(!heads[&OutputId(2)].as_ref().unwrap().adaptive_sync);
    Change::Close.apply(app.world_mut(), root);
    assert_eq!(request(&receiver).action, DisplayAction::Cancel { request: submitted.id });
    assert!(!app.world().resource::<DisplaySnapshot>().heads[0].adaptive_sync);
}

#[test]
fn color_draft_uses_the_captured_display_transaction_without_applying_before_confirmation() {
    use crate::host::{color::ColorMode, display::ColorStatus};
    let (mut app, root, receiver) = fixture();
    app.world_mut().resource_mut::<DisplaySnapshot>().color.insert(OutputId(1), ColorStatus {
        hdr10: true, prepared: Some(Default::default()), effective: Some(Default::default()), revision: 3, ..default() });
    Change::Open.apply(app.world_mut(), root);
    Change::Color.apply(app.world_mut(), root);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(app.world().get::<bevy::a11y::AccessibilityNode>(modal.color.0).unwrap().label(), Some("Color: HDR10 · change"));
    assert!(receiver.try_recv().is_err());
    Change::Apply.apply(app.world_mut(), root);
    let preview = request(&receiver);
    let DisplayAction::Preview { heads, .. } = preview.action else { panic!("Expected a captured preview"); };
    assert_eq!(heads[&OutputId(1)].as_ref().unwrap().color, ColorMode::Hdr10);
    assert_eq!(app.world().resource::<DisplaySnapshot>().heads[0].color, ColorMode::Srgb);
    Change::Close.apply(app.world_mut(), root);
    assert!(matches!(request(&receiver).action, DisplayAction::Cancel { request } if request == preview.id));
}

#[test]
fn an_unsupported_saved_hdr_request_has_a_reason_and_can_be_changed_back_to_sdr() {
    use crate::host::{color::ColorMode, display::ColorStatus};
    let (mut app, root, receiver) = fixture();
    app.world_mut().resource_mut::<DisplaySnapshot>().color.insert(OutputId(1), ColorStatus {
        reason: Some("This host has no physical HDR output control.".into()), ..default() });
    Change::Open.apply(app.world_mut(), root);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(app.world().get::<InteractionDisabled>(modal.color.0).is_some());
    assert!(app.world().get::<Text>(modal.color_status).unwrap().0.contains("no physical HDR"));
    Change::Close.apply(app.world_mut(), root);
    app.world_mut().resource_mut::<DisplaySnapshot>().heads[0].color = ColorMode::Hdr10;
    Change::Open.apply(app.world_mut(), root);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(app.world().get::<InteractionDisabled>(modal.color.0).is_none());
    Change::Color.apply(app.world_mut(), root);
    Change::Test.apply(app.world_mut(), root);
    let DisplayAction::Test { heads, .. } = request(&receiver).action else { panic!("Expected a display test"); };
    assert_eq!(heads[&OutputId(1)].as_ref().unwrap().color, ColorMode::Srgb);
}

#[test]
fn unsupported_adaptive_sync_retains_a_reason_and_an_unmet_saved_request_can_be_turned_off() {
    let (mut app, root, receiver) = fixture();
    app.world_mut().resource_mut::<DisplaySnapshot>().adaptive_sync.insert(OutputId(1), crate::host::display::AdaptiveSyncStatus {
        support: crate::host::display::AdaptiveSyncSupport::Unsupported, reason: Some("Fixture output has no adaptive-sync support.".into()), ..default() });
    Change::Open.apply(app.world_mut(), root);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(app.world().get::<InteractionDisabled>(modal.adaptive_sync.0).is_some());
    Change::AdaptiveSync.apply(app.world_mut(), root);
    assert!(!app.world().resource::<Editor>().modal.as_ref().unwrap().draft.settings[&OutputId(1)].adaptive_sync);
    assert!(app.world().resource::<Editor>().modal.as_ref().unwrap().message.contains("no adaptive-sync support"));
    Change::Close.apply(app.world_mut(), root);
    app.world_mut().resource_mut::<DisplaySnapshot>().heads[0].adaptive_sync = true;
    Change::Open.apply(app.world_mut(), root);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(app.world().get::<InteractionDisabled>(modal.adaptive_sync.0).is_none());
    Change::AdaptiveSync.apply(app.world_mut(), root);
    Change::Test.apply(app.world_mut(), root);
    let DisplayAction::Test { heads, .. } = request(&receiver).action else { panic!("Expected a test request"); };
    assert!(!heads[&OutputId(1)].as_ref().unwrap().adaptive_sync);
}

#[test]
fn applying_the_native_draft_normalizes_positions_and_preserves_the_named_mode_refresh() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    Change::Select(OutputId(2)).apply(app.world_mut(), root);
    input(&mut app, 0, "-1920");
    input(&mut app, 1, "-1080");
    input(&mut app, 2, "1.25");
    Change::Mode(1).apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    let submitted = request(&receiver);
    let DisplayAction::Preview { revision, heads } = submitted.action else {
        panic!("Apply must use the compositor preview transaction");
    };
    assert_eq!(revision, 7);
    assert_eq!(heads[&OutputId(2)].as_ref().unwrap().position, [0, 0]);
    assert_eq!(heads[&OutputId(1)].as_ref().unwrap().position, [1920, 1080]);
    assert_eq!(
        heads[&OutputId(2)].as_ref().unwrap().mode.refresh_mhz,
        144000
    );
    assert_eq!(heads[&OutputId(2)].as_ref().unwrap().scale, 1.25);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(
        app.world()
            .get::<InteractionDisabled>(modal.apply)
            .is_some()
    );
    assert_eq!(modal.selected, OutputId(2));
    assert_eq!(
        app.world()
            .get::<EditableText>(modal.fields[0])
            .unwrap()
            .value()
            .to_string(),
        "0"
    );
}

#[test]
fn status_updates_preserve_unsaved_fields_focus_and_the_underlying_panel() {
    let (mut app, root, _) = fixture();
    let draft = app
        .world_mut()
        .spawn((EditableText::new("underlying unsaved text"), ChildOf(root)))
        .id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(draft, FocusCause::Pressed);
    Change::Open.apply(app.world_mut(), root);
    input(&mut app, 0, "120");
    let fields = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .fields;
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(fields[0], FocusCause::Pressed);
    app.world_mut().resource_mut::<DisplaySnapshot>().response = Some(DisplayResponse {
        request: 99,
        outcome: DisplayOutcome::Identified,
    });
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.fields, fields);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(fields[0]));
    assert_eq!(
        app.world()
            .get::<EditableText>(fields[0])
            .unwrap()
            .value()
            .to_string(),
        "120"
    );
    Change::Close.apply(app.world_mut(), root);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(draft));
    assert_eq!(
        app.world()
            .get::<EditableText>(draft)
            .unwrap()
            .value()
            .to_string(),
        "underlying unsaved text"
    );
    assert!(!crate::native::shortcuts::keyboard_owned(app.world()));
}

#[test]
fn a_hotplug_marks_the_draft_stale_until_explicit_refresh_rebuilds_the_catalogue() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    input(&mut app, 0, "123");
    let original = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .overlay;
    let mut latest = snapshot();
    latest.revision = 8;
    latest.heads[1].id = OutputId(3);
    latest.heads[1].name = "DP-3".into();
    app.insert_resource(latest);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.overlay, original);
    assert_eq!(
        app.world()
            .get::<EditableText>(modal.fields[0])
            .unwrap()
            .value()
            .to_string(),
        "123"
    );
    assert!(
        app.world()
            .get::<InteractionDisabled>(modal.apply)
            .is_some()
    );
    Change::Reload.apply(app.world_mut(), root);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_ne!(modal.overlay, original);
    assert_eq!(modal.draft.origin.revision, 8);
    assert!(modal.rectangles.contains_key(&OutputId(3)));
    assert!(!modal.rectangles.contains_key(&OutputId(2)));
    assert!(receiver.try_recv().is_err());
}

#[test]
fn close_during_apply_cancels_the_pending_request_and_reopening_keeps_request_ids_unique() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    let first = request(&receiver);
    Change::Close.apply(app.world_mut(), root);
    let cancel = request(&receiver);
    assert_eq!(cancel.action, DisplayAction::Cancel { request: first.id });
    assert!(cancel.id > first.id);
    assert!(app.world().resource::<Editor>().modal.is_none());
    Change::Open.apply(app.world_mut(), root);
    Change::Test.apply(app.world_mut(), root);
    assert!(request(&receiver).id > cancel.id);
}

#[test]
fn confirmation_emits_only_the_live_token_and_preserves_the_fields_after_save_failure() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    let first = request(&receiver);
    let fields = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .fields;
    let mut latest = snapshot();
    latest.revision = 8;
    latest.preview = Some(DisplayPreview {
        token: 42,
        deadline: Instant::now() + Duration::from_secs(15),
        reverting: false,
    });
    latest.response = Some(DisplayResponse {
        request: first.id,
        outcome: DisplayOutcome::Previewed,
    });
    app.insert_resource(latest.clone());
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert!(app.world().get::<InteractionDisabled>(modal.keep).is_none());
    Change::Keep.apply(app.world_mut(), root);
    let keep = request(&receiver);
    assert_eq!(keep.action, DisplayAction::Keep { token: 42 });
    latest.response = Some(DisplayResponse {
        request: keep.id,
        outcome: DisplayOutcome::Failed("injected save failure".into()),
    });
    app.insert_resource(latest);
    refresh(&mut app);
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.fields, fields);
    assert!(app.world().get::<InteractionDisabled>(modal.keep).is_none());
    assert!(
        app.world()
            .get::<Text>(modal.status)
            .unwrap()
            .0
            .contains("injected save failure")
    );
}

#[test]
fn invalid_fields_and_all_disabled_outputs_cannot_emit_an_apply() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    input(&mut app, 2, "NaN");
    Change::Apply.apply(app.world_mut(), root);
    refresh(&mut app);
    assert!(receiver.try_recv().is_err());
    input(&mut app, 2, "1");
    Change::Enable.apply(app.world_mut(), root);
    Change::Select(OutputId(2)).apply(app.world_mut(), root);
    Change::Enable.apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    refresh(&mut app);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn removing_the_overlay_releases_input_and_cancels_pending_backend_work() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    let first = request(&receiver);
    let overlay = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .overlay;
    app.world_mut().despawn(overlay);
    refresh(&mut app);
    assert_eq!(
        request(&receiver).action,
        DisplayAction::Cancel { request: first.id }
    );
    assert!(!crate::native::shortcuts::keyboard_owned(app.world()));
}

#[test]
fn identify_cannot_overwrite_the_reply_awaited_by_a_pending_configuration_action() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    let first = request(&receiver);
    Change::Identify.apply(app.world_mut(), root);
    refresh(&mut app);
    assert!(receiver.try_recv().is_err());
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.pending, Some((first.id, false)));
    assert!(
        app.world()
            .get::<InteractionDisabled>(modal.identify)
            .is_some()
    );
}

#[test]
fn a_single_monitor_disables_alignment_controls_without_a_reference() {
    let (mut app, root, _) = fixture();
    app.world_mut()
        .resource_mut::<DisplaySnapshot>()
        .heads
        .truncate(1);
    Change::Open.apply(app.world_mut(), root);
    refresh(&mut app);
    let controls: Vec<_> = app
        .world_mut()
        .query_filtered::<Entity, With<NeedsReference>>()
        .iter(app.world())
        .collect();
    assert_eq!(controls.len(), 5);
    assert!(
        controls
            .iter()
            .all(|entity| app.world().get::<InteractionDisabled>(*entity).is_some())
    );
}

#[test]
fn an_unchanged_monitor_snapshot_does_not_rewrite_diagram_geometry_or_accessibility() {
    let (mut app, root, _) = fixture();
    Change::Open.apply(app.world_mut(), root);
    refresh(&mut app);
    let rectangle = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .rectangles[&OutputId(1)]
        .0;
    app.world_mut().clear_trackers();
    refresh(&mut app);
    assert!(
        !app.world()
            .entity(rectangle)
            .get_ref::<Node>()
            .unwrap()
            .is_changed()
    );
    assert!(
        !app.world()
            .entity(rectangle)
            .get_ref::<BackgroundColor>()
            .unwrap()
            .is_changed()
    );
    assert!(
        !app.world()
            .entity(rectangle)
            .get_ref::<bevy::a11y::AccessibilityNode>()
            .unwrap()
            .is_changed()
    );
}

#[test]
fn already_queued_controls_cannot_change_a_pending_draft_or_enqueue_a_second_save() {
    let (mut app, root, receiver) = fixture();
    Change::Open.apply(app.world_mut(), root);
    Change::Apply.apply(app.world_mut(), root);
    let first = request(&receiver);
    let settings = app
        .world()
        .resource::<Editor>()
        .modal
        .as_ref()
        .unwrap()
        .draft
        .settings
        .clone();
    for action in [
        Change::Select(OutputId(2)),
        Change::Mode(1),
        Change::Apply,
        Change::Keep,
        Change::Align(1),
    ] {
        action.apply(app.world_mut(), root);
    }
    let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
    assert_eq!(modal.pending, Some((first.id, false)));
    assert_eq!(modal.selected, OutputId(1));
    assert_eq!(modal.draft.settings, settings);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn hdr_and_vrr_controls_keep_requested_prepared_observed_and_saved_confirmation_separate_and_always_allow_revert() {
    use crate::host::{color::{ColorMode, OutputColor}, display::{AdaptiveSyncStatus, AdaptiveSyncSupport, ColorStatus}};
    let hdr = OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 };
    for (prepared, effective, pending, ready) in [
        (None, None, true, false),
        (Some(hdr), None, true, false),
        (Some(hdr), Some(OutputColor::default()), false, false),
        (Some(hdr), Some(OutputColor { peak_nits: 900, ..hdr }), false, false),
        (Some(hdr), Some(hdr), true, false),
        (Some(hdr), Some(hdr), false, true),
    ] {
        let (mut app, root, receiver) = fixture();
        {
            let mut snapshot = app.world_mut().resource_mut::<DisplaySnapshot>();
            snapshot.color.insert(OutputId(1), ColorStatus { hdr10: true, prepared: Some(OutputColor::default()),
                effective: Some(OutputColor::default()), revision: 1, ..default() });
            snapshot.adaptive_sync.insert(OutputId(1), AdaptiveSyncStatus { support: AdaptiveSyncSupport::Supported,
                prepared: Some(false), effective: Some(false), pending: false, reason: None });
        }
        Change::Open.apply(app.world_mut(), root);
        Change::Color.apply(app.world_mut(), root);
        Change::AdaptiveSync.apply(app.world_mut(), root);
        refresh(&mut app);
        let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
        assert!(app.world().get::<Text>(modal.color_status).unwrap().0.contains("last confirmed: SDR sRGB"));
        assert!(app.world().get::<Text>(modal.adaptive_sync_status).unwrap().0.contains("active: off"));
        assert_eq!(modal.draft.settings[&OutputId(1)].color, ColorMode::Hdr10);
        assert!(modal.draft.settings[&OutputId(1)].adaptive_sync);
        assert_eq!(app.world().resource::<DisplaySnapshot>().heads[0].color, ColorMode::Srgb);
        assert!(receiver.try_recv().is_err());
        Change::Test.apply(app.world_mut(), root);
        let tested = request(&receiver);
        let DisplayAction::Test { revision, heads } = tested.action else { panic!("Expected a captured color/VRR Test"); };
        assert_eq!(revision, 7);
        assert_eq!(heads[&OutputId(1)].as_ref().unwrap().color, ColorMode::Hdr10);
        assert!(heads[&OutputId(1)].as_ref().unwrap().adaptive_sync);
        assert_eq!(heads[&OutputId(2)].as_ref().unwrap().color, ColorMode::Srgb);
        app.world_mut().resource_mut::<DisplaySnapshot>().response = Some(DisplayResponse { request: tested.id, outcome: DisplayOutcome::Tested });
        refresh(&mut app);
        Change::Apply.apply(app.world_mut(), root);
        let applied = request(&receiver);
        let DisplayAction::Preview { heads: candidate, .. } = applied.action else { panic!("Expected the tested candidate to reach Preview"); };
        assert_eq!(candidate, heads);
        let fields = app.world().resource::<Editor>().modal.as_ref().unwrap().fields;
        {
            let mut snapshot = app.world_mut().resource_mut::<DisplaySnapshot>();
            snapshot.revision = 8;
            snapshot.heads[0].color = ColorMode::Hdr10;
            snapshot.heads[0].adaptive_sync = true;
            snapshot.preview = Some(DisplayPreview { token: 42, deadline: Instant::now() + Duration::from_secs(15), reverting: false });
            snapshot.response = Some(DisplayResponse { request: applied.id, outcome: DisplayOutcome::Previewed });
            snapshot.color.insert(OutputId(1), ColorStatus { hdr10: true, prepared, effective, pending, revision: 2, ..default() });
            snapshot.adaptive_sync.insert(OutputId(1), AdaptiveSyncStatus { support: AdaptiveSyncSupport::Supported,
                prepared: Some(true), effective: Some(false), pending: true, reason: None });
        }
        refresh(&mut app);
        let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
        assert_eq!(modal.fields, fields);
        assert_eq!(app.world().get::<InteractionDisabled>(modal.keep).is_none(), ready);
        assert!(app.world().get::<Text>(modal.adaptive_sync_status).unwrap().0.contains("active: off"));
        assert!(app.world().get::<Text>(modal.adaptive_sync_status).unwrap().0.contains("awaiting display confirmation"));
        if ready {
            Change::Keep.apply(app.world_mut(), root);
            let keep = request(&receiver);
            assert_eq!(keep.action, DisplayAction::Keep { token: 42 });
            app.world_mut().resource_mut::<DisplaySnapshot>().response = Some(DisplayResponse {
                request: keep.id, outcome: DisplayOutcome::Failed("Could not save the original color/VRR configuration".into()) });
            refresh(&mut app);
            let modal = app.world().resource::<Editor>().modal.as_ref().unwrap();
            assert_eq!(modal.fields, fields);
            assert!(app.world().get::<Text>(modal.status).unwrap().0.contains("Could not save"));
            assert!(app.world().get::<InteractionDisabled>(modal.keep).is_none());
        }
        Change::Revert.apply(app.world_mut(), root);
        assert_eq!(request(&receiver).action, DisplayAction::Revert { token: 42 });
        assert!(receiver.try_recv().is_err());
    }
}
