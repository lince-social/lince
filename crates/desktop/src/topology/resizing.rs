use super::*;
use crate::{
    canvas::CanvasItem,
    canvas_resize::{Edges, Resize},
};
use bevy::window::{CursorIcon, PrimaryWindow};

struct Original {
    entity: Entity,
    item: CanvasItem,
    spatial: Spatial,
    layout: Option<crate::layout::LayoutBox>,
}

pub(super) struct Gesture {
    entity: Entity,
    root: Entity,
    workspace: Option<crate::workspace::WorkspaceMember>,
    origin: DVec3,
    rotation: DQuat,
    start: Vec2,
    size: Vec2,
    pub edges: Edges,
    members: Vec<Original>,
}

fn local(world: &World, root: Entity, origin: DVec3, rotation: DQuat, point: Vec2) -> Option<Vec2> {
    let ray = input::ray(world, point)?;
    let normal = rotation * DVec3::Y;
    let ray_origin = ray.origin.as_dvec3() + presentation::origin(world, root);
    let denominator = normal.dot(ray.direction.as_dvec3());
    if denominator.abs() < 1e-6 {
        return None;
    }
    let distance = normal.dot(origin - ray_origin) / denominator;
    if distance < 0.0 {
        return None;
    }
    let offset = rotation.inverse() * (ray_origin + ray.direction.as_dvec3() * distance - origin);
    Some(Vec2::new(offset.x as f32, offset.z as f32))
}

fn edge(world: &World, entity: Entity, point: Vec2) -> Option<(Edges, Vec2)> {
    let root = world.get::<ChildOf>(entity)?.parent();
    let item = world.get::<CanvasItem>(entity)?;
    let spatial = spatial(world, entity);
    let origin = spatial.position(item.position);
    let rotation = spatial.rotation();
    let point = local(world, root, origin, rotation, point)?;
    let camera = world.get_resource::<presentation::SceneCamera>()?.0;
    let camera_transform = world.get::<GlobalTransform>(camera)?;
    let camera = world.get::<Camera>(camera)?;
    let render_origin = presentation::origin(world, root);
    let projected = camera
        .world_to_viewport(camera_transform, (origin - render_origin).as_vec3())
        .ok()?;
    let mut scale = Vec2::ZERO;
    for (axis, vector) in [DVec3::X, DVec3::Z].into_iter().enumerate() {
        let projected_axis = camera
            .world_to_viewport(
                camera_transform,
                (origin - render_origin + rotation * vector).as_vec3(),
            )
            .ok()?;
        scale[axis] = projected_axis.distance(projected).max(0.001);
    }
    let edges = Edges::at(
        point * scale,
        Rect::from_center_size(Vec2::ZERO, item.size * scale),
    );
    edges.cursor()?;
    Some((edges, point))
}

impl Gesture {
    pub fn start(world: &World, entity: Entity, point: Vec2) -> Option<Self> {
        let (edges, start) = edge(world, entity, point)?;
        let root = world.get::<ChildOf>(entity)?.parent();
        let item = world.get::<CanvasItem>(entity)?;
        let spatial = spatial(world, entity);
        let members = crate::canvas_selection::companions(world, root, entity)
            .into_iter()
            .filter_map(|entity| {
                Some(Original {
                    entity,
                    item: *world.get::<CanvasItem>(entity)?,
                    spatial: super::spatial(world, entity),
                    layout: world.get::<crate::layout::LayoutBox>(entity).copied(),
                })
            })
            .collect();
        Some(Self {
            entity,
            root,
            workspace: world
                .get::<crate::workspace::WorkspaceMember>(entity)
                .copied(),
            origin: spatial.position(item.position),
            rotation: spatial.rotation(),
            start,
            size: item.size,
            edges,
            members,
        })
    }

