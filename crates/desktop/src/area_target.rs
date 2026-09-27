use crate::{
    area::{AttractionTarget, InfluenceArea},
    area_panel::AreaEditor,
    canvas::CanvasView,
    edit_mode::EditMode,
};
use bevy::{
    ecs::message::MessageCursor,
    math::{DVec2, DVec3},
    picking::{
        hover::{HoverMap, generate_hovermap},
        pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput, PointerPress},
    },
    prelude::*,
    window::WindowEvent,
};

#[derive(Component, Clone, Copy)]
struct Handle {
    root: Entity,
    area: Entity,
}

struct Drag {
    handle: Handle,
    location: Location,
    view: CanvasView,
    placement: crate::topology::Spatial,
    camera: Option<(Entity, Mat4, Mat4, Rect)>,
    cursor: DVec2,
    original: InfluenceArea,
    last: AttractionTarget,
}

#[derive(Resource, Default)]
struct Gesture(Option<Drag>);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TargetInput;

pub(crate) struct AreaTargetPlugin;

impl Plugin for AreaTargetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Gesture>()
            .add_systems(
                PreUpdate,
                input
                    .in_set(TargetInput)
                    .after(generate_hovermap)
                    .before(crate::canvas_selection::SelectInput),
            )
            .add_systems(PostUpdate, draw.before(bevy::ui::UiSystems::Prepare));
    }
}

fn allowed(world: &World, handle: Handle) -> bool {
    crate::area_panel::owns(world, handle.root, handle.area)
        && world
            .get::<InfluenceArea>(handle.area)
            .is_some_and(InfluenceArea::validate)
        && world
            .get::<EditMode>(handle.root)
            .is_some_and(|m| m.enabled && m.areas)
        && world
            .get::<AreaEditor>(handle.root)
            .is_some_and(|e| e.selected == Some(handle.area) && e.tool.is_none())
}

fn camera(world: &World, root: Entity) -> Option<(Entity, Mat4, Mat4, Rect)> {
    world.get::<crate::topology::presentation::SpatialRoot>(root)?;
    let entity = world
        .get_resource::<crate::topology::presentation::SceneCamera>()?
        .0;
    Some((
        entity,
        world.get::<GlobalTransform>(entity)?.to_matrix(),
        world.get::<Camera>(entity)?.clip_from_view(),
        world.get::<Camera>(entity)?.logical_viewport_rect()?,
    ))
}

fn projected(world: &World, handle: Handle, size: Vec2) -> Option<Vec2> {
    let area = world.get::<InfluenceArea>(handle.area)?;
    if world
        .get::<crate::topology::presentation::SpatialRoot>(handle.root)
        .is_some()
    {
        let (entity, ..) = camera(world, handle.root)?;
        let placement = crate::topology::spatial(world, handle.area);
        let offset = area.target_position() - DVec2::from_array(area.center);
        let point = placement.position(DVec2::from_array(area.center))
            + placement.rotation() * DVec3::new(offset.x, 0.0, offset.y)
            - crate::topology::presentation::origin(world, handle.root);
        world
            .get::<Camera>(entity)?
            .world_to_viewport(world.get::<GlobalTransform>(entity)?, point.as_vec3())
            .ok()
    } else {
        let view = world.get::<CanvasView>(handle.root)?;
        Some(((area.target_position() - view.center) * view.zoom).as_vec2() + size * 0.5)
    }
}

fn cursor_point(world: &World, handle: Handle, screen: Vec2) -> Option<DVec2> {
    if world
        .get::<crate::topology::presentation::SpatialRoot>(handle.root)
        .is_none()
    {
        return Some(screen.as_dvec2() / world.get::<CanvasView>(handle.root)?.zoom);
    }
    let area = world.get::<InfluenceArea>(handle.area)?;
    let placement = crate::topology::spatial(world, handle.area);
    let ray = crate::topology::input::ray(world, screen)?;
    let center = placement.position(DVec2::from_array(area.center))
        - crate::topology::presentation::origin(world, handle.root);
    let normal = placement.rotation() * DVec3::Y;
    let denominator = normal.dot(ray.direction.as_dvec3());
    if denominator.abs() < 1e-6 {
        return None;
    }
    let distance = normal.dot(center - ray.origin.as_dvec3()) / denominator;
    if !distance.is_finite() || distance < 0.0 {
        return None;
    }
    let local = placement.rotation().inverse()
        * (ray.origin.as_dvec3() + ray.direction.as_dvec3() * distance - center);
    local.is_finite().then_some(DVec2::new(local.x, local.z))
}

