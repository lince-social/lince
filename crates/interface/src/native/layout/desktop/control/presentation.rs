// non-lean-yet
// Scope: enum Pause, enum MessageSource, struct Presentation, fn refresh.
// BeautifulRust (baseline lines):
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 125.

use super::*;
use crate::host::control::Status;
use std::fmt::Write;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pause {
    Active,
    Inactive,
    Locked,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MessageSource {
    Unavailable,
    Available {
        readers: u16,
        controllers: u16,
        pending: u16,
        pause: Pause,
    },
}

impl MessageSource {
    fn current(status: &Status) -> Self {
        if !status.available {
            return Self::Unavailable;
        }
        Self::Available {
            readers: status.readers,
            controllers: status.controllers,
            pending: status.pending,
            pause: if status.locked {
                Pause::Locked
            } else if !status.active {
                Pause::Inactive
            } else {
                Pause::Active
            },
        }
    }

    fn write(self, message: &mut String) {
        message.clear();
        match self {
            Self::Unavailable => message.push_str("External desktop automation is available in Park. Sandbox keeps control of its own window."),
            Self::Available { readers, controllers, pending, pause } => {
                let pause = match pause {
                    Pause::Locked => " · paused while locked",
                    Pause::Inactive => " · paused while inactive",
                    Pause::Active => "",
                };
                let _ = write!(message, "Connected helpers: {readers} may observe, {controllers} may issue commands · {pending} pending requests{pause}. Helpers use the permissions assigned to their launch connection. Revoking closes those automation connections; your external service manager controls restarts.");
            }
        }
    }
}

#[derive(Resource)]
struct Presentation {
    labels: QueryState<Entity, With<StatusLabel>>,
    buttons: QueryState<Entity, With<RevokeButton>>,
    label_buffer: Vec<Entity>,
    button_buffer: Vec<Entity>,
    source: Option<MessageSource>,
    message: String,
}

impl FromWorld for Presentation {
    fn from_world(world: &mut World) -> Self {
        Self {
            labels: world.query_filtered(),
            buttons: world.query_filtered(),
            label_buffer: Vec::new(),
            button_buffer: Vec::new(),
            source: None,
            message: String::new(),
        }
    }
}

impl Presentation {
    fn refresh(&mut self, world: &mut World) {
        self.label_buffer.clear();
        self.label_buffer.extend(self.labels.iter(world));
        self.button_buffer.clear();
        self.button_buffer.extend(self.buttons.iter(world));
        engine::performance::add(
            "automation-status-consumers",
            (self.label_buffer.len() + self.button_buffer.len()) as u64,
        );
        if self.label_buffer.is_empty() {
            self.source = None;
            self.message.clear();
            if self.button_buffer.is_empty() {
                return;
            }
        }

        let fallback = Status::default();
        let status = world.get_resource::<Status>().unwrap_or(&fallback);
        let disabled = !status.available
            || !status.active
            || status.locked
            || status.readers == 0 && status.controllers == 0;
        if !self.label_buffer.is_empty() {
            let source = MessageSource::current(status);
            if self.source != Some(source) {
                source.write(&mut self.message);
                self.source = Some(source);
                engine::performance::add("automation-status-messages-prepared", 1);
            }
            for &label in &self.label_buffer {
                input_settings::text(world, label, self.message.as_str());
            }
        }
        for &button in &self.button_buffer {
            input_settings::disable(world, button, disabled);
        }
    }
}

pub(super) fn refresh(world: &mut World) {
    let _presentation = engine::performance::Span::new("automation-status-presentation");
    world.init_resource::<Presentation>();
    world.resource_scope(|world, mut presentation: Mut<Presentation>| presentation.refresh(world));
}
