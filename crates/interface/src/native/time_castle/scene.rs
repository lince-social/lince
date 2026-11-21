// non-lean-yet
// Scope: struct SchedulePick, struct Scene, struct Connectors.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 123, 168, 176,
//   195, 234, 314, 375, 489, 569, 691.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   314.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (167, 176, 204, 216, 227, 357, 401, 427, 458, 489, 506, 529, 568, 569, 691).

use super::*;
use crate::native::actions::Action;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
};

#[derive(Component)]
pub struct SchedulePick {
    owner: Entity,
    ids: Vec<String>,
}

#[derive(Component)]
struct Scene {
    material: Handle<StandardMaterial>,
    meshes: Vec<Entity>,
}

#[derive(Component)]
struct Connectors(Entity);

#[derive(Component, PartialEq)]
struct ConnectorStamp(Vec<([f32; 3], [f32; 4], [f32; 4])>);

#[derive(Component)]
struct Hand(Entity);

#[derive(Default)]
struct Ribbon {
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Ribbon {
    fn quad(&mut self, points: [Vec3; 4], color: [f32; 4]) {
        let color = Color::srgba(color[0], color[1], color[2], color[3])
            .to_linear()
            .to_vec4()
            .to_array();
        let offset = self.positions.len() as u32;
        self.positions.extend(points.map(|point| point.to_array()));
        self.colors.extend([color; 4]);
        self.indices.extend([
            offset,
            offset + 1,
            offset + 2,
            offset,
            offset + 2,
            offset + 3,
        ]);
    }

    fn mesh(self) -> Mesh {
        let normals = vec![[0.0, 1.0, 0.0]; self.positions.len()];
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

fn clicked(mut event: On<PointerClick>, picks: Query<&SchedulePick>, mut commands: Commands) {
    if event.button != PointerButton::Primary {
        return;
    }
    let Ok(pick) = picks.get(event.entity) else {
        return;
    };
    let (owner, ids) = (pick.owner, pick.ids.clone());
    commands.queue(move |world: &mut World| ui::Select(ids).apply(world, owner));
    event.propagate(false);
}

fn spawn(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    ribbon: Ribbon,
    material: Handle<StandardMaterial>,
    ids: Option<Vec<String>>,
) -> Option<Entity> {
    if ribbon.positions.is_empty() {
        return None;
    }
    let mesh = world.resource_mut::<Assets<Mesh>>().add(ribbon.mesh());
    let entity = world
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::Inherited,
            crate::native::topology::presentation::VisualOwner(owner),
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .observe(clicked)
        .id();
    if let Some(ids) = ids {
        world
            .entity_mut(entity)
            .remove::<Pickable>()
            .insert(SchedulePick { owner, ids });
    }
    Some(entity)
}

fn replace(
    world: &mut World,
    parent: Entity,
    owner: Entity,
    ribbons: Vec<(Ribbon, Option<Vec<String>>)>,
) {
    let scene = world.get::<Scene>(parent).unwrap();
    let (material, previous) = (scene.material.clone(), scene.meshes.clone());
    let mut meshes = Vec::with_capacity(ribbons.len());
    for (index, (ribbon, ids)) in ribbons.into_iter().enumerate() {
        if let Some(entity) = previous
            .get(index)
            .copied()
            .filter(|entity| world.get_entity(*entity).is_ok())
        {
            let handle = world.get::<Mesh3d>(entity).unwrap().0.clone();
            let _ = world
                .resource_mut::<Assets<Mesh>>()
                .insert(handle.id(), ribbon.mesh());
            world
                .entity_mut(entity)
                .remove::<bevy::camera::primitives::Aabb>();
            if let Some(ids) = ids {
                world
                    .entity_mut(entity)
                    .remove::<Pickable>()
                    .insert(SchedulePick { owner, ids });
            } else {
                world
                    .entity_mut(entity)
                    .remove::<SchedulePick>()
                    .insert(Pickable::IGNORE);
            }
            meshes.push(entity);
        } else if let Some(entity) = spawn(world, parent, owner, ribbon, material.clone(), ids) {
            meshes.push(entity);
        }
    }
    for entity in previous.into_iter().skip(meshes.len()) {
        if world.get_entity(entity).is_ok() {
            world.despawn(entity);
        }
    }
    world.get_mut::<Scene>(parent).unwrap().meshes = meshes;
}

pub(super) fn update(
    world: &mut World,
    owner: Entity,
    settings: &Settings,
    now: i64,
    size: Vec2,
    duration: i64,
) {
    if !crate::native::topology::presentation::ready(world) {
        if let Some(previous) = world.get_mut::<View>(owner).unwrap().scene.take()
            && world.get_entity(previous).is_ok()
        {
            world.despawn(previous);
        }
        return;
    }
    let Some(surface) = world.get::<crate::native::topology::presentation::Surface>(owner) else {
        return;
    };
    let (visual, density) = (surface.visual, surface.density);
    let view = world.get::<View>(owner).unwrap();
    let _prepare = nucleus::performance::Span::new("time-scene-preparation");
    let (unwind, selected) = (view.unwind, &view.selected);
    let entries = &view.entries;
    let curve = settings.geometry(now, size.to_array(), unwind);
    let palette = view
        .palette
        .clone()
        .unwrap_or_else(|| palette::Palette::resolve(world, owner));
    let previous_scene = view
        .scene
        .filter(|entity| world.get_entity(*entity).is_ok());
    let mut ribbons = Vec::new();
    let settled;
    let mut bands: Vec<&motion::Band> = if let Some(motion) = world.get::<motion::Motion>(owner) {
        motion.bands.values().collect()
    } else {
        settled = model::clock_occurrences(settings, entries, now, now + duration)
            .into_iter()
            .map(|occurrence| motion::Band::settled(entries[occurrence.index].clone(), occurrence))
            .collect::<Vec<_>>();
        settled.iter().collect()
    };
    bands.sort_by(|a, b| {
        a.occurrence
            .time
            .from_ms
            .cmp(&b.occurrence.time.from_ms)
            .then_with(|| a.entry.id.cmp(&b.entry.id))
    });
    for band in bands {
        let occurrence = &band.occurrence;
        let entry = &band.entry;
        let color = palette::rgba(
            palette
                .event(occurrence.index, selected.contains(&entry.id))
                .with_alpha(band.opacity()),
        );
        let mut ribbon = Ribbon::default();
        let mut points = motion::points_in(band, &curve, settings, now, size, palette.width);
        for point in &mut points {
            point.y = 0.0;
        }
        let width = palette.width;
        if points.len() == 1 {
            let point = points[0] + Vec3::Y * 0.2;
            let cross = Vec3::from_array(curve.transverse(occurrence.time.from_ms)) * 3.0;
            let tangent = Vec3::new(-cross.z, 0.0, cross.x).normalize_or_zero()
                * density.recip().clamp(0.4, 3.0)
                * 0.5;
            ribbon.quad(
                [
                    point - cross - tangent,
                    point + cross - tangent,
                    point + cross + tangent,
                    point - cross + tangent,
                ],
                color,
            );
        } else {
            let end = occurrence.time.until_ms.unwrap();
            let cross = |index| {
                Vec3::from_array(curve.transverse(
                    occurrence.time.from_ms
                        + ((end - occurrence.time.from_ms) as f64 * index as f64
                            / (points.len() - 1) as f64) as i64,
                )) * width
                    * 0.5
            };
            let mut a_cross = cross(0);
            for (index, pair) in points.windows(2).enumerate() {
                let b_cross = cross(index + 1);
                let a = pair[0] + Vec3::Y * 0.2;
                let b = pair[1] + Vec3::Y * 0.2;
                ribbon.quad([a - a_cross, a + a_cross, b + b_cross, b - b_cross], color);
                a_cross = b_cross;
            }
        }
        ribbons.push((ribbon, (!band.retiring).then(|| vec![entry.id.clone()])));
    }
    let parent = if let Some(parent) = previous_scene {
        parent
    } else {
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                cull_mode: None,
                alpha_mode: AlphaMode::Blend,
                ..default()
            });
        let parent = world
            .spawn((
                Transform::default(),
                Visibility::Inherited,
                ChildOf(visual),
                crate::native::topology::presentation::VisualOwner(owner),
                Scene {
                    material,
                    meshes: Vec::new(),
                },
            ))
            .id();
        world.get_mut::<View>(owner).unwrap().scene = Some(parent);
        parent
    };
    replace(world, parent, owner, ribbons);
}

pub(super) fn annotations(
    world: &mut World,
    owner: Entity,
    labels: &[model::Label],
    palette: &palette::Palette,
) {
    let stamp = ConnectorStamp(
        labels
            .iter()
            .map(|label| {
                let opacity = world
                    .get::<motion::Motion>(owner)
                    .and_then(|motion| motion.bands.get(&label.id))
                    .map_or(1.0, motion::Band::opacity);
                let color = palette::rgba(
                    palette
                        .event(
                            label.occurrence.index,
                            world
                                .get::<View>(owner)
                                .unwrap()
                                .selected
                                .contains(&label.id),
                        )
                        .with_alpha(0.45 * opacity),
                );
                (label.anchor, label.rect, color)
            })
            .collect(),
    );
    if world.get::<ConnectorStamp>(owner) == Some(&stamp) {
        return;
    }
    world.entity_mut(owner).insert(stamp);
    if labels.is_empty() {
        if let Some(previous) = world
            .get::<Connectors>(owner)
            .map(|connectors| connectors.0)
        {
            world.despawn(previous);
            world.entity_mut(owner).remove::<Connectors>();
        }
        return;
    }
    let Some(visual) = world
        .get::<crate::native::topology::presentation::Surface>(owner)
        .map(|surface| surface.visual)
    else {
        return;
    };
    let mut ribbon = Ribbon::default();
    let selected = &world.get::<View>(owner).unwrap().selected;
    for label in labels {
        let anchor = Vec3::from_array(label.anchor) + Vec3::Y * 0.4;
        let direction = anchor.xz().normalize_or_zero();
        let elbow = anchor + Vec3::new(direction.x, 0.0, direction.y) * 24.0;
        let edge = card_edge(anchor.xz(), label.rect);
        let end = Vec3::new(edge.x, anchor.y, edge.y);
        let color = palette::rgba(
            palette
                .event(label.occurrence.index, selected.contains(&label.id))
                .with_alpha(
                    0.45 * world
                        .get::<motion::Motion>(owner)
                        .and_then(|motion| motion.bands.get(&label.id))
                        .map_or(1.0, motion::Band::opacity),
                ),
        );
        let curve =
            |t: f32| anchor * (1.0 - t).powi(2) + elbow * (2.0 * t * (1.0 - t)) + end * t * t;
        for index in 0..12 {
            let (a, b) = (curve(index as f32 / 12.0), curve((index + 1) as f32 / 12.0));
            let delta = (b - a).normalize_or_zero();
            let cross = Vec3::new(-delta.z, 0.0, delta.x) * 0.4;
            ribbon.quad([a - cross, a + cross, b + cross, b - cross], color);
        }
    }
    if let Some(entity) = world
        .get::<Connectors>(owner)
        .map(|connectors| connectors.0)
        .filter(|entity| world.get_entity(*entity).is_ok())
    {
        let handle = world.get::<Mesh3d>(entity).unwrap().0.clone();
        let _ = world
            .resource_mut::<Assets<Mesh>>()
            .insert(handle.id(), ribbon.mesh());
        world
            .entity_mut(entity)
            .remove::<bevy::camera::primitives::Aabb>();
    } else {
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                ..default()
            });
        let entity = spawn(world, visual, owner, ribbon, material, None).unwrap();
        world.entity_mut(owner).insert(Connectors(entity));
    }
}

