use super::*;
use bevy::{
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    render::{
        ExtractSchedule, Render, RenderSystems,
        render_phase::{ViewSortedRenderPhases, sort_phase_system},
        view::ExtractedView,
    },
};

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RoomLayer(Entity, u64, String);

#[derive(Component, Clone)]
struct Foreground(RoomLayer);

#[derive(Resource, Default)]
struct Layers {
    overlays: HashMap<Entity, RoomLayer>,
    clouds: HashMap<Entity, RoomLayer>,
    faces: HashMap<Entity, RoomLayer>,
    previous: HashMap<Entity, f32>,
}

pub(super) fn foreground(world: &mut World, owner: Entity, face: Entity) {
    if let Some(layer) = layer(world, owner) {
        if world
            .get::<Foreground>(face)
            .is_none_or(|current| current.0 != layer)
        {
            world.entity_mut(face).insert(Foreground(layer));
        }
    } else if world.get::<Foreground>(face).is_some() {
        world.entity_mut(face).remove::<Foreground>();
    }
}

pub(super) fn layer(world: &World, owner: Entity) -> Option<RoomLayer> {
    if !world
        .get::<crate::native::shader_effects::Surface>(owner)?
        .room_ambient
    {
        return None;
    }
    let member = &world.get::<super::super::rooms::Member>(owner)?.0;
    if member.role != nucleus::canvas::room::Role::Floor {
        return None;
    }
    Some(RoomLayer(
        world.get::<ChildOf>(owner)?.parent(),
        world
            .get::<crate::native::workspace::WorkspaceMember>(owner)?
            .0,
        member.room.clone(),
    ))
}

pub(crate) fn install(app: &mut bevy::app::SubApp) {
    app.init_resource::<Layers>()
        .add_systems(ExtractSchedule, extract)
        .add_systems(
            Render,
            order
                .in_set(RenderSystems::PhaseSort)
                .before(sort_phase_system::<Transparent3d>),
        );
}

fn extract(
    mut layers: ResMut<Layers>,
    overlays: Extract<Query<(Entity, &RoomLayer)>>,
    faces: Extract<Query<(Entity, &Foreground)>>,
    clouds: Extract<
        Query<(
            &super::super::assets::ImportedScene,
            &super::super::rooms::Member,
            &ChildOf,
            &crate::native::workspace::WorkspaceMember,
        )>,
    >,
    gaussians: Extract<Query<(), With<bevy_gaussian_splatting::PlanarGaussian3dHandle>>>,
) {
    let previous = layers
        .overlays
        .keys()
        .map(|entity| (*entity, 1.0))
        .chain(layers.faces.keys().map(|entity| (*entity, 2.0)))
        .collect();
    layers.previous = previous;
    layers.overlays.clear();
    layers.faces.clear();
    layers.clouds.clear();
    layers.overlays.extend(
        overlays
            .iter()
            .map(|(entity, layer)| (entity, layer.clone())),
    );
    layers
        .faces
        .extend(faces.iter().map(|(entity, face)| (entity, face.0.clone())));
    layers.clouds.extend(
        clouds
            .iter()
            .filter(|(scene, ..)| gaussians.get(scene.0).is_ok())
            .map(|(scene, member, parent, workspace)| {
                (
                    scene.0,
                    RoomLayer(parent.parent(), workspace.0, member.0.room.clone()),
                )
            }),
    );
}

