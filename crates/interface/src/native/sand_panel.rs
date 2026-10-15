use crate::native::actions::Action;
use bevy::prelude::*;
pub(crate) use lince_interface::controls::{clear, column, row, status, value};

pub(crate) mod tests;

pub(crate) fn button(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    caption: &str,
    action: impl crate::native::actions::Action,
) -> Entity {
    let entity = world
        .spawn((
            ChildOf(parent),
            crate::native::sand::button(0),
            crate::native::canvas_host::authoring::PartName(format!(
                "button-{}",
                caption.to_lowercase().replace(' ', "-")
            )),
            crate::native::actions::ActionButton::new(owner, crate::native::actions![action]),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                min_height: px(30),
                flex_shrink: 0.0,
                ..default()
            },
            crate::native::token_style::border(crate::native::tokens::Token::Accent),
        ))
        .id();
    if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
        node.set_label(caption);
    }
    world
        .entity_mut(entity)
        .insert(crate::native::icons::Tooltip(caption.into()));
    crate::native::edit_mode::label(world, entity, caption, 14.0);
    entity
}

pub(crate) fn field(world: &mut World, parent: Entity, caption: &str, value: &str) -> Entity {
    crate::native::edit_mode::label(world, parent, caption, 13.0);
    let entity = crate::native::sand_text::spawn(
        world,
        parent,
        crate::native::sand_text::SavedText {
            area: crate::native::sand_text::SandText::new(true),
            text: value.into(),
        },
    );
    let input = crate::native::sand::single_line_editor(
        value,
        world.resource::<crate::native::theme::Typography>(),
        0,
        4096,
    );
    world.entity_mut(entity).insert(input);
    world
        .entity_mut(entity)
        .insert(crate::native::canvas_host::authoring::PartName(format!(
            "field-{}",
            caption.to_lowercase().replace(' ', "-")
        )));
    crate::native::accessibility::input(world, entity, caption, false);
    world
        .entity_mut(entity)
        .insert(crate::native::icons::Tooltip(caption.into()));
    entity
}

pub(crate) fn frame(world: &mut World, sand: Entity, title: &str) -> Entity {
    if let Some(mut node) = world.get_mut::<Node>(sand) {
        node.padding = UiRect::all(px(10));
        node.row_gap = px(8);
        node.overflow = Overflow::clip();
    }
    crate::native::edit_mode::label(world, sand, title, 20.0);
    let body = column(world, sand);
    if let Some(mut node) = world.get_mut::<Node>(body) {
        node.flex_grow = 1.0;
        node.flex_shrink = 1.0;
    }
    body
}

pub(crate) fn send(world: &World, message: cell::ClientMessage) -> Result<(), String> {
    if crate::native::laboratory::active(world) {
        return Err("Changes are unavailable in the Laboratory".into());
    }
    world
        .get_non_send::<crate::native::cell_bridge::CellBridge>()
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
    attributions: &'static [crate::native::credits::Attribution],
) {
    let content = column(world, parent);
    world.entity_mut(content).insert((
        CreditsOwner(parent),
        Node {
            display: Display::None,
            width: percent(100),
            height: px(240),
            flex_shrink: 0.0,
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(),
            ..default()
        },
    ));
    crate::native::scroll_sand::attach(world, content);
    button(
        world,
        controls,
        content,
        "Licenses and credits",
        Credits(attributions),
    );
}

#[derive(Clone)]
struct Credits(&'static [crate::native::credits::Attribution]);
#[derive(Component)]
struct CreditsLoaded;

#[derive(Component)]
struct CreditsOwner(Entity);

pub(crate) fn show_credits(world: &mut World, owner: Entity) {
    let contents: Vec<_> = world
        .query::<(Entity, &CreditsOwner, &Node)>()
        .iter(world)
        .filter(|(_, context, node)| context.0 == owner && node.display == Display::None)
        .map(|(entity, _, _)| entity)
        .collect();
    for content in contents {
        let attributions = world
            .get::<crate::native::sand_store::SandCredits>(owner)
            .map_or(crate::native::credits::ATTRIBUTIONS, |credits| credits.0);
        Credits(attributions).apply(world, content);
    }
}

pub(crate) fn credits_visible(world: &mut World, owner: Entity) -> bool {
    world
        .query::<(&CreditsOwner, &Node, Option<&CreditsLoaded>)>()
        .iter(world)
        .any(|(context, node, loaded)| {
            context.0 == owner && node.display != Display::None && loaded.is_some()
        })
}
impl crate::native::actions::Action for Credits {
    fn tutorial_operations(&self) -> &'static [lince_interface::practice::Operation] {
        &[lince_interface::practice::Operation::InspectSandCredits]
    }
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
            crate::native::credits::render_list(world, content, self.0);
            crate::native::credits::render_list(
                world,
                content,
                crate::native::credits::ATTRIBUTIONS,
            );
            world.entity_mut(content).insert(CreditsLoaded);
        }
    }
}
