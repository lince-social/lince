use super::*;
use wayland_protocols_wlr::output_management::v1::client::{
    zwlr_output_manager_v1::{self as manager, ZwlrOutputManagerV1 as Manager},
    zwlr_output_head_v1::{self as head, ZwlrOutputHeadV1 as Head},
    zwlr_output_mode_v1::{self as mode, ZwlrOutputModeV1 as Mode},
    zwlr_output_configuration_v1::{self as configuration, ZwlrOutputConfigurationV1 as Configuration},
    zwlr_output_configuration_head_v1::ZwlrOutputConfigurationHeadV1 as Settings,
};

#[derive(Default)]
pub(super) struct Capture {
    pub(super) manager: Option<Manager>, pub(super) heads: Vec<Head>, pub(super) modes: Vec<(Head, Mode)>, pub(super) serial: u32,
    pub(super) outcomes: Vec<&'static str>, done: usize,
    transforms: Vec<(Head, wl_output::Transform)>,
    adaptive_sync: Vec<(Head, head::AdaptiveSyncState)>,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, version: u32, qh: &QueueHandle<Client>) {
    state.output_control.manager = Some(registry.bind(name, version.min(4), qh, ()));
}

impl Dispatch<Manager, ()> for Client {
    fn event(state: &mut Self, _: &Manager, event: manager::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            manager::Event::Head { head } => state.output_control.heads.push(head),
            manager::Event::Done { serial } => { state.output_control.serial = serial; state.output_control.done += 1; }
            _ => {}
        }
    }
    wayland_client::event_created_child!(Client, Manager, [0 => (Head, ())]);
}

impl Dispatch<Head, ()> for Client {
    fn event(state: &mut Self, head: &Head, event: head::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            head::Event::Mode { mode } => state.output_control.modes.push((head.clone(), mode)),
            head::Event::Transform { transform: wayland_client::WEnum::Value(transform) } => state.output_control.transforms.push((head.clone(), transform)),
            head::Event::AdaptiveSync { state: wayland_client::WEnum::Value(adaptive) } => state.output_control.adaptive_sync.push((head.clone(), adaptive)),
            _ => {}
        }
    }
    wayland_client::event_created_child!(Client, Head, [3 => (Mode, ())]);
}

impl Dispatch<Mode, ()> for Client {
    fn event(_: &mut Self, _: &Mode, _: mode::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<Configuration, ()> for Client {
    fn event(state: &mut Self, _: &Configuration, event: configuration::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            configuration::Event::Succeeded => state.output_control.outcomes.push("succeeded"),
            configuration::Event::Failed => state.output_control.outcomes.push("failed"),
            configuration::Event::Cancelled => state.output_control.outcomes.push("cancelled"),
            _ => {}
        }
    }
}

delegate_noop!(Client: ignore Settings);

fn harness() -> Harness {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![crate::launch::Capability::OutputControl]));
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    harness
}

fn configuration(harness: &Harness) -> (Configuration, Settings) {
    let capture = &harness.client.output_control;
    let object = capture.manager.as_ref().unwrap().create_configuration(capture.serial, &harness.queue.handle(), ());
    let head = object.enable_head(&capture.heads[0], &harness.queue.handle(), ());
    (object, head)
}

