use super::*;
use lince_interface::host::input::TabletAxes;
use smithay::{backend::input::{TabletToolCapabilities as Caps, TabletToolDescriptor, TabletToolType}, input::pointer::CursorImageStatus,
    wayland::tablet_manager::{TabletDescriptor, TabletSeatTrait}};
use wayland_protocols::wp::tablet::zv2::client::{zwp_tablet_manager_v2 as manager, zwp_tablet_seat_v2 as seat, zwp_tablet_tool_v2 as tool, zwp_tablet_v2 as tablet, zwp_tablet_pad_v2 as pad};

#[path = "tablet/lifetimes.rs"]
mod lifetimes;

#[derive(Debug, PartialEq)]
enum Sample {
    Enter(wl_surface::WlSurface), Motion([f64; 2]), Pressure(u32), Distance(u32), Tilt([f64; 2]), Rotation(f64), Slider(i32), Wheel(f64, i32),
    Down, Up, Button(u32, bool), Out, Frame, Removed,
}

#[derive(Default)]
pub(super) struct Capture {
    pub(in crate::server::tests) manager: Option<manager::ZwpTabletManagerV2>,
    devices: Vec<tablet::ZwpTabletV2>,
    pub(in crate::server::tests) tools: Vec<tool::ZwpTabletToolV2>,
    samples: Vec<(u32, Sample)>,
    pub(in crate::server::tests) serials: BTreeMap<u32, u32>,
    hardware: BTreeMap<u32, u64>,
    removed: Vec<u32>,
    enters: Vec<(u32, u32)>,
}

pub(super) fn bind(state: &mut Client, registry: &wl_registry::WlRegistry, name: u32, _: u32, qh: &QueueHandle<Client>) {
    state.tablet.manager = Some(registry.bind(name, 1, qh, ()));
}

delegate_noop!(Client: ignore manager::ZwpTabletManagerV2);
delegate_noop!(Client: ignore pad::ZwpTabletPadV2);

impl Dispatch<seat::ZwpTabletSeatV2, ()> for Client {
    fn event(state: &mut Self, _: &seat::ZwpTabletSeatV2, event: seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event { seat::Event::TabletAdded { id } => state.tablet.devices.push(id), seat::Event::ToolAdded { id } => state.tablet.tools.push(id), _ => {} }
    }
    wayland_client::event_created_child!(Client, seat::ZwpTabletSeatV2, [
        0 => (tablet::ZwpTabletV2, ()), 1 => (tool::ZwpTabletToolV2, ()), 2 => (pad::ZwpTabletPadV2, ())
    ]);
}

impl Dispatch<tablet::ZwpTabletV2, ()> for Client {
    fn event(state: &mut Self, object: &tablet::ZwpTabletV2, event: tablet::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let tablet::Event::Removed = event { state.tablet.removed.push(object.id().protocol_id()); object.destroy(); }
    }
}

impl Dispatch<tool::ZwpTabletToolV2, ()> for Client {
    fn event(state: &mut Self, object: &tool::ZwpTabletToolV2, event: tool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let id = object.id().protocol_id();
        let sample = match event {
            tool::Event::ProximityIn { serial, tablet, surface, .. } => { state.tablet.serials.insert(id, serial); state.tablet.enters.push((id, tablet.id().protocol_id())); Sample::Enter(surface) }
            tool::Event::Motion { x, y } => Sample::Motion([x, y]),
            tool::Event::Pressure { pressure } => Sample::Pressure(pressure),
            tool::Event::Distance { distance } => Sample::Distance(distance),
            tool::Event::Tilt { tilt_x, tilt_y } => Sample::Tilt([tilt_x, tilt_y]),
            tool::Event::Rotation { degrees } => Sample::Rotation(degrees),
            tool::Event::Slider { position } => Sample::Slider(position),
            tool::Event::Wheel { degrees, clicks } => Sample::Wheel(degrees, clicks),
            tool::Event::Down { .. } => Sample::Down,
            tool::Event::Up => Sample::Up,
            tool::Event::Button { button, state: wayland_client::WEnum::Value(value), .. } => Sample::Button(button, value == tool::ButtonState::Pressed),
            tool::Event::ProximityOut => Sample::Out,
            tool::Event::Frame { .. } => Sample::Frame,
            tool::Event::Removed => { object.destroy(); Sample::Removed }
            tool::Event::HardwareSerial { hardware_serial_hi, hardware_serial_lo } => { state.tablet.hardware.insert(id, (u64::from(hardware_serial_hi) << 32) | u64::from(hardware_serial_lo)); return; }
            _ => return,
        };
        state.tablet.samples.push((id, sample));
    }
}

