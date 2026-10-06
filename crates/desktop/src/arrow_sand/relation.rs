use super::*;

#[derive(Resource)]
struct Assets {
    mesh: Handle<Mesh>,
    materials: std::collections::HashMap<[u8; 4], Handle<StandardMaterial>>,
}

#[derive(Component)]
struct Lines([Entity; 3]);

pub(super) fn endpoints(world: &World, from: Entity, to: Entity) -> Option<(DVec3, DVec3)> {
    if from == to {
        return None;
    }
    for entity in [from, to] {
        let item = world.get::<CanvasItem>(entity)?;
        if !item.size.is_finite()
            || item.size.min_element() <= 0.0
            || !crate::topology::spatial(world, entity).valid()
        {
            return None;
        }
    }
    let first = crate::topology::position(world, from)?;
    let second = crate::topology::position(world, to)?;
    (first.is_finite() && second.is_finite() && first.distance_squared(second) > 1e-8)
        .then_some((first, second))
}

fn line_transform(from: DVec3, to: DVec3) -> Transform {
    let delta = to - from;
    Transform {
        translation: ((from + to) * 0.5).as_vec3(),
        rotation: DQuat::from_rotation_arc(DVec3::X, delta.normalize_or_zero()).as_quat(),
        scale: Vec3::new(delta.length() as f32, 0.02, 2.0),
    }
}

