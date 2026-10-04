use super::*;
use crate::actions::Action;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
};

#[derive(Component)]
pub struct SchedulePick {
    owner: Entity,
    ids: Vec<String>,
}

#[derive(Component, PartialEq)]
struct FaceCut {
    uv: Rect,
    regions: Vec<Rect>,
}

#[derive(Component)]
struct Scene {
    material: Handle<StandardMaterial>,
    meshes: Vec<Entity>,
}

#[derive(Component)]
struct Connectors(Entity);

#[derive(Component, PartialEq)]
struct ConnectorStamp(Vec<model::Label>, Vec<String>, palette::Palette);

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

    fn strip(
        &mut self,
        settings: &Settings,
        range: &nucleus::schedule::TimeRange,
        now: i64,
        size: Vec2,
        unwind: f32,
        width: f32,
        color: [f32; 4],
        budget: usize,
    ) {
        let points = render::samples(settings, range, now, size, unwind, budget);
        let end = range.until_ms.unwrap_or(range.from_ms);
        if points.len() == 1 {
            let point = points[0] + Vec3::Y * 0.05;
            let cross =
                Vec3::from_array(settings.transverse(range.from_ms.max(now), now, unwind)) * 10.0;
            let tangent = Vec3::new(-cross.z, 0.0, cross.x).normalize_or_zero() * width * 0.5;
            self.quad(
                [
                    point - cross - tangent,
                    point + cross - tangent,
                    point + cross + tangent,
                    point - cross + tangent,
                ],
                color,
            );
        } else {
            for (index, pair) in points.windows(2).enumerate() {
                let at = |index: usize| {
                    range.from_ms.max(now)
                        + ((end - range.from_ms.max(now)) as f64 * index as f64
                            / (points.len() - 1) as f64)
                            .round() as i64
                };
                let a_cross =
                    Vec3::from_array(settings.transverse(at(index), now, unwind)) * width * 0.5;
                let b_cross =
                    Vec3::from_array(settings.transverse(at(index + 1), now, unwind)) * width * 0.5;
                let a = pair[0] + Vec3::Y * 0.05;
                let b = pair[1] + Vec3::Y * 0.05;
                self.quad([a - a_cross, a + a_cross, b + b_cross, b - b_cross], color);
            }
        }
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

fn clicked(mut event: On<Pointer<Click>>, picks: Query<&SchedulePick>, mut commands: Commands) {
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
            crate::topology::presentation::VisualOwner(owner),
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
    spatial: bool,
) {
    if !crate::topology::presentation::ready(world) {
        if let Some(previous) = world.get_mut::<View>(owner).unwrap().scene.take()
            && world.get_entity(previous).is_ok()
        {
            world.despawn(previous);
        }
        return;
    }
    let Some(surface) = world.get::<crate::topology::presentation::Surface>(owner) else {
        return;
    };
    let (visual, density) = (surface.visual, surface.density);
    let view = world.get::<View>(owner).unwrap();
    let (unwind, selected) = (view.unwind, view.selected.clone());
    let entries = view.entries.clone();
    let palette = view
        .palette
        .clone()
        .unwrap_or_else(|| palette::Palette::resolve(world, owner));
    let duration = if spatial {
        settings.horizon_ms
    } else {
        (settings.aperture_ms as f64
            + (settings.horizon_ms - settings.aperture_ms) as f64 * f64::from(unwind))
            as i64
    };
    let occurrences = model::occurrences(&entries, now, now + duration, &settings.timezone);
    let parent = if let Some(parent) = view
        .scene
        .filter(|entity| world.get_entity(*entity).is_ok())
    {
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
                crate::topology::presentation::VisualOwner(owner),
                Scene {
                    material,
                    meshes: Vec::new(),
                },
            ))
            .id();
        world.get_mut::<View>(owner).unwrap().scene = Some(parent);
        parent
    };
    let mut background = Ribbon::default();
    if spatial {
        background.strip(
            settings,
            &nucleus::schedule::TimeRange {
                from_ms: now,
                until_ms: Some(now + settings.horizon_ms),
            },
            now,
            size,
            unwind,
            10.0,
            palette::rgba(palette.track),
            4096,
        );
    }
    let circular = size.min_element()
        * std::f32::consts::TAU
        * 0.4
        * (settings.horizon_ms as f64 / settings.aperture_ms as f64) as f32;
    let pixels = (circular * (1.0 - unwind) + size.x * unwind) * density;
    let interval = model::tick_interval(settings.horizon_ms, pixels, 25.0)
        .max(model::tick_interval(settings.horizon_ms, 256.0, 1.0));
    let major_interval = model::tick_interval(settings.horizon_ms, pixels, 85.0).max(interval);
    let mut at = now.div_euclid(interval) * interval + interval;
    while spatial && at < now + settings.horizon_ms {
        background.strip(
            settings,
            &nucleus::schedule::TimeRange {
                from_ms: at,
                until_ms: None,
            },
            now,
            size,
            unwind,
            density.recip().clamp(0.2, 3.0),
            if at.rem_euclid(major_interval) == 0 {
                palette::rgba(palette.muted)
            } else {
                palette::rgba(palette.track)
            },
            1,
        );
        at += interval;
    }
    if spatial {
        let point =
            Vec3::from_array(settings.position(now, now, size.to_array(), unwind)) + Vec3::Y * 0.2;
        let cross = Vec3::from_array(settings.transverse(now, now, unwind));
        let inner = point - cross * 18.0;
        let end = point + cross * 15.0;
        let tangent = Vec3::new(-cross.z, 0.0, cross.x) * density.recip().clamp(0.5, 3.0);
        background.quad(
            [
                inner - tangent,
                end - tangent,
                end + tangent,
                inner + tangent,
            ],
            palette::rgba(palette.present),
        );
    }
    let mut ribbons = if background.positions.is_empty() {
        Vec::new()
    } else {
        vec![(background, None)]
    };
    let mut bands: Vec<_> = world
        .get::<motion::Motion>(owner)
        .map(|motion| motion.bands.values().cloned().collect())
        .unwrap_or_else(|| {
            occurrences
                .into_iter()
                .map(|occurrence| {
                    motion::Band::settled(entries[occurrence.index].clone(), occurrence)
                })
                .collect()
        });
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
                .event(occurrence.lane, selected.contains(&entry.id))
                .with_alpha(band.opacity()),
        );
        let mut ribbon = Ribbon::default();
        let mut points = motion::points(&band, settings, now, size, unwind, palette.width);
        if !spatial {
            for point in &mut points {
                point.y = 0.0;
            }
        }
        let width = palette.width;
        if points.len() == 1 {
            let point = points[0] + Vec3::Y * 0.2;
            let cross =
                Vec3::from_array(settings.transverse(occurrence.time.from_ms, now, unwind)) * 3.0;
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
            for (index, pair) in points.windows(2).enumerate() {
                let cross = |index| {
                    Vec3::from_array(settings.transverse(
                        occurrence.time.from_ms
                            + ((end - occurrence.time.from_ms) as f64 * index as f64
                                / (points.len() - 1) as f64) as i64,
                        now,
                        unwind,
                    )) * width
                        * 0.5
                };
                let a = pair[0] + Vec3::Y * 0.2;
                let b = pair[1] + Vec3::Y * 0.2;
                ribbon.quad(
                    [
                        a - cross(index),
                        a + cross(index),
                        b + cross(index + 1),
                        b - cross(index + 1),
                    ],
                    color,
                );
            }
        }
        ribbons.push((ribbon, (!band.retiring).then(|| vec![entry.id.clone()])));
    }
    replace(world, parent, owner, ribbons);
}

