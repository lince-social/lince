use super::*;
use bevy::{
    input::{
        keyboard::{Key, NativeKey},
        mouse::{MouseButton, MouseScrollUnit},
        touch::TouchPhase,
    },
    prelude::KeyCode,
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

#[derive(Clone)]
pub(crate) enum Target {
    Popup(WlSurface),
    Empty,
    Native,
    Application(SurfaceId, WlSurface, OutputId),
    Layer(WlSurface, WlSurface),
}

pub(crate) enum Deferred {
    Button(u32, bool, u32, [f64; 2]),
    Key(u32, bool, u32),
    Axis([f64; 2], AxisFrame, [f64; 2]),
}

impl Desktop {
    pub(crate) fn move_pointer(&mut self, proposed: [f64; 2], time: u32) -> io::Result<()> {
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
        self.motion(time)
    }

    #[cfg(feature = "session")]
    pub(crate) fn relative_motion(&mut self, delta: [f64; 2], unaccelerated: [f64; 2], time: u64) {
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
            WinitEvent::CloseRequested => self.running = false,
            WinitEvent::Redraw => self.redraw = true,
            WinitEvent::Resized { size, .. } if size.w > 0 && size.h > 0 => {
                self.size = [size.w as u32, size.h as u32];
                configure_outputs(&mut self.server, self.size, self.output_count)?;
                self.scene_revision = 0;
                self.redraw = true;
            }
            WinitEvent::Focus(false) => {
                self.server.suspend_native_drop();
                self.focused = false;
                self.server.clear_input(0);
                self.keys.clear();
                self.compose.reset();
                #[cfg(feature = "session")]
                {
                    self.repeat = None;
                }
                self.buttons.clear();
                self.consumed.clear();
                self.grabbed = None;
                self.pending_focus = None;
                self.pending_output = None;
                self.deferred.clear();
                self.send(Input::Focus(false))?;
                self.native_keyboard = false;
                self.pointer_sent = None;
                self.pointer_target = None;
            }
            WinitEvent::Focus(true) => {
                self.focused = true;
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

    pub(crate) fn target(&self) -> Target {
        let fullscreen = self
            .frame_at(self.pointer)
            .and_then(|current| current.frame.geometry.as_ref())
            .and_then(|geometry| {
                geometry
                    .outputs
                    .iter()
                    .find(|output| output.bounds.contains(self.pointer))
            })
            .is_some_and(|output| self.fullscreen(output.id));
        let layer_hit = |layer: &LayerSnapshot| {
            if let Some(surface) = self
                .popups
                .iter()
                .rev()
                .filter(|popup| popup.owner == renderer::PopupOwner::Layer(layer.surface.clone()))
                .find_map(|popup| popup.hit(self.pointer))
            {
                return Some(Target::Layer(layer.surface.clone(), surface));
            }
            let point = [
                self.pointer[0] - f64::from(layer.origin[0]),
                self.pointer[1] - f64::from(layer.origin[1]),
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
            .find_map(|popup| popup.hit(self.pointer)) {
            return Target::Popup(surface);
        }
        let canvas = self
            .clips
            .iter()
            .any(|(_, clip)| clip.contains(self.pointer));
        if !canvas {
            return self
                .layers
                .iter()
                .rev()
                .filter(|layer| matches!(layer.layer, Layer::Bottom | Layer::Background))
                .find_map(layer_hit)
                .unwrap_or(Target::Empty);
        }
        let Some(current) = self.frame_at(self.pointer) else {
            return Target::Native;
        };
        if current.frame.input.modal
            && current.frame.geometry.as_ref().is_none_or(|geometry| {
                geometry.outputs.iter().any(|output| {
                    Some(output.id) == current.frame.output && output.bounds.contains(self.pointer)
                })
            })
            || current
                .frame
                .input
                .controls
                .iter()
                .any(|region| region.contains(self.pointer))
        {
            return Target::Native;
        }
        let Some(geometry) = &current.frame.geometry else {
            return Target::Native;
        };
        for popup in self.popups.iter().rev() {
            if let renderer::PopupOwner::Application(id) = popup.owner
                && geometry
                    .outputs
                    .iter()
                    .any(|output| output.id == popup.output && output.bounds.contains(self.pointer))
                && let Some(surface) = popup.hit(self.pointer)
            {
                return Target::Application(id, surface, popup.output);
            }
        }
        for item in geometry.items.iter().rev() {
            if !geometry
                .outputs
                .iter()
                .any(|output| output.id == item.output && output.bounds.contains(self.pointer))
            {
                continue;
            }
            let Some(point) = item.surface_point(self.pointer) else {
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

    pub(crate) fn motion(&mut self, time: u32) -> io::Result<()> {
        if self.server.locked() {
            self.focus()?;
            return self.server.lock_motion(self.pointer, time);
        }
        let native_position = self.native_position();
        if self.pointer_sent != Some(native_position) {
            self.pointer_stamp = self
                .worker
                .pointer(native_position.map(|value| value as f32))?;
            self.pointer_sent = Some(native_position);
        }
        let target = if self.server.dragging() {
            self.target()
        } else {
            self.grabbed.clone().unwrap_or_else(|| self.target())
        };
        let position = match target {
            Target::Popup(surface) => self.popups.iter().find(|popup|
                popup.owner == renderer::PopupOwner::Native && popup.nodes.iter().any(|node| node.surface == surface))
                .and_then(|popup| {
                    let node = popup.nodes.iter().find(|node| node.surface == surface)?;
                    let point = popup.point(self.pointer);
                    Some((surface, std::array::from_fn(|axis| point[axis] - f64::from(node.origin[axis]))))
                }),
            Target::Application(id, surface, output) => {
                self.frame_at(self.pointer).and_then(|current| {
                    if let Some(popup) = self.popups.iter().find(|popup| {
                        popup.owner == renderer::PopupOwner::Application(id)
                            && popup.output == output
                            && popup.nodes.iter().any(|node| node.surface == surface)
                    }) {
                        let node = popup.nodes.iter().find(|node| node.surface == surface)?;
                        let point = popup.point(self.pointer);
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
                    let point = item.mapping.surface_point_unbounded(self.pointer)?;
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
                        let point = popup.point(self.pointer);
                        return Some((
                            surface,
                            std::array::from_fn(|axis| point[axis] - f64::from(node.origin[axis])),
                        ));
                    }
                    let node = layer.nodes.iter().find(|node| node.surface == surface)?;
                    Some((
                        surface,
                        [
                            self.pointer[0]
                                - f64::from(layer.origin[0])
                                - f64::from(node.origin[0]),
                            self.pointer[1]
                                - f64::from(layer.origin[1])
                                - f64::from(node.origin[1]),
                        ],
                    ))
                }),
            Target::Native => self.server.native_surface().map(|surface| (surface, self.pointer)),
            Target::Empty => None,
        };
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
        if native && self.server.dragging() && self.pending_output.is_none()
            && let Some(output) = output
            && self.current.as_ref().and_then(|current| current.frame.output).is_some_and(|current| current != output)
        {
            let serial = self.send_tracked(Input::Output(output))?;
            self.pending_output = Some((serial, Some(output)));
            self.server.suspend_native_drop();
        }
        let target = (native && self.pending_output.is_none()).then(|| {
            self.frame_at(self.pointer)?.frame.input.drop_target.clone()
        }).flatten().filter(|target| Some(target.output) == output
            && target.pointer.iter().zip(self.pointer).all(|(target, actual)| (target - actual).abs() < 2.0));
        self.server.native_drop_target(target);
        Ok(())
    }

    pub(crate) fn button(&mut self, button: u32, pressed: bool, time: u32) -> io::Result<()> {
        if self.server.locked() {
            self.motion(time)?;
            self.server.pointer_button(button, pressed, time);
            return Ok(());
        }
        if self.pending_output.is_none() && pressed && self.buttons.is_empty() {
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
            return self.defer(Deferred::Button(button, pressed, time, self.pointer));
        }
        let target = if self.server.dragging() { self.target() } else { self.grabbed.clone().unwrap_or_else(|| self.target()) };
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
                }
                let serial = self.send_tracked(Input::Button(
                    mouse_button(button),
                    pressed,
                    self.native_position().map(|value| value as f32),
                    self.pointer_stamp,
                ))?;
                if pressed {
                    self.pending_focus = Some((serial, None));
                }
            }
            Target::Application(id, _, _) => {
                if pressed {
                    self.layer_focus = None;
                    let serial = self.send_tracked(Input::Application(id))?;
                    self.pending_focus = Some((serial, Some(id)));
                }
                self.server.pointer_button(button, pressed, time);
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
        }
        self.focus()?;
        self.motion(time)
    }

    fn native_position(&self) -> [f64; 2] {
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
        if self.server.locked() {
            self.focus()?;
            self.server.keyboard(code, pressed, time, |_| false)?;
            return Ok(());
        }
        if self.pending_output.is_some() {
            return self.defer(Deferred::Key(code, pressed, time));
        }
        let physical =
            bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
        let repeat = pressed && self.keys.contains_key(&code);
        let mut action = None;
        let consumed = self.consumed.contains(&code);
        let inhibited = self.server.shortcuts_inhibited();
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
                .map(|binding| binding.action)
                .filter(|action| !inhibited || matches!(action, DesktopAction::Lock | DesktopAction::Logout | DesktopAction::OpenOperation))
        };
        let event = if repeat {
            let event = self.server.key_symbol(code)?;
            action = binding(&event).filter(|action| {
                matches!(
                    action,
                    DesktopAction::Focus(_)
                        | DesktopAction::Move(_)
                        | DesktopAction::ResizeColumn(_)
                        | DesktopAction::ResizeItem(_)
                )
            });
            event
        } else {
            self.server.keyboard(code, pressed, time, |event| {
                if pressed {
                    action = binding(event);
                }
                consumed || action.is_some()
            })?
        };
        if let Some(action) = action {
            self.server.release_client_controls();
            self.compose.reset();
            self.consumed.insert(code);
            match action {
                DesktopAction::OpenTerminal | DesktopAction::OpenLauncher | DesktopAction::Lock => {
                    self.command(action)
                }
                DesktopAction::Logout => self.send(Input::Logout)?,
                _ => {
                    let serial = self.send_tracked(Input::Action(action))?;
                    if matches!(
                        action,
                        DesktopAction::FocusOutput(_)
                            | DesktopAction::MoveToOutput(_)
                            | DesktopAction::MoveWorkspaceToOutput(_)
                            | DesktopAction::Workspace(_)
                    ) {
                        self.pending_output = Some((serial, None));
                        self.native_keyboard = false;
                    }
                }
            }
        } else if self.native_keyboard && !consumed
            && self.server.native_ready() && repeat && !self.server.input_method_grabbed() {
            self.native_key(code, pressed, repeat, &event)?;
        }
        if pressed {
            self.keys.insert(code, event);
        } else {
            self.keys.remove(&code);
            self.consumed.remove(&code);
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

    pub(crate) fn native_key(&mut self, code: u32, pressed: bool, repeat: bool, event: &crate::server::KeyEvent) -> io::Result<()> {
        if !self.native_keyboard || !self.focused || self.server.locked()
            || self.consumed.contains(&code) || self.pending_output.is_some() { return Ok(()); }
        let physical = bevy::winit::converters::convert_physical_key_code(PhysicalKey::from_scancode(code));
        let text = self.compose.text(event, pressed, repeat);
        self.send(Input::Key { physical, logical: logical(physical, text.as_deref()), text, pressed, repeat })
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
        if !self.focused || self.server.locked() || !self.keys.contains_key(&code) {
            self.repeat = None;
            return Ok(());
        }
        self.repeat = Some((
            code,
            Instant::now()
                + Duration::from_secs_f64(1.0 / f64::from(self.keyboard.repeat_rate.max(1))),
        ));
        if self.pending_output.is_none() && (self.native_keyboard || self.consumed.contains(&code))
        {
            self.key(code, true, time)?;
        }
        Ok(())
    }

    fn defer(&mut self, event: Deferred) -> io::Result<()> {
        if self.deferred.len() >= 128 {
            return Err(io::Error::other("the Park deferred input queue is full"));
        }
        self.deferred.push_back(event);
        Ok(())
    }

    pub(crate) fn restore_modifiers(&self) -> io::Result<()> {
        for code in self
            .keys
            .keys()
            .filter(|code| !self.consumed.contains(*code))
        {
            let physical = bevy::winit::converters::convert_physical_key_code(
                PhysicalKey::from_scancode(*code),
            );
            if matches!(
                physical,
                KeyCode::ControlLeft
                    | KeyCode::ControlRight
                    | KeyCode::ShiftLeft
                    | KeyCode::ShiftRight
                    | KeyCode::AltLeft
                    | KeyCode::AltRight
                    | KeyCode::SuperLeft
                    | KeyCode::SuperRight
            ) {
                self.send(Input::Key {
                    physical,
                    logical: logical(physical, None),
                    text: None,
                    pressed: true,
                    repeat: false,
                })?;
            }
        }
        Ok(())
    }

    pub(crate) fn axis(&mut self, delta: [f64; 2], frame: AxisFrame) -> io::Result<()> {
        if self.server.locked() {
            self.server.pointer_axis(frame);
            return Ok(());
        }
        if self.pending_output.is_some() {
            return self.defer(Deferred::Axis(delta, frame, self.pointer));
        }
        match self.grabbed.clone().unwrap_or_else(|| self.target()) {
            Target::Native => self.send(Input::Scroll(
                delta.map(|value| -value as f32),
                MouseScrollUnit::Pixel,
                TouchPhase::Moved,
                self.native_position().map(|value| value as f32),
                self.pointer_stamp,
            ))?,
            Target::Application(..) | Target::Layer(..) | Target::Popup(..) => self.server.pointer_axis(frame),
            Target::Empty => {}
        }
        Ok(())
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
        self.focus()?;
        let position = self.pointer;
        while self.pending_output.is_none() {
            let Some(event) = self.deferred.pop_front() else {
                break;
            };
            match event {
                Deferred::Button(button, pressed, time, point) => {
                    self.pointer = point;
                    self.button(button, pressed, time)?;
                }
                Deferred::Key(code, pressed, time) => self.key(code, pressed, time)?,
                Deferred::Axis(delta, frame, point) => {
                    self.pointer = point;
                    self.axis(delta, frame)?;
                }
            }
        }
        self.pointer = position;
        self.motion(0)
    }
}

fn mouse_button(button: u32) -> MouseButton {
    match button {
        272 => MouseButton::Left,
        273 => MouseButton::Right,
        274 => MouseButton::Middle,
        275 => MouseButton::Back,
        276 => MouseButton::Forward,
        _ => MouseButton::Other(button.min(u16::MAX as u32) as u16),
    }
}

fn logical(key: KeyCode, text: Option<&str>) -> Key {
    match key {
        KeyCode::Escape => Key::Escape,
        KeyCode::Enter | KeyCode::NumpadEnter => Key::Enter,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Insert => Key::Insert,
        KeyCode::ArrowLeft => Key::ArrowLeft,
        KeyCode::ArrowRight => Key::ArrowRight,
        KeyCode::ArrowUp => Key::ArrowUp,
        KeyCode::ArrowDown => Key::ArrowDown,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::ControlLeft | KeyCode::ControlRight => Key::Control,
        KeyCode::AltLeft | KeyCode::AltRight => Key::Alt,
        KeyCode::ShiftLeft | KeyCode::ShiftRight => Key::Shift,
        KeyCode::SuperLeft | KeyCode::SuperRight => Key::Super,
        _ => text
            .map(|text| Key::Character(text.into()))
            .unwrap_or(Key::Unidentified(NativeKey::Unidentified)),
    }
}