pub(super) fn update(
    world: &mut World,
    entity: Entity,
    root: Entity,
    arrow: &ArrowSand,
    from: DVec3,
    to: DVec3,
) {
    if !world.contains_resource::<Assets>() {
        let mesh = world
            .resource_mut::<bevy::asset::Assets<Mesh>>()
            .add(Cuboid::default());
        world.insert_resource(Assets {
            mesh,
            materials: default(),
        });
    }
    let color = crate::token_style::resolve(world, entity, crate::tokens::Token::Accent).0;
    let crate::tokens::TokenValue::Color([r, g, b, a]) = color else {
        return;
    };
    let rgba = [r, g, b, a];
    let material = if let Some(material) = world.resource::<Assets>().materials.get(&rgba) {
        material.clone()
    } else {
        let material = world
            .resource_mut::<bevy::asset::Assets<StandardMaterial>>()
            .add(StandardMaterial {
                base_color: Color::srgba_u8(r, g, b, a),
                unlit: true,
                alpha_mode: if a == 255 {
                    AlphaMode::Opaque
                } else {
                    AlphaMode::Blend
                },
                ..default()
            });
        world
            .resource_mut::<Assets>()
            .materials
            .insert(rgba, material.clone());
        material
    };
    if world.get::<Lines>(entity).is_none() {
        let mesh = world.resource::<Assets>().mesh.clone();
        let lines = std::array::from_fn(|_| {
            world
                .spawn((
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::default(),
                    Pickable::IGNORE,
                    ChildOf(entity),
                ))
                .id()
        });
        world.entity_mut(entity).insert((
            Lines(lines),
            Transform::default(),
            Visibility::default(),
        ));
    }
    let visible = world
        .get::<crate::workspace::Workspaces>(root)
        .is_some_and(|spaces| {
            world
                .get::<WorkspaceMember>(entity)
                .is_some_and(|member| member.0 == spaces.active)
        })
        && world
            .get::<bevy::ecs::entity_disabling::Disabled>(root)
            .is_none()
        && world
            .get::<bevy::ecs::entity_disabling::Disabled>(entity)
            .is_none();
    world
        .get_mut::<Visibility>(entity)
        .unwrap()
        .set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    let center = (from + to) * 0.5;
    let origin = crate::topology::presentation::origin(world, root);
    world
        .get_mut::<Transform>(entity)
        .unwrap()
        .set_if_neq(Transform::from_translation((center - origin).as_vec3()));
    let item = CanvasItem {
        position: DVec2::new(center.x, center.z),
        size: Vec2::new(180.0, 32.0),
    };
    if world
        .get::<CanvasItem>(entity)
        .is_none_or(|old| old.position != item.position || old.size != item.size)
    {
        world.entity_mut(entity).insert(item);
    }
    let spatial = Spatial {
        elevation: center.y,
        depth: Some(0.01),
        ..default()
    };
    if world.get::<Spatial>(entity) != Some(&spatial) {
        world.entity_mut(entity).insert(spatial);
    }
    let direction = (to - from).normalize();
    let target = crate::topology::spatial(world, arrow.to);
    let local = target.rotation().inverse() * direction;
    let size = world.get::<CanvasItem>(arrow.to).unwrap().size.as_dvec2()
        * f64::from(
            world
                .get::<crate::area_effects::AreaScale>(arrow.to)
                .map_or(1.0, |scale| scale.0),
        )
        * 0.5;
    let boundary = (size.x / local.x.abs()).min(size.y / local.z.abs());
    let tip = to - direction * boundary.min(from.distance(to) * 0.5);
    let side = direction.cross(target.rotation() * DVec3::Y);
    let side = if side.length_squared() > 1e-8 {
        side.normalize()
    } else {
        direction.cross(target.rotation() * DVec3::X).normalize()
    };
    let offset = DVec3::new(0.0, -0.05, 0.0);
    let lines = world.get::<Lines>(entity).unwrap().0;
    for (line, (a, b)) in lines.into_iter().zip([
        (from, to),
        (tip, tip - direction * 12.0 + side * 7.0),
        (tip, tip - direction * 12.0 - side * 7.0),
    ]) {
        world
            .get_mut::<Transform>(line)
            .unwrap()
            .set_if_neq(line_transform(a - center + offset, b - center + offset));
        world
            .get_mut::<MeshMaterial3d<StandardMaterial>>(line)
            .unwrap()
            .set_if_neq(MeshMaterial3d(material.clone()));
    }
    let parts = world.get::<Parts>(entity).unwrap();
    let (shaft, tips, label) = (parts.shaft, parts.tips, parts.label);
    for line in [shaft, tips[0], tips[1]] {
        if world.get::<Node>(line).unwrap().display != Display::None {
            world.get_mut::<Node>(line).unwrap().display = Display::None;
        }
    }
    world
        .get_mut::<Text>(label)
        .unwrap()
        .set_if_neq(Text::new(&arrow.label));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_links_follow_centers_and_dragging_without_resizing_surfaces() {
        let mut world = World::new();
        world.init_resource::<bevy::asset::Assets<Mesh>>();
        world.init_resource::<bevy::asset::Assets<StandardMaterial>>();
        world.init_resource::<bevy::asset::Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<crate::tokens::ThemeSettings>();
        let root = world.spawn(crate::workspace::Workspaces::default()).id();
        let first = super::super::tests::record(
            &mut world,
            root,
            DVec2::new(500.0, 0.0),
            Vec2::new(200.0, 80.0),
        );
        let second =
            super::super::tests::record(&mut world, root, DVec2::ZERO, Vec2::new(200.0, 80.0));
        let entity = spawn(&mut world, first, second, "depends-on".into()).unwrap();
        world
            .entity_mut(entity)
            .insert(crate::relation_castle::RelationLink {
                owner: root,
                uid: "link".into(),
            });
        for position in [
            DVec2::new(500.0, 0.0),
            DVec2::new(500.0, 300.0),
            DVec2::new(80.0, 200.0),
            DVec2::new(0.0, 500.0),
        ] {
            world.get_mut::<CanvasItem>(first).unwrap().position = position;
            super::super::update(&mut world);
            let (from, to) = endpoints(&world, first, second).unwrap();
            assert_eq!(from, DVec3::new(position.x, 0.0, position.y));
            assert_eq!(to, DVec3::ZERO);
            assert_eq!(
                world.get::<CanvasItem>(entity).unwrap().size,
                Vec2::new(180.0, 32.0)
            );
            let shaft = world.get::<Lines>(entity).unwrap().0[0];
            let transform = world.get::<Transform>(shaft).unwrap();
            let parent = world.get::<Transform>(entity).unwrap();
            let a = parent.translation + transform.transform_point(Vec3::new(-0.5, 0.0, 0.0));
            let b = parent.translation + transform.transform_point(Vec3::new(0.5, 0.0, 0.0));
            assert!((a - (from.as_vec3() + Vec3::new(0.0, -0.05, 0.0))).length() < 0.001);
            assert!((b - (to.as_vec3() + Vec3::new(0.0, -0.05, 0.0))).length() < 0.001);
            let lines = world.get::<Lines>(entity).unwrap().0;
            assert_ne!(
                world.get::<Transform>(lines[1]),
                world.get::<Transform>(lines[2])
            );
        }
        world.get_mut::<CanvasItem>(first).unwrap().position = DVec2::ZERO;
        super::super::update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        world.get_mut::<CanvasItem>(first).unwrap().position.x = 500.0;
        world
            .get_mut::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active = 2;
        super::super::update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        world
            .get_mut::<crate::workspace::Workspaces>(root)
            .unwrap()
            .active = 1;
        world
            .entity_mut(entity)
            .insert(bevy::ecs::entity_disabling::Disabled);
        super::super::update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Hidden
        );
        world
            .entity_mut(entity)
            .remove::<bevy::ecs::entity_disabling::Disabled>();
        super::super::update(&mut world);
        assert_eq!(
            *world.get::<Visibility>(entity).unwrap(),
            Visibility::Inherited
        );
        world.despawn(first);
        super::super::update(&mut world);
        assert!(world.get_entity(entity).is_err());
    }
}
