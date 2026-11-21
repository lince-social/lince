// non-lean-yet
// Scope: mod controls, mod description, mod maintenance, struct View.
// BeautifulRust (baseline lines):
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (81).
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (31).

use super::*;
use crate::native::edit_mode::label;

mod controls;
mod description;
mod maintenance;

#[derive(Component, Clone, Copy)]
struct View {
    root: Entity,
    area: Option<Entity>,
    text: Entity,
    controls: Entity,
}

fn button(world: &mut World, parent: Entity, root: Entity, action: Control, title: &str) {
    let entity = world
        .spawn((
            crate::native::sand::button(0),
            crate::native::actions::ActionButton::new(root, crate::native::actions![action]),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world
        .get_mut::<bevy::a11y::AccessibilityNode>(entity)
        .unwrap()
        .set_label(title);
    label(world, entity, title, 14.0);
}

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity, area: Option<Entity>) {
    label(world, parent, "Rules affecting a Sand", 18.0);
    label(
        world,
        parent,
        "Click a Sand in edit mode. This explanation stays available while you adjust its Areas. Pauses stop Area effects, sounds and boundary changes. Submitted Record changes may still finish.",
        14.0,
    );
    let text = label(
        world,
        parent,
        "Select a Sand to inspect its evaluated rules.",
        14.0,
    );
    let controls = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    world.entity_mut(controls).insert(View {
        root,
        area,
        text,
        controls,
    });
}

pub(crate) fn describe(
    world: &World,
    root: Entity,
    sand: Entity,
    filter: Option<Entity>,
) -> String {
    description::Description::new(world, root, Some(sand), filter).to_string()
}

pub(crate) fn update(world: &mut World) {
    world
        .run_system_cached(maintenance::update)
        .expect("refresh current influence report panels");
}
