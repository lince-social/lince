use super::*;

pub(super) struct Overlay {
    entity: Entity,
    material: Handle<StandardMaterial>,
    output: Entity,
    last: Option<(model::Material, u32, Handle<Image>)>,
}

pub(crate) fn extract_images(
    mut nodes: ResMut<ExtractedUiNodes>,
    outputs: Extract<Query<Entity, With<LitEffectOutput>>>,
) {
    let outputs: HashSet<_> = outputs.iter().collect();
    nodes.uinodes.retain(|node| {
        !(outputs.contains(&node.main_entity.id()) && node.image != bevy::asset::AssetId::default())
    });
}

fn unmark(world: &mut World, owner: Entity, output: Entity) {
    if world
        .get::<LitEffectOutput>(output)
        .is_some_and(|marker| marker.0 == owner)
    {
        world.entity_mut(output).remove::<LitEffectOutput>();
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
    settings: &model::Material,
    opacity: f32,
) {
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
        if world.get::<super::room_order::RoomLayer>(overlay.entity) != Some(&layer) {
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
        world.entity_mut(output).insert(LitEffectOutput(owner));
    }
    let key = (settings.clone(), opacity.to_bits(), image.clone());
    if overlay.last.as_ref() != Some(&key) {
        *world
            .resource_mut::<Assets<StandardMaterial>>()
            .get_mut(&overlay.material)
            .unwrap() = StandardMaterial {
            base_color: Color::WHITE.with_alpha(opacity),
            base_color_texture: Some(image.clone()),
            emissive: LinearRgba::WHITE * settings.emissive,
            emissive_texture: if settings.emissive > 0.0 {
                Some(image)
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
        world
            .get_mut::<crate::native::description::shader::extent::SurfaceConsumer>(overlay.entity)
            .unwrap()
            .image = key.2.clone();
        overlay.last = Some(key);
    }
    let mesh = world.get::<Mesh3d>(painted.plate).unwrap().clone();
    if world.get::<Mesh3d>(overlay.entity).unwrap().0 != mesh.0 {
        world.entity_mut(overlay.entity).insert(mesh);
    }
    let mut transform = *world.get::<Transform>(painted.plate).unwrap();
    transform.translation.y += 0.002;
    world
        .get_mut::<Transform>(overlay.entity)
        .unwrap()
        .set_if_neq(transform);
    shadows(world, overlay.entity, false, settings.receive_shadows);
}