fn order(
    layers: Res<Layers>,
    views: Query<&ExtractedView>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
) {
    if layers.overlays.is_empty() && layers.faces.is_empty() && layers.previous.is_empty() {
        return;
    }
    for view in &views {
        let Some(phase) = phases.get_mut(&view.retained_view_entity) else {
            continue;
        };
        let range = view.rangefinder3d();
        for item in phase.items.values_mut() {
            let entity = item.entity.1.id();
            let base = if layers.overlays.contains_key(&entity) {
                Some(1.0)
            } else if layers.faces.contains_key(&entity) {
                Some(2.0)
            } else {
                layers.previous.get(&entity).copied()
            };
            if let Some(base) = base
                && let TransparentSortingInfo3d::Sorted { depth_bias, .. } = &mut item.sorting_info
            {
                *depth_bias = base;
            }
        }
        let mut clouds = HashMap::<&RoomLayer, f32>::new();
        for item in phase.items.values() {
            if let Some(layer) = layers.clouds.get(&item.entity.1.id()) {
                let distance = item.sorting_info.sort_distance(&range);
                if distance.is_finite() {
                    clouds
                        .entry(layer)
                        .and_modify(|nearest| *nearest = nearest.max(distance))
                        .or_insert(distance);
                }
            }
        }
        for item in phase.items.values_mut() {
            let entity = item.entity.1.id();
            let scoped = layers
                .overlays
                .get(&entity)
                .map(|layer| (layer, 1.0))
                .or_else(|| layers.faces.get(&entity).map(|layer| (layer, 2.0)));
            let Some((cloud, base)) =
                scoped.and_then(|(layer, base)| clouds.get(layer).map(|cloud| (cloud, base)))
            else {
                continue;
            };
            if let TransparentSortingInfo3d::Sorted {
                mesh_center,
                depth_bias,
            } = &mut item.sorting_info
            {
                let natural = range.distance(mesh_center);
                *depth_bias = bias(natural, *cloud, base);
            }
        }
    }
}

fn bias(natural: f32, cloud: f32, base: f32) -> f32 {
    let gap = 1.0_f32.max(cloud.abs() * f32::EPSILON * 8.0);
    (natural + base).max(cloud + gap * base) - natural
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_floor_overlay_sorts_after_the_scan_and_before_protected_text_in_view_space() {
        use bevy::{math::Affine3A, render::render_phase::ViewRangefinder3d};
        let range = ViewRangefinder3d::from_world_from_view(&Affine3A::IDENTITY);
        for (floor, cloud) in [(-150.0, -100.0), (-80.0, -100.0), (-1e8, -1e8 + 100.0)] {
            let overlay = TransparentSortingInfo3d::Sorted {
                mesh_center: Vec3::new(0.0, 0.0, floor),
                depth_bias: bias(floor, cloud, 1.0),
            }
            .sort_distance(&range);
            let text = TransparentSortingInfo3d::Sorted {
                mesh_center: Vec3::new(0.0, 0.0, floor),
                depth_bias: bias(floor, cloud, 2.0),
            }
            .sort_distance(&range);
            assert!(overlay > cloud && text > overlay);
            assert!(overlay >= floor + 1.0);
        }
    }

    #[test]
    fn room_floor_compositing_is_opt_in_and_scoped_to_the_room_root_and_workspace() {
        use crate::native::{
            shader_effects::Surface, topology::rooms::Member, workspace::WorkspaceMember,
        };
        use nucleus::canvas::room;
        let mut world = World::new();
        let first = world.spawn_empty().id();
        let second = world.spawn_empty().id();
        let member = Member(room::Member {
            room: nucleus::new_uid("placement"),
            parent: None,
            local: room::identity_pose(),
            role: room::Role::Floor,
        });
        let floor = world
            .spawn((
                Surface::cleaning(),
                member.clone(),
                WorkspaceMember(1),
                ChildOf(first),
            ))
            .id();
        let other_room = world
            .spawn((
                Surface::cleaning(),
                Member(room::Member {
                    room: nucleus::new_uid("placement"),
                    ..member.0.clone()
                }),
                WorkspaceMember(1),
                ChildOf(first),
            ))
            .id();
        let other_workspace = world
            .spawn((
                Surface::cleaning(),
                member.clone(),
                WorkspaceMember(2),
                ChildOf(first),
            ))
            .id();
        let other_root = world
            .spawn((
                Surface::cleaning(),
                member,
                WorkspaceMember(1),
                ChildOf(second),
            ))
            .id();
        let expected = layer(&world, floor).unwrap();
        for other in [other_room, other_workspace, other_root] {
            assert_ne!(layer(&world, other).unwrap(), expected);
        }
        world.get_mut::<Surface>(floor).unwrap().room_ambient = false;
        assert!(layer(&world, floor).is_none());
        world.get_mut::<Surface>(floor).unwrap().room_ambient = true;
        world.get_mut::<Member>(floor).unwrap().0.role = room::Role::Content;
        assert!(layer(&world, floor).is_none());
    }
}