#[test]
fn real_output_requests_publish_each_quarter_turn_and_reject_flipped_transforms_without_partial_state() {
    for (transform, rotation) in [(wl_output::Transform::Normal, lince_interface::host::mapping::QuarterTurn::Normal),
        (wl_output::Transform::_90, lince_interface::host::mapping::QuarterTurn::Clockwise90),
        (wl_output::Transform::_180, lince_interface::host::mapping::QuarterTurn::UpsideDown),
        (wl_output::Transform::_270, lince_interface::host::mapping::QuarterTurn::Clockwise270)] {
        let mut harness = harness();
        let (object, settings) = configuration(&harness);
        settings.set_transform(transform);
        settings.set_scale(2.0);
        object.apply(); harness.sync();
        let request = harness.server.output_requests().pop().unwrap();
        assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
        harness.server.output_configuration_finished(request.token, true); harness.sync();
        let viewport = harness.server.output_viewports().pop().unwrap().1;
        assert_eq!(viewport.rotation, rotation);
        assert_eq!(viewport.source_size(), if rotation.swaps_axes() { [480, 640] } else { [640, 480] });
        assert_eq!(viewport.bounds.size(), viewport.source_size().map(|size| f64::from(size) / 2.0));
        assert_eq!(harness.client.output_control.transforms.last().unwrap().1, transform);
        assert_eq!(harness.client.output_control.outcomes, vec!["succeeded"]);
        object.destroy(); harness.sync();
        let before = harness.server.display_snapshot().configuration();
        let (object, settings) = configuration(&harness);
        settings.set_transform(wl_output::Transform::Flipped);
        object.apply(); harness.sync();
        assert!(harness.server.output_requests().is_empty());
        assert_eq!(harness.client.output_control.outcomes.last(), Some(&"failed"));
        assert_eq!(harness.server.display_snapshot().configuration(), before);
        object.destroy(); harness.sync();
    }
}

#[test]
fn adaptive_sync_v4_reports_observed_state_and_nested_enable_fails_without_changing_the_output() {
    let mut harness = harness();
    assert_eq!(harness.client.output_control.manager.as_ref().unwrap().version(), 4);
    assert_eq!(harness.client.output_control.adaptive_sync.last().unwrap().1, head::AdaptiveSyncState::Disabled);
    let before = harness.server.display_snapshot();
    let (object, settings) = configuration(&harness);
    settings.set_adaptive_sync(head::AdaptiveSyncState::Enabled);
    object.apply(); harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(!harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, false); harness.sync();
    assert_eq!(harness.client.output_control.outcomes, vec!["failed"]);
    assert_eq!(harness.server.display_snapshot().configuration(), before.configuration());
    assert_eq!(harness.server.display_snapshot().outputs, before.outputs);
    object.destroy(); harness.sync();
    harness.server.output_adaptive_sync(OutputId(1), lince_interface::host::display::AdaptiveSyncStatus {
        support: lince_interface::host::display::AdaptiveSyncSupport::Supported, prepared: Some(true), effective: Some(false), pending: true, reason: None });
    harness.sync();
    assert_eq!(harness.client.output_control.adaptive_sync.last().unwrap().1, head::AdaptiveSyncState::Disabled);
    harness.server.output_adaptive_sync(OutputId(1), lince_interface::host::display::AdaptiveSyncStatus {
        support: lince_interface::host::display::AdaptiveSyncSupport::Supported, prepared: Some(true), effective: Some(true), pending: false, reason: None });
    harness.sync();
    assert_eq!(harness.client.output_control.adaptive_sync.last().unwrap().1, head::AdaptiveSyncState::Enabled);
    assert_eq!(harness.server.display_snapshot().revision, before.revision);
    harness.client.output_control.manager.as_ref().unwrap().stop(); harness.sync();
    let done = harness.client.output_control.done;
    let events = harness.client.output_control.adaptive_sync.len();
    harness.server.output_adaptive_sync(OutputId(1), lince_interface::host::display::AdaptiveSyncStatus {
        support: lince_interface::host::display::AdaptiveSyncSupport::Supported, prepared: Some(false), effective: Some(false), pending: false, reason: None });
    harness.sync();
    assert_eq!(harness.client.output_control.done, done);
    assert_eq!(harness.client.output_control.adaptive_sync.len(), events);
}

#[test]
fn output_control_is_available_only_to_the_explicit_capability() {
    for capability in [None, Some(crate::launch::Capability::WindowList), Some(crate::launch::Capability::WindowControl), Some(crate::launch::Capability::OutputControl)] {
        let harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(capability.into_iter().collect()));
        assert_eq!(harness.client.output_control.manager.is_some(), capability == Some(crate::launch::Capability::OutputControl));
    }
}

