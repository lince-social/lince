use crate::native::{actions::Action, message_content::ContentRef};
use bevy::{prelude::*, text::EditableText};
use nucleus::operation::{Priority, Step, StepState};
use serde::Deserialize;

#[derive(Component)]
struct Editor(Entity);

#[derive(Component)]
struct Pending {
    status: Entity,
}

pub(crate) fn composer(world: &mut World, owner: Entity, parent: Entity) {
    let input = world
        .spawn((
            crate::native::sand::text_editor(
                "",
                world.resource::<crate::native::theme::Typography>(),
                0,
            ),
            ChildOf(parent),
            crate::native::icons::Tooltip("Step list · one step per line".into()),
        ))
        .insert(Node {
            width: px(260),
            max_width: percent(100),
            max_height: px(96),
            ..default()
        })
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(8192);
    world.entity_mut(owner).insert(Editor(input));
    crate::native::description::button(world, parent, owner, "Add step list", Add);
}

#[derive(Clone)]
struct Add;
impl Action for Add {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<crate::native::message_content::Draft>(owner) else {
            return;
        };
        if draft.locked {
            return;
        }
        let Some(input) = world.get::<Editor>(owner).map(|editor| editor.0) else {
            return;
        };
        let steps: Vec<_> = world
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_cow()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| Step {
                content: line.into(),
                status: StepState::Pending,
                priority: Priority::Medium,
            })
            .collect();
        if nucleus::operation::validate_steps(&steps).is_err() {
            return;
        }
        let mut parts = draft.parts.clone();
        parts.push(nucleus::message::MessagePart::Steps { steps });
        if parts.validate().is_err() {
            return;
        }
        world
            .get_mut::<crate::native::message_content::Draft>(owner)
            .unwrap()
            .parts = parts;
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("");
        crate::native::message_content::render_draft(world, owner);
    }
}

#[derive(Component)]
struct StepsView {
    status: Entity,
    rows: Entity,
    children: crate::native::retained::Children<usize>,
}

#[derive(Component)]
struct StepView {
    value: Step,
    content: ContentRef,
    label: Entity,
    button: Entity,
    owner: Entity,
}

pub(crate) fn clear_steps(world: &mut World, parent: Entity) {
    world.entity_mut(parent).remove::<StepsView>();
}

pub(crate) fn steps(world: &mut World, parent: Entity, content: ContentRef, steps: &[Step]) {
    let _presentation = nucleus::performance::Span::new("message-step-list-presentation");
    let mut previous = world.entity_mut(parent).take::<StepsView>();
    if previous.as_ref().is_some_and(|view| {
        [view.status, view.rows].iter().any(|entity| {
            world
                .get::<ChildOf>(*entity)
                .is_none_or(|child| child.parent() != parent)
        })
    }) {
        world.entity_mut(parent).despawn_children();
        previous = None;
    }
    let mut view = previous.unwrap_or_else(|| {
        let status = crate::native::edit_mode::label(
            world,
            parent,
            "Step list · author can change status",
            13.0,
        );
        let rows = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();
        StepsView {
            status,
            rows,
            children: Default::default(),
        }
    });
    let status = view.status;
    view.children.reconcile(
        world,
        view.rows,
        steps
            .iter()
            .enumerate()
            .map(|(index, step)| (index, (index, step))),
        |world, parent, &(index, step)| create_step(world, parent, status, content, index, step),
        |world, row, &(index, step)| update_step(world, row, content, index, step),
    );
    world.entity_mut(parent).insert(view);
}

fn create_step(
    world: &mut World,
    parent: Entity,
    status: Entity,
    content: ContentRef,
    index: usize,
    step: &Step,
) -> Entity {
    nucleus::performance::add("message-step-rows-created", 1);
    let row = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let label = crate::native::edit_mode::label(
        world,
        row,
        &format!("{} · {:?} · {:?}", step.content, step.status, step.priority),
        13.0,
    );
    let owner = world
        .spawn((Node::default(), ChildOf(row), Pending { status }))
        .id();
    let button = crate::native::description::button(
        world,
        row,
        owner,
        "Change status",
        Change::for_step(content, index, step),
    );
    world.entity_mut(row).insert(StepView {
        value: step.clone(),
        content,
        label,
        button,
        owner,
    });
    row
}

fn update_step(world: &mut World, row: Entity, content: ContentRef, index: usize, step: &Step) {
    let Some(view) = world.get::<StepView>(row) else {
        return;
    };
    let changed = view.value != *step;
    if !changed && view.content == content {
        return;
    }
    let (label, button, owner) = (view.label, view.button, view.owner);
    if changed {
        world.get_mut::<Text>(label).unwrap().0 =
            format!("{} · {:?} · {:?}", step.content, step.status, step.priority);
    }
    world
        .entity_mut(button)
        .insert(crate::native::actions::ActionButton::new(
            owner,
            crate::native::actions![Change::for_step(content, index, step)],
        ));
    nucleus::performance::add("message-step-controls-updated", 1);
    let mut view = world.get_mut::<StepView>(row).unwrap();
    if changed {
        view.value.clone_from(step);
    }
    view.content = content;
}

