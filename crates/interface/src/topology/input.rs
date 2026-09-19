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
    pending_drag: Option<(Entity, Vec2, DVec3)>,
    last: Vec2,
    pan: Option<Vec2>,
    zoom: Option<(Entity, Vec2)>,
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
        state.pending_drag = None;
        state.pan = None;
        state.zoom = None;
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
    if overlay || state.zoom.is_some_and(|(_, start)| start != position) {
        state.zoom = None;
    }
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
        let filter = |entity| {
            owners
                .get(entity)
                .is_ok_and(|(owner, _)| !areas.contains(owner.0))
        };
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
        for (entity, area, placement, parent, member) in &areas {
            if parent.parent() != root || member.0 != spaces.active {
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
        if matches!(event, WindowEvent::MouseButtonInput(_))
            || matches!(event, WindowEvent::MouseWheel(input) if input.phase == bevy::input::touch::TouchPhase::Started)
        {
            state.zoom = None;
        }
        if let WindowEvent::MouseWheel(input) = event
            && state.zoom.is_none()
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
        if state.drag.is_none()
            && let WindowEvent::MouseButtonInput(input) = event
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
    {
        world.resource_mut::<PointerState>().pending_drag = None;
    }
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
        if world
            .resource::<PointerState>()
            .zoom
            .is_some_and(|(_, start)| start != event.location.position)
            || matches!(
                event.action,
                PointerAction::Press(_) | PointerAction::Cancel
            )
            || matches!(
                event.action,
                PointerAction::Scroll {
                    phase: bevy::input::touch::TouchPhase::Started,
                    ..
                }
            )
        {
            world.resource_mut::<PointerState>().zoom = None;
        }
        let hit = world.resource::<PointerState>().hit;
        let top = world
            .resource::<bevy::picking::hover::HoverMap>()
            .get(&PointerId::Mouse)
            .and_then(|hits| {
                hits.iter()
                    .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
            })
            .map(|(e, _)| *e);
        if top.is_none_or(|entity| {
            world.get::<SpatialRoot>(entity).is_none()
                && Some(entity) != hit.map(|(entity, _)| entity)
        }) {
            world.resource_mut::<PointerState>().zoom = None;
        }
        if matches!(event.action, PointerAction::Press(_))
            && top != hit.map(|(e, _)| e)
            && top.is_none_or(|e| world.get::<SpatialRoot>(e).is_none())
        {
            world.resource_mut::<PointerState>().zoom = None;
            continue;
        }
        if world
            .resource::<ButtonInput<KeyCode>>()
            .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        {
            world.resource_mut::<PointerState>().zoom = None;
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
                let task = world
                    .get::<crate::full_record::RecordCard>(entity)
                    .is_some();
                if task && editing_text(world) {
                    continue;
                }
                if !task
                    && !world
                        .get::<crate::edit_mode::EditMode>(root)
                        .is_some_and(|m| m.enabled)
                {
                    continue;
                }
                let Some(position) = super::position(world, entity) else {
                    continue;
                };
                if let Some(point) = plane_point(world, root, event.location.position, position.y) {
                    if task {
                        world.resource_mut::<PointerState>().pending_drag =
                            Some((entity, event.location.position, point));
                    } else {
                        world.resource_mut::<PointerState>().drag = Some((entity, point));
                    }
                }
            }
            PointerAction::Press(PointerButton::Secondary) => {
                let target = hit.and_then(|(entity, _)| {
                    let root = world.get::<ChildOf>(entity)?.parent();
                    let position = super::position(world, entity)?;
                    let point = plane_point(world, root, event.location.position, position.y)?;
                    Some((entity, point))
                });
                let mut state = world.resource_mut::<PointerState>();
                state.pending_drag = None;
                state.drag = target;
                state.pan = target.is_none().then_some(event.location.position);
                if target.is_some() {
                    if let Some(location) = state.cursor.take() {
                        world.write_message(PointerInput::new(
                            CONTENT_POINTER,
                            location,
                            PointerAction::Cancel,
                        ));
                    }
                    world
                        .resource_mut::<bevy::input_focus::InputFocus>()
                        .clear();
                }
            }
            PointerAction::Move { .. } => {
                if let Some((entity, start, point)) = world.resource::<PointerState>().pending_drag
                    && event.location.position.distance(start) >= 5.0
                {
                    world.resource_mut::<PointerState>().pending_drag = None;
                    crate::kanban::begin_drag(world, entity, point);
                    if let Some(location) = world.resource::<PointerState>().cursor.clone() {
                        world.write_message(PointerInput::new(
                            CONTENT_POINTER,
                            location,
                            PointerAction::Cancel,
                        ));
                    }
                }
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
                state.pending_drag = None;
                state.pan = None;
            }
            PointerAction::Scroll { y, phase, .. } => {
                if phase == bevy::input::touch::TouchPhase::Canceled {
                    world.resource_mut::<PointerState>().zoom = None;
                    continue;
                }
                let root = world
                    .query_filtered::<Entity, With<SpatialRoot>>()
                    .iter(world)
                    .next();
                if let Some(root) = root
                    && y.is_finite()
                    && y != 0.0
                    && ((top == Some(root) && hit.is_none())
                        || (world.resource::<PointerState>().zoom
                            == Some((root, event.location.position))
                            && (top == Some(root)
                                || top.is_some() && top == hit.map(|(entity, _)| entity))))
                    && !world
                        .get::<super::view::View>(root)
                        .is_some_and(|view| view.spatial)
                {
                    world.resource_mut::<PointerState>().zoom =
                        Some((root, event.location.position));
                    let mut view = world.get_mut::<crate::canvas::CanvasView>(root).unwrap();
                    let zoom = view.zoom * (f64::from(y).clamp(-10.0, 10.0) * 0.1).exp();
                    view.set_zoom(zoom);
                }
                if phase == bevy::input::touch::TouchPhase::Ended {
                    world.resource_mut::<PointerState>().zoom = None;
                }
            }
            _ => {}
        }
    }
}

