use super::presentation::{WorldCamera, origin};
use bevy::{math::DVec3, prelude::*};
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub spatial: bool,
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub plane: f64,
    pub selection_depth: f64,
}

impl Default for View {
    fn default() -> Self {
        Self {
            spatial: false,
            position: [0.0, 500.0, 500.0],
            yaw: 0.0,
            pitch: -0.6,
            plane: 0.0,
            selection_depth: 1000.0,
        }
    }
}

impl View {
    pub fn valid(&self) -> bool {
        DVec3::from_array(self.position).is_finite()
            && self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.pitch.abs() < std::f32::consts::FRAC_PI_2
            && self.plane.is_finite()
            && self.selection_depth.is_finite()
            && self.selection_depth > 0.0
    }
}

pub fn synchronize(world: &mut World) {
    let Some((root, canvas)) = world
        .query::<(Entity, &crate::canvas::CanvasView)>()
        .iter(world)
        .find(|(e, _)| world.get::<crate::workspace::Workspaces>(*e).is_some())
        .map(|(e, c)| (e, *c))
    else {
        return;
    };
    let Some(camera) = world
        .query_filtered::<Entity, With<WorldCamera>>()
        .iter(world)
        .next()
    else {
        return;
    };
    if world.get::<View>(root).is_none() {
        world.entity_mut(root).insert(View {
            position: [canvas.center.x, 500.0, canvas.center.y + 500.0],
            ..default()
        });
    }
    let mut view = *world.get::<View>(root).unwrap();
    let before = DVec3::from_array(view.position);
    let focus = world
        .get_resource::<bevy::input_focus::InputFocus>()
        .and_then(|f| f.get());
    if view.spatial && focus.is_none_or(|e| world.get::<bevy::text::EditableText>(e).is_none()) {
        let keys = world.resource::<ButtonInput<KeyCode>>();
        let mut direction = Vec3::ZERO;
        for (key, delta) in [
            (KeyCode::KeyW, -Vec3::Z),
            (KeyCode::KeyS, Vec3::Z),
            (KeyCode::KeyA, -Vec3::X),
            (KeyCode::KeyD, Vec3::X),
            (KeyCode::KeyQ, -Vec3::Y),
            (KeyCode::KeyE, Vec3::Y),
        ] {
            if keys.pressed(key) {
                direction += delta;
            }
        }
        let dt = world
            .get_resource::<Time>()
            .map_or(0.0, |t| t.delta_secs().min(0.05));
        let turn = dt * 1.5;
        for (key, sign) in [(KeyCode::ArrowLeft, 1.0), (KeyCode::ArrowRight, -1.0)] {
            if keys.pressed(key) {
                view.yaw += sign * turn;
            }
        }
        for (key, sign) in [(KeyCode::ArrowUp, 1.0), (KeyCode::ArrowDown, -1.0)] {
            if keys.pressed(key) {
                view.pitch = (view.pitch + sign * turn).clamp(-1.5, 1.5);
            }
        }
        let rotation = Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0);
        let delta = (rotation * direction.normalize_or_zero() * dt * 400.0).as_dvec3();
        let position = DVec3::from_array(view.position);
        view.position = (position + delta).to_array();
        if delta != DVec3::ZERO
            || turn != 0.0
                && keys.any_pressed([
                    KeyCode::ArrowLeft,
                    KeyCode::ArrowRight,
                    KeyCode::ArrowUp,
                    KeyCode::ArrowDown,
                ])
        {
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.ring();
            }
        }
    }
    if view.spatial
        && !world
            .get::<crate::edit_mode::EditMode>(root)
            .is_some_and(|m| m.enabled)
    {
        let ready: std::collections::HashSet<_> = world
            .query::<&super::physics::Body>()
            .iter(world)
            .filter(|body| body.root == root)
            .flat_map(|body| body.members.iter().copied())
            .collect();
        let active = world
            .get::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active;
        let loading = world
            .query::<(
                Entity,
                &super::assets::ImportedAsset,
                &ChildOf,
                &crate::workspace::WorkspaceMember,
            )>()
            .iter(world)
            .any(|(entity, _, parent, member)| {
                parent.parent() == root && member.0 == active && !ready.contains(&entity)
            });
        if loading {
            view.position = before.to_array();
        }
        let clear = world.get::<Navigation>(root).and_then(|n| n.clear);
        let delta = DVec3::from_array(view.position) - before;
        let next = world
            .run_system_cached_with(collide, (before, delta, clear, Some(root)))
            .ok()
            .flatten();
        if let Some(next) = next {
            view.position = next.to_array();
            world.entity_mut(root).insert(Navigation {
                clear: Some(view.position),
            });
        } else {
            view.position = before.to_array();
            crate::actions::Action::apply(&crate::edit_mode::EditAction::Open, world, root);
            crate::notifications::report(
                world,
                "Topology",
                "No clear flight position. Move out of the object before leaving edit mode.",
            );
        }
    }
    world.get_mut::<View>(root).unwrap().set_if_neq(view);
    if view.spatial {
        let position = DVec3::from_array(view.position);
        if (position - origin(world, root)).length() > 10_000.0 {
            world
                .get_mut::<crate::canvas::CanvasView>(root)
                .unwrap()
                .center = bevy::math::DVec2::new(position.x, position.z);
        }
    }
    let transform = if view.spatial {
        Transform::from_translation(
            (DVec3::from_array(view.position) - origin(world, root)).as_vec3(),
        )
        .with_rotation(Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, 0.0))
    } else {
        Transform::from_xyz(0.0, 10000.0, 0.0).looking_at(Vec3::ZERO, -Vec3::Z)
    };
    world
        .get_mut::<Transform>(camera)
        .unwrap()
        .set_if_neq(transform);
    let update_projection = match world.get::<Projection>(camera) {
        Some(Projection::Perspective(projection)) => {
            !view.spatial || projection.near != 0.5 || projection.far != 1_000_000.0
        }
        Some(Projection::Orthographic(projection)) => {
            view.spatial
                || projection.scale != 1.0 / canvas.zoom as f32
                || projection.near != -1_000_000.0
                || projection.far != 1_000_000.0
        }
        _ => true,
    };
    if !update_projection {
        return;
    }
    let projection = if view.spatial {
        Projection::Perspective(PerspectiveProjection {
            near: 0.5,
            far: 1_000_000.0,
            ..default()
        })
    } else {
        Projection::Orthographic(OrthographicProjection {
            scale: 1.0 / canvas.zoom as f32,
            near: -1_000_000.0,
            far: 1_000_000.0,
            ..OrthographicProjection::default_3d()
        })
    };
    world.entity_mut(camera).insert(projection);
}