fn subscribe(harness: &mut Harness) -> seat::ZwpTabletSeatV2 {
    let seat = harness.client.tablet.manager.as_ref().unwrap().get_tablet_seat(harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), ());
    harness.sync();
    seat
}

fn descriptor(serial: u64) -> TabletToolDescriptor {
    TabletToolDescriptor { tool_type: TabletToolType::Pen, hardware_serial: serial, hardware_id_wacom: 0, capabilities: Caps::PRESSURE | Caps::TILT | Caps::DISTANCE | Caps::ROTATION | Caps::SLIDER | Caps::WHEEL }
}

fn admit(harness: &mut Harness, device: u64, id: u64, serial: u64) -> tool::ZwpTabletToolV2 {
    assert!(harness.server.tablet_device(device, TabletDescriptor { name: "An identical tablet".into(), usb_id: Some((1, 2)), syspath: None }));
    assert!(harness.server.tablet_tool(device, id, descriptor(serial)));
    harness.sync();
    harness.client.tablet.tools.last().unwrap().clone()
}

#[test]
fn identical_unserialised_tablets_and_tools_have_independent_objects_and_real_axes() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let first = admit(&mut harness, 1, 11, 0);
    let second = admit(&mut harness, 2, 22, 0);
    assert_eq!(harness.client.tablet.devices.len(), 2);
    assert_ne!(first.id(), second.id());
    assert_eq!(harness.client.tablet.hardware[&first.id().protocol_id()], 0);
    assert_eq!(harness.client.tablet.hardware[&second.id().protocol_id()], 0);
    let axes = TabletAxes { pressure: Some(0.5), distance: Some(0.25), tilt: Some([-12.0, 17.0]), rotation: Some(45.0), slider: Some(-0.5), wheel: Some((15.0, 1)) };
    assert!(harness.server.tablet_motion(11, OutputId(1), [30.0, 40.0], Some((surface.clone(), [10.0, 20.0])), false, axes, 1));
    assert!(harness.server.tablet_tip(11, true, 2).is_some());
    assert!(harness.server.tablet_motion(22, OutputId(1), [80.0, 90.0], Some((surface, [60.0, 70.0])), false, TabletAxes { pressure: Some(0.75), ..Default::default() }, 3));
    harness.sync();
    let samples = harness.client.tablet.samples.iter().filter_map(|(id, sample)| (*id == first.id().protocol_id()).then_some(sample)).collect::<Vec<_>>();
    for expected in [Sample::Enter(window.surface.clone()), Sample::Motion([10.0, 20.0]), Sample::Pressure(32768), Sample::Distance(16384), Sample::Tilt([-12.0, 17.0]), Sample::Rotation(45.0), Sample::Slider(-32768), Sample::Wheel(15.0, 1), Sample::Down] { assert!(samples.contains(&&expected), "Missing {expected:?}"); }
    assert!(!harness.client.tablet.samples.iter().any(|(id, sample)| *id == second.id().protocol_id() && *sample == Sample::Down));
}

#[test]
fn wide_hardware_serial_is_preserved_without_namespace_bits() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let tool = admit(&mut harness, 1, 11, 0x123456789abcdef0);
    assert_eq!(harness.client.tablet.hardware[&tool.id().protocol_id()], 0x123456789abcdef0);
}

