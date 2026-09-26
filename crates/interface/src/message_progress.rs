use crate::{actions::Action, protein_area::RecordBinding};
use bevy::{prelude::*, text::EditableText};
use nucleus::operation::{Priority, Step, StepState};

#[derive(Component)]
struct Editor(Entity);

#[derive(Component)]
struct Pending {
    status: Entity,
}

pub(crate) fn composer(world: &mut World, owner: Entity, parent: Entity) {
    let input = world
        .spawn((
            crate::sand::text_editor("", world.resource::<crate::theme::Typography>(), 0),
            ChildOf(parent),
            crate::icons::Tooltip("Step list · one step per line".into()),
        ))
        .insert(Node {
            width: px(260),
            max_height: px(96),
            ..default()
        })
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(8192);
    world.entity_mut(owner).insert(Editor(input));
    crate::description::button(world, parent, owner, "Add step list", Add);
}

#[derive(Clone)]
struct Add;
impl Action for Add {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(draft) = world.get::<crate::message_content::Draft>(owner) else {
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
            .to_string()
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
        if nucleus::message::validate(&parts).is_err() {
            return;
        }
        world
            .get_mut::<crate::message_content::Draft>(owner)
            .unwrap()
            .parts = parts;
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("");
        crate::message_content::render_draft(world, owner);
    }
}

pub(crate) fn steps(
    world: &mut World,
    parent: Entity,
    binding: &RecordBinding,
    data: &serde_json::Value,
    part: usize,
    steps: &[Step],
) {
    let status =
        crate::edit_mode::label(world, parent, "Step list · author can change status", 13.0);
    for (index, step) in steps.iter().enumerate() {
        let row = world
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id();
        crate::edit_mode::label(
            world,
            row,
            &format!("{} · {:?} · {:?}", step.content, step.status, step.priority),
            13.0,
        );
        let mut next = data["content"].clone();
        next["parts"][part]["steps"][index]["status"] = serde_json::json!(match step.status {
            StepState::Pending => "in_progress",
            StepState::InProgress => "completed",
            StepState::Completed => "pending",
        });
        let owner = world.spawn((Node::default(), ChildOf(row))).id();
        world.entity_mut(owner).insert(Pending { status });
        crate::description::button(
            world,
            row,
            owner,
            "Change status",
            Change {
                binding: binding.clone(),
                uid: data["uid"].as_str().unwrap_or_default().into(),
                value: next,
            },
        );
    }
}

#[derive(Clone)]
struct Change {
    binding: RecordBinding,
    uid: String,
    value: serde_json::Value,
}
impl Action for Change {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(status) = world.get::<Pending>(owner).map(|pending| pending.status) else {
            return;
        };
        let result = crate::protein_area::execute(
            world,
            &self.binding,
            owner,
            engine::actions::Action::SetExtension {
                target: self.uid.clone(),
                namespace: "lince.message-content".into(),
                fds: self.value.clone(),
            },
        );
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

pub(crate) fn progress(world: &mut World, parent: Entity, value: &serde_json::Value) {
    if value.is_null() {
        return;
    }
    crate::edit_mode::label(
        world,
        parent,
        &format!(
            "Agent plan · {} · read-only",
            value["state"].as_str().unwrap_or("unknown")
        ),
        14.0,
    );
    if let Ok(steps) = serde_json::from_value::<Vec<Step>>(value["steps"].clone()) {
        crate::edit_mode::label(world, parent, &nucleus::operation::steps_text(&steps), 13.0);
    }
}
