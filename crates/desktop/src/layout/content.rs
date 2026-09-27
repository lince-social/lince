use bevy::{prelude::*, ui::Val2};

#[derive(Component, Clone, Copy)]
struct ContentScroll {
    original: Val2,
    applied: Val2,
}

pub(super) fn scroll(world: &mut World, parent: Entity, offset: Vec2) -> bool {
    if world.get::<crate::area::InfluenceArea>(parent).is_some() {
        return false;
    }
    let viewport = world
        .get::<ChildOf>(parent)
        .and_then(|parent| world.get::<ComputedNode>(parent.parent()))
        .map_or(Vec2::ZERO, |node| node.size() * node.inverse_scale_factor());
    let children: Vec<_> = world
        .get::<Children>(parent)
        .map(|children| children.iter().collect())
        .unwrap_or_default();
    let mut changed = false;
    for child in children {
        if world.get::<crate::sand_text::SandText>(child).is_some()
            || world.get::<crate::canvas::CanvasItem>(child).is_some()
        {
            continue;
        }
        let Some(transform) = world.get::<UiTransform>(child) else {
            continue;
        };
        let current = transform.translation;
        let mut state = world
            .get::<ContentScroll>(child)
            .copied()
            .unwrap_or(ContentScroll {
                original: current,
                applied: current,
            });
        if state.applied != current {
            state.original = current;
        }
        let next = if offset == Vec2::ZERO {
            state.original
        } else {
            let size = world
                .get::<ComputedNode>(child)
                .map_or(Vec2::ZERO, |node| node.size() * node.inverse_scale_factor());
            let position = state.original.resolve(1.0, size, viewport) - offset;
            Val2::px(position.x, position.y)
        };
        if current != next {
            world.get_mut::<UiTransform>(child).unwrap().translation = next;
            changed = true;
        }
        state.applied = next;
        if changed || world.get::<ContentScroll>(child).is_none() {
            world.entity_mut(child).insert(state);
        }
    }
    changed
}
