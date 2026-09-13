use super::presentation::{SceneCamera, SpatialRoot, Surface, VisualOwner};
use bevy::{
    asset::uuid::Uuid,
    ecs::message::MessageCursor,
    input::ButtonState,
    math::DVec3,
    picking::{
        backend::{HitData, PointerHits},
        pointer::{Location, PointerAction, PointerId, PointerInput},
    },
    prelude::*,
    window::{PrimaryWindow, WindowEvent},
};

pub const CONTENT_POINTER: PointerId =
    PointerId::Custom(Uuid::from_u128(0x4c696e6365546f706f6c6f6779));

#[derive(Resource, Default)]
pub struct PointerState {
    pub hit: Option<(Entity, Vec3)>,
    cursor: Option<Location>,
    pub drag: Option<(Entity, DVec3)>,
    last: Vec2,
    pan: Option<Vec2>,
}

pub fn ray(world: &World, point: Vec2) -> Option<Ray3d> {
    let entity = world.get_resource::<SceneCamera>()?.0;
    world
        .get::<Camera>(entity)?
        .viewport_to_world(world.get::<GlobalTransform>(entity)?, point)
        .ok()
}

pub fn plane_point(world: &World, root: Entity, point: Vec2, elevation: f64) -> Option<DVec3> {
    let ray = ray(world, point)?;
    if ray.direction.y.abs() < 1e-6 {
        return None;
    }
    let distance = (elevation as f32 - ray.origin.y) / ray.direction.y;
    if distance < 0.0 {
        return None;
    }
    Some(
        (ray.origin + ray.direction * distance).as_dvec3()
            + super::presentation::origin(world, root),
    )
}

