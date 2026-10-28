use super::*;
use bevy::{
    input::{
        keyboard::{Key, NativeKey},
        mouse::{MouseButton, MouseScrollUnit},
        touch::TouchPhase,
    },
};
use lince_interface::{host::SurfaceId, native::actions::Modifiers};
use smithay::reexports::wayland_server::Resource;
use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputEvent, KeyState,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent,
    },
    input::pointer::AxisFrame,
    reexports::winit::{keyboard::PhysicalKey, platform::scancode::PhysicalKeyExtScancode},
};

#[derive(Clone, PartialEq)]
pub(crate) enum Target {
    Popup(WlSurface),
    Empty,
    Native,
    Application(SurfaceId, WlSurface, OutputId),
    Layer(WlSurface, WlSurface),
}

pub(crate) struct DeferredPointer {
    point: [f64; 2],
    mapping: Option<(OutputId, u64)>,
    target: Target,
    native: Option<crate::native_input::PointerTarget>,
}

pub(crate) enum Deferred {
    Button(u32, bool, u32, DeferredPointer),
    Key(u32, bool, u32),
    Axis([f64; 2], AxisFrame, DeferredPointer),
}

impl Desktop {
    pub(crate) fn input_pending(&self) -> bool {
        if self.logout_pending { return true; }
        #[cfg(feature = "session")]
        if self.absolute_waiting() { return true; }
        self.pending_output.is_some()
    }

    pub(crate) fn move_pointer(&mut self, proposed: [f64; 2], time: u32) -> io::Result<()> {
        if proposed.iter().any(|value| !value.is_finite()) { return Err(io::Error::other("Pointer coordinates must be finite.")); }
        let previous = self.displayed_pointer();
        let proposed = self.logical_point(proposed);
        let constrained = self.pointer_target.as_ref().and_then(|(surface, local)| {
            let popup = self.popups.iter().find_map(|popup| {
                let node = popup.nodes.iter().find(|node| &node.surface == surface)?;
                Some((&popup.mapping, node.layout.size))
            });
            let (mapping, size) = popup.or_else(|| {
                let current = self.frame_at(self.pointer)?;
                let (window, node) = current.windows.iter().find_map(|window| {
                    window.nodes.iter().find(|node| &node.surface == surface).map(|node| (window, node))
                })?;
                let item = current.frame.geometry.as_ref()?.items.iter().find(|item| item.id == ItemId::Application(window.id))?;
                Some((&item.mapping, node.layout.size))
            })?;
            let previous = mapping.surface_point_unbounded(self.pointer)?;
            let next = mapping.surface_point_unbounded(proposed)?;
            let position = std::array::from_fn(|axis| local[axis] + next[axis] - previous[axis]);
            let position = self.server.constrain_pointer(surface, *local, position, size);
            mapping.canvas_point(std::array::from_fn(|axis| previous[axis] + position[axis] - local[axis]))
        });
        self.pointer = constrained.unwrap_or(proposed);
        self.display_pointer = Some(self.displayed_point(self.pointer));
        let moved = self.displayed_pointer() != previous;
        if moved {
            self.pointer_epoch = self.pointer_epoch.checked_add(1)
                .ok_or_else(|| io::Error::other("Park pointer epochs exhausted"))?;
        }
        self.motion(time)?;
        if moved {
            self.follow_pointer()?;
        }
        Ok(())
    }

    #[cfg(any(feature = "session", all(test, feature = "server-tests")))]
    pub(crate) fn relative_motion(&mut self, delta: [f64; 2], unaccelerated: [f64; 2], time: u64) {
        let delta = self.logical_delta(delta);
        let unaccelerated = self.logical_delta(unaccelerated);
        let Some((surface, _)) = &self.pointer_target else { return };
        let mapping = self.popups.iter().find(|popup| popup.nodes.iter().any(|node| &node.surface == surface))
            .map(|popup| &popup.mapping)
            .or_else(|| {
                let current = self.frame_at(self.pointer)?;
                let window = current.windows.iter().find(|window| window.nodes.iter().any(|node| &node.surface == surface))?;
                current.frame.geometry.as_ref()?.items.iter()
                    .find(|item| item.id == ItemId::Application(window.id))
                    .map(|item| &item.mapping)
            });
        let Some(mapping) = mapping else { return };
        let Some(current) = mapping.surface_point_unbounded(self.pointer) else { return };
        let converted = |delta: [f64; 2]| mapping.surface_point_unbounded(
            std::array::from_fn(|axis| self.pointer[axis] + delta[axis]),
        ).map(|next| std::array::from_fn(|axis| next[axis] - current[axis]));
        let Some(delta) = converted(delta) else { return };
        let Some(unaccelerated) = converted(unaccelerated) else { return };
        self.server.relative_motion(
            delta, unaccelerated, time,
        );
    }