fn draw(world: &mut World) {
    let old: Vec<_> = world
        .query::<(Entity, &Handle)>()
        .iter(world)
        .map(|(e, h)| (e, *h))
        .collect();
    for (entity, handle) in &old {
        if !allowed(world, *handle) {
            world.despawn(*entity);
        }
    }
    let roots: Vec<_> = world
        .query::<(Entity, &AreaEditor)>()
        .iter(world)
        .filter_map(|(root, e)| e.selected.map(|area| Handle { root, area }))
        .filter(|handle| allowed(world, *handle))
        .collect();
    for handle in roots {
        let Some(view) = world.get::<CanvasView>(handle.root) else {
            continue;
        };
        let Some(computed) = world.get::<ComputedNode>(handle.root) else {
            continue;
        };
        let size = computed.size() * computed.inverse_scale_factor();
        let position = projected(world, handle, size).unwrap_or(Vec2::splat(f32::NAN));
        let visible = position.is_finite()
            && view.zoom.is_finite()
            && view.zoom > 0.0
            && size.is_finite()
            && size.min_element() > 0.0
            && Rect::from_corners(Vec2::ZERO, size).contains(position);
        let position = if visible { position } else { Vec2::ZERO };
        let node = Node {
            position_type: PositionType::Absolute,
            left: px(position.x - 11.0),
            top: px(position.y - 11.0),
            width: px(22),
            height: px(22),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::MAX,
            display: if visible {
                Display::Flex
            } else {
                Display::None
            },
            ..default()
        };
        if let Some((entity, _)) = old.iter().find(|(e, h)| {
            h.root == handle.root && h.area == handle.area && world.get_entity(*e).is_ok()
        }) {
            if world.get::<Node>(*entity) != Some(&node) {
                world.entity_mut(*entity).insert(node);
            }
            continue;
        }
        let entity = world.spawn((
            node,
            handle,
            GlobalZIndex(3),
            crate::inspection::InspectionExcluded,
            crate::icons::Tooltip("Attraction / repulsion target. Drag to move it without changing the Area boundary or reach. Escape cancels. Offsets and reset are in the Area panel.".into()),
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
            ChildOf(handle.root),
        )).id();
        for (width, height) in [(10.0, 2.0), (2.0, 10.0)] {
            world.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px((18.0 - width) * 0.5),
                    top: px((18.0 - height) * 0.5),
                    width: px(width),
                    height: px(height),
                    ..default()
                },
                Pickable::IGNORE,
                crate::token_style::background(crate::tokens::Token::Accent),
                ChildOf(entity),
            ));
        }
    }
}

fn cancel(world: &mut World) {
    let Some(drag) = world.resource_mut::<Gesture>().0.take() else {
        return;
    };
    if let Some(mut area) = world.get_mut::<InfluenceArea>(drag.handle.area)
        && area.target == drag.last
    {
        area.target = drag.original.target;
    }
    if drag.original.target == AttractionTarget::Center && allowed(world, drag.handle) {
        refresh_panel(world, drag.handle.root);
    }
}

fn refresh_panel(world: &mut World, root: Entity) {
    let panel = world.get::<EditMode>(root).unwrap().panel;
    let scroll = world.get::<ScrollPosition>(panel).map(|scroll| scroll.0);
    crate::edit_mode::render_panel(world, root);
    if let Some(scroll) = scroll {
        world.entity_mut(panel).insert(ScrollPosition(scroll));
    }
}