pub fn pointer(
    mut events: MessageReader<WindowEvent>,
    windows: Query<(Entity, &Window), With<PrimaryWindow>>,
    camera: Option<Res<SceneCamera>>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    roots: Query<
        (
            Entity,
            &crate::edit_mode::EditMode,
            &crate::canvas::CanvasView,
            &crate::workspace::Workspaces,
        ),
        With<SpatialRoot>,
    >,
    surfaces: Query<&Surface>,
    owners: Query<(&VisualOwner, &GlobalTransform)>,
    parents: Query<&ChildOf>,
    assets: Query<
        (
            Entity,
            &super::physics::MeshCollider,
            &crate::canvas::CanvasItem,
            &super::Spatial,
            &ChildOf,
            &crate::workspace::WorkspaceMember,
        ),
        With<super::assets::Ready>,
    >,
    areas: Query<(
        Entity,
        &crate::area::InfluenceArea,
        Option<&super::Spatial>,
        Option<&crate::layout::LayoutBox>,
        &ChildOf,
        &crate::workspace::WorkspaceMember,
    )>,
    excluded: Query<(), With<crate::inspection::InspectionExcluded>>,
    hover: Res<bevy::picking::hover::HoverMap>,
    mut raycast: MeshRayCast,
    mut state: ResMut<PointerState>,
    mut inputs: MessageWriter<PointerInput>,
    mut hits: MessageWriter<PointerHits>,
) {
    let Some(camera_id) = camera.map(|c| c.0) else {
        return;
    };
    let Ok((camera, camera_transform)) = cameras.get(camera_id) else {
        return;
    };
    let Ok((_, window)) = windows.single() else {
        return;
    };
    let Some(position) = window.cursor_position() else {
        state.hit = None;
        state.drag = None;
        state.pan = None;
        if let Some(location) = state.cursor.take() {
            inputs.write(PointerInput::new(
                CONTENT_POINTER,
                location,
                PointerAction::Cancel,
            ));
        }
        events.clear();
        return;
    };
    let Some((root, mode, canvas, spaces)) = roots.iter().next() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, position) else {
        return;
    };
    let overlay = hover
        .get(&PointerId::Mouse)
        .and_then(|hits| {
            hits.iter()
                .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
        })
        .is_some_and(|(entity, _)| {
            let mut cursor = Some(*entity);
            while let Some(entity) = cursor {
                if excluded.contains(entity) {
                    return true;
                }
                cursor = parents.get(entity).ok().map(ChildOf::parent);
            }
            false
        });
    let mut hit_owner = None;
    let mut location = None;
    if !overlay {
        let render_origin = DVec3::new(canvas.center.x, 0.0, canvas.center.y);
        let mut nearest = f64::MAX;
        for (entity, shape, item, placement, parent, member) in &assets {
            if parent.parent() != root || member.0 != spaces.active {
                continue;
            }
            let inverse = placement.rotation().inverse();
            if let Some(distance) = shape.ray_distance(
                inverse
                    * (ray.origin.as_dvec3() + render_origin - placement.position(item.position)),
                inverse * ray.direction.as_dvec3(),
            ) && distance < nearest
            {
                nearest = distance;
                hit_owner = Some((entity, ray.get_point(distance as f32)));
            }
        }
        let filter = |entity| owners.contains(entity);
        if let Some((mesh, hit)) = raycast
            .cast_ray(ray, &MeshRayCastSettings::default().with_filter(&filter))
            .first()
            && f64::from(hit.distance) < nearest
            && let Ok((owner, _)) = owners.get(*mesh)
        {
            hit_owner = Some((owner.0, hit.point));
        }
    }
    if let Some((owner, hit)) = hit_owner {
        if !mode.enabled
            && let Ok(surface) = surfaces.get(owner)
            && let Ok((_, transform)) = owners.get(surface.face)
        {
            let point = transform.affine().inverse().transform_point3(hit);
            if point.z.abs() <= 0.1 && point.x.abs() <= 0.501 && point.y.abs() <= 0.501 {
                let uv = Vec2::new(point.x + 0.5, 0.5 - point.y).clamp(Vec2::ZERO, Vec2::ONE);
                let uv = surface.uv.min + uv * surface.uv.size();
                location = Some(Location {
                    target: bevy::camera::NormalizedRenderTarget::Image(
                        bevy::camera::ImageRenderTarget {
                            handle: surface.image.clone(),
                            scale_factor: surface.density,
                        },
                    ),
                    position: uv * surface.pixels.as_vec2() / surface.density,
                });
            }
        }
    }
    if !overlay {
        let render_origin = DVec3::new(canvas.center.x, 0.0, canvas.center.y);
        for (entity, area, placement, layout, parent, member) in &areas {
            if parent.parent() != root || member.0 != spaces.active {
                continue;
            }
            if !mode.enabled && layout.is_none() {
                continue;
            }
            let placement = placement.copied().unwrap_or_default();
            let normal = placement.rotation() * DVec3::Y;
            let center =
                placement.position(bevy::math::DVec2::from_array(area.center)) - render_origin;
            let denominator = normal.dot(ray.direction.as_dvec3());
            if denominator.abs() < 1e-6 {
                continue;
            }
            let distance = normal.dot(center - ray.origin.as_dvec3()) / denominator;
            if distance < 0.0
                || hit_owner.is_some_and(|(_, hit)| f64::from(hit.distance(ray.origin)) <= distance)
            {
                continue;
            }
            let point = ray.origin.as_dvec3() + ray.direction.as_dvec3() * distance;
            let local = placement.rotation().inverse() * (point - center);
            if area.contains(
                bevy::math::DVec2::from_array(area.center)
                    + bevy::math::DVec2::new(local.x, local.z),
            ) {
                hit_owner = Some((entity, point.as_vec3()));
            }
        }
    }
    state.hit = hit_owner;
    let entity = if location.is_some() {
        root
    } else {
        hit_owner.map_or(root, |(e, _)| e)
    };
    hits.write(PointerHits::new(
        PointerId::Mouse,
        vec![(
            entity,
            HitData::new(camera_id, 0.0, hit_owner.map(|(_, p)| p), None),
        )],
        -1.0,
    ));
    if let Some(location) = location {
        inputs.write(PointerInput::new(
            CONTENT_POINTER,
            location.clone(),
            PointerAction::Move {
                delta: location.position - state.last,
            },
        ));
        state.last = location.position;
        state.cursor = Some(location);
    } else if let Some(location) = state.cursor.take() {
        inputs.write(PointerInput::new(
            CONTENT_POINTER,
            location,
            PointerAction::Cancel,
        ));
    }
    for event in events.read() {
        if let WindowEvent::MouseWheel(input) = event
            && let Some(location) = &state.cursor
        {
            inputs.write(PointerInput::new(
                CONTENT_POINTER,
                location.clone(),
                PointerAction::Scroll {
                    unit: input.unit,
                    x: input.x,
                    y: input.y,
                    phase: input.phase,
                },
            ));
        }
        if let WindowEvent::MouseButtonInput(input) = event
            && let Some(location) = &state.cursor
        {
            let button = match input.button {
                MouseButton::Left => PointerButton::Primary,
                MouseButton::Right => PointerButton::Secondary,
                _ => continue,
            };
            let action = match input.state {
                ButtonState::Pressed => PointerAction::Press(button),
                ButtonState::Released => PointerAction::Release(button),
            };
            inputs.write(PointerInput::new(CONTENT_POINTER, location.clone(), action));
        }
    }
}