    pub(crate) fn event(&mut self, event: WinitEvent) -> io::Result<()> {
        if matches!(&event, WinitEvent::Input(_)) { self.server.activity(); }
        match event {
            WinitEvent::CloseRequested => self.logout()?,
            WinitEvent::Redraw => self.redraw = true,
            WinitEvent::Resized { size, .. } if size.w > 0 && size.h > 0 => {
                self.resize_virtual_outputs([size.w as u32, size.h as u32])?;
            }
            WinitEvent::Focus(false) => {
                self.worker.host_active(false);
                #[cfg(feature = "session")]
                self.cancel_absolute();
                self.invalidate_pointer_policy()?;
                self.cancel_manipulation()?;
                self.server.suspend_native_drop();
                self.focused = false;
                self.server.control_active(false);
                self.server.clear_input(0);
                self.keys.clear();
                self.compose.reset();
                #[cfg(feature = "session")]
                {
                    self.repeat = None;
                }
                self.buttons.clear();
                self.consumed.clear();
                self.consumed_actions.clear();
                self.modal_keys.clear();
                self.grabbed = None;
                self.pending_focus = None;
                self.pending_output = None;
                self.deferred.clear();
                self.blur_native()?;
                self.native_keyboard = false;
                self.pointer_sent = None;
                self.pointer_target = None;
            }
            WinitEvent::Focus(true) => {
                self.focused = true;
                self.server.control_active(true);
                self.focus()?;
                self.motion(0)?;
                self.redraw = true;
            }
            WinitEvent::Input(InputEvent::PointerMotionAbsolute { event }) => {
                let point =
                    event.position_transformed((self.size[0] as i32, self.size[1] as i32).into());
                self.move_pointer([point.x, point.y], event.time_msec())?;
                self.redraw = true;
            }
            WinitEvent::Input(InputEvent::PointerButton { event }) => {
                self.button(
                    event.button_code(),
                    event.state() == ButtonState::Pressed,
                    event.time_msec(),
                )?;
            }
            WinitEvent::Input(InputEvent::Keyboard { event }) => {
                self.key(
                    event.key_code().raw().saturating_sub(8),
                    event.state() == KeyState::Pressed,
                    event.time_msec(),
                )?;
            }
            WinitEvent::Input(InputEvent::PointerAxis { event }) => {
                let mut delta = [0.0; 2];
                let mut frame = AxisFrame::new(event.time_msec()).source(event.source());
                for (index, axis) in [Axis::Horizontal, Axis::Vertical].into_iter().enumerate() {
                    delta[index] = event
                        .amount(axis)
                        .unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * 15.0 / 120.0);
                    frame = frame.value(axis, delta[index]);
                    if let Some(value) = event.amount_v120(axis) {
                        frame = frame.v120(axis, value as i32);
                    }
                    if delta[index] == 0.0 && event.source() == AxisSource::Finger {
                        frame = frame.stop(axis);
                    }
                }
                self.axis(delta, frame)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn target(&self) -> Target { self.target_at(self.pointer) }

    pub(crate) fn target_at(&self, point: [f64; 2]) -> Target {
        let fullscreen = self
            .frame_at(point)
            .and_then(|current| current.frame.geometry.as_ref())
            .and_then(|geometry| {
                geometry
                    .outputs
                    .iter()
                    .find(|output| output.bounds.contains(point))
            })
            .is_some_and(|output| self.fullscreen(output.id));
        let layer_hit = |layer: &LayerSnapshot| {
            if let Some(surface) = self
                .popups
                .iter()
                .rev()
                .filter(|popup| popup.owner == renderer::PopupOwner::Layer(layer.surface.clone()))
                .find_map(|popup| popup.hit(point))
            {
                return Some(Target::Layer(layer.surface.clone(), surface));
            }
            let point = [
                point[0] - f64::from(layer.origin[0]),
                point[1] - f64::from(layer.origin[1]),
            ];
            layer.nodes.iter().find_map(|node| {
                node.hit(point)
                    .filter(|_| node.surface.is_alive())
                    .map(|_| Target::Layer(layer.surface.clone(), node.surface.clone()))
            })
        };
        if let Some(target) = self
            .layers
            .iter()
            .rev()
            .filter(|layer| {
                layer.layer == Layer::Overlay || !fullscreen && layer.layer == Layer::Top
            })
            .find_map(layer_hit)
        {
            return target;
        }
        if let Some(surface) = self.popups.iter().rev()
            .filter(|popup| popup.owner == renderer::PopupOwner::Native)
            .find_map(|popup| popup.hit(point)) {
            return Target::Popup(surface);
        }
        let canvas = self
            .clips
            .iter()
            .any(|(_, clip)| clip.contains(point));
        if let Some(current) = self.frame_at(point)
            && current.frame.owns_modal_pointer(point)
        {
            return Target::Native;
        }
        for popup in self.popups.iter().rev() {
            if let renderer::PopupOwner::Application(id) = popup.owner
                && let Some(surface) = popup.hit(point)
            {
                return Target::Application(id, surface, popup.output);
            }
        }
        if !canvas {
            return self
                .layers
                .iter()
                .rev()
                .filter(|layer| matches!(layer.layer, Layer::Bottom | Layer::Background))
                .find_map(layer_hit)
                .unwrap_or(Target::Empty);
        }
        let Some(current) = self.frame_at(point) else {
            return Target::Native;
        };
        let Some(geometry) = &current.frame.geometry else {
            return Target::Native;
        };
        if current
            .frame
            .input
            .controls
            .iter()
            .any(|region| region.contains(point))
        {
            return Target::Native;
        }
        for item in geometry.items.iter().rev() {
            if !geometry
                .outputs
                .iter()
                .any(|output| output.id == item.output && output.bounds.contains(point))
            {
                continue;
            }
            let Some(point) = item.surface_point(point) else {
                continue;
            };
            let ItemId::Application(id) = item.id else {
                return Target::Native;
            };
            if let Some(window) = current.windows.iter().find(|window| window.id == id)
                && let Some((surface, _)) = window.hit_content(point)
                && surface.is_alive()
            {
                return Target::Application(id, surface, item.output);
            }
        }
        Target::Native
    }

    pub(crate) fn target_position(&self, target: Target, point: [f64; 2]) -> Option<(WlSurface, [f64; 2])> {
        match target {
            Target::Popup(surface) => self.popups.iter().find(|popup|
                popup.owner == renderer::PopupOwner::Native && popup.nodes.iter().any(|node| node.surface == surface))
                .and_then(|popup| {
                    let node = popup.nodes.iter().find(|node| node.surface == surface)?;
                    let point = popup.point(point);
                    Some((surface, std::array::from_fn(|axis| point[axis] - f64::from(node.origin[axis]))))
                }),
            Target::Application(id, surface, output) => {
                self.frame_at(point).and_then(|current| {
                    if let Some(popup) = self.popups.iter().find(|popup| {
                        popup.owner == renderer::PopupOwner::Application(id)
                            && popup.output == output
                            && popup.nodes.iter().any(|node| node.surface == surface)
                    }) {
                        let node = popup.nodes.iter().find(|node| node.surface == surface)?;
                        let point = popup.point(point);
                        return Some((
                            surface,
                            std::array::from_fn(|axis| point[axis] - f64::from(node.origin[axis])),
                        ));
                    }
                    let item =
                        current.frame.geometry.as_ref()?.items.iter().find(|item| {
                            item.id == ItemId::Application(id) && item.output == output
                        })?;
                    let window = current.windows.iter().find(|window| window.id == id)?;
                    let node = window.nodes.iter().find(|node| node.surface == surface)?;
                    let bounds = window.bounds()?;
                    let point = item.mapping.surface_point_unbounded(point)?;
                    Some((
                        surface,
                        [
                            point[0] + bounds.loc.x - f64::from(node.origin[0]),
                            point[1] + bounds.loc.y - f64::from(node.origin[1]),
                        ],
                    ))
                })
            }
            Target::Layer(root, surface) => self
                .layers
                .iter()
                .find(|layer| layer.surface == root)
                .and_then(|layer| {
                    if let Some(popup) = self.popups.iter().find(|popup| {
                        popup.owner == renderer::PopupOwner::Layer(root.clone())
                            && popup.nodes.iter().any(|node| node.surface == surface)
                    }) {
                        let node = popup.nodes.iter().find(|node| node.surface == surface)?;
                        let point = popup.point(point);
                        return Some((
                            surface,
                            std::array::from_fn(|axis| point[axis] - f64::from(node.origin[axis])),
                        ));
                    }
                    let node = layer.nodes.iter().find(|node| node.surface == surface)?;
                    Some((
                        surface,
                        [
                            point[0]
                                - f64::from(layer.origin[0])
                                - f64::from(node.origin[0]),
                            point[1]
                                - f64::from(layer.origin[1])
                                - f64::from(node.origin[1]),
                        ],
                    ))
                }),
            Target::Native => self.server.native_surface().map(|surface| (surface, point)),
            Target::Empty => None,
        }
    }

    pub(crate) fn motion(&mut self, time: u32) -> io::Result<()> {
        if self.server.locked() {
            self.focus()?;
            return self.server.lock_motion(self.pointer, time);
        }
        let native_position = self.native_position();
        if self.pointer_sent != Some(native_position) {
            self.pointer_stamp = self
                .worker
                .pointer_with_epoch(native_position.map(|value| value as f32), self.pointer_epoch)?;
            self.pointer_sent = Some(native_position);
        }
        if self.manipulation.owns() {
            self.pointer_target = None;
            self.server.suspend_native_drop();
            return self.server.pointer_motion(None, time);
        }
        let target = if self.server.dragging() {
            self.target()
        } else {
            self.grabbed.clone().unwrap_or_else(|| self.target())
        };
        let position = self.target_position(target, self.pointer);
        let position = position.filter(|(surface, _)| surface.is_alive());
        if self.pointer_target != position {
            self.server.pointer_motion(position.clone(), time)?;
            self.pointer_target = position;
        }
        self.refresh_native_drop()?;
        Ok(())
    }

    fn refresh_native_drop(&mut self) -> io::Result<()> {
        if !self.focused || self.server.locked() {
            self.server.suspend_native_drop();
            return Ok(());
        }
        let native = matches!(self.target(), Target::Native);
        let output = self.frame_at(self.pointer).and_then(|current| current.frame.geometry.as_ref())
            .and_then(|geometry| geometry.outputs.iter().find(|output| output.bounds.contains(self.pointer)))
            .map(|output| output.id);
        if native && self.server.dragging() && !self.input_pending()
            && let Some(output) = output
            && self.current.as_ref().and_then(|current| current.frame.output).is_some_and(|current| current != output)
        {
            let serial = self.send_tracked(Input::Output(output))?;
            self.pending_output = Some((serial, Some(output)));
            self.server.native_drop_target(None);
        }
        let target = (native && !self.input_pending()).then(|| {
            self.frame_at(self.pointer)?.frame.input.drop_target.clone()
        }).flatten().filter(|target| Some(target.output) == output
            && target.pointer.iter().zip(self.pointer).all(|(target, actual)| (target - actual).abs() < 2.0));
        self.server.native_drop_target(target);
        Ok(())
    }

    pub(crate) fn button(&mut self, button: u32, pressed: bool, time: u32) -> io::Result<()> {
        self.button_captured(button, pressed, time, None)
    }

    fn button_captured(&mut self, button: u32, pressed: bool, time: u32, native: Option<crate::native_input::PointerTarget>) -> io::Result<()> {
        #[cfg(feature = "session")]
        if !self.server.locked() && self.absolute_waiting() {
            return self.defer_absolute(Deferred::Button(button, pressed, time, self.deferred_pointer()));
        }
        #[cfg(feature = "session")]
        self.cancel_desktop_gesture()?;
        if self.manipulation_button(button, pressed, time)? {
            return Ok(());
        }
        if self.server.locked() {
            self.motion(time)?;
            self.server.pointer_button(button, pressed, time);
            return Ok(());
        }
        let retain_application_focus = pressed && self.current.as_ref().is_some_and(|current| !current.frame.input.focus_policy.click)
            && matches!(self.target(), Target::Application(..));
        if self.pending_output.is_none() && pressed && self.buttons.is_empty() && !retain_application_focus {
            let output = self
                .frame_at(self.pointer)
                .and_then(|current| current.frame.geometry.as_ref())
                .and_then(|geometry| {
                    geometry
                        .outputs
                        .iter()
                        .find(|output| output.bounds.contains(self.pointer))
                })
                .map(|output| output.id);
            if let Some(output) = output
                && self
                    .current
                    .as_ref()
                    .is_some_and(|current| current.frame.output != Some(output))
            {
                let serial = self.send_tracked(Input::Output(output))?;
                self.pending_output = Some((serial, Some(output)));
                self.native_keyboard = false;
            }
        }
        if self.pending_output.is_some() {
            return self.defer(Deferred::Button(button, pressed, time, self.deferred_pointer()));
        }
        let target = if self.server.dragging() { self.target() } else { self.grabbed.clone().unwrap_or_else(|| self.target()) };
        let native = if pressed && matches!(target, Target::Native) {
            let captured = native.or_else(|| self.frame_at(self.pointer).and_then(|frame| frame.frame.input.native_pointer.target(self.pointer)));
            let Some(captured) = captured else { return Ok(()); };
            Some(captured)
        } else { native };
        if pressed && self.begin_pointer_manipulation(button, &target, time)? {
            return Ok(());
        }
        if pressed && self.server.popup_grab_active() {
            let surface = match &target {
                Target::Application(_, surface, _) | Target::Layer(_, surface) | Target::Popup(surface) => Some(surface),
                _ => None,
            };
            if surface.is_none_or(|surface| !self.server.popup_target(surface)) {
                self.server.dismiss_popup_grab();
            }
        }
        if pressed {
            self.server.native_input();
        }
        if pressed {
            self.buttons.insert(button);
            if self.grabbed.is_none() {
                self.grabbed = Some(target.clone());
            }
        }
        self.motion(time)?;
        match target {
            Target::Popup(_) => self.server.pointer_button(button, pressed, time),
            Target::Native => {
                if self.server.dragging() {
                    self.server.dispatch()?;
                    self.server.pointer_button(button, pressed, time);
                }
                if pressed {
                    self.layer_focus = None;
                    self.worker.focus_native(true);
                }
                let position = self.native_position().map(|value| value as f32);
                let event = if let Some(target) = native.filter(|_| pressed) {
                    Input::CapturedButton(mouse_button(button), pressed, position, self.pointer_stamp, target)
                } else { Input::Button(mouse_button(button), pressed, position, self.pointer_stamp) };
                let serial = self.send_tracked(event)?;
                if pressed {
                    self.pending_focus = Some((serial, None));
                }
                if self.current.as_ref().is_some_and(|current| current.frame.input.keyboard_modal_triggers.iter().any(|bounds| bounds.contains(self.pointer))) {
                    self.pending_output = Some((serial, None));
                }
            }
            Target::Application(id, source, _) => {
                let click_focus = self.current.as_ref().is_none_or(|current| current.frame.input.focus_policy.click);
                let focus_serial = if pressed && click_focus {
                    self.layer_focus = None;
                    let serial = self.send_tracked(Input::Application(id))?;
                    self.pending_focus = Some((serial, Some(id)));
                    Some(serial)
                } else {
                    None
                };
                if let (Some(serial), Some(focus_serial)) = (self.server.pointer_button_serial(button, pressed, time), focus_serial) {
                    self.remember_pointer_press(button, id, source, serial, focus_serial);
                }
            }
            Target::Layer(root, _) => {
                if pressed
                    && self.layers.iter().any(|layer| {
                        layer.surface == root && layer.keyboard != KeyboardInteractivity::None
                    })
                {
                    self.layer_focus = Some(root);
                }
                self.server.pointer_button(button, pressed, time);
            }
            Target::Empty => {}
        }
        if !pressed {
            self.buttons.remove(&button);
            if self.buttons.is_empty() {
                self.grabbed = None;
            }
            self.release_pointer_press();
        }
        self.focus()?;
        self.motion(time)
    }

    pub(super) fn native_position(&self) -> [f64; 2] {
        let origin = self
            .current
            .as_ref()
            .map(|current| {
                let bounds = current
                    .frame
                    .geometry
                    .as_ref()
                    .and_then(|geometry| {
                        geometry
                            .outputs
                            .iter()
                            .find(|output| Some(output.id) == current.frame.output)
                    })
                    .map_or([0.0; 2], |output| output.bounds.origin());
                std::array::from_fn::<_, 2, _>(|axis| bounds[axis] + current.frame.origin[axis])
            })
            .unwrap_or([0.0; 2]);
        std::array::from_fn(|axis| self.pointer[axis] - origin[axis])
    }

    pub(crate) fn key(&mut self, code: u32, pressed: bool, time: u32) -> io::Result<()> {
        #[cfg(feature = "session")]
        self.cancel_desktop_gesture()?;
        if self.server.locked() {
            return self.locked_key(code, pressed, time);
        }
        #[cfg(feature = "session")]
        if self.absolute_waiting() { return self.defer_absolute(Deferred::Key(code, pressed, time)); }
        if self.pending_output.is_some() {
            return self.defer(Deferred::Key(code, pressed, time));
        }
        let physical =
            bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
        let repeat = pressed && self.keys.contains_key(&code);
        let modal = self.current.as_ref().is_some_and(|current| current.frame.input.keyboard_modal);
        if modal && pressed { self.modal_keys.insert(code); }
        let modal_key = self.modal_keys.contains(&code);
        let mut action = None;
        let consumed = self.consumed.contains(&code);
        let inhibited = self.server.shortcuts_inhibited();
        let ordered_native = {
            #[cfg(feature = "session")]
            { self.replaying_absolute() && self.native_keyboard && self.server.native_ready() && !self.server.input_method_grabbed() }
            #[cfg(not(feature = "session"))]
            { false }
        };
        let manipulating = self.manipulation.active();
        let pointer_dragging = self.server.pointer_dragging();
        let mut cancel_manipulation = false;
        let mut cancel_pointer_drag = false;
        let external = &self.external;
        let bindings = self
            .current
            .as_ref()
            .map(|current| &current.frame.input.bindings);
        let binding = |event: &crate::server::KeyEvent| {
            bindings
                .into_iter()
                .flatten()
                .find(|binding| {
                    binding.key == physical
                        && binding.modifiers
                            == Modifiers {
                                alt: event.modifiers.alt,
                                control: event.modifiers.ctrl,
                                shift: event.modifiers.shift,
                                super_key: event.modifiers.logo,
                            }
                })
                .filter(|binding| match binding.action {
                    DesktopAction::External(action) => external.binding_allowed(action, false, inhibited, modal),
                    _ => !inhibited || matches!(binding.action, DesktopAction::Lock | DesktopAction::Logout | DesktopAction::OpenOperation),
                })
        };
        let event = if repeat {
            let event = self.server.key_symbol(code)?;
            action = binding(&event).filter(|binding| binding.repeat && !modal_key && self.consumed_actions.get(&code) == Some(&binding.action)).map(|binding| binding.action);
            event
        } else {
            self.server.keyboard(code, pressed, time, |event| {
                if pressed {
                    action = binding(event).map(|binding| binding.action);
                }
                cancel_manipulation = manipulating && pressed
                    && event.symbol == smithay::input::keyboard::keysyms::KEY_Escape
                    && !event.modifiers.alt && !event.modifiers.ctrl && !event.modifiers.shift && !event.modifiers.logo;
                cancel_pointer_drag = pointer_dragging && pressed
                    && event.symbol == smithay::input::keyboard::keysyms::KEY_Escape
                    && !event.modifiers.alt && !event.modifiers.ctrl && !event.modifiers.shift && !event.modifiers.logo;
                if cancel_pointer_drag { action = None; }
                consumed || modal_key || ordered_native || action.is_some() || cancel_manipulation || cancel_pointer_drag
            })?
        };
        if cancel_manipulation {
            self.consumed.insert(code);
            self.cancel_manipulation()?;
        }
        if cancel_pointer_drag {
            if !modal_key { self.consumed.insert(code); }
            self.cancel_pointer_drag(time)?;
        }
        if let Some(action) = action {
            if !matches!(action, DesktopAction::External(external) if external.lockable()) {
                self.server.release_client_controls();
                self.compose.reset();
            }
            self.consumed.insert(code);
            match action {
                action if action.is_recent() => {
                    self.worker.focus_native(true);
                    self.restore_modifiers()?;
                    self.recent_key(code, pressed, repeat, &event)?;
                }
                DesktopAction::OpenTerminal | DesktopAction::OpenLauncher | DesktopAction::Lock | DesktopAction::External(_) => {
                    self.command(action, repeat)
                }
                _ => {
                    let serial = self.send_tracked(Input::Action(action))?;
                    if matches!(
                        action,
                        DesktopAction::FocusOutput(_)
                            | DesktopAction::MoveToOutput(_)
                            | DesktopAction::MoveRowToOutput(_)
                            | DesktopAction::MoveWorkspaceToOutput(_)
                            | DesktopAction::Workspace(_)
                            | DesktopAction::PreviousWorkspace
                            | DesktopAction::NextWorkspace
                            | DesktopAction::MoveWorkspaceEarlier
                            | DesktopAction::MoveWorkspaceLater
                            | DesktopAction::MoveToWorkspace(_)
                            | DesktopAction::Focus(_)
                            | DesktopAction::ToggleFocus
                            | DesktopAction::ToggleOverview
                            | DesktopAction::ToggleEdit
                            | DesktopAction::OpenOperation
                            | DesktopAction::Logout
                            | DesktopAction::InspectWindowRecord
                            | DesktopAction::ReapplyOpeningRules
                            | DesktopAction::SelectPlacement { .. }
                            | DesktopAction::Unfocus
                            | DesktopAction::Move(_)
                            | DesktopAction::ToggleTiling
                            | DesktopAction::ToggleFloating
                    ) {
                        self.pending_output = Some((serial, None));
                        self.native_keyboard = false;
                    }
                }
            }
            if !repeat { self.consumed_actions.insert(code, action); }
        } else if !pressed && self.consumed_actions.get(&code).is_some_and(|action| action.is_recent()) {
            self.recent_key(code, pressed, repeat, &event)?;
        } else if self.native_keyboard && !consumed
            && self.server.native_ready() && (modal_key || repeat || ordered_native) && (modal_key || !self.server.input_method_grabbed()) {
            self.native_key(code, pressed, repeat, &event)?;
        }
        if pressed {
            self.keys.insert(code, event);
        } else {
            self.keys.remove(&code);
            self.consumed.remove(&code);
            self.consumed_actions.remove(&code);
            self.modal_keys.remove(&code);
        }
        #[cfg(feature = "session")]
        {
            if pressed && !repeat && self.keyboard.repeat_rate > 0 && self.server.key_repeats(code)
            {
                self.repeat = Some((
                    code,
                    Instant::now()
                        + Duration::from_millis(u64::from(self.keyboard.repeat_delay_ms)),
                ));
            } else if !pressed && self.repeat.is_some_and(|(key, _)| key == code) {
                self.repeat = None;
            }
        }
        Ok(())
    }

    fn locked_key(&mut self, code: u32, pressed: bool, time: u32) -> io::Result<()> {
        self.focus()?;
        let physical = bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
        let repeat = pressed && self.keys.contains_key(&code);
        let consumed = self.consumed.contains(&code);
        let bindings = self.current.as_ref().map(|current| current.frame.input.hardware_bindings.clone()).unwrap_or_default();
        let external = &self.external;
        let binding = |event: &crate::server::KeyEvent| {
            bindings.iter().find(|binding| {
                binding.key == physical
                    && binding.modifiers == Modifiers {
                        alt: event.modifiers.alt, control: event.modifiers.ctrl,
                        shift: event.modifiers.shift, super_key: event.modifiers.logo,
                    }
                    && matches!(binding.action, DesktopAction::External(action) if external.binding_allowed(action, true, false, false))
            })
        };
        let mut action = None;
        let event = if repeat {
            let event = self.server.key_symbol(code)?;
            action = binding(&event).filter(|binding| binding.repeat && self.consumed_actions.get(&code) == Some(&binding.action)).map(|binding| binding.action);
            event
        } else {
            self.server.keyboard(code, pressed, time, |event| {
                if pressed { action = binding(event).map(|binding| binding.action); }
                consumed || action.is_some()
            })?
        };
        if let Some(action) = action {
            self.consumed.insert(code);
            self.command(action, repeat);
            if !repeat { self.consumed_actions.insert(code, action); }
        }
        if pressed {
            self.keys.insert(code, event);
        } else {
            self.keys.remove(&code);
            self.consumed.remove(&code);
            self.consumed_actions.remove(&code);
        }
        #[cfg(feature = "session")]
        {
            if pressed && !repeat && action.is_some_and(|action| matches!(action, DesktopAction::External(external) if external.repeats()))
                && self.keyboard.repeat_rate > 0 && self.server.key_repeats(code)
            {
                self.repeat = Some((code, Instant::now() + Duration::from_millis(u64::from(self.keyboard.repeat_delay_ms))));
            } else if !pressed && self.repeat.is_some_and(|(key, _)| key == code) {
                self.repeat = None;
            }
        }
        Ok(())
    }

    fn recent_key(&mut self, code: u32, pressed: bool, repeat: bool, event: &crate::server::KeyEvent) -> io::Result<()> {
        let physical = bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
        let label = self.keyboard_snapshot.as_ref().and_then(|snapshot| snapshot.labels.get(&format!("{physical:?}")));
        let serial = self.send_tracked(Input::Key { physical, logical: logical(label, None, event.symbol), text: None, pressed, repeat })?;
        self.pending_output = Some((serial, None));
        self.native_keyboard = false;
        Ok(())
    }

    pub(crate) fn native_key(&mut self, code: u32, pressed: bool, repeat: bool, event: &crate::server::KeyEvent) -> io::Result<()> {
        #[cfg(all(test, feature = "session"))]
        if self.replaying_absolute() {
            eprintln!("Absolute native key: code={code}, pressed={pressed}, native={}, focused={}, locked={}, consumed={}, pending={}, symbol={}",
                self.native_keyboard, self.focused, self.server.locked(), self.consumed.contains(&code), self.input_pending(), event.symbol);
        }
        if !self.native_keyboard || !self.focused || self.server.locked()
            || self.consumed.contains(&code) || self.input_pending() { return Ok(()); }
        let physical = bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
        let capture = self.current.as_ref().is_some_and(|current| current.frame.input.keyboard_capture
            || !current.frame.input.keyboard_modal && self.modal_keys.contains(&code));
        let text = if capture { self.compose.reset(); None } else { self.compose.text(event, pressed, repeat) };
        let label = self.keyboard_snapshot.as_ref().and_then(|snapshot| snapshot.labels.get(&format!("{physical:?}")));
        let logical = native_logical(label, text.as_deref(), event);
        self.send(Input::Key { physical, logical, text, pressed, repeat })
    }

    #[cfg(feature = "session")]
    pub(crate) fn repeat_timeout(&self) -> Option<Duration> {
        self.repeat.map(|(_, deadline)| deadline.saturating_duration_since(Instant::now()))
            .into_iter().chain(self.server.idle_timeout()).chain(self.server.native_timeout()).min()
    }

    #[cfg(feature = "session")]
    pub(crate) fn repeat_key(&mut self, time: u32) -> io::Result<()> {
        let Some((code, deadline)) = self.repeat else {
            return Ok(());
        };
        if deadline > Instant::now() {
            return Ok(());
        }
        if !self.focused || !self.keys.contains_key(&code)
            || self.server.locked() && !self.consumed_actions.get(&code).is_some_and(|action| {
                matches!(action, DesktopAction::External(external) if external.repeats() && self.external.snapshot.settings.command(*external, true).is_ok())
            }) {
            self.repeat = None;
            return Ok(());
        }
        self.repeat = Some((
            code,
            Instant::now()
                + Duration::from_secs_f64(1.0 / f64::from(self.keyboard.repeat_rate.max(1))),
        ));
        if !self.input_pending() && (self.native_keyboard || self.consumed.contains(&code))
        {
            self.key(code, true, time)?;
        }
        Ok(())
    }

    fn defer(&mut self, event: Deferred) -> io::Result<()> {
        if self.deferred.len() >= 128 {
            #[cfg(feature = "session")]
            { self.recover_input_pressure(); return Ok(()); }
            #[cfg(not(feature = "session"))]
            return Err(io::Error::other("the Park deferred input queue is full"));
        }
        self.deferred.push_back(event);
        Ok(())
    }

    fn deferred_pointer(&self) -> DeferredPointer {
        let target = self.grabbed.clone().unwrap_or_else(|| self.target());
        let native = matches!(target, Target::Native).then(|| self.frame_at(self.pointer)?.frame.input.native_pointer.target(self.pointer)).flatten();
        DeferredPointer { point: self.pointer, mapping: self.input_mapping_stamp(self.pointer),
            target, native }
    }

    fn deferred_pointer_current(&self, point: &DeferredPointer) -> bool {
        point.mapping == self.input_mapping_stamp(point.point)
            && point.target == self.grabbed.clone().unwrap_or_else(|| self.target())
            && (!matches!(point.target, Target::Native) || point.native.as_ref().is_some_and(|target|
                self.frame_at(point.point).and_then(|frame| frame.frame.input.native_pointer.target(point.point)).as_ref() == Some(target)))
    }

    pub(crate) fn restore_modifiers(&self) -> io::Result<()> {
        let mut pressed = Vec::new();
        for (code, event) in self
            .keys
            .iter()
            .filter(|(code, _)| !self.consumed.contains(*code))
        {
            let physical = bevy::winit::converters::convert_physical_key_code(
                PhysicalKey::from_scancode(*code),
            );
            let label = self.keyboard_snapshot.as_ref().and_then(|snapshot| snapshot.labels.get(&format!("{physical:?}")));
            let logical = logical(label, None, event.symbol);
            if label.is_some_and(|label| label.is_modifier)
                || matches!(logical, Key::Control | Key::Shift | Key::Alt | Key::Super | Key::AltGraph) {
                pressed.push((physical, logical));
            }
        }
        self.send(Input::Modifiers(pressed))
    }

    pub(crate) fn axis(&mut self, delta: [f64; 2], frame: AxisFrame) -> io::Result<()> {
        self.axis_captured(delta, frame, None)
    }

    fn axis_captured(&mut self, delta: [f64; 2], frame: AxisFrame, native: Option<crate::native_input::PointerTarget>) -> io::Result<()> {
        #[cfg(feature = "session")]
        if !self.server.locked() && self.absolute_waiting() {
            return self.defer_absolute(Deferred::Axis(delta, frame, self.deferred_pointer()));
        }
        #[cfg(feature = "session")]
        self.cancel_desktop_gesture()?;
        if self.manipulation.owns() {
            return Ok(());
        }
        if self.server.locked() {
            self.server.pointer_axis(frame);
            return Ok(());
        }
        if self.pending_output.is_some() {
            return self.defer(Deferred::Axis(delta, frame, self.deferred_pointer()));
        }
        match self.grabbed.clone().unwrap_or_else(|| self.target()) {
            Target::Native => {
                let position = self.native_position().map(|value| value as f32);
                let delta = delta.map(|value| -value as f32);
                let target = native.or_else(|| self.frame_at(self.pointer)?.frame.input.native_pointer.target(self.pointer));
                if let Some(target) = target {
                    self.send(Input::CapturedScroll(delta, MouseScrollUnit::Pixel, TouchPhase::Moved, position, self.pointer_stamp, target))?;
                }
            },
            Target::Application(..) | Target::Layer(..) | Target::Popup(..) => self.server.pointer_axis(frame),
            Target::Empty => {}
        }
        Ok(())
    }

    pub(crate) fn replay_event(&mut self, event: Deferred) -> io::Result<()> {
        let position = self.pointer;
        let result = match event {
            Deferred::Button(button, pressed, time, point) => {
                self.pointer = point.point;
                if self.deferred_pointer_current(&point) || !pressed && self.buttons.contains(&button) {
                    self.button_captured(button, pressed, time, point.native)
                } else { Ok(()) }
            }
            Deferred::Key(code, pressed, time) => self.key(code, pressed, time),
            Deferred::Axis(delta, frame, point) => {
                self.pointer = point.point;
                if self.deferred_pointer_current(&point) { self.axis_captured(delta, frame, point.native) } else { Ok(()) }
            }
        };
        self.pointer = position;
        result
    }

    pub(crate) fn replay(&mut self) -> io::Result<()> {
        let ready = self.pending_output.is_some_and(|(serial, output)| {
            self.current.as_ref().is_some_and(|current| {
                current.input_serial >= serial
                    && output.is_none_or(|output| current.frame.output == Some(output))
            })
        });
        if !ready {
            return Ok(());
        }
        self.pending_output = None;
        self.pointer_sent = None;
        #[cfg(feature = "session")]
        self.absolute_replaying(true);
        let result = (|| {
            self.focus()?;
            while self.pending_output.is_none() {
                let Some(event) = self.deferred.pop_front() else {
                    break;
                };
                self.replay_event(event)?;
            }
            Ok::<_, io::Error>(())
        })();
        #[cfg(feature = "session")]
        self.absolute_replaying(false);
        result?;
        self.motion(0)
    }
}

pub(super) fn mouse_button(button: u32) -> MouseButton {
    match button {
        272 => MouseButton::Left,
        273 => MouseButton::Right,
        274 => MouseButton::Middle,
        275 => MouseButton::Back,
        276 => MouseButton::Forward,
        _ => MouseButton::Other(button.min(u16::MAX as u32) as u16),
    }
}

fn native_logical(label: Option<&lince_interface::host::input::KeyLabel>, text: Option<&str>, event: &crate::server::KeyEvent) -> Key {
    let identity = if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.logo {
        smithay::input::keyboard::Keysym::from(event.symbol).key_char().filter(|character| !character.is_control()).map(|character| character.to_string())
    } else { None };
    logical(label, text.or(identity.as_deref()), event.symbol)
}

pub(super) fn logical(label: Option<&lince_interface::host::input::KeyLabel>, text: Option<&str>, symbol: u32) -> Key {
    if let Some(label) = label.filter(|label| label.is_modifier) {
        if label.modifiers.control { return Key::Control; }
        if label.modifiers.alt { return Key::Alt; }
        if label.modifiers.shift { return Key::Shift; }
        if label.modifiers.super_key { return Key::Super; }
    }
    use smithay::input::keyboard::keysyms;
    match symbol {
        keysyms::KEY_Escape => Key::Escape,
        keysyms::KEY_Return | keysyms::KEY_KP_Enter => Key::Enter,
        keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => Key::Tab,
        keysyms::KEY_BackSpace => Key::Backspace,
        keysyms::KEY_Delete => Key::Delete,
        keysyms::KEY_Insert => Key::Insert,
        keysyms::KEY_Left => Key::ArrowLeft,
        keysyms::KEY_Right => Key::ArrowRight,
        keysyms::KEY_Up => Key::ArrowUp,
        keysyms::KEY_Down => Key::ArrowDown,
        keysyms::KEY_Home => Key::Home,
        keysyms::KEY_End => Key::End,
        keysyms::KEY_Prior => Key::PageUp,
        keysyms::KEY_Next => Key::PageDown,
        keysyms::KEY_space => Key::Space,
        keysyms::KEY_Print => Key::PrintScreen,
        keysyms::KEY_XF86MonBrightnessDown => Key::BrightnessDown,
        keysyms::KEY_XF86MonBrightnessUp => Key::BrightnessUp,
        keysyms::KEY_XF86AudioLowerVolume => Key::AudioVolumeDown,
        keysyms::KEY_XF86AudioRaiseVolume => Key::AudioVolumeUp,
        keysyms::KEY_XF86AudioMute => Key::AudioVolumeMute,
        keysyms::KEY_XF86AudioMicMute => Key::MicrophoneVolumeMute,
        keysyms::KEY_XF86AudioPlay => Key::MediaPlay,
        keysyms::KEY_XF86AudioPause => Key::MediaPause,
        keysyms::KEY_XF86AudioNext => Key::MediaTrackNext,
        keysyms::KEY_XF86AudioPrev => Key::MediaTrackPrevious,
        keysyms::KEY_ISO_Level3_Shift | keysyms::KEY_ISO_Level5_Shift => Key::AltGraph,
        keysyms::KEY_Control_L | keysyms::KEY_Control_R => Key::Control,
        keysyms::KEY_Alt_L | keysyms::KEY_Alt_R => Key::Alt,
        keysyms::KEY_Shift_L | keysyms::KEY_Shift_R => Key::Shift,
        keysyms::KEY_Super_L | keysyms::KEY_Super_R | keysyms::KEY_Meta_L | keysyms::KEY_Meta_R => Key::Super,
        _ => text
            .map(|text| Key::Character(text.into()))
            .unwrap_or(Key::Unidentified(NativeKey::Unidentified)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_character_identity_does_not_commit_composition_text() {
        use smithay::input::keyboard::{ModifiersState, keysyms};
        for symbol in [keysyms::KEY_a, keysyms::KEY_c, keysyms::KEY_v, keysyms::KEY_x, keysyms::KEY_z] {
            let event = crate::server::KeyEvent { symbol, text: None, modifiers: ModifiersState { ctrl: true, ..Default::default() } };
            assert_eq!(native_logical(None, None, &event), Key::Character(char::from_u32(symbol).unwrap().to_string().into()));
        }
        for symbol in [keysyms::KEY_a, keysyms::KEY_dead_acute] {
            let event = crate::server::KeyEvent { symbol, text: None, modifiers: ModifiersState::default() };
            assert_eq!(native_logical(None, None, &event), Key::Unidentified(NativeKey::Unidentified));
        }
    }

    #[test]
    fn hardware_symbols_reach_native_controls_with_the_same_physical_evdev_identity() {
        let mut server = crate::server::Server::new([640, 480]).unwrap();
        server.configure_keyboard(lince_interface::host::input::KeyboardSettings { layout: "us".into(), ..Default::default() }).unwrap();
        let snapshot = server.keyboard_snapshot();
        for (code, expected) in [
            (224, Key::BrightnessDown), (225, Key::BrightnessUp),
            (115, Key::AudioVolumeUp), (114, Key::AudioVolumeDown),
            (113, Key::AudioVolumeMute), (248, Key::MicrophoneVolumeMute),
            (163, Key::MediaTrackNext), (165, Key::MediaTrackPrevious),
        ] {
            let physical = bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
            let event = server.key_symbol(code).unwrap();
            assert_eq!(logical(snapshot.labels.get(&format!("{physical:?}")), None, event.symbol), expected);
        }
    }

    #[test]
    fn configured_symbols_and_modifier_roles_reach_native_controls_without_changing_physical_keys() {
        let mut server = crate::server::Server::new([640, 480]).unwrap();
        let mut settings = lince_interface::host::input::KeyboardSettings { layout: "us".into(), options: "caps:escape".into(), ..Default::default() };
        server.configure_keyboard(settings.clone()).unwrap();
        let snapshot = server.keyboard_snapshot();
        let event = server.key_symbol(58).unwrap();
        assert_eq!(logical(snapshot.labels.get("CapsLock"), None, event.symbol), Key::Escape);
        settings.options = "caps:ctrl_modifier".into();
        server.configure_keyboard(settings).unwrap();
        let snapshot = server.keyboard_snapshot();
        assert!(snapshot.labels["CapsLock"].is_modifier);
        assert!(snapshot.labels["CapsLock"].modifiers.control);
        let event = server.keyboard(58, true, 1, |_| true).unwrap();
        assert!(event.modifiers.ctrl);
        assert_eq!(logical(snapshot.labels.get("CapsLock"), None, event.symbol), Key::Control);
        server.keyboard(58, false, 2, |_| true).unwrap();
        server.configure_keyboard(lince_interface::host::input::KeyboardSettings { layout: "br".into(), ..Default::default() }).unwrap();
        let snapshot = server.keyboard_snapshot();
        let event = server.key_symbol(39).unwrap();
        assert_eq!(logical(snapshot.labels.get("Semicolon"), event.text.as_deref(), event.symbol), Key::Character("ç".into()));
    }
}