fn input(
    world: &mut World,
    mut cursor: Local<MessageCursor<PointerInput>>,
    mut windows: Local<MessageCursor<WindowEvent>>,
) {
    let interrupted = windows
        .read(world.resource::<Messages<WindowEvent>>())
        .any(|event| {
            matches!(event, WindowEvent::WindowFocused(event) if !event.focused)
                || matches!(event, WindowEvent::CursorLeft(_))
        });
    let escape = world
        .resource::<ButtonInput<KeyCode>>()
        .just_pressed(KeyCode::Escape);
    let invalid = world.resource::<Gesture>().0.as_ref().is_some_and(|drag| {
        !allowed(world, drag.handle)
            || crate::topology::spatial(world, drag.handle.area) != drag.placement
            || camera(world, drag.handle.root) != drag.camera
            || world
                .get::<CanvasView>(drag.handle.root)
                .is_none_or(|view| view.center != drag.view.center || view.zoom != drag.view.zoom)
            || world
                .get::<InfluenceArea>(drag.handle.area)
                .is_none_or(|area| {
                    area.target != drag.last
                        || area.center != drag.original.center
                        || area.size != drag.original.size
                        || area.shape != drag.original.shape
                })
    });
    if interrupted || escape || invalid {
        cancel(world);
    }
    let mut events = world.remove_resource::<Messages<PointerInput>>().unwrap();
    for event in cursor.read_mut(&mut events) {
        if event.pointer_id != PointerId::Mouse {
            continue;
        }
        if interrupted || escape {
            continue;
        }
        if world.resource::<Gesture>().0.is_none() {
            if !matches!(event.action, PointerAction::Press(PointerButton::Primary))
                || world
                    .resource::<ButtonInput<KeyCode>>()
                    .any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
            {
                continue;
            }
            let hit = world
                .resource::<HoverMap>()
                .get(&PointerId::Mouse)
                .and_then(|hits| {
                    hits.iter()
                        .min_by(|(_, a), (_, b)| a.depth.total_cmp(&b.depth))
                        .map(|(e, _)| *e)
                });
            let Some(handle) = hit
                .and_then(|e| world.get::<Handle>(e))
                .copied()
                .filter(|h| allowed(world, *h))
            else {
                continue;
            };
            let Some(view) = world.get::<CanvasView>(handle.root).copied() else {
                continue;
            };
            if !view.zoom.is_finite() || view.zoom <= 0.0 || !event.location.position.is_finite() {
                continue;
            }
            let original = world.get::<InfluenceArea>(handle.area).unwrap().clone();
            let Some(start) = cursor_point(world, handle, event.location.position) else {
                continue;
            };
            let placement = crate::topology::spatial(world, handle.area);
            let camera = camera(world, handle.root);
            let last = AttractionTarget::Point(
                (original.target_position() - DVec2::from_array(original.center)).to_array(),
            );
            crate::area_mutation::disarm(
                world,
                handle.area,
                "Property changes inactive after a target edit. Configured changes resume automatically.",
            );
            world.get_mut::<InfluenceArea>(handle.area).unwrap().target = last;
            let changed_mode = original.target == AttractionTarget::Center;
            world.resource_mut::<Gesture>().0 = Some(Drag {
                handle,
                location: event.location.clone(),
                view,
                placement,
                camera,
                cursor: start,
                original,
                last,
            });
            if changed_mode {
                refresh_panel(world, handle.root);
            }
        } else {
            let drag = world.resource::<Gesture>().0.as_ref().unwrap();
            if event.location.target != drag.location.target
                || matches!(event.action, PointerAction::Cancel)
            {
                cancel(world);
            } else {
                if matches!(
                    event.action,
                    PointerAction::Move { .. } | PointerAction::Release(PointerButton::Primary)
                ) && let Some(point) = cursor_point(world, drag.handle, event.location.position)
                {
                    let offset = drag.original.target_position()
                        - DVec2::from_array(drag.original.center)
                        + point
                        - drag.cursor;
                    let target = AttractionTarget::Point(offset.to_array());
                    let mut area = world
                        .get::<InfluenceArea>(drag.handle.area)
                        .unwrap()
                        .clone();
                    area.target = target;
                    if area.validate()
                        && area != *world.get::<InfluenceArea>(drag.handle.area).unwrap()
                    {
                        let entity = drag.handle.area;
                        world.entity_mut(entity).insert(area);
                        world.resource_mut::<Gesture>().0.as_mut().unwrap().last = target;
                    }
                }
                if matches!(event.action, PointerAction::Release(PointerButton::Primary)) {
                    world.resource_mut::<Gesture>().0 = None;
                }
            }
        }
        event.action = PointerAction::Cancel;
        world
            .resource_mut::<bevy::input_focus::InputFocus>()
            .clear();
        for (id, mut press) in world
            .query::<(&PointerId, &mut PointerPress)>()
            .iter_mut(world)
        {
            if *id == PointerId::Mouse {
                *press = PointerPress::default();
            }
        }
    }
    world.insert_resource(events);
}

pub(crate) mod tests {
    use super::*;
    use crate::{actions::Action, area::AreaShape, edit_mode::EditAction};