#[derive(Clone)]
struct Change {
    content: ContentRef,
    index: usize,
    state: StepState,
}
impl Change {
    fn for_step(content: ContentRef, index: usize, step: &Step) -> Self {
        Self {
            content,
            index,
            state: match step.status {
                StepState::Pending => StepState::InProgress,
                StepState::InProgress => StepState::Completed,
                StepState::Completed => StepState::Pending,
            },
        }
    }
}

impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(status) = world.get::<Pending>(owner).map(|pending| pending.status) else {
            return;
        };
        let result = self.content.update(world, owner, |part| {
            let step = part
                .get_mut("steps")
                .and_then(serde_json::Value::as_array_mut)
                .and_then(|steps| steps.get_mut(self.index))
                .filter(|step| step.is_object())
                .ok_or("This message step is no longer available.")?;
            step["status"] = serde_json::json!(self.state);
            Ok(())
        });
        world.get_mut::<Text>(status).unwrap().0 =
            result.err().unwrap_or_else(|| "Saving step…".into());
    }
}

pub(crate) fn finished(world: &mut World, owner: Entity, error: Option<String>) -> bool {
    let Some(status) = world.get::<Pending>(owner).map(|pending| pending.status) else {
        return false;
    };
    if let Some(mut text) = world.get_mut::<Text>(status) {
        text.0 = error.unwrap_or_else(|| "Step saved".into());
    }
    true
}

#[derive(Component)]
struct AgentProgress {
    state: String,
    header: Entity,
    steps: Option<Entity>,
    steps_value: Option<serde_json::Value>,
}

/// The parent is a dedicated progress section. Only presentation inputs are
/// retained: unrelated progress metadata does not invalidate these labels.
pub(crate) fn progress(world: &mut World, parent: Entity, value: &serde_json::Value) {
    let _presentation = nucleus::performance::Span::new("message-agent-progress-presentation");
    if value.is_null() {
        if world.entity_mut(parent).take::<AgentProgress>().is_some() {
            world.entity_mut(parent).despawn_children();
        }
        return;
    }
    let state = value["state"].as_str().unwrap_or("unknown");
    let steps = &value["steps"];
    let previous = world.get::<AgentProgress>(parent);
    let valid = previous.is_some_and(|view| {
        std::iter::once(view.header)
            .chain(view.steps)
            .all(|entity| {
                world.get::<Text>(entity).is_some()
                    && world
                        .get::<ChildOf>(entity)
                        .is_some_and(|child| child.parent() == parent)
            })
    });
    let steps_changed = !valid
        || previous.is_none_or(|view| {
            view.steps_value
                .as_ref()
                .is_none_or(|old| !nucleus::json::same_value_encoding(old, steps))
        });
    if !steps_changed && previous.is_some_and(|view| view.state == state) {
        return;
    }
    let previous = world.entity_mut(parent).take::<AgentProgress>();
    let mut view = if valid {
        previous.unwrap()
    } else {
        world.entity_mut(parent).despawn_children();
        let header = crate::native::edit_mode::label(
            world,
            parent,
            &format!("Agent plan · {state} · read-only"),
            14.0,
        );
        nucleus::performance::add("message-agent-progress-text-updates", 1);
        AgentProgress {
            state: state.into(),
            header,
            steps: None,
            steps_value: None,
        }
    };
    if view.state != state {
        world.get_mut::<Text>(view.header).unwrap().0 = format!("Agent plan · {state} · read-only");
        view.state = state.into();
        nucleus::performance::add("message-agent-progress-text-updates", 1);
    }
    if steps_changed {
        nucleus::performance::add("message-agent-progress-decodes", 1);
        match Vec::<Step>::deserialize(steps) {
            Ok(steps) => {
                let text = nucleus::operation::steps_text(&steps);
                if let Some(entity) = view.steps {
                    if world.get::<Text>(entity).unwrap().0 != text {
                        world.get_mut::<Text>(entity).unwrap().0 = text;
                        nucleus::performance::add("message-agent-progress-text-updates", 1);
                    }
                } else {
                    view.steps = Some(crate::native::edit_mode::label(world, parent, &text, 13.0));
                    nucleus::performance::add("message-agent-progress-text-updates", 1);
                }
            }
            Err(_) => {
                if let Some(entity) = view.steps.take() {
                    world.despawn(entity);
                }
            }
        }
        view.steps_value = Some(steps.clone());
    }
    world.entity_mut(parent).insert(view);
}
