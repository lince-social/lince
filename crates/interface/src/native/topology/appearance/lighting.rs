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

pub(super) fn synchronize(
    world: &mut World,
    runtime: &mut Runtime,
    sources: &mut QueryState<(Entity, &'static super::super::presentation::Surface), With<Light>>,
    scratch: &mut preparation::Lights,
) -> usize {
    let _lighting = nucleus::performance::Span::new("appearance-lighting");
    scratch.candidates.clear();
    scratch.live.clear();
    if crate::native::shader_effects::disabled(world) {
        *scratch = preparation::Lights::default();
        return 0;
    }
    scratch.candidates.extend(
        sources
            .iter(world)
            .filter(|(owner, surface)| active(world, *owner, surface))
            .map(|(owner, surface)| (owner, surface.visual, surface.visible)),
    );
    nucleus::performance::add(
        "appearance-light-candidates",
        scratch.candidates.len() as u64,
    );
    if scratch.candidates.is_empty() {
        *scratch = preparation::Lights::default();
        return 0;
    }
    let preparation::Lights {
        candidates,
        live: lights,
    } = scratch;
    candidates.sort_by_key(|(owner, _, visible)| (!*visible, owner.to_bits()));
    let mut shadow_count = 0;
    let mut shadow_size = 0;
    for (owner, visual, _) in candidates.drain(..) {
        if lights.len() >= model::MAX_LIGHTS {
            break;
        }
        let Some(settings) = world
            .get::<Light>(owner)
            .map(|value| &value.0)
            .filter(|value| value.valid() && value.enabled)
        else {
            continue;
        };
        if crate::native::shader_effects::disabled(world) {
            continue;
        }
        let opacity = opacity(world, owner);
        let factor = strength(world, owner, settings.drive) * opacity;
        if factor <= 0.0 || settings.intensity <= 0.0 {
            continue;
        }
        let settings = model::Light::clone(settings);
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
        let color = Color::srgb(r, g, b);
        let intensity = settings.intensity * factor * CANVAS_UNITS_PER_METER.powi(2);
        let current = world.get::<PointLight>(entity).unwrap();
        if current.color != color
            || current.intensity != intensity
            || current.range != settings.range
            || current.shadow_maps_enabled != shadow.is_some()
        {
            *world.get_mut::<PointLight>(entity).unwrap() = PointLight {
                color,
                intensity,
                range: settings.range,
                radius: 1.0,
                shadow_maps_enabled: shadow.is_some(),
                shadow_map_near_z: 1.0,
                ..default()
            };
            nucleus::performance::add("appearance-light-component-writes", 1);
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
    nucleus::performance::add("appearance-lights-admitted", lights.len() as u64);
    shadow_size
}
