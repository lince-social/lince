// non-lean-yet
// Scope: struct Overlay, fn unmark, fn release, fn update.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 49, 53, 60, 114,
//   117, 132, 134, 138.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (44, 49, 73, 112, 136, 137, 142, 146).

use super::*;
use bevy::ui_render::SuppressUiImage;

pub(super) struct Overlay {
    entity: Entity,
    material: Handle<StandardMaterial>,
    output: Entity,
    last: Option<(Arc<model::Material>, u32, Handle<Image>)>,
}

fn unmark(world: &mut World, owner: Entity, output: Entity) {
    if world
        .get::<LitEffectOutput>(output)
        .is_some_and(|marker| marker.0 == owner)
    {
        world.entity_mut(output).remove::<(LitEffectOutput, SuppressUiImage)>();
    }
}

pub(super) fn release(world: &mut World, owner: Entity, overlay: &mut Option<Overlay>) {
    if let Some(overlay) = overlay.take() {
        unmark(world, owner, overlay.output);
        if let Ok(entity) = world.get_entity_mut(overlay.entity) {
            entity.despawn();
        }
        world
            .resource_mut::<Assets<StandardMaterial>>()
            .remove(overlay.material.id());
    }
}

pub(super) fn update(
    world: &mut World,
    owner: Entity,
    painted: &mut Painted,
    settings: &Arc<model::Material>,
    opacity: f32,
) {
    let _presentation = nucleus::performance::Span::new("appearance-overlay-presentation");
    let Some((output, image)) = crate::native::shader_effects::surface_image(world, owner) else {
        release(world, owner, &mut painted.overlay);
        return;
    };
    let visual = world.get::<ChildOf>(painted.plate).unwrap().parent();
    if painted.overlay.is_none() {
        let material = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let mesh = world.get::<Mesh3d>(painted.plate).unwrap().clone();
        let entity = world
            .spawn((
                mesh,
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Inherited,
                Pickable::IGNORE,
                ChildOf(visual),
                super::super::presentation::VisualOwner(owner),
                crate::native::description::shader::extent::SurfaceConsumer {
                    image: image.clone(),
                    surface: owner,
                },
                NotShadowCaster,
            ))
            .id();
        painted.overlay = Some(Overlay {
            entity,
            material,
            output,
            last: None,
        });
    }
    let overlay = painted.overlay.as_mut().unwrap();
    if let Some(layer) = super::room_order::layer(world, owner) {
        if world
            .get::<super::room_order::RoomLayer>(overlay.entity)
            .map(super::room_order::RoomLayer::as_ref)
            != Some(layer)
        {
            let layer = layer.into_owned();
            world.entity_mut(overlay.entity).insert(layer);
        }
    } else if world
        .get::<super::room_order::RoomLayer>(overlay.entity)
        .is_some()
    {
        world
            .entity_mut(overlay.entity)
            .remove::<super::room_order::RoomLayer>();
    }
    if overlay.output != output {
        unmark(world, owner, overlay.output);
        overlay.output = output;
    }
    if world
        .get::<LitEffectOutput>(output)
        .is_none_or(|marker| marker.0 != owner)
    {
        world.entity_mut(output).insert((LitEffectOutput(owner), SuppressUiImage));
    }
    if overlay
        .last
        .as_ref()
        .is_none_or(|(previous, alpha, texture)| {
            !Arc::ptr_eq(previous, settings) || *alpha != opacity.to_bits() || texture != &image
        })
    {
        nucleus::performance::add("appearance-overlay-material-writes", 1);
        *world
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&overlay.material)
            .unwrap() = StandardMaterial {
            base_color: Color::WHITE.with_alpha(opacity),
            base_color_texture: Some(image.clone()),
            emissive: LinearRgba::WHITE * settings.emissive,
            emissive_texture: if settings.emissive > 0.0 {
                Some(image.clone())
            } else {
                None
            },
            perceptual_roughness: settings.roughness,
            metallic: settings.metallic,
            unlit: !settings.lit,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 1.0,
            ..default()
        };
        if let Some(mut consumer) = world
            .get_mut::<crate::native::description::shader::extent::SurfaceConsumer>(overlay.entity)
            && consumer.image != image
        {
            consumer.image = image.clone();
        }
        overlay.last = Some((settings.clone(), opacity.to_bits(), image));
    }
    let mesh = &world.get::<Mesh3d>(painted.plate).unwrap().0;
    if world.get::<Mesh3d>(overlay.entity).unwrap().0 != *mesh {
        let mesh = mesh.clone();
        world.entity_mut(overlay.entity).insert(Mesh3d(mesh));
        nucleus::performance::add("appearance-overlay-mesh-writes", 1);
    }
    let mut transform = *world.get::<Transform>(painted.plate).unwrap();
    transform.translation.y += 0.002;
    world
        .get_mut::<Transform>(overlay.entity)
        .unwrap()
        .set_if_neq(transform);
    shadows(world, overlay.entity, false, settings.receive_shadows);
}
