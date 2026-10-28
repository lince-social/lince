use super::*;

#[derive(Component)]
struct Readout(Entity);

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    crate::native::edit_mode::label(world, parent, "Rendering", 16.0);
    let message = status(world, root);
    let label = crate::native::edit_mode::label(world, parent, &message, 12.0);
    world
        .entity_mut(label)
        .insert((Readout(root), Name::new("rendering.diagnostics")));
}

fn status(world: &mut World, root: Entity) -> String {
    let Some(budget) = world.get_resource::<Diagnostics>() else {
        return "Preparing rendering diagnostics…".into();
    };
    let mut message = format!(
        "Capture/shader target grants: {:.1} / {} MiB · {} reduced targets · device extent limit {}",
        budget.granted_bytes as f64 / (1024.0 * 1024.0),
        BYTE_BUDGET / (1024 * 1024),
        budget.limited_targets,
        budget.maximum_extent
    );
    if let Some(usage) = world.get_resource::<residency::Usage>() {
        let usage = usage.snapshot();
        let mib = |bytes| bytes as f64 / (1024.0 * 1024.0);
        message.push_str(&format!(
            "\nObserved offscreen textures: {:.1} MiB current, {:.1} MiB retained images, {:.1} MiB pending GPU, {:.1} MiB estimated cache. Peak: {:.1} MiB. Currently tracking {} textures.{}",
            mib(usage.current_bytes),
            mib(usage.held_bytes),
            mib(usage.inflight_bytes),
            mib(usage.cached_estimate_bytes),
            mib(usage.peak_bytes),
            usage.textures,
            if usage.lower_bound { " Partial measurement." } else { "" },
        ));
        message.push_str(
            "\nThese estimates cover native capture, shader and Fiote preview targets; client/output buffers and other scene resources are separate.",
        );
    }
    if let Some(entity) = crate::native::layout::desktop::focused_entity(world, root)
        && let Some(raster) = world.get::<NativeRaster>(entity)
    {
        message.push_str(&format!(
            "\nFocused capture target: {}×{} pixels; requested {}×{}{}.",
            raster.pixels.x,
            raster.pixels.y,
            raster.requested.x,
            raster.requested.y,
            reason(*raster)
        ));
    }
    message
}

fn reason(raster: NativeRaster) -> &'static str {
    match (raster.device_limited, raster.budget_limited) {
        (true, true) => "; reduced by device and target budget limits",
        (true, false) => "; reduced by the device extent limit",
        (false, true) => "; reduced by the target budget",
        (false, false) => "",
    }
}

pub(crate) fn reduction(world: &World, mut entity: Entity) -> Option<String> {
    for _ in 0..128 {
        if let Some(raster) = world
            .get::<NativeRaster>(entity)
            .filter(|raster| raster.device_limited || raster.budget_limited)
        {
            return Some(format!(
                "Sand capture {}×{} of {}×{} pixels{}",
                raster.pixels.x,
                raster.pixels.y,
                raster.requested.x,
                raster.requested.y,
                reason(*raster)
            ));
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
    None
}

pub(super) fn update(world: &mut World) {
    let readouts: Vec<_> = world
        .query::<(Entity, &Readout)>()
        .iter(world)
        .map(|(entity, readout)| (entity, readout.0))
        .collect();
    for (entity, root) in readouts {
        let message = status(world, root);
        if let Some(mut text) = world.get_mut::<Text>(entity)
            && text.0 != message
        {
            text.0 = message;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_refresh_preserves_the_settings_entities_and_keyboard_owner() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<Diagnostics>();
        let root = world.spawn_empty().id();
        let field = world.spawn_empty().id();
        world.insert_resource(bevy::input_focus::InputFocus::from_entity(field));
        panel(&mut world, root, root);
        let children: Vec<_> = world.get::<Children>(root).unwrap().iter().collect();
        let label = world
            .query_filtered::<Entity, With<Readout>>()
            .single(&world)
            .unwrap();
        world.resource_mut::<Diagnostics>().limited_targets = 2;
        update(&mut world);
        assert!(
            world
                .get::<Text>(label)
                .unwrap()
                .0
                .contains("2 reduced targets")
        );
        assert_eq!(
            world
                .get::<Children>(root)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            children
        );
        assert_eq!(
            world.resource::<bevy::input_focus::InputFocus>().get(),
            Some(field)
        );
        world.resource_mut::<Diagnostics>().limited_targets = 0;
        update(&mut world);
        assert!(
            world
                .get::<Text>(label)
                .unwrap()
                .0
                .contains("0 reduced targets")
        );
        assert_eq!(world.query::<&Readout>().iter(&world).count(), 1);
    }

    #[test]
    fn native_capture_reduction_is_reported_from_descendants_and_clears_on_recovery() {
        let mut world = World::new();
        let surface = world
            .spawn(NativeRaster {
                requested: UVec2::new(900, 450),
                pixels: UVec2::new(450, 225),
                device_limited: false,
                budget_limited: true,
            })
            .id();
        let child = world.spawn(ChildOf(surface)).id();
        let message = reduction(&world, child).unwrap();
        assert!(message.contains("450×225 of 900×450"));
        assert!(message.contains("target budget"));
        world
            .get_mut::<NativeRaster>(surface)
            .unwrap()
            .budget_limited = false;
        assert!(reduction(&world, child).is_none());
        world
            .get_mut::<NativeRaster>(surface)
            .unwrap()
            .device_limited = true;
        assert!(reduction(&world, child).unwrap().contains("device extent"));
        world.despawn(surface);
        assert!(reduction(&world, child).is_none());
    }
}
