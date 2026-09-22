use super::*;

#[derive(Component)]
#[require(
    HoverEvents,
    bevy::ui_widgets::Button,
    bevy::input_focus::tab_navigation::TabIndex
)]
pub struct TooltipIcon {
    pub source: Entity,
}

#[derive(Component, Clone)]
struct Attachment {
    icon: Entity,
    padding: (Val, Val),
    width: (Val, Val),
    margin: (Val, Val),
}

fn reserve(value: Val) -> Val {
    match value {
        Val::Px(value) => px(value + 20.0),
        _ => px(20),
    }
}

pub(super) fn sync(world: &mut World) {
    if !world.resource::<TooltipSettings>().enabled {
        let attachments: Vec<_> = world
            .query::<(Entity, &Attachment)>()
            .iter(world)
            .map(|(entity, attachment)| (entity, attachment.clone()))
            .collect();
        for (entity, attachment) in attachments {
            remove(world, entity, &attachment);
        }
        return;
    }
    let removed: Vec<_> = world
        .query_filtered::<(Entity, &Attachment), Without<Tooltip>>()
        .iter(world)
        .map(|(entity, attachment)| (entity, attachment.clone()))
        .collect();
    for (entity, attachment) in removed {
        remove(world, entity, &attachment);
    }
    let sources: Vec<_> = world
        .query::<(Entity, Ref<Tooltip>, Ref<Node>, Option<&Attachment>)>()
        .iter(world)
        .filter(|(_, tip, node, attachment)| {
            tip.is_changed()
                || node.is_changed()
                || attachment
                    .is_none_or(|attachment| world.get::<TooltipIcon>(attachment.icon).is_none())
        })
        .map(|(entity, tip, _, attachment)| (entity, tip.0.clone(), attachment.cloned()))
        .collect();
    for (source, text, mut attachment) in sources {
        if let Some(previous) = &attachment
            && world.get::<TooltipIcon>(previous.icon).is_none()
        {
            remove(world, source, previous);
            attachment = None;
        }
        if text.is_empty() {
            if let Some(attachment) = attachment {
                remove(world, source, &attachment);
            }
            continue;
        }
        let outside = world.get::<IconButton>(source).is_some();
        let mut attachment = attachment.unwrap_or_else(|| {
            let icon = world
                .spawn((
                    TooltipIcon { source },
                    crate::sand::Borderless,
                    image(world, Icon::Info).unwrap(),
                    Node {
                        position_type: PositionType::Absolute,
                        width: px(16),
                        height: px(16),
                        right: px(if outside { -18 } else { 2 }),
                        top: percent(50),
                        margin: UiRect::top(px(-8)),
                        ..default()
                    },
                    ZIndex(1),
                    ChildOf(source),
                ))
                .id();
            let node = world.get::<Node>(source).unwrap();
            Attachment {
                icon,
                padding: (node.padding.right, node.padding.right),
                width: (node.width, node.width),
                margin: (node.margin.right, node.margin.right),
            }
        });
        let mut accessibility = world.get_mut::<AccessibilityNode>(attachment.icon).unwrap();
        let label = format!("Info: {text}");
        if accessibility.label() != Some(label.as_str()) {
            accessibility.set_label(label);
        }
        let node = world.get::<Node>(source).unwrap();
        if node.padding.right != attachment.padding.1 {
            attachment.padding.0 = node.padding.right;
        }
        if node.width != attachment.width.1 {
            attachment.width.0 = node.width;
        }
        if node.margin.right != attachment.margin.1 {
            attachment.margin.0 = node.margin.right;
        }
        attachment.margin.1 = if outside {
            reserve(attachment.margin.0)
        } else {
            attachment.margin.0
        };
        attachment.padding.1 = if outside {
            attachment.padding.0
        } else {
            reserve(attachment.padding.0)
        };
        attachment.width.1 = match (outside, attachment.width.0) {
            (false, Val::Px(width)) => px(width + 20.0),
            (_, width) => width,
        };
        if node.padding.right != attachment.padding.1
            || node.width != attachment.width.1
            || node.margin.right != attachment.margin.1
        {
            let mut node = world.get_mut::<Node>(source).unwrap();
            node.padding.right = attachment.padding.1;
            node.width = attachment.width.1;
            node.margin.right = attachment.margin.1;
        }
        world.entity_mut(source).insert(attachment);
    }
}

fn remove(world: &mut World, source: Entity, attachment: &Attachment) {
    if let Some(mut node) = world.get_mut::<Node>(source) {
        if node.padding.right == attachment.padding.1 {
            node.padding.right = attachment.padding.0;
        }
        if node.width == attachment.width.1 {
            node.width = attachment.width.0;
        }
        if node.margin.right == attachment.margin.1 {
            node.margin.right = attachment.margin.0;
        }
    }
    if world.get_entity(attachment.icon).is_ok() {
        world.despawn(attachment.icon);
    }
    world.entity_mut(source).remove::<Attachment>();
}
