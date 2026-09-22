use bevy::{prelude::*, text::EditableText};

#[cfg(test)]
pub(crate) mod tests;

pub(crate) fn column(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(100),
                min_height: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

pub(crate) fn row(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            ChildOf(parent),
            Node {
                width: percent(100),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(8),
                row_gap: px(6),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id()
}

pub(crate) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    caption: &str,
    action: impl crate::actions::Action,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            crate::sand::button(0),
            crate::actions::ActionButton::new(owner, crate::actions![action]),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                min_height: px(30),
                flex_shrink: 0.0,
                ..default()
            },
            crate::token_style::border(crate::tokens::Token::Accent),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(caption);
    }
    world
        .entity_mut(entity)
        .insert(crate::icons::Tooltip(caption.into()));
    crate::edit_mode::label(world, entity, caption, 14.0);
    entity
}

pub(crate) fn field(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    crate::edit_mode::label(world, parent, caption, 13.0);
    let entity = crate::sand_text::spawn(
        world,
        parent,
        crate::sand_text::SavedText {
            area: crate::sand_text::SandText::new(true),
            text: value.into(),
        },
    );
    if let Some(mut input) = world.get_mut::<EditableText>(entity) {
        input.allow_newlines = false;
        input.visible_lines = Some(1.0);
    }
    world.entity_mut(entity).insert((
        crate::icons::Tooltip(caption.into()),
        Node {
            width: percent(100),
            min_height: px(32),
            flex_shrink: 0.0,
            padding: UiRect::all(px(6)),
            ..default()
        },
    ));
    entity
}

pub(crate) fn value(world: &World, entity: Entity) -> Result<String, String> {
    let input = world
        .get::<EditableText>(entity)
        .ok_or("Field is unavailable")?;
    if input.is_composing() {
        return Err("Finish typing before saving".into());
    }
    Ok(input.value().to_string())
}

pub(crate) fn clear(world: &mut World, parent: Entity) {
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    for child in children {
        world.despawn(child);
    }
}

pub(crate) fn status(world: &mut World, entity: Entity, message: impl Into<String>) {
    if let Some(mut text) = world.get_mut::<Text>(entity) {
        text.set_if_neq(Text::new(message.into()));
    }
}

pub(crate) fn frame(world: &mut World, sand: Entity, title: &str) -> Entity {
    if let Some(mut node) = world.get_mut::<Node>(sand) {
        node.padding = UiRect::all(px(10));
        node.row_gap = px(8);
        node.overflow = Overflow::clip();
    }
    crate::edit_mode::label(world, sand, title, 20.0);
    let body = column(world, sand);
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
    }
    body
}

pub(crate) fn send(world: &World, message: cell::ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    world
        .get_non_send::<crate::cell_bridge::CellBridge>()
        .ok_or("Not connected to the local Organ")?
        .outgoing
        .try_send(message)
        .map_err(|error| match error {
            tokio::sync::mpsc::error::TrySendError::Full(_) => "Connection busy; try again".into(),
            tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                "Connection closed; reopen the interface".into()
            }
        })
}

pub(crate) fn credits(
    world: &mut World,
    controls: Entity,
    parent: Entity,
    attributions: &'static [crate::credits::Attribution],
) {
    let content = column(world, parent);
    world.entity_mut(content).insert(Node {
        display: Display::None,
        width: percent(100),
        height: px(240),
        flex_shrink: 0.0,
        flex_direction: FlexDirection::Column,
        overflow: Overflow::scroll_y(),
        ..default()
    });
    crate::scroll_sand::attach(world, content);
    button(
        world,
        controls,
        content,
        "Licenses and credits",
        Credits(attributions),
    );
}

#[derive(Clone)]
struct Credits(&'static [crate::credits::Attribution]);
#[derive(Component)]
struct CreditsLoaded;
impl crate::actions::Action for Credits {
    fn apply(&self, world: &mut World, content: Entity) {
        let Some(mut node) = world.get_mut::<Node>(content) else {
            return;
        };
        let showing = node.display == Display::None;
        node.display = if showing {
            Display::Flex
        } else {
            Display::None
        };
        if showing && world.get::<CreditsLoaded>(content).is_none() {
            clear(world, content);
            crate::credits::render_list(world, content, self.0);
            crate::credits::render_list(world, content, crate::credits::ATTRIBUTIONS);
            world.entity_mut(content).insert(CreditsLoaded);
        }
    }
}
