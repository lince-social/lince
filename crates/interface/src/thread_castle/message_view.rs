use super::*;

#[derive(Component)]
pub(super) struct Status;

pub(crate) fn status(world: &mut World, parent: Entity) -> Entity {
    let entity = crate::edit_mode::label(world, parent, "", 12.0);
    world.entity_mut(entity).insert(Status);
    world.get_mut::<Node>(entity).unwrap().display = Display::None;
    entity
}

pub(super) fn statuses(mut labels: Query<(&Text, &mut Node), With<Status>>) {
    for (text, mut node) in &mut labels {
        node.display = if text.0.is_empty() || text.0 == "Saved" {
            Display::None
        } else {
            Display::Flex
        };
    }
}

pub(super) fn sync_edits(world: &mut World) {
    let updates: Vec<_> = world
        .query::<&Message>()
        .iter(world)
        .filter_map(|message| {
            if !crate::record_binding::active(world, message.input) {
                return None;
            }
            let text = world
                .get::<EditableText>(message.input)?
                .value()
                .to_string();
            (world
                .get::<crate::description::Description>(message.preview)?
                .source
                != text)
                .then_some((message.preview, text))
        })
        .collect();
    for (preview, text) in updates {
        crate::description::set(world, preview, &text);
    }
}

pub(super) fn spawn(
    world: &mut World,
    parent: Entity,
    binding: &RecordBinding,
    data: &Value,
) -> Entity {
    let uid = data["uid"].as_str().unwrap();
    let entity = world
        .spawn((
            Node {
                width: percent(100),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                row_gap: px(2),
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let identity = world
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                margin: UiRect::top(px(10)),
                ..default()
            },
            ChildOf(entity),
        ))
        .id();
    world.spawn((
        Node {
            width: px(20),
            height: px(20),
            flex_shrink: 0.0,
            ..default()
        },
        BackgroundColor(Color::WHITE),
        ChildOf(identity),
    ));
    let author_name = crate::edit_mode::label(world, identity, "", 14.0);
    let content = world
        .spawn((
            Node {
                width: percent(100),
                padding: UiRect::left(px(28)),
                flex_direction: FlexDirection::Column,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(entity),
        ))
        .id();
    let status = status(world, content);
    let input = world
        .spawn((
            crate::sand::text_editor(
                data["body"].as_str().unwrap_or_default(),
                world.resource::<crate::theme::Typography>(),
                0,
            ),
            crate::sand::Borderless,
            ChildOf(content),
        ))
        .id();
    world.get_mut::<EditableText>(input).unwrap().max_characters = Some(65_536);
    world.get_mut::<Node>(input).unwrap().border = UiRect::ZERO;
    world.get_mut::<TextFont>(input).unwrap().font_size = 16.0.into();
    let preview = crate::description::spawn(
        world,
        content,
        data["body"].as_str().unwrap_or_default(),
        crate::description::Context {
            owner: entity,
            source: binding.source.clone(),
        },
    );
    world.get_mut::<Node>(input).unwrap().display = Display::None;
    transcript::populate(world, entity, binding, data);
    let actions = world
        .spawn((
            Node {
                column_gap: px(8),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    for (title, action) in [("Edit", true), ("Delete", false)] {
        let button = if action {
            control(world, actions, entity, title, Edit)
        } else {
            control(world, actions, entity, title, AskDelete(true))
        };
        world.entity_mut(button).insert((
            crate::icons::Tooltip(
                if action {
                    "Edit message"
                } else {
                    "Delete message"
                }
                .into(),
            ),
            crate::sand::Borderless,
            Node {
                padding: UiRect::axes(px(0), px(2)),
                ..default()
            },
        ));
    }
    let confirmation = world
        .spawn((
            Node {
                display: Display::None,
                width: percent(100),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(6),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    crate::edit_mode::label(world, confirmation, "Delete this message Record?", 14.0);
    control(world, confirmation, entity, "Delete Record", Delete);
    control(world, confirmation, entity, "Cancel", AskDelete(false));
    world.entity_mut(entity).insert(Message {
        binding: binding.clone(),
        uid: uid.into(),
        status,
        confirmation,
        pending: false,
        identity,
        author_name,
        input,
        preview,
    });
    entity
}

pub(super) fn refresh(world: &mut World, entity: Entity, data: &Value, previous: Option<&str>) {
    let message = world.get::<Message>(entity).unwrap();
    let (identity, author_name, input, preview) = (
        message.identity,
        message.author_name,
        message.input,
        message.preview,
    );
    let author = data["author"].as_str();
    world.get_mut::<Node>(identity).unwrap().display = if author.is_some() && author == previous {
        Display::None
    } else {
        Display::Flex
    };
    let name = data["author_name"]
        .as_str()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            (data["organ_uid"].as_str() == author)
                .then(|| data["organ_name"].as_str())
                .flatten()
        })
        .or(author)
        .unwrap_or("Unknown author");
    world.get_mut::<Text>(author_name).unwrap().0 = name.into();
    if world.get::<Node>(input).unwrap().display == Display::None
        && !world
            .get::<crate::record_binding::TextBinding>(input)
            .is_some_and(|binding| {
                binding.unsaved(
                    &world
                        .get::<EditableText>(input)
                        .unwrap()
                        .value()
                        .to_string(),
                )
            })
    {
        let body = data["body"].as_str().unwrap_or_default();
        if world
            .get::<crate::record_binding::TextBinding>(input)
            .is_none()
        {
            world
                .get_mut::<EditableText>(input)
                .unwrap()
                .editor
                .set_text(body);
        }
        crate::description::set(world, preview, body);
    }
}

#[derive(Clone)]
struct Edit;
impl Action for Edit {
    fn apply(&self, world: &mut World, owner: Entity) {
        let Some(message) = world.get::<Message>(owner) else {
            return;
        };
        let (input, preview, status) = (message.input, message.preview, message.status);
        let binding = RecordBinding {
            uid: message.uid.clone(),
            ..message.binding.clone()
        };
        if world
            .get::<crate::record_binding::TextBinding>(input)
            .is_none()
        {
            crate::record_binding::attach(world, input, binding, "body", Some(status));
        }
        let editing = world.get::<Node>(input).unwrap().display == Display::None;
        if !editing {
            let body = world
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string();
            crate::description::set(world, preview, &body);
        }

        world.get_mut::<Node>(input).unwrap().display = if editing {
            Display::Flex
        } else {
            Display::None
        };
        world.get_mut::<Node>(preview).unwrap().display = if editing {
            Display::None
        } else {
            Display::Flex
        };
        if editing
            && let Some(mut focus) = world.get_resource_mut::<bevy::input_focus::InputFocus>()
        {
            focus.set(input, bevy::input_focus::FocusCause::Navigated);
        }
    }
}

#[derive(Clone)]
pub(crate) struct Details;
impl Action for Details {
    fn apply(&self, world: &mut World, owner: Entity) {
        if let Some(mut node) = world.get_mut::<Node>(owner) {
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_messages_refresh_without_overwriting_an_open_editor() {
        let (mut world, castle, mut data) = super::super::tests::fixture();
        let page = world.get::<ThreadCastle>(castle).unwrap().pages["thread-a"];
        let entity = world.get::<Page>(page).unwrap().messages["message-a"];
        let input = world.get::<Message>(entity).unwrap().input;
        let preview = world.get::<Message>(entity).unwrap().preview;
        assert!(
            world
                .get::<crate::record_binding::TextBinding>(input)
                .is_none()
        );
        data["threads"][0]["messages"][0]["body"] = "Streaming reply".into();
        super::super::refresh(&mut world, castle, &data);
        assert_eq!(
            world
                .get::<crate::description::Description>(preview)
                .unwrap()
                .source,
            "Streaming reply"
        );
        Edit.apply(&mut world, entity);
        assert!(
            world
                .get::<crate::record_binding::TextBinding>(input)
                .is_some()
        );
        assert_eq!(world.get::<Node>(preview).unwrap().display, Display::None);
        world
            .get_mut::<EditableText>(input)
            .unwrap()
            .editor
            .set_text("My concurrent edit");
        data["threads"][0]["messages"][0]["body"] = "Streaming reply completed".into();
        super::super::refresh(&mut world, castle, &data);
        assert_eq!(
            world
                .get::<EditableText>(input)
                .unwrap()
                .value()
                .to_string(),
            "My concurrent edit"
        );
        Edit.apply(&mut world, entity);
        super::super::refresh(&mut world, castle, &data);
        assert_eq!(
            world
                .get::<crate::description::Description>(preview)
                .unwrap()
                .source,
            "My concurrent edit"
        );
        assert_eq!(world.get::<Node>(input).unwrap().display, Display::None);
    }
}