fn editing_text(world: &World) -> bool {
    world
        .resource::<bevy::picking::hover::HoverMap>()
        .get(&CONTENT_POINTER)
        .is_some_and(|hits| {
            hits.keys().any(|entity| {
                let mut cursor = Some(*entity);
                while let Some(entity) = cursor {
                    if world.get::<bevy::text::EditableText>(entity).is_some() {
                        return true;
                    }
                    cursor = world.get::<ChildOf>(entity).map(ChildOf::parent);
                }
                false
            })
        })
}

pub(crate) mod tests {
    use super::*;
    use crate::canvas::CanvasView;

    #[test]
    fn right_drag_moves_sands_and_areas_in_normal_edit_and_spatial_views() {
        use bevy::{camera::CameraProjection, math::DVec2, picking::pointer::PointerButton};
        for editing in [false, true] {
            for spatial in [false, true] {
                for area in [false, true] {
                    let (mut app, root) = crate::edit_mode::tests::fixture();
                    app.init_resource::<PointerState>()
                        .init_resource::<ButtonInput<KeyCode>>()
                        .init_resource::<bevy::picking::hover::HoverMap>()
                        .add_message::<PointerInput>()
                        .add_systems(Update, gestures);
                    app.world_mut().entity_mut(root).insert((
                        SpatialRoot,
                        CanvasView::default(),
                        super::super::view::View {
                            spatial,
                            ..default()
                        },
                    ));
                    app.world_mut()
                        .get_mut::<crate::edit_mode::EditMode>(root)
                        .unwrap()
                        .enabled = editing;
                    let mut projection = OrthographicProjection::default_3d();
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
                    app.insert_resource(SceneCamera(camera));
                    let item = app
                        .world_mut()
                        .spawn((
                            crate::canvas::CanvasItem {
                                position: DVec2::ZERO,
                                size: Vec2::splat(100.0),
                            },
                            ChildOf(root),
                        ))
                        .id();
                    if area {
                        app.world_mut()
                            .entity_mut(item)
                            .insert(crate::area::InfluenceArea::new(
                                crate::area::AreaShape::Circle,
                                DVec2::ZERO,
                                DVec2::splat(100.0),
                            ));
                    }
                    app.world_mut().resource_mut::<PointerState>().hit = Some((item, Vec3::ZERO));
                    app.world_mut()
                        .resource_mut::<bevy::picking::hover::HoverMap>()
                        .entry(PointerId::Mouse)
                        .or_default()
                        .insert(root, HitData::new(camera, 0.0, None, None));
                    let mut location = Location {
                        target: bevy::camera::NormalizedRenderTarget::Image(
                            Handle::<Image>::default().into(),
                        ),
                        position: Vec2::new(400.0, 300.0),
                    };
                    app.world_mut().write_message(PointerInput::new(
                        PointerId::Mouse,
                        location.clone(),
                        PointerAction::Press(PointerButton::Secondary),
                    ));
                    app.update();
                    assert_eq!(
                        app.world()
                            .resource::<PointerState>()
                            .drag
                            .map(|(entity, _)| entity),
                        Some(item)
                    );
                    assert!(app.world().resource::<PointerState>().pan.is_none());
                    location.position += Vec2::new(40.0, 20.0);
                    app.world_mut().write_message(PointerInput::new(
                        PointerId::Mouse,
                        location.clone(),
                        PointerAction::Move {
                            delta: Vec2::new(40.0, 20.0),
                        },
                    ));
                    app.update();
                    let moved = app.world().get::<crate::canvas::CanvasItem>(item).unwrap();
                    assert!(moved.position.length() > 0.0);
                    assert_eq!(moved.size, Vec2::splat(100.0));
                    assert_eq!(
                        app.world().get::<CanvasView>(root).unwrap().center,
                        DVec2::ZERO
                    );
                    if area {
                        assert_eq!(
                            app.world()
                                .get::<crate::area::InfluenceArea>(item)
                                .unwrap()
                                .center,
                            moved.position.to_array()
                        );
                    }
                    app.world_mut().write_message(PointerInput::new(
                        PointerId::Mouse,
                        location,
                        PointerAction::Release(PointerButton::Secondary),
                    ));
                    app.update();
                    assert!(app.world().resource::<PointerState>().drag.is_none());
                }
            }
        }
    }

