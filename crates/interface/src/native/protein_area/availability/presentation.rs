// non-lean-yet
// Scope: struct Message, struct Prepared, fn prepare, enum VisibilityChange.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (54, 146, 149, 226, 236).

use super::*;
use crate::host::performance::{self, Count, Stage};

struct Message<'a> {
    text: &'a str,
    suffix: &'static str,
}

impl<'a> Message<'a> {
    fn plain(text: &'a str) -> Self {
        Self { text, suffix: "" }
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty() && self.suffix.is_empty()
    }

    fn matches(&self, current: &str) -> bool {
        current.strip_prefix(self.text) == Some(self.suffix)
    }

    fn into_owned(self) -> String {
        performance::count(Count::RecordAvailabilityMessages);
        let mut text = String::with_capacity(self.text.len() + self.suffix.len());
        text.push_str(self.text);
        text.push_str(self.suffix);
        text
    }
}

pub(super) struct Prepared {
    panel: Entity,
    row: Entity,
    data: Option<usize>,
    label: Entity,
    message: Option<String>,
    visible: bool,
    retry: Entity,
    retry_label: Entity,
    retry_caption: &'static str,
    retry_changed: bool,
    retry_display: Display,
}

pub(super) fn prepare(world: &World, owner: Entity, state: &State) -> Option<Prepared> {
    let _prepare = performance::Span::new(Stage::RecordAvailabilityPreparation);
    let row = state.row_entities.get(state.single_record.as_ref()?)?;
    let entity = world.get::<RowPanel>(*row)?.0;
    let panel = world.get::<Panel>(entity)?;
    if panel.binding.area != owner {
        return None;
    }
    performance::count(Count::RecordAvailabilityPanels);
    let config = state.applied.as_deref().unwrap();
    let creating = config.creating();
    let creation = world.get::<crate::native::full_record::window::Creation>(owner);
    let can_retry = creation.is_some_and(|creation| creation.retry_available());
    let data = if creating {
        None
    } else {
        state
            .data
            .iter()
            .position(|data| data["uid"].as_str() == Some(panel.binding.uid.as_str()))
    };
    let loading = world.get::<rows::Loading>(panel.row).is_some();
    let suffix = if loading { "" } else { " Your draft is kept." };
    let message = if creating {
        Message::plain(creation.map_or(
            "Window Record creation is waiting for its original Park session.",
            |creation| creation.message(),
        ))
    } else if state.login {
        Message {
            text: "Log in to this Record's Organ.",
            suffix,
        }
    } else if state.refreshing {
        Message {
            text: "Loading Record…",
            suffix,
        }
    } else if data.is_none() && state.ready {
        Message {
            text: "Record unavailable. It may have been deleted or access may have changed.",
            suffix,
        }
    } else if !state.ready {
        Message {
            text: lince_interface::text::prefix(&state.status, 1024),
            suffix: if loading {
                "."
            } else {
                ". Your draft is kept."
            },
        }
    } else {
        Message::plain(
            crate::native::record_binding::failed_message(
                world,
                &config.source,
                &panel.binding.uid,
            )
            .map_or("", |message| lince_interface::text::prefix(message, 1024)),
        )
    };
    let retry_caption = if creating {
        "Retry Record creation"
    } else {
        "Refresh Record"
    };
    let retry_changed = world
        .get::<Text>(panel.retry_label)
        .is_some_and(|text| text.0 != retry_caption);
    let visible = !message.is_empty();
    let changed = world.get::<Text>(panel.label).is_some_and(|text| {
        let current = if panel.label == panel.retry_label && retry_changed {
            retry_caption
        } else {
            text.0.as_str()
        };
        !message.matches(current)
    });
    Some(Prepared {
        panel: entity,
        row: panel.row,
        data,
        label: panel.label,
        message: changed.then(|| message.into_owned()),
        visible,
        retry: panel.retry,
        retry_label: panel.retry_label,
        retry_caption,
        retry_changed,
        retry_display: if creating && !can_retry || !creating && (state.refreshing || state.login) {
            Display::None
        } else {
            Display::Flex
        },
    })
}

impl Prepared {
    pub(super) fn publish(self, world: &mut World) -> (Entity, Entity, Option<usize>) {
        if self.retry_changed {
            world.get_mut::<Text>(self.retry_label).unwrap().0 = self.retry_caption.into();
        }
        if let Some(message) = self.message {
            world.get_mut::<Text>(self.label).unwrap().0 = message;
        }
        set_display(
            world,
            self.panel,
            if self.visible {
                Display::Flex
            } else {
                Display::None
            },
        );
        set_display(world, self.retry, self.retry_display);
        (self.panel, self.row, self.data)
    }
}

enum VisibilityChange {
    Hide {
        entity: Entity,
        previous: Option<Display>,
        write_display: bool,
    },
    Restore {
        entity: Entity,
        display: Display,
        write_display: bool,
    },
}

pub(super) fn children(world: &mut World, row: Entity, available: bool) {
    let _visibility = performance::Span::new(Stage::RecordAvailabilityVisibility);
    let Some(children) = world.get::<Children>(row) else {
        return;
    };
    performance::add(Count::RecordAvailabilityChildren, children.len() as u64);
    let mut changes = Vec::new();
    for entity in children.iter() {
        if chrome(world, entity) {
            continue;
        }
        let hidden = world.get::<Hidden>(entity);
        let node = world.get::<Node>(entity);
        if available {
            if let Some(previous) = hidden {
                changes.push(VisibilityChange::Restore {
                    entity,
                    display: previous.0,
                    write_display: node.is_some_and(|node| node.display != previous.0),
                });
            }
        } else if let Some(node) = node {
            let previous = hidden.is_none().then_some(node.display);
            let write_display = node.display != Display::None;
            if previous.is_some() || write_display {
                changes.push(VisibilityChange::Hide {
                    entity,
                    previous,
                    write_display,
                });
            }
        }
    }
    performance::add(
        Count::RecordAvailabilityChildrenUpdated,
        changes.len() as u64,
    );
    for change in changes {
        match change {
            VisibilityChange::Hide {
                entity,
                previous,
                write_display,
            } => {
                if let Some(previous) = previous {
                    world.entity_mut(entity).insert(Hidden(previous));
                }
                if write_display {
                    world.get_mut::<Node>(entity).unwrap().display = Display::None;
                }
            }
            VisibilityChange::Restore {
                entity,
                display,
                write_display,
            } => {
                world.entity_mut(entity).remove::<Hidden>();
                if write_display {
                    world.get_mut::<Node>(entity).unwrap().display = display;
                }
            }
        }
    }
}
