use super::*;

fn active(world: &World, owner: Entity, surface: &super::super::presentation::Surface) -> bool {
    use bevy::ecs::entity_disabling::Disabled;
    if world
        .get::<crate::native::sand_placement::Pinned>(owner)
        .is_some()
        || world
            .get::<Node>(owner)
            .is_some_and(|node| node.display == Display::None)
        || world
            .get::<Visibility>(owner)
            .is_some_and(|visibility| *visibility == Visibility::Hidden)
    {
        return false;
    }
    let root = world.get::<ChildOf>(owner).map(ChildOf::parent);
    if root.is_some_and(|root| world.get::<Disabled>(root).is_some()) {
        return false;
    }
    root.zip(world.get::<crate::native::workspace::WorkspaceMember>(owner))
        .map_or(surface.visible, |(root, member)| {
            super::super::holes::live(world, root, member.0)
                && world
                    .get::<crate::native::layout::desktop::Suppressed>(owner)
                    .is_none()
        })
}

pub(super) fn synchronize(world: &mut World, runtime: &mut Runtime) -> (HashSet<Entity>, usize) {
    let mut candidates: Vec<_> = world
        .query::<(Entity, &super::super::presentation::Surface)>()
        .iter(world)
        .filter(|(owner, surface)| active(world, *owner, surface))
        .map(|(owner, surface)| (owner, surface.visual, surface.visible))
        .collect();
    candidates.sort_by_key(|(owner, _, visible)| (!*visible, owner.to_bits()));
    let mut lights = HashSet::new();
    let mut shadow_count = 0;
    let mut shadow_size = 0;
    for (owner, visual, _) in candidates {
        let opacity = opacity(world, owner);
        let Some(settings) = world
            .get::<Light>(owner)
            .map(|value| value.0.clone())
            .filter(|value| value.valid() && value.enabled)
        else {
            continue;
        };
        if lights.len() >= model::MAX_LIGHTS || crate::native::shader_effects::disabled(world) {
            continue;
        }
        let factor = strength(world, owner, settings.drive) * opacity;
        if factor <= 0.0 || settings.intensity <= 0.0 {
            continue;
        }
        lights.insert(owner);
        let shadow = settings
            .shadows
            .resolution()
            .filter(|_| shadow_count < model::MAX_SHADOW_LIGHTS);
        if let Some(size) = shadow {
            shadow_count += 1;
            shadow_size = shadow_size.max(size as usize);
        }
        if runtime
            .lights
            .get(&owner)
            .is_some_and(|entity| world.get::<PointLight>(*entity).is_none())
        {
            runtime.lights.remove(&owner);
        }
        let entity = *runtime.lights.entry(owner).or_insert_with(|| {
            world
                .spawn((
                    PointLight::default(),
                    Transform::default(),
                    Visibility::Visible,
                    super::super::presentation::VisualOwner(owner),
                ))
                .id()
        });
        let [r, g, b] = settings.color;
        let desired = PointLight {
            color: Color::srgb(r, g, b),
            intensity: settings.intensity * factor * CANVAS_UNITS_PER_METER.powi(2),
            range: settings.range,
            radius: 1.0,
            shadow_maps_enabled: shadow.is_some(),
            shadow_map_near_z: 1.0,
            ..default()
        };
        let current = world.get::<PointLight>(entity).unwrap();
        if current.color != desired.color
            || current.intensity != desired.intensity
            || current.range != desired.range
            || current.shadow_maps_enabled != desired.shadow_maps_enabled
        {
            *world.get_mut::<PointLight>(entity).unwrap() = desired;
        }
        let position = world
            .get::<Transform>(visual)
            .unwrap()
            .transform_point(bevy::math::DVec3::from_array(settings.position).as_vec3());
        world
            .get_mut::<Transform>(entity)
            .unwrap()
            .set_if_neq(Transform::from_translation(position));
    }
    (lights, shadow_size)
}
