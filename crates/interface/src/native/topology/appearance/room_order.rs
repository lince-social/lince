use super::*;
use bevy::{
    core_pipeline::core_3d::{Transparent3d, TransparentSortingInfo3d},
    render::{
        Extract, ExtractSchedule, Render, RenderSystems,
        render_phase::{ViewSortedRenderPhases, sort_phase_system},
        view::ExtractedView,
    },
};

#[derive(Component, Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RoomLayer(Entity, u64, String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Layer<'a>(Entity, u64, &'a str);

impl RoomLayer {
    pub(super) fn as_ref(&self) -> Layer<'_> {
        Layer(self.0, self.1, &self.2)
    }
}

impl Layer<'_> {
    pub(super) fn into_owned(self) -> RoomLayer {
        RoomLayer(self.0, self.1, self.2.to_owned())
    }
}

#[derive(Component, Clone)]
struct Foreground(RoomLayer);

#[derive(Resource, Default)]
struct Layers {
    overlays: HashMap<Entity, usize>,
    clouds: HashMap<Entity, usize>,
    faces: HashMap<Entity, usize>,
    previous: HashMap<Entity, f32>,
    rooms: usize,
}

#[derive(Default)]
struct Nearest {
    values: Vec<Option<f32>>,
    touched: Vec<usize>,
}

impl Nearest {
    fn prepare(&mut self, rooms: usize) {
        self.clear();
        self.values.resize(rooms, None);
    }

    fn include(&mut self, room: usize, distance: f32) {
        match &mut self.values[room] {
            Some(nearest) => *nearest = nearest.max(distance),
            current @ None => {
                *current = Some(distance);
                self.touched.push(room);
            }
        }
    }

    fn clear(&mut self) {
        for room in self.touched.drain(..) {
            self.values[room] = None;
        }
    }
}

pub(super) fn foreground(world: &mut World, owner: Entity, face: Entity) {
    if let Some(layer) = layer(world, owner) {
        if world
            .get::<Foreground>(face)
            .is_none_or(|current| current.0.as_ref() != layer)
        {
            let layer = layer.into_owned();
            world.entity_mut(face).insert(Foreground(layer));
        }
    } else if world.get::<Foreground>(face).is_some() {
        world.entity_mut(face).remove::<Foreground>();
    }
}

pub(super) fn layer(world: &World, owner: Entity) -> Option<Layer<'_>> {
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
    Some(Layer(
        world.get::<ChildOf>(owner)?.parent(),
        world
            .get::<crate::native::workspace::WorkspaceMember>(owner)?
            .0,
        &member.room,
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
    let _extraction = nucleus::performance::Span::new("appearance-room-extraction");
    let Layers {
        overlays: overlay_layers,
        clouds: cloud_layers,
        faces: face_layers,
        previous,
        rooms: room_count,
    } = &mut *layers;
    previous.clear();
    previous.extend(
        overlay_layers
            .keys()
            .map(|entity| (*entity, 1.0))
            .chain(face_layers.keys().map(|entity| (*entity, 2.0))),
    );
    overlay_layers.clear();
    face_layers.clear();
    cloud_layers.clear();
    let mut rooms = HashMap::new();
    let mut intern = |layer| {
        let next = rooms.len();
        *rooms.entry(layer).or_insert(next)
    };
    overlay_layers.extend(
        overlays
            .iter()
            .map(|(entity, layer)| (entity, intern(layer.as_ref()))),
    );
    face_layers.extend(
        faces
            .iter()
            .map(|(entity, face)| (entity, intern(face.0.as_ref()))),
    );
    if !overlay_layers.is_empty() || !face_layers.is_empty() {
        for (scene, member, parent, workspace) in clouds
            .iter()
            .filter(|(scene, ..)| gaussians.get(scene.0).is_ok())
        {
            let layer = Layer(parent.parent(), workspace.0, &member.0.room);
            if let Some(&room) = rooms.get(&layer) {
                cloud_layers.insert(scene.0, room);
            } else {
                cloud_layers.remove(&scene.0);
            }
        }
    }
    *room_count = rooms.len();
    nucleus::performance::add("appearance-room-identities", *room_count as u64);
    nucleus::performance::add("appearance-room-clouds", cloud_layers.len() as u64);
    if overlay_layers.is_empty() && face_layers.is_empty() && previous.is_empty() {
        *layers = Layers::default();
    }
}

fn order(
    layers: Res<Layers>,
    views: Query<&ExtractedView>,
    mut phases: ResMut<ViewSortedRenderPhases<Transparent3d>>,
    mut nearest: Local<Nearest>,
) {
    let _ordering = nucleus::performance::Span::new("appearance-room-ordering");
    if layers.overlays.is_empty() && layers.faces.is_empty() && layers.previous.is_empty() {
        *nearest = Nearest::default();
        return;
    }
    if layers.clouds.is_empty() {
        *nearest = Nearest::default();
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
        if layers.clouds.is_empty() {
            continue;
        }
        nearest.prepare(layers.rooms);
        for item in phase.items.values() {
            if let Some(&room) = layers.clouds.get(&item.entity.1.id()) {
                let distance = item.sorting_info.sort_distance(&range);
                if distance.is_finite() {
                    nearest.include(room, distance);
                }
            }
        }
        if nearest.touched.is_empty() {
            continue;
        }
        nucleus::performance::add("appearance-room-cloud-views", 1);
        for item in phase.items.values_mut() {
            let entity = item.entity.1.id();
            let scoped = layers
                .overlays
                .get(&entity)
                .map(|&room| (room, 1.0))
                .or_else(|| layers.faces.get(&entity).map(|&room| (room, 2.0)));
            let Some((cloud, base)) =
                scoped.and_then(|(room, base)| nearest.values[room].map(|cloud| (cloud, base)))
            else {
                continue;
            };
            if let TransparentSortingInfo3d::Sorted {
                mesh_center,
                depth_bias,
            } = &mut item.sorting_info
            {
                let natural = range.distance(mesh_center);
                *depth_bias = bias(natural, cloud, base);
            }
        }
    }
    nearest.clear();
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
