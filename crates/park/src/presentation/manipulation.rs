use super::*;
use native::layout::desktop::manipulation::{self, pointer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Active {
    pub token: u64,
    pub target: pointer::Target,
}

#[derive(Clone, Copy, Debug)]
pub struct Request {
    pub active: Active,
    pub operation: lince_interface::host::Manipulation,
    pub edges: Option<[i8; 2]>,
    pub start: [f32; 2],
    pub current: [f32; 2],
}

pub enum Event {
    Begin(Request),
    End {
        token: u64,
        position: [f32; 2],
        commit: bool,
    },
}

impl Presentation {
    pub fn manipulation(&mut self, event: Event) -> Result<(), ProbeFailure> {
        if self.suspended() { return Ok(()); }
        match event {
            Event::Begin(request) => {
                if self.manipulation_snapshot().is_some() || request.active.token == 0 {
                    return Ok(());
                }
                let window = self.input_window()?;
                if self.canvas_root()? != request.active.target.root
                    || !pointer::targets(self.app.world(), request.active.target.root)
                        .contains(&request.active.target)
                    || request
                        .start
                        .iter()
                        .chain(&request.current)
                        .any(|value| !value.is_finite())
                {
                    return Ok(());
                }
                self.focus(true)?;
                self.pointer(request.start)?;
                let point = self
                    .app
                    .world()
                    .get::<Window>(window)
                    .unwrap()
                    .cursor_position()
                    .unwrap();
                if pointer::press_with_edges(
                    self.app.world_mut(),
                    request.active.target.root,
                    window,
                    point,
                    request.operation,
                    Some(request.active.target),
                    request.edges,
                ) {
                    self.manipulation = Some(request.active);
                    self.pointer(request.current)?;
                }
            }
            Event::End {
                token,
                position,
                commit,
            } => {
                if self.manipulation.is_none_or(|active| active.token != token) {
                    return Ok(());
                }
                if commit && position.iter().all(|value| value.is_finite()) {
                    self.pointer(position)?;
                    let window = self.input_window()?;
                    let point = self
                        .app
                        .world()
                        .get::<Window>(window)
                        .unwrap()
                        .cursor_position();
                    if let Some(point) = point {
                        pointer::release(self.app.world_mut(), point);
                    } else {
                        pointer::cancel(self.app.world_mut());
                    }
                } else {
                    pointer::cancel(self.app.world_mut());
                }
                self.manipulation = None;
            }
        }
        Ok(())
    }

    pub(super) fn manipulation_snapshot(&mut self) -> Option<Active> {
        if !manipulation::active(self.app.world()) {
            self.manipulation = None;
        }
        self.manipulation
    }

    pub(super) fn manipulate_pointer(&mut self, point: Vec2) {
        if self.manipulation.is_some() && !pointer::motion(self.app.world_mut(), point) {
            self.manipulation = None;
        }
    }
}
