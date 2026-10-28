use super::*;
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_hold_v1::{self as hold, ZwpPointerGestureHoldV1 as Hold},
    zwp_pointer_gesture_pinch_v1::{self as pinch, ZwpPointerGesturePinchV1 as Pinch},
    zwp_pointer_gesture_swipe_v1::{self as swipe, ZwpPointerGestureSwipeV1 as Swipe},
    zwp_pointer_gestures_v1::ZwpPointerGesturesV1 as Manager,
};

#[derive(Debug, PartialEq)]
enum Event {
    SwipeBegin(u32),
    SwipeUpdate([f64; 2]),
    SwipeEnd(bool),
    PinchBegin(u32),
    PinchUpdate([f64; 4]),
    PinchEnd(bool),
    HoldBegin(u32),
    HoldEnd(bool),
}

#[derive(Default)]
pub(super) struct Capture {
    manager: Option<Manager>,
    events: Vec<Event>,
}

pub(super) fn bind(
    state: &mut Client,
    registry: &wl_registry::WlRegistry,
    name: u32,
    version: u32,
    qh: &QueueHandle<Client>,
) {
    state.gestures.manager = Some(registry.bind(name, version.min(3), qh, ()));
}

delegate_noop!(Client: ignore Manager);

impl Dispatch<Swipe, ()> for Client {
    fn event(
        state: &mut Self,
        _: &Swipe,
        event: swipe::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            swipe::Event::Begin { fingers, .. } => Event::SwipeBegin(fingers),
            swipe::Event::Update { dx, dy, .. } => Event::SwipeUpdate([dx, dy]),
            swipe::Event::End { cancelled, .. } => Event::SwipeEnd(cancelled != 0),
            _ => return,
        };
        state.gestures.events.push(event);
    }
}

impl Dispatch<Pinch, ()> for Client {
    fn event(
        state: &mut Self,
        _: &Pinch,
        event: pinch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            pinch::Event::Begin { fingers, .. } => Event::PinchBegin(fingers),
            pinch::Event::Update {
                dx,
                dy,
                scale,
                rotation,
                ..
            } => Event::PinchUpdate([dx, dy, scale, rotation]),
            pinch::Event::End { cancelled, .. } => Event::PinchEnd(cancelled != 0),
            _ => return,
        };
        state.gestures.events.push(event);
    }
}

impl Dispatch<Hold, ()> for Client {
    fn event(
        state: &mut Self,
        _: &Hold,
        event: hold::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let event = match event {
            hold::Event::Begin { fingers, .. } => Event::HoldBegin(fingers),
            hold::Event::End { cancelled, .. } => Event::HoldEnd(cancelled != 0),
            _ => return,
        };
        state.gestures.events.push(event);
    }
}

fn objects(harness: &mut Harness) -> (Window, wl_pointer::WlPointer, Swipe, Pinch, Hold) {
    let window = harness.window();
    let _buffer = harness.attach(&window, [320, 240]);
    let surface = harness.server.windows()[0].surface.clone();
    let qh = harness.queue.handle();
    let pointer = harness.client.seat.as_ref().unwrap().get_pointer(&qh, ());
    let manager = harness.client.gestures.manager.as_ref().unwrap();
    assert_eq!(manager.version(), 3);
    let swipe = manager.get_swipe_gesture(&pointer, &qh, ());
    let pinch = manager.get_pinch_gesture(&pointer, &qh, ());
    let hold = manager.get_hold_gesture(&pointer, &qh, ());
    harness
        .server
        .pointer_motion(Some((surface, [20.0, 20.0])), 1)
        .unwrap();
    harness.sync();
    (window, pointer, swipe, pinch, hold)
}

#[test]
fn ordinary_clients_receive_complete_swipe_pinch_and_hold_sequences() {
    let mut harness = Harness::connect_authorized(Server::new([640, 480]).unwrap(), Some(vec![]));
    let _objects = objects(&mut harness);
    harness.server.gesture_swipe_begin(2, 10);
    harness.server.gesture_swipe_update([2.0, -3.0], 11);
    harness.server.gesture_swipe_end(false, 12);
    harness.server.gesture_swipe_end(false, 13);
    harness.server.gesture_pinch_begin(2, 20);
    harness
        .server
        .gesture_pinch_update([1.0, 2.0], 1.5, 15.0, 21);
    harness.server.gesture_pinch_end(false, 22);
    harness.server.gesture_hold_begin(2, 30);
    harness.server.gesture_hold_end(false, 31);
    harness.sync();
    assert_eq!(
        harness.client.gestures.events,
        [
            Event::SwipeBegin(2),
            Event::SwipeUpdate([2.0, -3.0]),
            Event::SwipeEnd(false),
            Event::PinchBegin(2),
            Event::PinchUpdate([1.0, 2.0, 1.5, 15.0]),
            Event::PinchEnd(false),
            Event::HoldBegin(2),
            Event::HoldEnd(false),
        ]
    );
}

#[test]
fn malformed_updates_and_pointer_leave_cancel_once_without_late_delivery() {
    let mut harness = Harness::new();
    let _objects = objects(&mut harness);
    harness.server.gesture_swipe_begin(3, 10);
    harness.server.gesture_swipe_update([f64::NAN, 0.0], 11);
    harness.server.gesture_swipe_update([4.0, 0.0], 12);
    harness.server.gesture_swipe_end(false, 13);
    harness.server.gesture_pinch_begin(2, 20);
    harness
        .server
        .gesture_pinch_update([0.0, 0.0], 0.0, 0.0, 21);
    harness.server.gesture_pinch_end(false, 22);
    harness.server.gesture_hold_begin(2, 30);
    harness.server.pointer_motion(None, 31).unwrap();
    harness.server.gesture_hold_end(false, 32);
    harness.sync();
    assert_eq!(
        harness.client.gestures.events,
        [
            Event::SwipeBegin(3),
            Event::SwipeEnd(true),
            Event::PinchBegin(2),
            Event::PinchEnd(true),
            Event::HoldBegin(2),
            Event::HoldEnd(true),
        ]
    );
}

#[test]
fn lock_cancels_client_gestures_and_blocks_new_sequences() {
    let mut harness = Harness::new();
    let _objects = objects(&mut harness);
    harness.server.gesture_swipe_begin(3, 10);
    let _lock = harness
        .client
        .lock_manager
        .as_ref()
        .unwrap()
        .lock(&harness.queue.handle(), ());
    harness.sync();
    assert!(harness.server.locked());
    let before = harness.client.gestures.events.len();
    harness.server.gesture_swipe_begin(3, 20);
    harness.server.gesture_swipe_update([100.0, 0.0], 21);
    harness.server.gesture_swipe_end(false, 22);
    harness.server.gesture_pinch_begin(2, 30);
    harness.server.gesture_hold_begin(2, 40);
    harness.sync();
    assert_eq!(harness.client.gestures.events.len(), before);
    assert_eq!(
        harness.client.gestures.events,
        [Event::SwipeBegin(3), Event::SwipeEnd(true)]
    );
}
