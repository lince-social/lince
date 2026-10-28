use super::*;
use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge;
use smithay::{
    input::pointer::*,
    utils::{Point, SERIAL_COUNTER},
};

pub(crate) struct Request {
    pub surface: SurfaceId,
    pub source: WlSurface,
    pub button: u32,
    pub serial: Serial,
    pub edges: Option<[i8; 2]>,
    claim: Serial,
}

pub(super) fn edges(edge: ResizeEdge) -> Option<[i8; 2]> {
    Some(match edge {
        ResizeEdge::Top => [0, -1],
        ResizeEdge::Bottom => [0, 1],
        ResizeEdge::Left => [-1, 0],
        ResizeEdge::Right => [1, 0],
        ResizeEdge::TopLeft => [-1, -1],
        ResizeEdge::TopRight => [1, -1],
        ResizeEdge::BottomLeft => [-1, 1],
        ResizeEdge::BottomRight => [1, 1],
        _ => return None,
    })
}

impl State {
    pub(super) fn request_manipulation(
        &mut self,
        surface: ToplevelSurface,
        seat: WlSeat,
        serial: Serial,
        edges: Option<[i8; 2]>,
    ) {
        if self.lock.owner.is_some()
            || self.dragging
            || self.manipulations.len() >= 32
            || Seat::<State>::from_resource(&seat) != Some(self.seat.clone())
        {
            return;
        }
        let Some(id) = self
            .id(surface.wl_surface())
            .filter(|id| self.windows.get(id).is_some_and(|window| window.mapped))
        else {
            return;
        };
        let Some(pointer) = self.seat.get_pointer().filter(|pointer| {
            pointer.has_grab(serial)
                && pointer
                    .with_grab(|_, grab| grab.is::<ClickGrab<State>>())
                    .unwrap_or(false)
        }) else {
            return;
        };
        let Some(start) = pointer.grab_start_data() else {
            return;
        };
        let Some((source, _)) = &start.focus else {
            return;
        };
        if !self.buttons.contains(&start.button)
            || !source.is_alive()
            || source.client().map(|client| client.id())
                != surface.wl_surface().client().map(|client| client.id())
            || seat.client().map(|client| client.id())
                != surface.wl_surface().client().map(|client| client.id())
        {
            return;
        }
        let mut root = source.clone();
        for _ in 0..32 {
            match get_parent(&root) {
                Some(parent) => root = parent,
                None => break,
            }
        }
        if &root != surface.wl_surface() {
            return;
        }
        let request = Request {
            surface: id,
            source: source.clone(),
            button: start.button,
            serial,
            edges,
            claim: SERIAL_COUNTER.next_serial(),
        };
        pointer.set_grab(
            self,
            Grab(GrabStartData {
                focus: None,
                ..start
            }),
            request.claim,
            Focus::Clear,
        );
        self.manipulations.push_back(request);
    }
}

struct Grab(GrabStartData<State>);

impl PointerGrab<State> for Grab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        handle.motion(data, None, event);
    }

    fn relative_motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, None, event);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, event.serial, event.time, false);
        }
    }

    fn axis(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: AxisFrame,
    ) {
        handle.axis(data, event);
    }

    fn frame(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event);
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event);
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event);
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event);
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event);
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event);
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event);
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event);
    }

    fn start_data(&self) -> &GrabStartData<State> {
        &self.0
    }

    fn unset(&mut self, _: &mut State) {}
}

impl Server {
    pub(crate) fn manipulations(&mut self) -> Vec<Request> {
        self.state.manipulations.drain(..).collect()
    }

    pub(crate) fn owns_manipulation(&self, request: &Request) -> bool {
        !self.locked()
            && self.state.buttons.contains(&request.button)
            && self
                .state
                .windows
                .get(&request.surface)
                .is_some_and(|window| window.mapped)
            && self
                .state
                .seat
                .get_pointer()
                .is_some_and(|pointer| pointer.has_grab(request.claim))
            && self.manipulation_grabbed()
    }
    pub(crate) fn manipulation_grabbed(&self) -> bool {
        self.state
            .seat
            .get_pointer()
            .and_then(|pointer| pointer.with_grab(|_, grab| grab.is::<Grab>()))
            .unwrap_or(false)
    }

    pub(crate) fn begin_manipulation(&mut self, button: u32) -> bool {
        if self.locked()
            || self.dragging()
            || self.popup_grab_active()
            || self.shortcuts_inhibited()
            || !self.state.buttons.is_empty()
        {
            return false;
        }
        let Some(pointer) = self
            .state
            .seat
            .get_pointer()
            .filter(|pointer| !pointer.is_grabbed())
        else {
            return false;
        };
        self.release_client_controls();
        let start = GrabStartData {
            focus: None,
            button,
            location: pointer.current_location(),
        };
        pointer.set_grab(
            &mut self.state,
            Grab(start),
            SERIAL_COUNTER.next_serial(),
            Focus::Clear,
        );
        true
    }

    pub(crate) fn manipulation_button(&mut self, button: u32, pressed: bool, time: u32) {
        if self.manipulation_grabbed() {
            self.pointer_button(button, pressed, time);
        } else if !pressed {
            self.state.buttons.remove(&button);
        }
    }

    pub(crate) fn pointer_modifiers(&self) -> lince_interface::native::actions::Modifiers {
        let state = self
            .state
            .seat
            .get_keyboard()
            .map(|keyboard| keyboard.modifier_state())
            .unwrap_or_default();
        lince_interface::native::actions::Modifiers {
            alt: state.alt,
            control: state.ctrl,
            shift: state.shift,
            super_key: state.logo,
        }
    }
}
