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
    manager: Option<Manager>, heads: Vec<Head>, modes: Vec<(Head, Mode)>, serial: u32,
    outcomes: Vec<&'static str>, done: usize,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, version: u32, qh: &QueueHandle<Client>) {
    state.output_control.manager = Some(registry.bind(name, version.min(3), qh, ()));
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
        if let head::Event::Mode { mode } = event { state.output_control.modes.push((head.clone(), mode)); }
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
    let catalog = vec![OutputHead { id: OutputId(1), name: "Park".into(),
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
