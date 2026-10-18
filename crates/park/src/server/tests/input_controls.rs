use super::*;
use wayland_protocols::wp::{
    pointer_constraints::zv1::client::{zwp_pointer_constraints_v1::{self as constraints, ZwpPointerConstraintsV1 as Constraints},
        zwp_locked_pointer_v1::{self as locked, ZwpLockedPointerV1 as Locked}, zwp_confined_pointer_v1::ZwpConfinedPointerV1 as Confined},
    relative_pointer::zv1::client::{zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1 as RelativeManager,
        zwp_relative_pointer_v1::{self as relative, ZwpRelativePointerV1 as Relative}},
    keyboard_shortcuts_inhibit::zv1::client::{zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1 as InhibitManager,
        zwp_keyboard_shortcuts_inhibitor_v1::{self as inhibitor, ZwpKeyboardShortcutsInhibitorV1 as Inhibitor}},
};

#[derive(Default)]
pub(super) struct Controls {
    constraints: Option<Constraints>,
    relative: Option<RelativeManager>,
    inhibit: Option<InhibitManager>,
    locked: Vec<bool>,
    inhibited: Vec<bool>,
    deltas: Vec<[f64; 4]>,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, interface: &str, qh: &QueueHandle<Client>) {
    match interface {
        "zwp_pointer_constraints_v1" => state.controls.constraints = Some(registry.bind(name, 1, qh, ())),
        "zwp_relative_pointer_manager_v1" => state.controls.relative = Some(registry.bind(name, 1, qh, ())),
        "zwp_keyboard_shortcuts_inhibit_manager_v1" => state.controls.inhibit = Some(registry.bind(name, 1, qh, ())),
        _ => {}
    }
}

delegate_noop!(Client: ignore Constraints);
delegate_noop!(Client: ignore Confined);
delegate_noop!(Client: ignore RelativeManager);
delegate_noop!(Client: ignore InhibitManager);

impl Dispatch<Locked, ()> for Client {
    fn event(state: &mut Self, _: &Locked, event: locked::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            locked::Event::Locked => state.controls.locked.push(true),
            locked::Event::Unlocked => state.controls.locked.push(false),
            _ => {}
        }
    }
}

impl Dispatch<Inhibitor, ()> for Client {
    fn event(state: &mut Self, _: &Inhibitor, event: inhibitor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            inhibitor::Event::Active => state.controls.inhibited.push(true),
            inhibitor::Event::Inactive => state.controls.inhibited.push(false),
            _ => {}
        }
    }
}

impl Dispatch<Relative, ()> for Client {
    fn event(state: &mut Self, _: &Relative, event: relative::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let relative::Event::RelativeMotion { dx, dy, dx_unaccel, dy_unaccel, .. } = event {
            state.controls.deltas.push([dx, dy, dx_unaccel, dy_unaccel]);
        }
    }
}

#[test]
fn pointer_lock_and_shortcut_escape_release_until_focus_changes() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    harness.server.focus_surface(Some(surface.clone()), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.pointer_motion(Some((surface.clone(), [20.0, 20.0])), 1).unwrap();
    let qh = harness.queue.handle();
    let pointer = harness.client.seat.as_ref().unwrap().get_pointer(&qh, ());
    let locked = harness.client.controls.constraints.as_ref().unwrap().lock_pointer(&window.surface, &pointer, None, constraints::Lifetime::Persistent, &qh, ());
    let inhibitor = harness.client.controls.inhibit.as_ref().unwrap().inhibit_shortcuts(&window.surface, harness.client.seat.as_ref().unwrap(), &qh, ());
    harness.sync();
    assert!(harness.server.pointer_locked());
    assert!(harness.server.shortcuts_inhibited());
    assert_eq!(harness.client.controls.locked, [true]);
    assert_eq!(harness.client.controls.inhibited, [true]);
    harness.server.release_client_controls();
    harness.sync();
    assert!(!harness.server.pointer_locked());
    assert!(!harness.server.shortcuts_inhibited());
    for _ in 0..3 { harness.sync(); }
    assert_eq!(harness.client.controls.locked, [true, false]);
    assert_eq!(harness.client.controls.inhibited, [true, false]);
    harness.server.focus_surface(None, smithay::utils::SERIAL_COUNTER.next_serial());
    harness.sync();
    harness.server.focus_surface(Some(surface), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.sync();
    assert!(harness.server.pointer_locked());
    assert!(harness.server.shortcuts_inhibited());
    locked.destroy();
    inhibitor.destroy();
    harness.sync();
    assert!(!harness.server.pointer_locked());
    assert!(!harness.server.shortcuts_inhibited());
}

#[test]
fn relative_events_require_matching_keyboard_focus_and_stop_during_lock() {
    let mut harness = Harness::new();
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let qh = harness.queue.handle();
    let pointer = harness.client.seat.as_ref().unwrap().get_pointer(&qh, ());
    let relative = harness.client.controls.relative.as_ref().unwrap().get_relative_pointer(&pointer, &qh, ());
    harness.server.pointer_motion(Some((surface.clone(), [20.0, 20.0])), 1).unwrap();
    harness.sync();
    harness.server.focus_surface(None, smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.relative_motion([2.0, -3.0], [1.0, -2.0], 1000);
    harness.sync();
    assert!(harness.client.controls.deltas.is_empty());
    harness.server.focus_surface(Some(surface), smithay::utils::SERIAL_COUNTER.next_serial());
    harness.server.relative_motion([2.0, -3.0], [1.0, -2.0], 2000);
    harness.sync();
    assert_eq!(harness.client.controls.deltas, [[2.0, -3.0, 1.0, -2.0]]);
    harness.server.relative_motion([f64::NAN, 0.0], [0.0; 2], 3000);
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&qh, ());
    harness.sync();
    harness.server.relative_motion([1.0; 2], [1.0; 2], 4000);
    harness.sync();
    assert_eq!(harness.client.controls.deltas.len(), 1);
    relative.destroy();
    harness.sync();
}