#[test]
fn hover_crosses_surfaces_but_a_held_tool_cannot_retarget_and_can_recover() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let first = harness.window();
    let _first = harness.attach(&first, [320, 240]);
    let second = harness.window();
    let _second = harness.attach(&second, [320, 240]);
    let surfaces = harness.server.windows().into_iter().map(|window| window.surface).collect::<Vec<_>>();
    let tool = admit(&mut harness, 1, 11, 0);
    assert!(harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surfaces[0].clone(), [20.0, 20.0])), false, Default::default(), 1));
    assert!(harness.server.tablet_motion(11, OutputId(1), [30.0, 30.0], Some((surfaces[1].clone(), [30.0, 30.0])), false, Default::default(), 2));
    harness.sync();
    assert!(harness.client.tablet.samples.contains(&(tool.id().protocol_id(), Sample::Enter(second.surface.clone()))));
    let _ = harness.server.tablet_tip(11, true, 3);
    assert!(!harness.server.tablet_motion(11, OutputId(1), [40.0, 40.0], Some((surfaces[0].clone(), [40.0, 40.0])), false, Default::default(), 4));
    harness.sync();
    assert!(!harness.server.tablet_active(11));
    assert!(harness.client.tablet.samples.ends_with(&[(tool.id().protocol_id(), Sample::Up), (tool.id().protocol_id(), Sample::Out), (tool.id().protocol_id(), Sample::Frame)]));
    assert!(harness.server.tablet_motion(11, OutputId(1), [50.0, 50.0], Some((surfaces[0].clone(), [50.0, 50.0])), false, Default::default(), 5));
}

#[test]
fn tool_cursor_requests_are_per_instance_and_old_proximity_serials_are_ignored() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let first = admit(&mut harness, 1, 11, 0);
    let second = admit(&mut harness, 2, 22, 0);
    for id in [11, 22] { assert!(harness.server.tablet_motion(id, OutputId(1), [20.0, 20.0], Some((surface.clone(), [20.0, 20.0])), false, Default::default(), 1)); }
    harness.sync();
    first.set_cursor(harness.client.tablet.serials[&first.id().protocol_id()], None, 0, 0);
    harness.sync();
    let cursors = harness.server.tablet_cursors();
    assert!(matches!(cursors.iter().find(|cursor| cursor.tool == 11).unwrap().image, CursorImageStatus::Hidden));
    assert!(matches!(cursors.iter().find(|cursor| cursor.tool == 22).unwrap().image, CursorImageStatus::Named(_)));
    let old = harness.client.tablet.serials[&second.id().protocol_id()];
    harness.server.tablet_out(22, 2);
    assert!(harness.server.tablet_motion(22, OutputId(1), [40.0, 40.0], Some((surface, [40.0, 40.0])), false, Default::default(), 3));
    harness.sync();
    second.set_cursor(old, None, 0, 0);
    harness.sync();
    assert!(matches!(harness.server.tablet_cursors().iter().find(|cursor| cursor.tool == 22).unwrap().image, CursorImageStatus::Named(_)));
}

#[test]
fn removal_releases_buttons_tip_and_proximity_then_announces_both_objects_removed_once() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let tool = admit(&mut harness, 1, 11, 0);
    let device = harness.client.tablet.devices[0].id().protocol_id();
    assert!(harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surface, [20.0, 20.0])), false, Default::default(), 1));
    let _ = harness.server.tablet_tip(11, true, 2);
    let _ = harness.server.tablet_button(11, 331, true, 3);
    harness.server.tablet_remove_device(1, 4);
    harness.sync(); harness.sync();
    let id = tool.id().protocol_id();
    assert!(harness.client.tablet.samples.contains(&(id, Sample::Button(331, false))));
    assert!(harness.client.tablet.samples.contains(&(id, Sample::Up)));
    assert!(harness.client.tablet.samples.contains(&(id, Sample::Out)));
    assert_eq!(harness.client.tablet.samples.iter().filter(|sample| **sample == (id, Sample::Removed)).count(), 1);
    assert_eq!(harness.client.tablet.removed, [device]);
    assert_eq!(harness.server.state.seat.tablet_seat().count_tools(), 0);
    assert_eq!(harness.server.state.seat.tablet_seat().count_tablets(), 0);
    assert!(!harness.server.tablet_active(11));
}

