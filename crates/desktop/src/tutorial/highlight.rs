use bevy::prelude::*;

#[derive(Component)]
pub struct TutorialHighlight {
    pub target: Entity,
}

#[derive(Component, Default)]
pub(super) struct Highlights {
    targets: Vec<Entity>,
    outlines: Vec<Entity>,
    reveal: bool,
}

pub(super) fn active(highlights: Query<(), With<Highlights>>) -> bool {
    !highlights.is_empty()
}

pub(crate) fn clear(world: &mut World, root: Entity) {
    if let Some(state) = world.entity_mut(root).take::<Highlights>() {
        for entity in state.outlines {
            world.despawn(entity);
        }
    }
}

pub(crate) fn update_entities(world: &mut World, root: Entity, targets: Vec<Entity>) {
    if world
        .get::<Highlights>(root)
        .is_some_and(|state| state.targets == targets)
    {
        return;
    }
    clear(world, root);
    let outlines = targets
        .iter()
        .map(|target| {
            world
                .spawn((
                    TutorialHighlight { target: *target },
                    Node {
                        position_type: PositionType::Absolute,
                        display: Display::None,
                        border: UiRect::all(px(3)),
                        border_radius: BorderRadius::all(px(6)),
                        ..default()
                    },
                    crate::token_style::border(crate::tokens::Token::Connections),
                    GlobalZIndex(96),
                    Pickable::IGNORE,
                    ChildOf(root),
                ))
                .id()
        })
        .collect();
    world.entity_mut(root).insert(Highlights {
        targets,
        outlines,
        reveal: true,
    });
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
}

fn bounds(world: &World, entity: Entity) -> Option<(Vec2, Vec2)> {
    if let Some(rect) = crate::topology::presentation::bounds(world, entity) {
        let scale = world.get_resource::<UiScale>().map_or(1.0, |scale| scale.0);
        return Some((rect.min / scale, rect.max / scale));
    }
    let rect = crate::inspection::bounds(world, entity).or_else(|| {
        let root = world.get::<ChildOf>(entity)?.parent();
        crate::canvas_selection::screen_bounds(world, root, entity)
    })?;
    (rect.size().min_element() > 0.0).then_some((rect.min, rect.max))
}

fn reveal(world: &mut World, entity: Entity) -> bool {
    let Some((start, end)) = bounds(world, entity) else {
        return true;
    };
    let mut parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
    while let Some(entity) = parent {
        parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
        if world.get::<ScrollPosition>(entity).is_none() {
            continue;
        }
        let Some((min, max)) = bounds(world, entity) else {
            continue;
        };
        let node = world.get::<ComputedNode>(entity).unwrap();
        let scale = node.inverse_scale_factor();
        let ratio = node.size() * scale / (max - min);
        let limit = (node.content_size() - node.size()).max(Vec2::ZERO) * scale;
        let delta = Vec2::new(
            if start.x < min.x {
                start.x - min.x - 6.0
            } else if end.x > max.x {
                end.x - max.x + 6.0
            } else {
                0.0
            },
            if start.y < min.y {
                start.y - min.y - 6.0
            } else if end.y > max.y {
                end.y - max.y + 6.0
            } else {
                0.0
            },
        );
        let mut scroll = world.get_mut::<ScrollPosition>(entity).unwrap();
        let next = (scroll.0 + delta * ratio).clamp(Vec2::ZERO, limit);
        if (next - scroll.0).length_squared() > 1.0 {
            scroll.0 = next;
            return true;
        }
    }
    false
}

pub(super) fn position(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<Highlights>>()
        .iter(world)
        .collect();
    for root in roots {
        let active = crate::instinct::practice::visible(world, root);
        if !active {
            clear(world, root);
            continue;
        }
        let state = world.get::<Highlights>(root).unwrap();
        let targets = state.targets.clone();
        let outlines = state.outlines.clone();
        let mut moving = false;
        if state.reveal {
            for target in &targets {
                moving |= reveal(world, *target);
            }
            world.get_mut::<Highlights>(root).unwrap().reveal = moving;
        }
        let Some((origin, root_end)) = bounds(world, root) else {
            continue;
        };
        for (target, outline) in targets.into_iter().zip(outlines) {
            let mut rect = bounds(world, target);
            let mut parent = world.get::<ChildOf>(target).map(ChildOf::parent);
            while let Some(entity) = parent {
                if world.get::<ScrollPosition>(entity).is_some()
                    && let (Some((start, end)), Some((min, max))) = (rect, bounds(world, entity))
                {
                    rect = Some((start.max(min), end.min(max)));
                }
                parent = world.get::<ChildOf>(entity).map(ChildOf::parent);
            }
            let node = rect
                .map(|(start, end)| (start.max(origin), end.min(root_end)))
                .filter(|(start, end)| (end - start).min_element() > 0.0);
            let Some(mut next) = world.get::<Node>(outline).cloned() else {
                continue;
            };
            if let Some((start, end)) = node {
                let position = start - origin;
                let size = end - start;
                next.display = Display::Flex;
                next.left = px(position.x - 3.0);
                next.top = px(position.y - 3.0);
                next.width = px(size.x + 6.0);
                next.height = px(size.y + 6.0);
            } else {
                next.display = Display::None;
            }
            if world.get::<Node>(outline) != Some(&next) {
                world.entity_mut(outline).insert(next);
                moving = true;
            }
        }
        if moving && let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
            wake.ring();
        }
    }
}