#[test]
fn output_test_does_not_change_layout_and_apply_waits_for_backend_confirmation() {
    let mut harness = harness();
    let before = harness.server.output(OutputId(1)).unwrap().current_scale().fractional_scale();
    let (test, head) = configuration(&harness);
    head.set_scale(2.0);
    test.test();
    harness.sync();
    assert!(harness.client.output_control.outcomes.is_empty());
    let request = harness.server.output_requests().pop().unwrap();
    assert!(request.test);
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    assert_eq!(harness.server.output(OutputId(1)).unwrap().current_scale().fractional_scale(), before);
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert_eq!(harness.client.output_control.outcomes, vec!["succeeded"]);
    test.destroy();
    let (apply, head) = configuration(&harness);
    head.set_mode(&harness.client.output_control.modes[0].1);
    head.set_scale(2.0);
    apply.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(!request.test);
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert_eq!(harness.server.output(OutputId(1)).unwrap().current_scale().fractional_scale(), 2.0);
    assert_eq!(harness.client.output_control.outcomes, vec!["succeeded", "succeeded"]);
    let done = harness.client.output_control.done;
    for _ in 0..4 { harness.sync(); }
    assert_eq!(harness.client.output_control.done, done);
    apply.destroy();
    harness.sync();
}

#[test]
fn stale_output_serial_is_cancelled_and_unsupported_requests_leave_layout_unchanged() {
    let mut harness = harness();
    let (stale, _) = configuration(&harness);
    harness.sync();
    let catalog = vec![OutputHead { color: Default::default(), adaptive_sync: Default::default(), rotation: Default::default(), id: OutputId(1), name: "Park".into(),
        modes: vec![OutputMode { size: [640, 480], refresh_mhz: 60_000, preferred: true }],
        current: Some(0), position: [1, 0], scale: 1.0 }];
    harness.server.output_catalog(catalog).unwrap();
    stale.test();
    harness.sync();
    assert_eq!(harness.client.output_control.outcomes, vec!["cancelled"]);
    assert!(harness.server.output_requests().is_empty());
    stale.destroy();
    for invalid in 0..3 {
        let (object, head) = configuration(&harness);
        match invalid {
            0 => head.set_scale(0.125),
            1 => head.set_position(-1, 0),
            _ => head.set_custom_mode(123, 456, 60_000),
        }
        object.apply();
        harness.sync();
        assert!(harness.server.output_requests().is_empty());
        assert_eq!(harness.client.output_control.outcomes.last(), Some(&"failed"));
        assert_eq!(harness.server.output(OutputId(1)).unwrap().current_location().x, 0);
        object.destroy();
        harness.sync();
    }
}

#[test]
fn destroying_configuration_cancels_queued_work_and_lock_denies_new_requests() {
    let mut harness = harness();
    let (object, _) = configuration(&harness);
    object.apply();
    harness.sync();
    object.destroy();
    harness.sync();
    assert!(harness.server.output_requests().is_empty());
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    let (object, _) = configuration(&harness);
    object.test();
    harness.sync();
    assert!(harness.server.output_requests().is_empty());
    assert_eq!(harness.client.output_control.outcomes.last(), Some(&"failed"));
    object.destroy();
    harness.sync();
}

#[test]
fn disabled_outputs_remain_configurable_and_can_be_enabled_again() {
    let mut harness = harness();
    harness.server.configure_outputs(&[
        Monitor { id: OutputId(1), name: "Park-1".into(), origin: [0, 0], size: [320, 480] },
        Monitor { id: OutputId(2), name: "Park-2".into(), origin: [320, 0], size: [320, 480] },
    ]).unwrap();
    harness.server.registered_output_catalog().unwrap();
    harness.sync();
    let heads: Vec<_> = harness.client.output_control.heads.iter().rev().take(2).cloned().collect();
    let manager = harness.client.output_control.manager.as_ref().unwrap().clone();
    let object = manager.create_configuration(harness.client.output_control.serial, &harness.queue.handle(), ());
    object.disable_head(&heads[0]);
    object.enable_head(&heads[1], &harness.queue.handle(), ());
    object.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert!(harness.server.output(OutputId(2)).is_none());
    assert!(harness.server.output(OutputId(1)).is_some());
    object.destroy();
    let object = manager.create_configuration(harness.client.output_control.serial, &harness.queue.handle(), ());
    for head in &heads { object.enable_head(head, &harness.queue.handle(), ()); }
    object.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert!(harness.server.output(OutputId(2)).is_some());
    object.destroy();
    harness.sync();
}

