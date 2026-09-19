use super::guide::{Guide, Target};
use super::*;
use crate::{
    actions::ActionButton,
    edit_mode::{EditAction, EditControl},
    icons::{IconButton, Tooltip},
};
use bevy::a11y::AccessibilityNode;

#[derive(Component)]
pub struct TutorialHighlight {
    pub target: Entity,
}

#[derive(Component, Default)]
struct Highlights {
    targets: Vec<Entity>,
    outlines: Vec<Entity>,
    reveal: bool,
}

pub(super) fn under(world: &World, mut entity: Entity, parent: Entity) -> bool {
    loop {
        if entity == parent {
            return true;
        }
        let Some(next) = world.get::<ChildOf>(entity) else {
            return false;
        };
        entity = next.parent();
    }
}

fn visible(world: &World, mut entity: Entity) -> bool {
    loop {
        if world
            .get::<Node>(entity)
            .is_some_and(|node| node.display == Display::None)
            || world.get::<Visibility>(entity) == Some(&Visibility::Hidden)
        {
            return false;
        }
        let Some(next) = world.get::<ChildOf>(entity) else {
            return true;
        };
        entity = next.parent();
    }
}

fn button(world: &mut World, owner: Entity, title: &str) -> Option<Entity> {
    world
        .query::<(
            Entity,
            &ActionButton,
            Option<&Tooltip>,
            Option<&IconButton>,
            Option<&AccessibilityNode>,
        )>()
        .iter(world)
        .find(|(entity, button, tip, icon, node)| {
            button.target == owner
                && visible(world, *entity)
                && (tip.is_some_and(|tip| tip.0 == title)
                    || icon.is_some_and(|icon| icon.label == title)
                    || node.is_some_and(|node| node.label() == Some(title)))
        })
        .map(|(entity, ..)| entity)
}

pub(super) fn targets(world: &mut World, root: Entity, target: &Target) -> Vec<Entity> {
    let entity = match target {
        Target::Edit(action) => world
            .query::<(Entity, &EditControl)>()
            .iter(world)
            .find(|(entity, control)| {
                control.root == root
                    && (control.action == *action
                        || (*action == EditAction::Open && control.action == EditAction::Toggle))
                    && visible(world, *entity)
            })
            .map(|(entity, _)| entity),
        Target::Button(owner, title) => button(world, *owner, title),
        Target::Menu(owner, name, choice) => {
            let menus: Vec<_> = world
                .query::<(Entity, &crate::dropdown::Dropdown, &AccessibilityNode)>()
                .iter(world)
                .filter(|(_, _, node)| node.label() == Some(*name))
                .map(|(entity, dropdown, _)| (entity, dropdown.menu))
                .collect();
            menus.into_iter().find_map(|(toggle, menu)| {
                let option = world
                    .query::<(Entity, &ActionButton, &AccessibilityNode, &ChildOf)>()
                    .iter(world)
                    .find(|(_, button, node, parent)| {
                        button.target == *owner
                            && node.label() == Some(*choice)
                            && parent.parent() == menu
                    })
                    .map(|(entity, ..)| entity)?;
                visible(world, toggle).then(|| {
                    if visible(world, option) {
                        option
                    } else {
                        toggle
                    }
                })
            })
        }
        Target::Field(field) => world
            .query::<(Entity, &TutorialField)>()
            .iter(world)
            .find(|(entity, value)| *value == field && visible(world, *entity))
            .map(|(entity, _)| entity),
        Target::PanelField(title) => {
            let panel = world
                .get::<crate::edit_mode::EditMode>(root)
                .map(|mode| mode.panel);
            world
                .query::<(Entity, &AccessibilityNode)>()
                .iter(world)
                .find(|(entity, node)| {
                    node.label() == Some(*title)
                        && panel.is_some_and(|panel| under(world, *entity, panel))
                        && visible(world, *entity)
                })
                .map(|(entity, _)| entity)
        }
        Target::Control(title) => world
            .query::<(Entity, &AccessibilityNode)>()
            .iter(world)
            .find(|(entity, node)| {
                node.label() == Some(*title)
                    && under(world, *entity, root)
                    && visible(world, *entity)
            })
            .map(|(entity, _)| entity),
        Target::Canvas(entities) => {
            return entities
                .iter()
                .copied()
                .filter(|entity| world.get_entity(*entity).is_ok() && visible(world, *entity))
                .collect();
        }
        Target::None => None,
    };
    entity.into_iter().collect()
}

pub(super) fn clear(world: &mut World, root: Entity) {
    if let Some(state) = world.entity_mut(root).take::<Highlights>() {
        for entity in state.outlines {
            world.despawn(entity);
        }
    }
}

pub(super) fn reveal_current(world: &mut World, root: Entity) {
    if let Some(mut state) = world.get_mut::<Highlights>(root) {
        state.reveal = true;
    }
}

pub(super) fn update(world: &mut World, root: Entity, target: &Target) {
    let targets = targets(world, root, target);
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
        let active = world.get::<Session>(root).is_some_and(|session| {
            !session.hidden
                && world
                    .get::<Workspaces>(root)
                    .is_some_and(|spaces| spaces.active == session.workspace)
        });
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
            if let Some(current) = world.get::<Guide>(root).and_then(|guide| guide.current) {
                moving |= reveal(world, current);
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
