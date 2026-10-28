use super::*;
use crate::presentation::manipulation::{Active, Event, Request};
use lince_interface::host::SurfaceId;

struct Press {
    button: u32,
    serial: smithay::utils::Serial,
    input_serial: u64,
    source: WlSurface,
    target: lince_interface::native::layout::desktop::manipulation::pointer::Target,
    position: [f32; 2],
    viewport: OutputViewport,
    origin: [f64; 2],
    retired: bool,
}

impl Press {
    fn matches(&self, request: &crate::server::manipulation::Request) -> bool {
        self.button == request.button
            && self.serial == request.serial
            && self.source == request.source
            && self.target.item == ItemId::Application(request.surface)
    }

    fn context(&self, frame: &Completed) -> bool {
        !frame.frame.input.modal
            && !frame.frame.input.keyboard_modal
            && frame.frame.output == Some(self.target.output)
            && frame.frame.origin == self.origin
            && frame.frame.input.pointer_targets.contains(&self.target)
            && frame
                .frame
                .geometry
                .as_ref()
                .is_some_and(|geometry| geometry.outputs.contains(&self.viewport))
    }
}

pub(super) struct Drag {
    active: Active,
    button: u32,
    serial: u64,
}

#[derive(Default)]
pub(super) struct State {
    next: u64,
    active: Option<Drag>,
    swallowed: HashSet<u32>,
    press: Option<Press>,
}

impl State {
    pub(super) fn swallow_releases(&mut self, buttons: impl IntoIterator<Item = u32>) {
        self.swallowed.extend(buttons);
        self.press = None;
    }

    pub(super) fn active(&self) -> bool {
        self.active.is_some()
    }

    pub(super) fn owns(&self) -> bool {
        self.active.is_some() || !self.swallowed.is_empty()
    }

    pub(super) fn observe(&mut self, frame: &Completed) {
        if let Some(press) = &mut self.press
            && frame.input_serial >= press.input_serial
            && !press.context(frame)
        {
            press.retired = true;
        }
        if self.active.as_ref().is_some_and(|drag| {
            frame.input_serial >= drag.serial && frame.frame.input.manipulation != Some(drag.active)
        }) {
            self.active = None;
        }
    }
}

impl Desktop {
    pub(crate) fn cancel_manipulation(&mut self) -> io::Result<()> {
        let claimed = self.server.manipulations().iter().any(|request| {
            self.buttons.contains(&request.button)
                && (self.server.owns_manipulation(request)
                    || self
                        .manipulation
                        .press
                        .as_ref()
                        .is_some_and(|press| press.matches(request)))
        });
        if claimed {
            self.manipulation
                .swallowed
                .extend(self.buttons.iter().copied());
            self.grabbed = None;
        }
        self.manipulation.press = None;
        if let Some(drag) = self.manipulation.active.take() {
            self.send(Input::Manipulation(
                Event::End {
                    token: drag.active.token,
                    position: self.native_position().map(|value| value as f32),
                    commit: false,
                },
                self.pointer_stamp,
            ))?;
        }
        Ok(())
    }

    pub(crate) fn release_pointer_press(&mut self) {
        if self.buttons.is_empty() {
            self.manipulation.press = None;
        }
    }

    pub(crate) fn remember_pointer_press(
        &mut self,
        button: u32,
        surface: SurfaceId,
        source: WlSurface,
        serial: smithay::utils::Serial,
        input_serial: u64,
    ) {
        if self.buttons.len() != 1 || self.manipulation.press.is_some() {
            return;
        }
        let Some(frame) = &self.current else { return };
        let Some(target) = frame
            .frame
            .input
            .pointer_targets
            .iter()
            .find(|target| target.item == ItemId::Application(surface))
            .copied()
        else {
            return;
        };
        let Some(viewport) = frame
            .frame
            .geometry
            .as_ref()
            .and_then(|geometry| {
                geometry
                    .outputs
                    .iter()
                    .find(|output| output.id == target.output)
            })
            .cloned()
        else {
            return;
        };
        self.manipulation.press = Some(Press {
            button,
            serial,
            input_serial,
            source,
            target,
            viewport,
            origin: frame.frame.origin,
            position: self.native_position().map(|value| value as f32),
            retired: false,
        });
    }