#[test]
fn an_output_change_before_backend_processing_cancels_the_old_request() {
    let mut harness = harness();
    let (object, _) = configuration(&harness);
    object.apply();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    harness.server.output_catalog(Vec::new()).unwrap();
    assert!(!harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, false);
    harness.sync();
    assert_eq!(harness.client.output_control.outcomes, vec!["cancelled"]);
    object.destroy();
    harness.sync();
}

#[test]
fn fixed_point_output_scales_round_to_the_supported_fractional_step() {
    let mut harness = harness();
    let (object, head) = configuration(&harness);
    head.set_scale(1.1);
    object.test();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert_eq!(request.heads[&OutputId(1)].as_ref().unwrap().scale, 1.1);
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    object.destroy();
    harness.sync();
}

#[test]
fn native_confirmation_rejects_external_apply_but_preserves_external_test_semantics() {
    use lince_interface::host::display::{DisplayAction, DisplayRequest};
    let mut harness = harness();
    let snapshot = harness.server.display_snapshot();
    let mut heads = snapshot.configuration();
    heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().scale = 2.0;
    harness.server.display_request(DisplayRequest { id: 1, action: DisplayAction::Preview { revision: snapshot.revision, heads } }, |_| unreachable!());
    for test in [true, false] {
        let request = harness.server.output_requests().pop().unwrap();
        assert_eq!(request.test, test);
        assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
        harness.server.output_configuration_finished(request.token, true);
    }
    harness.sync();
    let (apply, _) = configuration(&harness);
    apply.apply();
    harness.sync();
    assert_eq!(harness.client.output_control.outcomes, vec!["failed"]);
    assert!(harness.server.output_requests().is_empty());
    let (test, _) = configuration(&harness);
    test.test();
    harness.sync();
    let request = harness.server.output_requests().pop().unwrap();
    assert!(request.test);
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    harness.sync();
    assert_eq!(harness.client.output_control.outcomes, vec!["failed", "succeeded"]);
    assert!(harness.server.display_snapshot().preview.is_some());
    apply.destroy();
    test.destroy();
    harness.sync();
}

#[test]
fn native_controller_cannot_persist_a_prepared_hdr_preview_until_its_matching_color_is_observed() {
    use lince_interface::host::{color::{ColorMode, OutputColor}, display::{ColorStatus, DisplayAction, DisplayOutcome, DisplayRequest}};
    let mut harness = harness();
    let before = harness.server.display_snapshot();
    let mut heads = before.configuration();
    heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().color = ColorMode::Hdr10;
    harness.server.display_request(DisplayRequest { id: 1, action: DisplayAction::Preview { revision: before.revision, heads } }, |_| unreachable!());
    let test = harness.server.output_requests().pop().unwrap();
    assert!(test.test);
    harness.server.output_configuration_finished(test.token, true);
    let apply = harness.server.output_requests().pop().unwrap();
    assert!(!apply.test);
    let mut catalog = before.heads;
    catalog[0].color = ColorMode::Hdr10;
    harness.server.output_catalog(catalog).unwrap();
    let pipeline = OutputColor { mode: ColorMode::Hdr10, peak_nits: 1000 };
    let status = ColorStatus { hdr10: true, prepared: Some(pipeline), effective: Some(Default::default()), revision: 4,
        pending: true, ..Default::default() };
    harness.server.output_color(OutputId(1), status.clone());
    harness.server.output_configuration_finished(apply.token, true);
    let token = harness.server.display_snapshot().preview.unwrap().token;
    let revision = harness.server.display_snapshot().revision;
    let mut saves = 0;
    harness.server.display_request(DisplayRequest { id: 2, action: DisplayAction::Keep { token } }, |_| { saves += 1; Ok(()) });
    assert_eq!(saves, 0);
    assert!(matches!(harness.server.display_snapshot().response.unwrap().outcome, DisplayOutcome::Failed(_)));
    assert!(harness.server.display_snapshot().preview.is_some());
    harness.server.output_color(OutputId(1), ColorStatus { pending: false,
        effective: Some(OutputColor { peak_nits: 600, ..pipeline }), ..status.clone() });
    harness.server.display_request(DisplayRequest { id: 3, action: DisplayAction::Keep { token } }, |_| { saves += 1; Ok(()) });
    assert_eq!(saves, 0);
    harness.server.output_color(OutputId(1), ColorStatus { pending: false, effective: Some(pipeline), ..status });
    assert_eq!(harness.server.display_snapshot().revision, revision);
    harness.server.display_request(DisplayRequest { id: 4, action: DisplayAction::Keep { token } }, |heads| {
        assert_eq!(heads[0].color, ColorMode::Hdr10); saves += 1; Ok(())
    });
    assert_eq!(saves, 1);
    assert!(matches!(harness.server.display_snapshot().response.unwrap().outcome, DisplayOutcome::Kept));
    assert!(harness.server.display_snapshot().preview.is_none());
}