    fn fixture(zoom: f64) -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .init_resource::<HoverMap>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_message::<PointerInput>()
            .add_message::<WindowEvent>()
            .add_plugins((
                crate::workspace::WorkspacePlugin,
                crate::edit_mode::EditModePlugin,
                AreaTargetPlugin,
            ));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        EditAction::Open.apply(app.world_mut(), root);
        EditAction::Areas.apply(app.world_mut(), root);
        app.world_mut().get_mut::<ComputedNode>(root).unwrap().size = Vec2::splat(1000.0);
        app.world_mut().get_mut::<CanvasView>(root).unwrap().zoom = zoom;
        let area = crate::area_panel::insert(
            app.world_mut(),
            root,
            InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(100.0)),
        )
        .unwrap();
        app.update();
        let handle = app
            .world_mut()
            .query_filtered::<Entity, With<Handle>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .resource_mut::<HoverMap>()
            .entry(PointerId::Mouse)
            .or_default()
            .insert(
                handle,
                bevy::picking::backend::HitData::new(root, 0.0, None, None),
            );
        (app, root, area, handle)
    }

    fn send(app: &mut App, root: Entity, action: PointerAction, position: Vec2) {
        app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: bevy::camera::RenderTarget::Window(bevy::window::WindowRef::Entity(root))
                    .normalize(None)
                    .unwrap(),
                position,
            },
            action,
        ));
        app.update();
    }

    #[cfg_attr(test, test)]
    fn target_drag_is_zoom_correct_consumes_input_and_does_not_move_the_boundary() {
        for zoom in [0.25, 1.0, 4.0] {
            let (mut app, root, area, handle) = fixture(zoom);
            let original = app.world().get::<InfluenceArea>(area).unwrap().clone();
            let panel = app.world().get::<EditMode>(root).unwrap().panel;
            app.world_mut()
                .get_mut::<ScrollPosition>(panel)
                .unwrap()
                .0
                .y = 120.0;
            let start = Vec2::new(507.0, 501.0);
            send(
                &mut app,
                root,
                PointerAction::Press(PointerButton::Primary),
                start,
            );
            assert_eq!(app.world().get::<ScrollPosition>(panel).unwrap().0.y, 120.0);
            let end = start + Vec2::new(40.0, -20.0);
            send(
                &mut app,
                root,
                PointerAction::Move { delta: end - start },
                end,
            );
            send(
                &mut app,
                root,
                PointerAction::Release(PointerButton::Primary),
                end,
            );
            let changed = app.world().get::<InfluenceArea>(area).unwrap();
            assert_eq!(changed.center, original.center);
            assert_eq!(changed.size, original.size);
            assert_eq!(changed.depth, original.depth);
            assert_eq!(changed.reach, original.reach);
            assert_eq!(changed.target_position(), DVec2::new(40.0, -20.0) / zoom);
            assert_eq!(app.world().get::<Node>(handle).unwrap().left, px(529.0));
            assert!(app.world().resource::<Gesture>().0.is_none());
            let messages = app.world().resource::<Messages<PointerInput>>();
            assert!(
                messages
                    .get_cursor()
                    .read(messages)
                    .all(|event| matches!(event.action, PointerAction::Cancel))
            );
            app.world_mut().clear_trackers();
            draw(app.world_mut());
            assert!(
                !app.world()
                    .entity(handle)
                    .get_ref::<Node>()
                    .unwrap()
                    .is_changed()
            );
        }
    }

    #[cfg_attr(test, test)]
    fn target_drag_cancels_on_escape_and_workspace_switch_and_stale_handles_cannot_edit() {
        let (mut app, root, area, handle) = fixture(1.0);
        let start = Vec2::splat(500.0);
        send(
            &mut app,
            root,
            PointerAction::Press(PointerButton::Primary),
            start,
        );
        send(
            &mut app,
            root,
            PointerAction::Move {
                delta: Vec2::X * 100.0,
            },
            start + Vec2::X * 100.0,
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(
            app.world().get::<InfluenceArea>(area).unwrap().target,
            AttractionTarget::Center
        );
        assert!(app.world().resource::<Gesture>().0.is_none());
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        EditAction::Areas.apply(app.world_mut(), root);
        send(
            &mut app,
            root,
            PointerAction::Press(PointerButton::Primary),
            start,
        );
        send(
            &mut app,
            root,
            PointerAction::Move {
                delta: Vec2::Y * 40.0,
            },
            start + Vec2::Y * 40.0,
        );
        app.world_mut()
            .get_mut::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active = 2;
        app.update();
        assert_eq!(
            app.world().get::<InfluenceArea>(area).unwrap().target,
            AttractionTarget::Center
        );
        assert!(app.world().get_entity(handle).is_err());
        send(
            &mut app,
            root,
            PointerAction::Press(PointerButton::Primary),
            start,
        );
        assert!(app.world().resource::<Gesture>().0.is_none());
        assert_eq!(
            app.world().get::<InfluenceArea>(area).unwrap().target,
            AttractionTarget::Center
        );
    }

    crate::laboratory_cases! {
        spatial_target_handles_project_and_drag_on_the_rotated_area_plane,
        target_drag_is_zoom_correct_consumes_input_and_does_not_move_the_boundary,
        target_drag_cancels_on_escape_and_workspace_switch_and_stale_handles_cannot_edit,
    }

    #[cfg_attr(test, test)]
    fn spatial_target_handles_project_and_drag_on_the_rotated_area_plane() {
        use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
        for perspective in [false, true] {
            let (mut app, root, area, handle) = fixture(1.0);
            let transform = if perspective {
                Transform::from_xyz(0.0, 450.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y)
            } else {
                Transform::from_xyz(0.0, 500.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z)
            };
            let clip_from_view = if perspective {
                PerspectiveProjection {
                    aspect_ratio: 1.0,
                    ..default()
                }
                .get_clip_from_view()
            } else {
                OrthographicProjection {
                    area: Rect::new(-500.0, -500.0, 500.0, 500.0),
                    ..OrthographicProjection::default_3d()
                }
                .get_clip_from_view()
            };
            let camera = app
                .world_mut()
                .spawn((
                    Camera {
                        computed: ComputedCameraValues {
                            clip_from_view,
                            target_info: Some(RenderTargetInfo {
                                physical_size: UVec2::splat(1000),
                                scale_factor: 1.0,
                            }),
                            ..default()
                        },
                        ..default()
                    },
                    GlobalTransform::from(transform),
                ))
                .id();
            app.world_mut()
                .insert_resource(crate::topology::presentation::SceneCamera(camera));
            app.world_mut()
                .entity_mut(root)
                .insert(crate::topology::presentation::SpatialRoot);
            let placement = crate::topology::Spatial {
                elevation: 35.0,
                rotation: bevy::math::DQuat::from_rotation_z(0.4).to_array(),
                ..default()
            };
            app.world_mut().entity_mut(area).insert(placement);
            app.world_mut()
                .get_mut::<InfluenceArea>(area)
                .unwrap()
                .target = AttractionTarget::Point([10.0, 15.0]);
            app.update();
            let screen = |world: &World, offset: DVec2| {
                let point = placement.position(DVec2::ZERO)
                    + placement.rotation() * DVec3::new(offset.x, 0.0, offset.y);
                world
                    .get::<Camera>(camera)
                    .unwrap()
                    .world_to_viewport(
                        world.get::<GlobalTransform>(camera).unwrap(),
                        point.as_vec3(),
                    )
                    .unwrap()
            };
            let start = screen(app.world(), DVec2::new(10.0, 15.0));
            let node = app.world().get::<Node>(handle).unwrap();
            assert_eq!(node.left, px(start.x - 11.0));
            assert_eq!(node.top, px(start.y - 11.0));
            let end = screen(app.world(), DVec2::new(40.0, -10.0));
            send(
                &mut app,
                root,
                PointerAction::Press(PointerButton::Primary),
                start,
            );
            send(
                &mut app,
                root,
                PointerAction::Move { delta: end - start },
                end,
            );
            send(
                &mut app,
                root,
                PointerAction::Release(PointerButton::Primary),
                end,
            );
            let saved = app.world().get::<InfluenceArea>(area).unwrap();
            assert!((saved.target_position() - DVec2::new(40.0, -10.0)).length() < 0.001);
            assert_eq!(saved.center, [0.0; 2]);
            assert_eq!(saved.depth, 100.0);
            assert_eq!(crate::topology::spatial(app.world(), area), placement);
            let original = saved.target;
            send(
                &mut app,
                root,
                PointerAction::Press(PointerButton::Primary),
                end,
            );
            send(
                &mut app,
                root,
                PointerAction::Move { delta: start - end },
                start,
            );
            app.world_mut()
                .get_mut::<crate::topology::Spatial>(area)
                .unwrap()
                .elevation = 60.0;
            app.update();
            assert!(app.world().resource::<Gesture>().0.is_none());
            assert_eq!(
                app.world().get::<InfluenceArea>(area).unwrap().target,
                original
            );
        }
    }
}