    pub(crate) fn begin_application_manipulation(
        &mut self,
        request: crate::server::manipulation::Request,
    ) -> io::Result<()> {
        if self.manipulation.owns()
            || !self.buttons.contains(&request.button)
            || !self
                .manipulation
                .press
                .as_ref()
                .is_some_and(|press| press.matches(&request))
        {
            return Ok(());
        }
        let press = self.manipulation.press.take().unwrap();
        self.manipulation
            .swallowed
            .extend(self.buttons.iter().copied());
        self.grabbed = None;
        let eligible = !press.retired
            && self
                .current
                .as_ref()
                .is_some_and(|frame| press.context(frame));
        if !self.focused
            || self.pending_output.is_some()
            || !eligible
            || !self.server.owns_manipulation(&request)
        {
            self.focus()?;
            return self.motion(0);
        }
        self.start_manipulation(
            request.button,
            Request {
                active: Active {
                    token: 0,
                    target: press.target,
                },
                operation: if request.edges.is_some() {
                    lince_interface::host::Manipulation::Resize
                } else {
                    lince_interface::host::Manipulation::Move
                },
                edges: request.edges,
                start: press.position,
                current: self.native_position().map(|value| value as f32),
            },
        )
    }

    fn start_manipulation(&mut self, button: u32, mut request: Request) -> io::Result<()> {
        self.manipulation.next = self
            .manipulation
            .next
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Park pointer interaction identities exhausted"))?;
        request.active.token = self.manipulation.next;
        let active = request.active;
        let serial = self.send_tracked(Input::Manipulation(
            Event::Begin(request),
            self.pointer_stamp,
        ))?;
        self.manipulation.active = Some(Drag {
            active,
            button,
            serial,
        });
        self.layer_focus = None;
        self.focus()?;
        self.motion(0)
    }

    pub(crate) fn manipulation_button(
        &mut self,
        button: u32,
        pressed: bool,
        time: u32,
    ) -> io::Result<bool> {
        if self.manipulation.swallowed.contains(&button) {
            if !pressed {
                if self
                    .manipulation
                    .active
                    .as_ref()
                    .is_some_and(|drag| drag.button == button)
                {
                    let drag = self.manipulation.active.take().unwrap();
                    self.send(Input::Manipulation(
                        Event::End {
                            token: drag.active.token,
                            position: self.native_position().map(|value| value as f32),
                            commit: self.focused && !self.server.locked(),
                        },
                        self.pointer_stamp,
                    ))?;
                }
                self.manipulation.swallowed.remove(&button);
                self.buttons.remove(&button);
            }
            self.server.manipulation_button(button, pressed, time);
            self.focus()?;
            self.motion(time)?;
            return Ok(true);
        }
        if pressed && self.manipulation.owns() {
            self.manipulation.swallowed.insert(button);
            self.buttons.insert(button);
            self.server.manipulation_button(button, true, time);
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn begin_pointer_manipulation(
        &mut self,
        button: u32,
        target: &input::Target,
        time: u32,
    ) -> io::Result<bool> {
        if !self.focused
            || !self.buttons.is_empty()
            || self.grabbed.is_some()
            || self.manipulation.owns()
        {
            return Ok(false);
        }
        let Some(frame) = &self.current else {
            return Ok(false);
        };
        if frame.frame.input.modal || frame.frame.input.keyboard_modal {
            return Ok(false);
        }
        let (item, output) = match target {
            input::Target::Application(surface, node, output) => {
                if self.popups.iter().any(|popup| {
                    popup
                        .nodes
                        .iter()
                        .any(|candidate| &candidate.surface == node)
                }) {
                    return Ok(false);
                }
                (ItemId::Application(*surface), *output)
            }
            input::Target::Native => {
                let Some(item) = frame
                    .frame
                    .geometry
                    .as_ref()
                    .and_then(|geometry| {
                        geometry.items.iter().rev().find(|item| {
                            geometry.outputs.iter().any(|output| {
                                output.id == item.output && output.bounds.contains(self.pointer)
                            }) && item.surface_point(self.pointer).is_some()
                        })
                    })
                    .filter(|item| matches!(item.id, ItemId::Native(_)))
                else {
                    return Ok(false);
                };
                (item.id, item.output)
            }
            _ => return Ok(false),
        };
        if frame.frame.output != Some(output) {
            return Ok(false);
        }
        let modifiers = self.server.pointer_modifiers();
        let Some(binding) = frame
            .frame
            .input
            .pointer_bindings
            .iter()
            .find(|binding| {
                binding.chord.button == input::mouse_button(button)
                    && binding.chord.modifiers == modifiers
            })
            .copied()
        else {
            return Ok(false);
        };
        let Some(target) = frame
            .frame
            .input
            .pointer_targets
            .iter()
            .find(|target| target.item == item && target.output == output)
            .copied()
        else {
            return Ok(false);
        };
        if !self.server.begin_manipulation(button) {
            return Ok(false);
        }
        self.manipulation.swallowed.insert(button);
        self.buttons.insert(button);
        self.server.native_input();
        self.server.manipulation_button(button, true, time);
        let position = self.native_position().map(|value| value as f32);
        self.start_manipulation(
            button,
            Request {
                active: Active { token: 0, target },
                operation: binding.operation,
                edges: None,
                start: position,
                current: position,
            },
        )?;
        Ok(true)
    }
}
