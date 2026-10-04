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
        .id();
    if let Some(ids) = ids {
        world
            .entity_mut(entity)
            .remove::<Pickable>()
            .insert(SchedulePick { owner, ids })
            .observe(clicked);
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
                world.entity_mut(entity).insert(SchedulePick { owner, ids });
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
    if !spatial || !crate::topology::presentation::ready(world) {
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
    let occurrences =
        model::occurrences(&entries, now, now + settings.horizon_ms, &settings.timezone);
    let lanes = occurrences
        .iter()
        .map(|entry| entry.lane + 1)
        .max()
        .unwrap_or(1);
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
    let circular = size.min_element()
        * std::f32::consts::TAU
        * 0.4
        * (settings.horizon_ms as f64 / settings.aperture_ms as f64) as f32;
    let pixels = (circular * (1.0 - unwind) + size.x * unwind) * density;
    let interval = model::tick_interval(settings.horizon_ms, pixels, 25.0)
        .max(model::tick_interval(settings.horizon_ms, 256.0, 1.0));
    let major_interval = model::tick_interval(settings.horizon_ms, pixels, 85.0).max(interval);
    let mut at = now.div_euclid(interval) * interval + interval;
    while at < now + settings.horizon_ms {
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
    let mut ribbons = vec![(background, None)];
    for occurrence in occurrences {
        let entry = &entries[occurrence.index];
        let color = palette::rgba(palette.event(occurrence.lane, selected.contains(&entry.id)));
        let mut ribbon = Ribbon::default();
        let points = render::occurrence_points(settings, &occurrence, lanes, now, size, unwind);
        let width = palette
            .width
            .min((size.min_element() * 0.088 / lanes as f32).max(0.8));
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
        ribbons.push((ribbon, Some(vec![entry.id.clone()])));
    }
    replace(world, parent, owner, ribbons);
}

pub(super) fn annotations(
    world: &mut World,
    owner: Entity,
    labels: &[model::Label],
    palette: &palette::Palette,
) {
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
    let entries = &world.get::<View>(owner).unwrap().entries;
    let selected = &world.get::<View>(owner).unwrap().selected;
    for label in labels {
        let anchor = Vec3::from_array(label.anchor).with_y(0.03);
        let direction = anchor.xz().normalize_or_zero();
        let elbow = anchor + Vec3::new(direction.x, 0.0, direction.y) * 32.0;
        let end = Vec3::new(
            elbow.x.clamp(label.rect[0], label.rect[0] + label.rect[2]),
            0.03,
            elbow.z.clamp(label.rect[1], label.rect[1] + label.rect[3]),
        );
        let color = palette::rgba(
            palette
                .event(
                    label.occurrence.lane,
                    selected.contains(&entries[label.occurrence.index].id),
                )
                .with_alpha(0.45),
        );
        for (a, b) in [(anchor, elbow), (elbow, end)] {
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
