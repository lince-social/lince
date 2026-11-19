use super::*;
use bevy::core_pipeline::prepass::DepthPrepass;

#[derive(Component)]
pub(super) struct VolumeDepth;

pub(super) fn source(
    source: &str,
    settings: &nucleus::shader_effect::spatial::VolumeSettings,
) -> String {
    format!(
        "{}\n{}\n{}\nconst LINCE_VOLUME_STEPS: u32 = {}u;",
        include_str!("volume.wesl"),
        header(),
        source,
        settings.quality.steps()
    )
}

pub(super) fn preview(world: &mut World, camera: Entity, enabled: bool) {
    if camera == Entity::PLACEHOLDER {
        return;
    }
    if enabled && world.get::<DepthPrepass>(camera).is_none() {
        world.entity_mut(camera).insert((DepthPrepass, VolumeDepth));
    } else if !enabled && world.get::<VolumeDepth>(camera).is_some() {
        world
            .entity_mut(camera)
            .remove::<(DepthPrepass, VolumeDepth)>();
    }
}

pub(super) fn world_views(world: &mut World) {
    world
        .run_system_cached(enable_world_views)
        .expect("prepare current volume view depth");
}

fn enable_world_views(
    world: &mut World,
    cameras: &mut QueryState<
        Entity,
        (
            Without<DepthPrepass>,
            Or<(
                With<crate::native::topology::presentation::WorldCamera>,
                With<crate::native::topology::output_views::WorldView>,
            )>,
        ),
    >,
    mut targets: Local<Vec<Entity>>,
) {
    let _prepasses = nucleus::performance::Span::new("shader-volume-prepass-maintenance");
    targets.clear();
    targets.extend(cameras.iter(world));
    if targets.is_empty() {
        *targets = Vec::new();
        return;
    }
    for camera in targets.drain(..) {
        preview(world, camera, true);
    }
}

pub(super) fn prune(world: &mut World) {
    world
        .run_system_cached(prune_world_views)
        .expect("retire unused volume view depth");
}

fn prune_world_views(
    world: &mut World,
    instances: &mut QueryState<(&'static InstanceOf, &'static Visibility), With<WorldInstance>>,
    cameras: &mut QueryState<
        Entity,
        (
            With<VolumeDepth>,
            Or<(
                With<crate::native::topology::presentation::WorldCamera>,
                With<crate::native::topology::output_views::WorldView>,
            )>,
        ),
    >,
    mut targets: Local<Vec<Entity>>,
) {
    let _prepasses = nucleus::performance::Span::new("shader-volume-prepass-maintenance");
    targets.clear();
    let disabled = crate::native::shader_effects::disabled(world);
    let active = !disabled
        && instances.iter(world).any(|(owner, visible)| {
            *visible == Visibility::Visible
                && world
                    .resource::<MeshEffects>()
                    .0
                    .get(&owner.0)
                    .is_some_and(|state| {
                        state
                            .working
                            .iter()
                            .chain(state.candidate.iter())
                            .any(|bundle| matches!(bundle.effect.mode, Mode::Volume(_)))
                    })
        });
    if !active {
        targets.extend(cameras.iter(world));
        if targets.is_empty() {
            *targets = Vec::new();
            return;
        }
        for camera in targets.drain(..) {
            preview(world, camera, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_depth_cleanup_preserves_preexisting_camera_prepasses() {
        let mut world = World::new();
        world.init_resource::<MeshEffects>();
        let borrowed = world
            .spawn((
                crate::native::topology::presentation::WorldCamera,
                DepthPrepass,
            ))
            .id();
        let owned = world
            .spawn(crate::native::topology::presentation::WorldCamera)
            .id();
        world_views(&mut world);
        assert!(world.get::<DepthPrepass>(borrowed).is_some());
        assert!(world.get::<DepthPrepass>(owned).is_some());
        prune(&mut world);
        assert!(world.get::<DepthPrepass>(borrowed).is_some());
        assert!(world.get::<DepthPrepass>(owned).is_none());
        assert!(world.get::<VolumeDepth>(borrowed).is_none());
        assert!(world.get::<VolumeDepth>(owned).is_none());
    }
}