#[test]
fn lock_and_invalid_axes_terminate_ordinary_delivery_without_releasing_protection() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let _tool = admit(&mut harness, 1, 11, 0);
    assert!(harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surface.clone(), [20.0, 20.0])), false, Default::default(), 1));
    assert!(!harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surface.clone(), [20.0, 20.0])), false, TabletAxes { pressure: Some(f64::NAN), ..Default::default() }, 2));
    assert!(!harness.server.tablet_active(11));
    assert!(harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surface.clone(), [20.0, 20.0])), false, Default::default(), 3));
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(!harness.server.tablet_active(11));
    assert!(harness.server.tablet_tip(11, true, 4).is_none());
    assert!(!harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surface, [20.0, 20.0])), false, Default::default(), 5));
    harness.server.tablet_cancel(6);
    assert!(harness.server.locked());
}

#[test]
fn an_identified_tool_can_change_tablets_only_after_its_previous_proximity_ends() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let tool = admit(&mut harness, 1, 11, 0x123456789abcdef0);
    assert!(harness.server.tablet_device(2, TabletDescriptor { name: "An identical tablet".into(), usb_id: Some((1, 2)), syspath: None }));
    harness.sync();
    assert!(harness.server.tablet_motion(11, OutputId(1), [20.0, 20.0], Some((surface.clone(), [20.0, 20.0])), false, Default::default(), 1));
    assert!(!harness.server.tablet_tool(2, 11, descriptor(0x123456789abcdef0)));
    harness.server.tablet_out(11, 2);
    assert!(harness.server.tablet_tool(2, 11, descriptor(0x123456789abcdef0)));
    assert!(harness.server.tablet_motion(11, OutputId(1), [40.0, 40.0], Some((surface, [40.0, 40.0])), false, Default::default(), 3));
    harness.sync();
    assert_eq!(harness.client.tablet.tools.len(), 1);
    assert_eq!(harness.client.tablet.enters.last(), Some(&(tool.id().protocol_id(), harness.client.tablet.devices[1].id().protocol_id())));
}

#[test]
fn cursor_surface_cannot_be_reused_by_another_tool_and_a_fresh_client_recovers() {
    let mut harness = Harness::new();
    let _seat = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let first = admit(&mut harness, 1, 11, 0);
    let second = admit(&mut harness, 2, 22, 0);
    for id in [11, 22] { assert!(harness.server.tablet_motion(id, OutputId(1), [20.0, 20.0], Some((surface.clone(), [20.0, 20.0])), false, Default::default(), 1)); }
    harness.sync();
    let cursor = harness.client.compositor.as_ref().unwrap().create_surface(&harness.queue.handle(), ());
    first.set_cursor(harness.client.tablet.serials[&first.id().protocol_id()], Some(&cursor), 0, 0);
    harness.sync();
    second.set_cursor(harness.client.tablet.serials[&second.id().protocol_id()], Some(&cursor), 0, 0);
    assert!(synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).is_err());
    let mut fresh = Harness::connect(harness.server);
    let _seat = subscribe(&mut fresh);
    assert_eq!(fresh.client.tablet.tools.len(), 2);
}

#[test]
fn excessive_tablet_seat_objects_disconnect_only_the_requesting_client() {
    let mut harness = Harness::new();
    let mut seats = Vec::new();
    for _ in 0..4 { seats.push(subscribe(&mut harness)); }
    let _excess = harness.client.tablet.manager.as_ref().unwrap().get_tablet_seat(harness.client.seat.as_ref().unwrap(), &harness.queue.handle(), ());
    assert!(synchronize(&mut harness.server, &harness.connection, &mut harness.queue, &mut harness.client).is_err());
    let mut fresh = Harness::connect(harness.server);
    let _seat = subscribe(&mut fresh);
    let _tool = admit(&mut fresh, 1, 11, 0);
    assert_eq!(fresh.client.tablet.tools.len(), 1);
}
