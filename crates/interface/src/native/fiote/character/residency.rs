use super::*;
use crate::native::topology::surface_budget::residency::Target;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Group {
    pub entities: Vec<Entity>,
    pub visible: bool,
}

/// Fresh presentation inputs shared by selection, residency and portraits.
/// Visibility and token ancestry are resolved once per avatar, not retained
/// across updates where either can change independently of the avatar itself.
pub(super) struct Demand {
    pub groups: BTreeMap<Palette, Group>,
    pub edge: u32,
}

impl Demand {
    pub fn capture(world: &mut World) -> Self {
        let _preparation = nucleus::performance::Span::new("fiote-avatar-preparation");
        let mut avatars = world.query_filtered::<(Entity, Option<&ComputedNode>), With<Avatar>>();
        let mut groups = BTreeMap::<Palette, Group>::new();
        let mut measured = 0.0_f32;
        for (entity, node) in avatars.iter(world) {
            nucleus::performance::add("fiote-avatar-inputs-prepared", 1);
            let visible = super::visible(world, entity);
            if visible && let Some(node) = node {
                measured = measured.max(node.size().max_element());
            }
            let group = groups.entry(Palette::resolve(world, entity)).or_default();
            group.entities.push(entity);
            group.visible |= visible;
        }
        let edge = if groups.is_empty() {
            64
        } else {
            density_at(world, measured)
        };
        Self { groups, edge }
    }
}

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

pub(super) fn synchronize(world: &mut World, demand: &Demand, showing: bool) {
    let _residency = nucleus::performance::Span::new("fiote-avatar-residency");
    let edge = demand.edge.clamp(128, 256);
    world.resource_scope(|world, mut character: Mut<Character>| {
        let mut resized_any = false;
        for scene in &mut character.scenes {
            let visible = showing
                && demand
                    .groups
                    .get(&scene.palette)
                    .is_some_and(|group| group.visible);
            if let Some(mut visibility) = world.get_mut::<Visibility>(scene.root) {
                visibility.set_if_neq(if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            if let Some(mut target) = world.get_mut::<Target>(scene.camera) {
                if target.release == visible {
                    target.release = !visible;
                }
            }
            let desired = if visible { edge } else { 1 };
            let resized = world
                .resource::<Assets<Image>>()
                .get(&scene.image)
                .is_some_and(|image| image.width() != desired || image.height() != desired);
            if resized {
                world
                    .resource_mut::<Assets<Image>>()
                    .get_mut(&scene.image)
                    .unwrap()
                    .resize(Extent3d {
                        width: desired,
                        height: desired,
                        depth_or_array_layers: 1,
                    });
                scene.rendered = None;
                resized_any = true;
            }
            scene.visible = visible;
        }
        if resized_any {
            character.alarm = None;
        }
        let visible = if showing {
            demand.groups.values().filter(|group| group.visible).count()
        } else {
            0
        };
        character.limited_palettes =
            visible.saturating_sub(crate::native::render_layers::FIOTE.len);
    });
}

#[cfg(test)]
mod tests;