    pub fn apply(&self, world: &mut World, point: Vec2) -> bool {
        if world.get_entity(self.entity).is_err()
            || !world
                .get::<crate::edit_mode::EditMode>(self.root)
                .is_some_and(|mode| mode.enabled)
            || world
                .get::<crate::workspace::Workspaces>(self.root)
                .is_some_and(|spaces| {
                    self.workspace
                        .is_some_and(|member| member.0 != spaces.active)
                })
        {
            return false;
        }
        let Some(point) = local(world, self.root, self.origin, self.rotation, point) else {
            return false;
        };
        let resize = Resize {
            edges: self.edges,
            zoom: 1.0,
            scale: 1.0,
            start: self.start,
            original: CanvasItem {
                position: DVec2::ZERO,
                size: self.size,
            },
            minimum: Vec2::splat(80.0),
        };
        let regular = world
            .get::<crate::area::InfluenceArea>(self.entity)
            .is_some_and(|area| area.shape.kind() != crate::area::ShapeKind::Drawn);
        let Some(next) = (if regular {
            resize.apply_square(point)
        } else {
            resize.apply(point)
        }) else {
            return false;
        };
        let scale = next.size / self.size;
        let origin =
            self.origin + self.rotation * DVec3::new(next.position.x, 0.0, next.position.y);
        for original in &self.members {
            let Some(before) = world.get::<CanvasItem>(original.entity).copied() else {
                continue;
            };
            let mut offset = self.rotation.inverse()
                * (original.spatial.position(original.item.position) - self.origin);
            offset.x *= f64::from(scale.x);
            offset.z *= f64::from(scale.y);
            set_position(world, original.entity, origin + self.rotation * offset);
            let size = original.item.size * scale;
            world.get_mut::<CanvasItem>(original.entity).unwrap().size = size;
            if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(original.entity) {
                area.size = size.as_dvec2().to_array();
            }
            let after = *world.get::<CanvasItem>(original.entity).unwrap();
            crate::layout::edited(world, original.entity, before, after);
        }
        let members = groups::members(world, self.entity);
        if members.len() > 1 {
            groups::attach(world, &members);
        }
        true
    }

    pub fn cancel(self, world: &mut World) {
        for original in &self.members {
            if world.get_entity(original.entity).is_err() {
                continue;
            }
            world
                .entity_mut(original.entity)
                .insert((original.item, original.spatial));
            if let Some(mut area) = world.get_mut::<crate::area::InfluenceArea>(original.entity) {
                area.center = original.item.position.to_array();
                area.size = original.item.size.as_dvec2().to_array();
            }
            if let Some(layout) = original.layout {
                world.entity_mut(original.entity).insert(layout);
                crate::layout::edited(world, original.entity, original.item, original.item);
            } else {
                world
                    .entity_mut(original.entity)
                    .remove::<crate::layout::LayoutBox>();
                crate::layout::records::forget(world, original.entity);
            }
        }
        let members = groups::members(world, self.entity);
        if members.len() > 1 {
            groups::attach(world, &members);
        }
    }
}

#[derive(Resource)]
struct PreviousCursor(Entity, Option<CursorIcon>);