fn card_edge(anchor: Vec2, rect: [f32; 4]) -> Vec2 {
    let bounds = Rect::from_corners(
        Vec2::new(rect[0], rect[1]),
        Vec2::new(rect[0] + rect[2], rect[1] + rect[3]),
    );
    let nearest = anchor.clamp(bounds.min, bounds.max);
    if bounds.contains(anchor) {
        [
            Vec2::new(bounds.min.x, anchor.y),
            Vec2::new(bounds.max.x, anchor.y),
            Vec2::new(anchor.x, bounds.min.y),
            Vec2::new(anchor.x, bounds.max.y),
        ]
        .into_iter()
        .min_by(|a, b| {
            a.distance_squared(anchor)
                .total_cmp(&b.distance_squared(anchor))
        })
        .unwrap()
    } else {
        nearest
    }
}

pub(super) fn hand(
    world: &mut World,
    owner: Entity,
    settings: &Settings,
    now: i64,
    size: Vec2,
    visible: bool,
    palette: &palette::Palette,
) {
    if !visible {
        if let Some(hand) = world.get::<Hand>(owner).map(|hand| hand.0) {
            world.despawn(hand);
            world.entity_mut(owner).remove::<Hand>();
        }
        return;
    }
    let Some(visual) = world
        .get::<crate::native::topology::presentation::Surface>(owner)
        .map(|surface| surface.visual)
    else {
        return;
    };
    let ring = face::Ring::new(settings, now, size);
    let direction = Vec3::from_array(settings.transverse(now, now, 0.0));
    let mut ribbon = Ribbon::default();
    let cross = Vec3::new(-direction.z, 0.0, direction.x) * 0.6;
    let a = direction * (ring.inner + 0.5) + Vec3::Y * 0.3;
    let b = direction * (ring.outer - 0.5) + Vec3::Y * 0.3;
    ribbon.quad(
        [a - cross, a + cross, b + cross, b - cross],
        palette::rgba(palette.present),
    );
    if let Some(entity) = world.get::<Hand>(owner).map(|hand| hand.0) {
        world.get_mut::<Transform>(entity).unwrap().translation.y = 0.5;
        let handle = world.get::<Mesh3d>(entity).unwrap().0.clone();
        let _ = world
            .resource_mut::<Assets<Mesh>>()
            .insert(handle.id(), ribbon.mesh());
        world
            .entity_mut(entity)
            .remove::<bevy::camera::primitives::Aabb>();
    } else {
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial {
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                cull_mode: None,
                depth_bias: 3.0,
                ..default()
            });
        if let Some(entity) = spawn(world, visual, owner, ribbon, material, None) {
            world
                .entity_mut(entity)
                .insert(Transform::from_xyz(0.0, 0.5, 0.0));
            world.entity_mut(owner).insert(Hand(entity));
        }
    }
}