pub fn gestures(world: &mut World, mut cursor: Local<MessageCursor<PointerInput>>) {
    if world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::Escape)
        && let Some((entity, _)) = world.resource::<PointerState>().drag
        && let Some(point) = world
            .get::<crate::area_mutation::HeldPoint>(entity)
            .map(|held| held.0)
    {
        super::set_position(world, entity, point);
        world.resource_mut::<PointerState>().drag = None;
    }
    let events: Vec<_> = cursor
        .read(world.resource::<Messages<PointerInput>>())
        .filter(|e| e.pointer_id == PointerId::Mouse)
        .cloned()
        .collect();
    for event in events {
        let hit = world.resource::<PointerState>().hit;
        let top = world
            .resource::<bevy::picking::hover::HoverMap>()
            .get(&PointerId::Mouse)
            .and_then(|hits| {
                hits.iter()
                    .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
            })
            .map(|(e, _)| *e);
        if matches!(event.action, PointerAction::Press(_))
            && top != hit.map(|(e, _)| e)
            && top.is_none_or(|e| world.get::<SpatialRoot>(e).is_none())
        {
            continue;
        }
        if world
            .resource::<ButtonInput<KeyCode>>()
            .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        {
            continue;
        }
        match event.action {
            PointerAction::Press(PointerButton::Primary) => {
                let Some((entity, _)) = hit else {
                    world.resource_mut::<PointerState>().pan = Some(event.location.position);
                    continue;
                };
                let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
                    continue;
                };
                if !world
                    .get::<crate::edit_mode::EditMode>(root)
                    .is_some_and(|m| m.enabled)
                {
                    continue;
                }
                let Some(position) = super::position(world, entity) else {
                    continue;
                };
                if let Some(point) = plane_point(world, root, event.location.position, position.y) {
                    world.resource_mut::<PointerState>().drag = Some((entity, point));
                }
            }
            PointerAction::Press(PointerButton::Secondary) => {
                world.resource_mut::<PointerState>().pan = Some(event.location.position);
            }
            PointerAction::Move { .. } => {
                if let Some(last) = world.resource::<PointerState>().pan {
                    let root = world
                        .query_filtered::<Entity, With<SpatialRoot>>()
                        .iter(world)
                        .next();
                    if let Some(root) = root {
                        let delta = event.location.position - last;
                        if world
                            .get::<super::view::View>(root)
                            .is_some_and(|v| v.spatial)
                        {
                            let mut view = world.get_mut::<super::view::View>(root).unwrap();
                            view.yaw -= delta.x * 0.005;
                            view.pitch = (view.pitch - delta.y * 0.005).clamp(-1.5, 1.5);
                        } else if let Some(mut view) =
                            world.get_mut::<crate::canvas::CanvasView>(root)
                        {
                            let zoom = view.zoom;
                            view.center -= delta.as_dvec2() / zoom;
                        }
                    }
                    world.resource_mut::<PointerState>().pan = Some(event.location.position);
                    continue;
                }
                let Some((entity, last)) = world.resource::<PointerState>().drag else {
                    continue;
                };
                let Some(root) = world.get::<ChildOf>(entity).map(ChildOf::parent) else {
                    continue;
                };
                if let Some(point) = plane_point(world, root, event.location.position, last.y) {
                    super::groups::transform(
                        world,
                        entity,
                        point - last,
                        bevy::math::DQuat::IDENTITY,
                    );
                    world.resource_mut::<PointerState>().drag = Some((entity, point));
                }
            }
            PointerAction::Release(_) | PointerAction::Cancel => {
                let mut state = world.resource_mut::<PointerState>();
                state.drag = None;
                state.pan = None;
            }
            PointerAction::Scroll { y, .. } => {
                let root = world
                    .query_filtered::<Entity, With<SpatialRoot>>()
                    .iter(world)
                    .next();
                if let Some(root) = root
                    && !world
                        .get::<super::view::View>(root)
                        .is_some_and(|view| view.spatial)
                    && (hit.is_none()
                        || world
                            .get::<crate::edit_mode::EditMode>(root)
                            .is_some_and(|mode| mode.enabled))
                {
                    let mut view = world.get_mut::<crate::canvas::CanvasView>(root).unwrap();
                    let zoom = view.zoom * (f64::from(y).clamp(-10.0, 10.0) * 0.1).exp();
                    view.set_zoom(zoom);
                }
            }
            _ => {}
        }
    }
}