pub(super) fn cursor(world: &mut World, active: Option<Edges>) {
    let window = world
        .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
        .iter(world)
        .next()
        .map(|(entity, window)| (entity, window.cursor_position()));
    let Some((window, position)) = window else {
        return;
    };
    let hover = position.and_then(|point| {
        let entity = world.resource::<input::PointerState>().hit?.0;
        let root = world.get::<ChildOf>(entity)?.parent();
        world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|mode| mode.enabled)
            .then_some(())?;
        edge(world, entity, point).map(|(edges, _)| edges)
    });
    let icon = active.or(hover).and_then(Edges::cursor);
    if let Some(icon) = icon {
        if !world.contains_resource::<PreviousCursor>() {
            world.insert_resource(PreviousCursor(
                window,
                world.get::<CursorIcon>(window).cloned(),
            ));
        }
        world.entity_mut(window).insert(CursorIcon::System(icon));
    } else if let Some(previous) = world.remove_resource::<PreviousCursor>()
        && let Ok(mut window) = world.get_entity_mut(previous.0)
    {
        if let Some(icon) = previous.1 {
            window.insert(icon);
        } else {
            window.remove::<CursorIcon>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        camera::CameraProjection,
        picking::{
            backend::HitData,
            hover::HoverMap,
            pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput},
        },
    };

    #[test]
    fn native_edge_drag_resizes_castles_and_loose_areas_at_each_zoom() {
        for zoom in [0.5, 1.0, 2.0] {
            for area in [false, true] {
                let (mut app, root) = crate::edit_mode::tests::fixture();
                app.init_resource::<input::PointerState>()
                    .init_resource::<ButtonInput<KeyCode>>()
                    .init_resource::<HoverMap>()
                    .add_message::<PointerInput>()
                    .add_systems(Update, input::gestures);
                app.world_mut().entity_mut(root).insert((
                    presentation::SpatialRoot,
                    crate::canvas::CanvasView { zoom, ..default() },
                ));
                app.world_mut()
                    .get_mut::<crate::edit_mode::EditMode>(root)
                    .unwrap()
                    .enabled = true;
                let mut projection = OrthographicProjection::default_3d();
                projection.scale = 1.0 / zoom as f32;
                projection.update(800.0, 600.0);
                let mut camera = Camera::default();
                camera.computed.target_info = Some(bevy::camera::RenderTargetInfo {
                    physical_size: UVec2::new(800, 600),
                    scale_factor: 1.0,
                });
                camera.computed.clip_from_view = projection.get_clip_from_view();
                let camera = app
                    .world_mut()
                    .spawn((
                        camera,
                        GlobalTransform::from(
                            Transform::from_xyz(0.0, 1000.0, 0.0)
                                .looking_at(Vec3::ZERO, Vec3::NEG_Z),
                        ),
                    ))
                    .id();
                app.insert_resource(presentation::SceneCamera(camera));
                let item = app
                    .world_mut()
                    .spawn((
                        CanvasItem {
                            position: DVec2::ZERO,
                            size: Vec2::splat(200.0),
                        },
                        crate::workspace::WorkspaceMember(1),
                        ChildOf(root),
                    ))
                    .id();
                if area {
                    app.world_mut()
                        .entity_mut(item)
                        .insert(crate::area::InfluenceArea::new(
                            crate::area::AreaShape::Square,
                            DVec2::ZERO,
                            DVec2::splat(200.0),
                        ));
                }
                let source = if !area {
                    let source = crate::area::spawn_area(
                        app.world_mut(),
                        root,
                        1,
                        crate::area::InfluenceArea::new(
                            crate::area::AreaShape::Polygon(vec![
                                [-0.5, -0.5],
                                [0.5, -0.5],
                                [0.5, 0.5],
                                [-0.5, 0.5],
                                [-0.5, -0.5],
                            ]),
                            DVec2::ZERO,
                            DVec2::splat(200.0),
                        ),
                    )
                    .unwrap();
                    for entity in [source, item] {
                        app.world_mut()
                            .entity_mut(entity)
                            .insert(crate::canvas_selection::SandGroup([1; 16]));
                    }
                    groups::attach(app.world_mut(), &[source, item]);
                    Some(source)
                } else {
                    None
                };
                app.world_mut().resource_mut::<input::PointerState>().hit =
                    Some((item, Vec3::ZERO));
                app.world_mut()
                    .resource_mut::<HoverMap>()
                    .entry(PointerId::Mouse)
                    .or_default()
                    .insert(item, HitData::new(camera, 0.0, None, None));
                let project = |world: &World, point: Vec3| {
                    world
                        .get::<Camera>(camera)
                        .unwrap()
                        .world_to_viewport(world.get::<GlobalTransform>(camera).unwrap(), point)
                        .unwrap()
                };
                let start = project(app.world(), Vec3::new(98.0, 0.0, 0.0));
                let end = project(app.world(), Vec3::new(148.0, 0.0, 0.0));
                let send = |app: &mut App, position, action| {
                    app.world_mut().write_message(PointerInput::new(
                        PointerId::Mouse,
                        Location {
                            target: bevy::camera::NormalizedRenderTarget::Image(
                                Handle::<Image>::default().into(),
                            ),
                            position,
                        },
                        action,
                    ));
                    app.update();
                };
                send(
                    &mut app,
                    start,
                    PointerAction::Press(PointerButton::Primary),
                );
                send(&mut app, end, PointerAction::Move { delta: end - start });
                let resized = *app.world().get::<CanvasItem>(item).unwrap();
                assert!((resized.size.x - 250.0).abs() < 0.1, "{:?}", resized.size);
                assert!((resized.position.x - 25.0).abs() < 0.1);
                assert!((resized.size.y - if area { 250.0 } else { 200.0 }).abs() < 0.1);
                assert!(app.world().get::<crate::layout::LayoutBox>(item).is_some());
                if let Some(source) = source {
                    assert_eq!(
                        app.world().get::<CanvasItem>(source).unwrap().size,
                        resized.size
                    );
                    assert_eq!(
                        app.world()
                            .get::<crate::area::InfluenceArea>(source)
                            .unwrap()
                            .size,
                        resized.size.as_dvec2().to_array()
                    );
                }
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .press(KeyCode::Escape);
                app.update();
                assert_eq!(
                    app.world().get::<CanvasItem>(item).unwrap().size,
                    Vec2::splat(200.0)
                );
                assert_eq!(
                    app.world().get::<CanvasItem>(item).unwrap().position,
                    DVec2::ZERO
                );
                assert!(app.world().get::<crate::layout::LayoutBox>(item).is_none());
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .reset_all();
                let start = project(app.world(), Vec3::ZERO);
                let end = project(app.world(), Vec3::new(40.0, 0.0, 20.0));
                send(
                    &mut app,
                    start,
                    PointerAction::Press(PointerButton::Primary),
                );
                send(&mut app, end, PointerAction::Move { delta: end - start });
                send(
                    &mut app,
                    end,
                    PointerAction::Release(PointerButton::Primary),
                );
                let moved = app.world().get::<CanvasItem>(item).unwrap();
                assert!(moved.position.distance(DVec2::new(40.0, 20.0)) < 0.1);
                assert_eq!(moved.size, Vec2::splat(200.0));
                if area {
                    assert_eq!(
                        app.world()
                            .get::<crate::area::InfluenceArea>(item)
                            .unwrap()
                            .center,
                        moved.position.to_array()
                    );
                }
            }
        }
    }
}
