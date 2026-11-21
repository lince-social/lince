// non-lean-yet
// Scope: struct Presentation, struct Controls, fn render, fn create.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 84.

use super::*;
use crate::native::{actions::ActionButton, retained};

#[derive(Component, Default)]
struct Presentation(retained::Children<u64>);

#[derive(Component)]
struct Controls {
    index: usize,
    remove: Entity,
    convert: Option<Entity>,
}

pub(super) fn render(
    world: &mut World,
    list: Entity,
    owner: Entity,
    binding: &RecordBinding,
    parts: &Parts,
) {
    let _presentation = nucleus::performance::Span::new("message-draft-presentation");
    let mut presentation = world
        .entity_mut(list)
        .take::<Presentation>()
        .unwrap_or_default();
    presentation.0.reconcile(
        world,
        list,
        parts
            .rows()
            .enumerate()
            .map(|(index, (key, part))| (key, (index, part))),
        |world, parent, &(index, part)| create(world, parent, owner, binding, index, part),
        |world, row, &(index, _)| {
            let Some(mut controls) = world.get_mut::<Controls>(row) else {
                return;
            };
            if controls.index == index {
                return;
            }
            controls.index = index;
            let (remove, convert) = (controls.remove, controls.convert);
            world.entity_mut(remove).insert(ActionButton::new(
                owner,
                crate::native::actions![Remove(index)],
            ));
            if let Some(convert) = convert {
                world.entity_mut(convert).insert(ActionButton::new(
                    owner,
                    crate::native::actions![Convert(index)],
                ));
            }
        },
    );
    world.entity_mut(list).insert(presentation);
}

fn create(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    binding: &RecordBinding,
    index: usize,
    part: &MessagePart,
) -> Entity {
    nucleus::performance::add("message-draft-rows-created", 1);
    let row = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    if matches!(part, MessagePart::Attachment { .. }) {
        attachments::draft_preview(world, row, binding, part);
    } else {
        let label = match part {
            MessagePart::Question { question } => {
                format!("Question for {}: {}", question.responder, question.prompt)
            }
            MessagePart::Steps { steps } => nucleus::operation::steps_text(steps),
            MessagePart::Text { text } => text.clone(),
            MessagePart::Reference { name, uri } => format!("{name} · reference: {uri}"),
            MessagePart::Attachment { .. } => unreachable!(),
        };
        crate::native::edit_mode::label(world, row, &label, 13.0);
    }
    let remove = crate::native::description::button(world, row, owner, "Remove", Remove(index));
    let convert = if matches!(part, MessagePart::Attachment { mime_type, .. } if mime_type.starts_with("text/") || mime_type == "application/json")
    {
        Some(crate::native::description::button(
            world,
            row,
            owner,
            "Convert this attachment to text",
            Convert(index),
        ))
    } else {
        None
    };
    world.entity_mut(row).insert(Controls {
        index,
        remove,
        convert,
    });
    row
}