#[test]
fn locking_a_native_preview_allows_only_the_trusted_rollback_to_change_outputs() {
    use lince_interface::host::display::{DisplayAction, DisplayOutcome, DisplayRequest};
    let mut harness = harness();
    let before = harness.server.display_snapshot();
    let mut heads = before.configuration();
    heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().scale = 2.0;
    harness.server.display_request(DisplayRequest { id: 1, action: DisplayAction::Preview { revision: before.revision, heads } }, |_| unreachable!());
    for _ in 0..2 {
        let request = harness.server.output_requests().pop().unwrap();
        assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
        harness.server.output_configuration_finished(request.token, true);
    }
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    harness.server.advance_display(std::time::Instant::now());
    let request = harness.server.output_requests().pop().unwrap();
    assert!(harness.server.output_configuration_recovery(&request));
    assert!(harness.server.output_configuration_current(&request));
    assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
    harness.server.output_configuration_finished(request.token, true);
    assert!(harness.server.locked());
    assert_eq!(harness.server.display_snapshot().heads, before.heads);
    harness.server.display_request(DisplayRequest { id: 2, action: DisplayAction::Test { revision: before.revision, heads: before.configuration() } }, |_| unreachable!());
    assert!(matches!(harness.server.display_snapshot().response.unwrap().outcome, DisplayOutcome::Failed(_)));
    assert!(harness.server.output_requests().is_empty());
}

#[test]
fn native_rollback_keeps_priority_when_external_test_requests_fill_the_queue() {
    use lince_interface::host::display::{DisplayAction, DisplayRequest};
    let mut harness = harness();
    let before = harness.server.display_snapshot();
    let mut heads = before.configuration();
    heads.get_mut(&OutputId(1)).unwrap().as_mut().unwrap().scale = 2.0;
    harness.server.display_request(DisplayRequest { id: 1, action: DisplayAction::Preview { revision: before.revision, heads } }, |_| unreachable!());
    for _ in 0..2 {
        let request = harness.server.output_requests().pop().unwrap();
        assert!(harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap());
        harness.server.output_configuration_finished(request.token, true);
    }
    harness.sync();
    let clients: Vec<_> = (0..8).map(|_| {
        let (test, _) = configuration(&harness);
        test.test();
        test
    }).collect();
    harness.sync();
    let deadline = harness.server.display_snapshot().preview.unwrap().deadline;
    harness.server.advance_display(deadline);
    let requests = harness.server.output_requests();
    assert_eq!(requests.len(), 9);
    assert!(harness.server.output_configuration_recovery(&requests[0]));
    for request in requests {
        let accepted = harness.server.apply_virtual_output_configuration(&request, [640, 480]).unwrap();
        harness.server.output_configuration_finished(request.token, accepted);
    }
    assert_eq!(harness.server.display_snapshot().heads, before.heads);
    assert!(harness.server.display_snapshot().preview.is_none());
    for client in clients { client.destroy(); }
    harness.sync();
}