    #[test]
    fn task_body_drag_waits_for_movement_and_cancels_the_content_click() {
        use bevy::camera::CameraProjection;
        let mut app = App::new();
        app.init_resource::<PointerState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy::picking::hover::HoverMap>()
            .add_message::<PointerInput>()
            .add_systems(Update, gestures);
        let root = app
            .world_mut()
            .spawn((SpatialRoot, CanvasView::default()))
            .id();
        let mut projection = OrthographicProjection::default_3d();
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
                    Transform::from_xyz(0.0, 1000.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
                ),
            ))
            .id();
        app.insert_resource(SceneCamera(camera));
        let card = app
            .world_mut()
            .spawn((
                crate::full_record::RecordCard,
                crate::canvas::CanvasItem {
                    position: bevy::math::DVec2::ZERO,
                    size: Vec2::new(316.0, 80.0),
                },
                ChildOf(root),
            ))
            .id();
        app.world_mut().resource_mut::<PointerState>().hit = Some((card, Vec3::ZERO));
        app.world_mut()
            .resource_mut::<bevy::picking::hover::HoverMap>()
            .entry(PointerId::Mouse)
            .or_default()
            .insert(root, HitData::new(camera, 0.0, None, None));
        let mut location = Location {
            target: bevy::camera::NormalizedRenderTarget::Image(Handle::<Image>::default().into()),
            position: Vec2::new(400.0, 300.0),
        };
        app.world_mut().resource_mut::<PointerState>().cursor = Some(location.clone());
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            PointerAction::Press(PointerButton::Primary),
        ));
        app.update();
        assert!(
            app.world()
                .resource::<PointerState>()
                .pending_drag
                .is_some()
        );
        assert!(app.world().resource::<PointerState>().drag.is_none());
        location.position.x += 3.0;
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            PointerAction::Move {
                delta: Vec2::new(3.0, 0.0),
            },
        ));
        app.update();
        assert!(app.world().resource::<PointerState>().drag.is_none());
        location.position.x += 10.0;
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            PointerAction::Move {
                delta: Vec2::new(10.0, 0.0),
            },
        ));
        app.update();
        assert_eq!(
            app.world()
                .resource::<PointerState>()
                .drag
                .map(|(entity, _)| entity),
            Some(card)
        );
        assert!(
            app.world()
                .get::<crate::canvas::CanvasItem>(card)
                .unwrap()
                .position
                .x
                > 0.0
        );
        assert!(
            app.world()
                .get::<crate::area_mutation::HeldPoint>(card)
                .is_some()
        );
        let mut messages = MessageCursor::<PointerInput>::default();
        assert!(
            messages
                .read(app.world().resource::<Messages<PointerInput>>())
                .any(|event| event.pointer_id == CONTENT_POINTER
                    && matches!(event.action, PointerAction::Cancel))
        );
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            location,
            PointerAction::Release(PointerButton::Primary),
        ));
        app.update();
        assert!(app.world().resource::<PointerState>().drag.is_none());
        assert!(
            app.world()
                .resource::<PointerState>()
                .pending_drag
                .is_none()
        );
    }

    #[cfg_attr(test, test)]
    fn wheel_zoom_only_accepts_uncovered_canvas_background() {
        let mut app = App::new();
        app.init_resource::<PointerState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy::picking::hover::HoverMap>()
            .add_message::<PointerInput>()
            .add_systems(Update, gestures);
        let root = app
            .world_mut()
            .spawn((SpatialRoot, CanvasView::default()))
            .id();
        let panel = app.world_mut().spawn_empty().id();
        let sand = app.world_mut().spawn_empty().id();
        for (target, hit, zooms) in [
            (Some(root), None, true),
            (Some(panel), None, false),
            (Some(panel), Some(sand), false),
            (Some(root), Some(sand), false),
            (Some(sand), Some(sand), false),
            (None, None, false),
        ] {
            app.world_mut().resource_mut::<PointerState>().zoom = None;
            app.world_mut().get_mut::<CanvasView>(root).unwrap().zoom = 1.0;
            app.world_mut().resource_mut::<PointerState>().hit =
                hit.map(|entity| (entity, Vec3::ZERO));
            let mut hover = app
                .world_mut()
                .resource_mut::<bevy::picking::hover::HoverMap>();
            hover.clear();
            if let Some(target) = target {
                hover
                    .entry(PointerId::Mouse)
                    .or_default()
                    .insert(target, HitData::new(root, 0.0, None, None));
            }
            app.world_mut().write_message(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target: bevy::camera::NormalizedRenderTarget::Image(
                        bevy::camera::ImageRenderTarget {
                            handle: Handle::default(),
                            scale_factor: 1.0,
                        },
                    ),
                    position: Vec2::ZERO,
                },
                PointerAction::Scroll {
                    unit: bevy::input::mouse::MouseScrollUnit::Line,
                    x: 0.0,
                    y: 1.0,
                    phase: bevy::input::touch::TouchPhase::Moved,
                },
            ));
            app.update();
            assert_eq!(
                app.world().get::<CanvasView>(root).unwrap().zoom > 1.0,
                zooms
            );
        }
    }

    crate::laboratory_cases! {
        wheel_zoom_only_accepts_uncovered_canvas_background,
        wheel_zoom_keeps_its_background_owner_until_pointer_movement_or_gesture_end,
    }

    #[cfg_attr(test, test)]
    fn wheel_zoom_keeps_its_background_owner_until_pointer_movement_or_gesture_end() {
        use bevy::input::{mouse::MouseScrollUnit, touch::TouchPhase};
        let mut app = App::new();
        app.init_resource::<PointerState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<bevy::picking::hover::HoverMap>()
            .add_message::<PointerInput>()
            .add_systems(Update, gestures);
        let root = app
            .world_mut()
            .spawn((SpatialRoot, CanvasView::default()))
            .id();
        let sand = app.world_mut().spawn_empty().id();
        let panel = app.world_mut().spawn_empty().id();
        for (top, hit, position, phase, zooms) in [
            (root, None, Vec2::ZERO, TouchPhase::Moved, true),
            (sand, Some(sand), Vec2::ZERO, TouchPhase::Moved, true),
            (sand, Some(sand), Vec2::ZERO, TouchPhase::Moved, true),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Moved, false),
            (root, None, Vec2::ONE, TouchPhase::Started, true),
            (panel, Some(sand), Vec2::ONE, TouchPhase::Moved, false),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Moved, false),
            (root, None, Vec2::ONE, TouchPhase::Moved, true),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Ended, true),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Moved, false),
            (root, None, Vec2::ONE, TouchPhase::Moved, true),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Started, false),
            (root, None, Vec2::ONE, TouchPhase::Moved, true),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Canceled, false),
            (sand, Some(sand), Vec2::ONE, TouchPhase::Moved, false),
        ] {
            let before = app.world().get::<CanvasView>(root).unwrap().zoom;
            app.world_mut().resource_mut::<PointerState>().hit =
                hit.map(|entity| (entity, Vec3::ZERO));
            let mut hover = app
                .world_mut()
                .resource_mut::<bevy::picking::hover::HoverMap>();
            hover.clear();
            hover
                .entry(PointerId::Mouse)
                .or_default()
                .insert(top, HitData::new(root, 0.0, None, None));
            app.world_mut().write_message(PointerInput::new(
                PointerId::Mouse,
                Location {
                    target: bevy::camera::NormalizedRenderTarget::None {
                        width: 800,
                        height: 600,
                    },
                    position,
                },
                PointerAction::Scroll {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y: -1.0,
                    phase,
                },
            ));
            app.update();
            assert_eq!(
                app.world().get::<CanvasView>(root).unwrap().zoom < before,
                zooms
            );
        }
    }
}
