use super::*;
use crate::native::topology::surface_budget::residency::Target;
use std::collections::BTreeMap;

pub(super) struct Saved {
    pub behavior: Behavior,
    pub players: Vec<(AnimationPlayer, AnimationTransitions)>,
}

pub(super) fn restore(
    player: &mut AnimationPlayer,
    saved: Option<(AnimationPlayer, AnimationTransitions)>,
    index: AnimationNodeIndex,
) -> AnimationTransitions {
    if let Some((saved, transitions)) = saved {
        *player = saved;
        transitions
    } else {
        let mut transitions = AnimationTransitions::new();
        transitions.play(player, index, Duration::ZERO).repeat();
        transitions
    }
}

pub(super) fn remember(world: &mut World, scene: &Scene) {
    let players = scene
        .players
        .iter()
        .filter_map(|entity| {
            let mut player = world.get::<AnimationPlayer>(*entity)?.clone();
            player.pause_all();
            let transitions = world
                .get::<AnimationTransitions>(*entity)
                .or_else(|| scene.paused.get(entity))
                .cloned()
                .unwrap_or_default();
            Some((player, transitions))
        })
        .collect();
    world.resource_mut::<Character>().saved.insert(
        scene.palette,
        Saved {
            behavior: scene.behavior.clone(),
            players,
        },
    );
}

pub(super) fn synchronize(
    world: &mut World,
    groups: &BTreeMap<Palette, Vec<Entity>>,
    showing: bool,
) {
    let plans: Vec<_> = world
        .resource::<Character>()
        .scenes
        .iter()
        .map(|scene| {
            let visible = showing
                && groups.get(&scene.palette).is_some_and(|entities| {
                    entities.iter().any(|entity| super::visible(world, *entity))
                });
            (
                scene.palette,
                scene.root,
                scene.camera,
                scene.image.clone(),
                visible,
            )
        })
        .collect();
    let edge = if plans.iter().any(|plan| plan.4) {
        density(world).clamp(128, 256)
    } else {
        1
    };
    for (palette, root, camera, image, visible) in plans {
        if let Some(mut visibility) = world.get_mut::<Visibility>(root) {
            visibility.set_if_neq(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        if let Some(mut target) = world.get_mut::<Target>(camera) {
            if target.release == visible {
                target.release = !visible;
            }
        }
        let desired = if visible { edge } else { 1 };
        let resized = world
            .resource::<Assets<Image>>()
            .get(&image)
            .is_some_and(|image| image.width() != desired || image.height() != desired);
        if resized {
            world
                .resource_mut::<Assets<Image>>()
                .get_mut(&image)
                .unwrap()
                .resize(Extent3d {
                    width: desired,
                    height: desired,
                    depth_or_array_layers: 1,
                });
        }
        let mut character = world.resource_mut::<Character>();
        let scene = character
            .scenes
            .iter_mut()
            .find(|scene| scene.palette == palette)
            .unwrap();
        scene.visible = visible;
        if resized {
            scene.rendered = None;
            character.alarm = None;
        }
    }
    let visible = if showing {
        groups
            .values()
            .filter(|entities| entities.iter().any(|entity| super::visible(world, *entity)))
            .count()
    } else {
        0
    };
    world.resource_mut::<Character>().limited_palettes =
        visible.saturating_sub(crate::native::render_layers::FIOTE.len);
}

#[cfg(test)]
mod tests;