pub(super) fn placement(world: &mut World, owner: Entity, viewport: Entity) {
    let Some(surface) = world.get::<crate::native::topology::presentation::Surface>(owner) else {
        return;
    };
    if world.get::<ComputedNode>(viewport).is_none() {
        return;
    }
    let Some(transform) = world.get::<UiGlobalTransform>(viewport) else {
        return;
    };
    let center = transform.translation / surface.density - surface.size * 0.5;
    if let Some(scene) = world.get::<View>(owner).and_then(|view| view.scene)
        && let Some(mut transform) = world.get_mut::<Transform>(scene)
    {
        transform.translation = Vec3::new(center.x, 0.0, center.y);
    }
    if let Some(entity) = world
        .get::<Connectors>(owner)
        .map(|connectors| connectors.0)
        && let Some(mut transform) = world.get_mut::<Transform>(entity)
    {
        let translation = Vec3::new(center.x, 0.0, center.y);
        if transform.translation != translation {
            transform.translation = translation;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_mode_keeps_the_same_clock_face_and_hand() {
        use crate::native::{canvas, topology, workspace};

        for mode in [Mode::Coiled, Mode::Straight] {
            let mut world = World::new();
            world.insert_resource(crate::native::theme::Typography(Handle::default()));
            world.init_resource::<bevy::input_focus::InputFocus>();
            world.init_resource::<Assets<Image>>();
            world.init_resource::<Assets<Mesh>>();
            world.init_resource::<Assets<StandardMaterial>>();
            world.spawn(Window {
                resolution: (1920, 1080).into(),
                ..default()
            });
            let root = world
                .spawn((
                    workspace::Workspaces::default(),
                    canvas::CanvasView::default(),
                    topology::view::View::default(),
                ))
                .id();
            let owner = world
                .spawn((
                    Node::default(),
                    canvas::CanvasItem {
                        position: bevy::math::DVec2::ZERO,
                        size: Vec2::splat(420.0),
                    },
                    workspace::WorkspaceMember(1),
                    ChildOf(root),
                    TimeSettings(Settings { mode, ..default() }),
                ))
                .id();
            populate(&mut world, owner);
            let viewport = world.get::<View>(owner).unwrap().viewport;
            let mut node = world.get_mut::<ComputedNode>(viewport).unwrap();
            node.size = Vec2::splat(420.0);
            node.inverse_scale_factor = 1.0;

            topology::presentation::synchronize(&mut world);
            world.run_system_cached(render::update).unwrap();
            let image = world.get::<ImageNode>(viewport).unwrap().image.id();
            let hand = |world: &World| world.get::<Hand>(owner).map(|hand| hand.0);
            let initial_hand = hand(&world);
            assert_eq!(initial_hand.is_some(), mode == Mode::Coiled);
            for spatial in [true, false] {
                world.get_mut::<topology::view::View>(root).unwrap().spatial = spatial;
                topology::presentation::synchronize(&mut world);
                world.run_system_cached(render::update).unwrap();

                let surface = world.get::<topology::presentation::Surface>(owner).unwrap();
                assert!(surface.visible);
                assert_eq!(world.get::<ImageNode>(viewport).unwrap().image.id(), image);
                assert_eq!(hand(&world), initial_hand);
                let scene = world.get::<View>(owner).unwrap().scene.unwrap();
                assert!(world.get::<Scene>(scene).unwrap().meshes.is_empty());
                assert_eq!(
                    world.get::<Visibility>(surface.body),
                    Some(&Visibility::Hidden)
                );
            }
        }
    }

    #[test]
    fn strings_end_at_the_nearest_card_edge_even_during_entry_motion() {
        let card = [10.0, 20.0, 100.0, 50.0];
        assert_eq!(card_edge(Vec2::new(0.0, 40.0), card), Vec2::new(10.0, 40.0));
        assert_eq!(
            card_edge(Vec2::new(50.0, 10.0), card),
            Vec2::new(50.0, 20.0)
        );
        assert_eq!(
            card_edge(Vec2::new(12.0, 40.0), card),
            Vec2::new(10.0, 40.0)
        );
    }

    #[test]
    fn schedule_updates_keep_render_entities_and_asset_handles() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<StandardMaterial>>();
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let owner = world.spawn_empty().id();
        let parent = world
            .spawn((
                Transform::default(),
                Visibility::Inherited,
                Scene {
                    material,
                    meshes: Vec::new(),
                },
            ))
            .id();
        let ribbon = |offset: f32| {
            let mut ribbon = Ribbon::default();
            ribbon.quad(
                [
                    Vec3::new(offset, 0.0, 0.0),
                    Vec3::new(offset + 1.0, 0.0, 0.0),
                    Vec3::new(offset + 1.0, 0.0, 1.0),
                    Vec3::new(offset, 0.0, 1.0),
                ],
                [1.0; 4],
            );
            ribbon
        };
        replace(
            &mut world,
            parent,
            owner,
            vec![
                (ribbon(0.0), None),
                (ribbon(1.0), Some(vec!["first".into()])),
            ],
        );
        let previous = world.get::<Scene>(parent).unwrap().meshes.clone();
        let handle = world.get::<Mesh3d>(previous[1]).unwrap().0.id();
        replace(
            &mut world,
            parent,
            owner,
            vec![
                (ribbon(2.0), None),
                (ribbon(3.0), Some(vec!["second".into()])),
            ],
        );
        assert_eq!(world.get::<Scene>(parent).unwrap().meshes, previous);
        assert_eq!(world.get::<Mesh3d>(previous[1]).unwrap().0.id(), handle);
        assert_eq!(
            world.get::<SchedulePick>(previous[1]).unwrap().ids,
            ["second"]
        );
        let mesh = world.resource::<Assets<Mesh>>().get(handle).unwrap();
        let bevy::mesh::VertexAttributeValues::Float32x3(points) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!()
        };
        assert_eq!(points[0], [3.0, 0.0, 0.0]);
        replace(&mut world, parent, owner, vec![(ribbon(4.0), None)]);
        assert!(world.get_entity(previous[1]).is_err());
    }
}