#[derive(Component, Default)]
struct Navigation {
    clear: Option<[f64; 3]>,
}

fn collide(
    In((position, delta, clear, root)): In<(DVec3, DVec3, Option<[f64; 3]>, Option<Entity>)>,
    movement: avian3d::character_controller::move_and_slide::MoveAndSlide,
    bodies: Query<(Entity, &super::physics::Body)>,
    colliders: Query<(Entity, &avian3d::prelude::ColliderOf)>,
) -> Option<DVec3> {
    use avian3d::{
        character_controller::move_and_slide::{
            DepenetrationConfig, MoveAndSlideConfig, MoveAndSlideHitResponse,
        },
        prelude::*,
    };
    let shape = Collider::sphere(8.0);
    let filter = SpatialQueryFilter::default().with_excluded_entities(
        colliders
            .iter()
            .filter(|(_, collider)| {
                root.is_some_and(|root| {
                    bodies
                        .get(collider.body)
                        .is_ok_and(|(_, body)| body.root != root)
                })
            })
            .map(|(entity, _)| entity),
    );
    let recovered = position
        + movement.depenetrate(
            &shape,
            position,
            bevy::math::DQuat::IDENTITY,
            &DepenetrationConfig::default(),
            &filter,
        );
    let start = if movement
        .spatial_query
        .shape_intersections(&shape, recovered, bevy::math::DQuat::IDENTITY, &filter)
        .is_empty()
    {
        recovered
    } else {
        let clear = DVec3::from_array(clear?);
        if !movement
            .spatial_query
            .shape_intersections(&shape, clear, bevy::math::DQuat::IDENTITY, &filter)
            .is_empty()
        {
            return None;
        }
        clear
    };
    Some(
        movement
            .move_and_slide(
                &shape,
                start,
                bevy::math::DQuat::IDENTITY,
                delta,
                std::time::Duration::from_secs(1),
                &MoveAndSlideConfig::default(),
                &filter,
                |_| MoveAndSlideHitResponse::Accept,
            )
            .position,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use avian3d::prelude::*;

    #[test]
    fn flight_stops_at_triangles_and_preserves_open_space() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin, PhysicsPlugins::default()))
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_millis(40),
            ));
        app.finish();
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for (left, right) in [(-100.0, -20.0), (20.0, 100.0)] {
            let base = vertices.len() as u32;
            vertices.extend([
                DVec3::new(left, -100.0, 0.0),
                DVec3::new(right, -100.0, 0.0),
                DVec3::new(right, 100.0, 0.0),
                DVec3::new(left, 100.0, 0.0),
            ]);
            indices.extend([[base, base + 1, base + 2], [base, base + 2, base + 3]]);
        }
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::trimesh(vertices, indices),
            Transform::default(),
        ));
        for _ in 0..4 {
            app.update();
        }
        let travel = DVec3::new(0.0, 0.0, -200.0);
        let open = app
            .world_mut()
            .run_system_cached_with(collide, (DVec3::new(0.0, 0.0, 100.0), travel, None, None))
            .unwrap()
            .unwrap();
        assert!(open.z < -90.0, "{open:?}");
        let blocked = app
            .world_mut()
            .run_system_cached_with(collide, (DVec3::new(50.0, 0.0, 100.0), travel, None, None))
            .unwrap()
            .unwrap();
        assert!(blocked.z >= 7.5, "{blocked:?}");
    }
}