pub(super) fn annotations(
    world: &mut World,
    owner: Entity,
    labels: &[model::Label],
    palette: &palette::Palette,
) {
    let stamp = ConnectorStamp(
        labels.to_vec(),
        world.get::<View>(owner).unwrap().selected.clone(),
        palette.clone(),
    );
    if world.get::<ConnectorStamp>(owner) == Some(&stamp)
        && !world
            .get::<motion::Motion>(owner)
            .is_some_and(|motion| motion.active)
    {
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
        .get::<crate::topology::presentation::Surface>(owner)
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
                .event(label.occurrence.lane, selected.contains(&label.id))
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
        .get::<crate::topology::presentation::Surface>(owner)
        .map(|surface| surface.visual)
    else {
        return;
    };
    let radius = size.min_element() * 0.4;
    let direction = Vec3::from_array(settings.transverse(now, now, 0.0));
    let regions = [
        Rect::from_corners(
            Vec2::new(-size.x * 0.22, -size.y * 0.14),
            Vec2::new(size.x * 0.22, size.y * 0.20),
        ),
        Rect::from_corners(
            Vec2::new(-radius * 0.50, -radius * 0.77),
            Vec2::new(radius * 0.50, -radius * 0.35),
        ),
        Rect::from_corners(
            Vec2::new(-20.0, size.y * 0.25 - 20.0),
            Vec2::new(20.0, size.y * 0.25 + 20.0),
        ),
    ];
    let mut ribbon = Ribbon::default();
    let cross = Vec3::new(-direction.z, 0.0, direction.x) * 0.6;
    for index in 0..48 {
        let at = |index: usize| {
            direction * (radius * 0.58 + (radius * 0.42 - 4.0) * index as f32 / 48.0)
                + Vec3::Y * 0.3
        };
        let (a, b) = (at(index), at(index + 1));
        if regions
            .iter()
            .any(|region| region.contains(((a + b) * 0.5).xz()))
        {
            continue;
        }
        ribbon.quad(
            [a - cross, a + cross, b + cross, b - cross],
            palette::rgba(palette.present),
        );
    }
    let tip = direction * (radius - 1.0) + Vec3::Y * 0.3;
    let base = direction * (radius - 7.0) + Vec3::Y * 0.3;
    ribbon.quad(
        [tip, base - cross * 3.0, base + cross * 3.0, tip],
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

fn face_mesh(uv: Rect, rectangles: &[Rect]) -> Mesh {
    let mut positions = Vec::new();
    let mut coords = Vec::new();
    let mut indices = Vec::new();
    for rect in rectangles {
        if rect.size().min_element() <= 0.0 {
            continue;
        }
        let points = [
            rect.min,
            Vec2::new(rect.max.x, rect.min.y),
            rect.max,
            Vec2::new(rect.min.x, rect.max.y),
        ];
        let offset = positions.len() as u32;
        for point in points {
            let position = (point - uv.min) / uv.size() - Vec2::splat(0.5);
            positions.push([position.x, -position.y, 0.0]);
            coords.push(point.to_array());
        }
        indices.extend([
            offset,
            offset + 2,
            offset + 1,
            offset,
            offset + 3,
            offset + 2,
        ]);
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, coords)
    .with_inserted_indices(Indices::U32(indices))
}

pub(super) fn placement(world: &mut World, owner: Entity, viewport: Entity, spatial: bool) {
    let Some(surface) = world.get::<crate::topology::presentation::Surface>(owner) else {
        return;
    };
    let (face, body, density, size, uv) = (
        surface.face,
        surface.body,
        surface.density,
        surface.size,
        surface.uv,
    );
    if world.get::<ComputedNode>(viewport).is_none() {
        return;
    }
    let Some(transform) = world.get::<UiGlobalTransform>(viewport) else {
        return;
    };
    let center = transform.translation / density - size * 0.5;
    let regions = if spatial {
        chrome::surfaces(world, owner)
            .into_iter()
            .filter_map(|entity| {
                let node = world.get::<ComputedNode>(entity)?;
                let transform = world.get::<UiGlobalTransform>(entity)?;
                let center = transform.translation / density;
                let extent = node.size() / density * 0.5;
                let rect = Rect::from_corners(
                    ((center - extent) / size).max(uv.min),
                    ((center + extent) / size).min(uv.max),
                );
                (rect.size().min_element() > 0.0).then_some(rect)
            })
            .collect()
    } else {
        vec![uv]
    };
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
    if let Some(mut visibility) = world.get_mut::<Visibility>(body) {
        *visibility = Visibility::Hidden;
    }
    let cut = FaceCut { uv, regions };
    if world.get::<FaceCut>(face) != Some(&cut) {
        let handle = world.get::<Mesh3d>(face).unwrap().0.clone();
        let mesh = face_mesh(uv, &cut.regions);
        let _ = world
            .resource_mut::<Assets<Mesh>>()
            .insert(handle.id(), mesh);
        world
            .entity_mut(face)
            .remove::<bevy::camera::primitives::Aabb>()
            .insert(cut);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn face_has_a_real_opening_and_valid_clipped_uvs() {
        let uv = Rect::from_corners(Vec2::splat(0.1), Vec2::splat(0.9));
        let hole = Rect::from_corners(Vec2::splat(0.3), Vec2::splat(0.7));
        let regions = [
            Rect::from_corners(uv.min, Vec2::new(uv.max.x, hole.min.y)),
            Rect::from_corners(Vec2::new(uv.min.x, hole.max.y), uv.max),
        ];
        let mesh = face_mesh(uv, &regions);
        assert_eq!(mesh.count_vertices(), 8);
        assert_eq!(mesh.indices().unwrap().len(), 12);
        let bevy::mesh::VertexAttributeValues::Float32x2(coords) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
        else {
            panic!()
        };
        assert!(
            coords
                .iter()
                .all(|point| uv.contains(Vec2::from_array(*point))
                    && (point[1] <= hole.min.y || point[1] >= hole.max.y))
        );
    }
}
