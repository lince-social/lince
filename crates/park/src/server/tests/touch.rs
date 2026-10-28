use super::*;
use wayland_client::protocol::wl_touch::{self, WlTouch};

#[derive(Debug, PartialEq)]
enum Sample {
    Down(i32, wl_surface::WlSurface, [f64; 2]),
    Motion(i32, [f64; 2]),
    Up(i32),
    Frame,
    Cancel,
}

#[derive(Default)]
pub(super) struct Capture { samples: Vec<Sample>, serial: Option<u32> }

impl Dispatch<WlTouch, ()> for Client {
    fn event(state: &mut Self, _: &WlTouch, event: wl_touch::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let sample = match event {
            wl_touch::Event::Down { id, surface, x, y, serial, .. } => { state.touch.serial = Some(serial); Sample::Down(id, surface, [x, y]) }
            wl_touch::Event::Motion { id, x, y, .. } => Sample::Motion(id, [x, y]),
            wl_touch::Event::Up { id, serial, .. } => { state.touch.serial = Some(serial); Sample::Up(id) }
            wl_touch::Event::Frame => Sample::Frame,
            wl_touch::Event::Cancel => Sample::Cancel,
            _ => return,
        };
        state.touch.samples.push(sample);
    }
}

fn subscribe(harness: &mut Harness) -> WlTouch {
    let touch = harness.client.seat.as_ref().unwrap().get_touch(&harness.queue.handle(), ());
    harness.sync();
    touch
}

#[test]
fn real_touch_preserves_surface_local_coordinates_frames_and_input_authority() {
    let mut harness = Harness::new();
    let _touch = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let serial = harness.server.touch_down(0, surface.clone(), [25.0, 40.0], 10).unwrap();
    assert!(harness.server.state.authorized_input(serial, &surface));
    assert!(harness.server.touch_motion(0, [51.0, 67.0], 11));
    harness.server.touch_frame();
    harness.sync();
    assert_eq!(harness.client.touch.samples, [Sample::Down(0, window.surface.clone(), [25.0, 40.0]), Sample::Motion(0, [51.0, 67.0]), Sample::Frame]);
    harness.server.touch_up(0, 12);
    harness.server.touch_frame();
    harness.sync();
    assert_eq!(&harness.client.touch.samples[3..], [Sample::Up(0), Sample::Frame]);
    assert!(!harness.server.touch_active(0));
    let released = harness.client.touch.serial.unwrap().into();
    assert!(harness.server.state.authorized_input(released, &surface));
    harness.server.touch_cancel();
    assert!(!harness.server.state.authorized_input(released, &surface));
}

#[test]
fn seat_cancellation_retires_all_slots_once_and_a_fresh_contact_works() {
    let mut harness = Harness::new();
    let _touch = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    assert!(harness.server.touch_down(0, surface.clone(), [10.0, 10.0], 1).is_some());
    assert!(harness.server.touch_down(1, surface.clone(), [20.0, 20.0], 2).is_some());
    harness.server.touch_frame();
    harness.sync();
    harness.server.touch_cancel();
    harness.server.touch_up(0, 3);
    harness.server.touch_up(1, 3);
    assert!(!harness.server.touch_motion(0, [30.0, 30.0], 4));
    harness.sync();
    assert_eq!(harness.client.touch.samples.iter().filter(|sample| **sample == Sample::Cancel).count(), 1);
    assert_eq!(harness.client.touch.samples.iter().filter(|sample| matches!(sample, Sample::Up(_))).count(), 0);
    assert!(harness.server.touch_down(0, surface, [40.0, 40.0], 5).is_some());
    harness.server.touch_up(0, 6);
    harness.server.touch_frame();
    harness.sync();
    assert!(harness.client.touch.samples.ends_with(&[Sample::Down(0, window.surface, [40.0, 40.0]), Sample::Up(0), Sample::Frame]));
}

#[test]
fn unmapped_or_locked_targets_refuse_delivery_and_clear_existing_contacts() {
    let mut harness = Harness::new();
    let _touch = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    harness.server.touch_down(0, surface.clone(), [10.0, 10.0], 1).unwrap();
    harness.server.touch_frame();
    harness.sync();
    window.surface.attach(None, 0, 0);
    window.surface.commit();
    harness.sync();
    assert!(!harness.server.touch_active(0));
    assert!(harness.server.touch_down(0, surface.clone(), [10.0, 10.0], 2).is_none());
    window.surface.commit();
    harness.sync();
    let _replacement = harness.attach(&window, [320, 240]);
    harness.server.touch_down(0, surface.clone(), [10.0, 10.0], 3).unwrap();
    let _lock = harness.client.lock_manager.as_ref().unwrap().lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(!harness.server.touch_active(0));
    assert!(harness.server.touch_down(0, surface, [10.0, 10.0], 4).is_none());
    assert_eq!(harness.client.touch.samples.iter().filter(|sample| **sample == Sample::Cancel).count(), 2);
}

#[test]
fn malformed_duplicate_and_excess_slots_do_not_replace_live_contacts() {
    let mut harness = Harness::new();
    let _touch = subscribe(&mut harness);
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    assert!(harness.server.touch_down(0, surface.clone(), [f64::NAN, 10.0], 1).is_none());
    for slot in 0..64 { assert!(harness.server.touch_down(slot, surface.clone(), [10.0, 10.0], 2).is_some()); }
    assert!(harness.server.touch_down(0, surface.clone(), [20.0, 20.0], 3).is_none());
    assert!(harness.server.touch_down(64, surface, [20.0, 20.0], 3).is_none());
    assert_eq!(harness.server.state.touches.len(), 64);
    harness.server.touch_cancel();
    assert!(harness.server.state.touches.is_empty());
}
