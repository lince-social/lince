use bevy::prelude::*;
use bevy::{
    light::{NotShadowCaster, NotShadowReceiver, PointLightShadowMap},
    render::Extract,
    ui_render::{ExtractedUiItem, ExtractedUiNodes, NodeType},
};
use nucleus::canvas::appearance::{self as model, LightDrive};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

mod lighting;
mod overlay;
mod room_order;
pub(crate) use overlay::extract_images;
pub(crate) use room_order::install as install_room_order;
mod shapes;
#[cfg(test)]
mod tests;
mod textures;
mod ui;
pub(crate) use ui::controls;

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Material(pub nucleus::canvas::appearance::Material);

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Light(pub nucleus::canvas::appearance::Light);

#[derive(Component)]
pub(crate) struct Applied;

#[derive(Component)]
pub(crate) struct LitEffectOutput(Entity);

const CANVAS_UNITS_PER_METER: f32 = 100.0;

#[derive(Resource, Default)]
struct Runtime {
    materials: HashMap<Entity, Painted>,
    lights: HashMap<Entity, Entity>,
    textures: HashMap<model::Texture, Handle<Image>>,
    texture_keys: HashSet<model::Texture>,
    mesh_textures: HashSet<model::Texture>,
    shadow_size: Option<usize>,
}

struct Painted {
    overlay: Option<overlay::Overlay>,
    geometry: shapes::Geometry,
    body: Entity,
    plate: Entity,
    original: Handle<StandardMaterial>,
    material: Handle<StandardMaterial>,
    settings: Option<model::Material>,
    texture: Option<Handle<Image>>,
}

pub(crate) fn extract_backgrounds(
    mut nodes: ResMut<ExtractedUiNodes>,
    materials: Extract<Query<(Entity, &Material, &super::presentation::Surface), With<Applied>>>,
) {
    let owners: HashSet<_> = materials
        .iter()
        .filter(|(_, value, surface)| value.0.valid() && surface.visible)
        .map(|(entity, _, _)| entity)
        .collect();
    nodes.uinodes.retain(|node| {
        !(owners.contains(&node.main_entity.id())
            && node.image == bevy::asset::AssetId::default()
            && matches!(
                node.item,
                ExtractedUiItem::Node {
                    node_type: NodeType::Rect,
                    ..
                }
            ))
    });
}

fn shadows(world: &mut World, entity: Entity, cast: bool, receive: bool) {
    if cast {
        world.entity_mut(entity).remove::<NotShadowCaster>();
    } else {
        if world.get::<NotShadowCaster>(entity).is_none() {
            world.entity_mut(entity).insert(NotShadowCaster);
        }
    }
    if receive {
        world.entity_mut(entity).remove::<NotShadowReceiver>();
    } else {
        if world.get::<NotShadowReceiver>(entity).is_none() {
            world.entity_mut(entity).insert(NotShadowReceiver);
        }
    }
}

fn strength(world: &World, owner: Entity, drive: LightDrive) -> f32 {
    use nucleus::shader_effect::TaskState;
    if drive == LightDrive::Constant {
        return 1.0;
    }
    let Some(inputs) = crate::native::shader_effects::lighting_inputs(world, owner) else {
        return 0.0;
    };
    match drive {
        LightDrive::Constant => 1.0,
        LightDrive::Pending => f32::from(inputs.task == TaskState::Pending),
        LightDrive::Completed => f32::from(inputs.task == TaskState::Completed),
        LightDrive::Progress => inputs.progress.unwrap_or(0.0).clamp(0.0, 1.0),
    }
}

fn opacity(world: &World, owner: Entity) -> f32 {
    let value = world
        .get::<super::presentation::Surface>(owner)
        .and_then(|surface| {
            world
                .resource::<Assets<StandardMaterial>>()
                .get(&surface.material)
        })
        .map_or(1.0, |material| material.base_color.alpha());
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn mesh_appearance(world: &World, owner: Entity) -> Option<(model::Material, f32)> {
    let settings = &world.get::<Material>(owner)?.0;
    settings
        .valid()
        .then(|| (settings.clone(), opacity(world, owner)))
}

pub(crate) fn mesh_texture(world: &mut World, key: &model::Texture) -> Option<Handle<Image>> {
    if matches!(key, model::Texture::Plain) || !key.valid() {
        return None;
    }
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    if !runtime.texture_keys.contains(key) && runtime.texture_keys.len() >= model::MAX_TEXTURES {
        world.insert_resource(runtime);
        return None;
    }
    let texture = textures::get(world, &mut runtime.textures, &mut runtime.texture_keys, key);
    runtime.mesh_textures.insert(key.clone());
    world.insert_resource(runtime);
    texture
}

pub fn update(world: &mut World) {
    ui::update(world);
    if !super::presentation::ready(world) {
        return;
    }
    let mut runtime = world.remove_resource::<Runtime>().unwrap_or_default();
    textures::poll(world, &mut runtime.textures);
    let mut surfaces: Vec<_> = world
        .query::<(Entity, &super::presentation::Surface)>()
        .iter(world)
        .filter(|(_, surface)| surface.visible)
        .map(|(owner, surface)| (owner, surface.visual, surface.body, surface.face))
        .collect();
    surfaces.sort_by_key(|(owner, _, _, _)| owner.to_bits());
    let living: HashMap<_, _> = surfaces
        .iter()
        .filter(|(owner, _, _, _)| {
            world
                .get::<Material>(*owner)
                .is_some_and(|value| value.0.valid())
        })
        .take(model::MAX_MATERIALS)
        .map(|(owner, _, body, _)| (*owner, *body))
        .collect();
    let stale: Vec<_> = runtime
        .materials
        .keys()
        .copied()
        .filter(|owner| living.get(owner) != Some(&runtime.materials[owner].body))
        .collect();
    for owner in stale {
        if let Ok(mut entity) = world.get_entity_mut(owner) {
            entity.remove::<Applied>();
        }
        let mut painted = runtime.materials.remove(&owner).unwrap();
        overlay::release(world, owner, &mut painted.overlay);
        painted.geometry.release(world, painted.body);
        if let Ok(mut body) = world.get_entity_mut(painted.body) {
            body.insert(MeshMaterial3d(painted.original));
            body.remove::<(NotShadowCaster, NotShadowReceiver)>();
        }
        if let Ok(plate) = world.get_entity_mut(painted.plate) {
            plate.despawn();
        }
        world
            .resource_mut::<Assets<StandardMaterial>>()
            .remove(painted.material.id());
        crate::native::shader_effects::protect_material_text(world, owner);
    }
    let mut used_textures = std::mem::take(&mut runtime.mesh_textures);
    for (owner, visual, body, face) in surfaces {
        room_order::foreground(world, owner, face);
        let opacity = opacity(world, owner);
        world
            .entity_mut(face)
            .insert((NotShadowCaster, NotShadowReceiver));
        if let Some(settings) = world
            .get::<Material>(owner)
            .map(|value| value.0.clone())
            .filter(|value| value.valid() && living.contains_key(&owner))
        {
            crate::native::shader_effects::protect_text(world, owner, true);
            if world.get::<Applied>(owner).is_none() {
                world.entity_mut(owner).insert(Applied);
            }
            let texture = textures::get(
                world,
                &mut runtime.textures,
                &mut used_textures,
                &settings.texture,
            );
            if !runtime.materials.contains_key(&owner) {
                let original = world
                    .get::<MeshMaterial3d<StandardMaterial>>(body)
                    .unwrap()
                    .0
                    .clone();
                let material = world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .add(StandardMaterial::default());
                let plate = world
                    .spawn((
                        world.get::<Mesh3d>(face).unwrap().clone(),
                        MeshMaterial3d(material.clone()),
                        Transform::default(),
                        Visibility::Inherited,
                        Pickable::IGNORE,
                        ChildOf(visual),
                        super::presentation::VisualOwner(owner),
                    ))
                    .id();
                runtime.materials.insert(
                    owner,
                    Painted {
                        overlay: None,
                        geometry: shapes::Geometry::new(world, body),
                        body,
                        plate,
                        original,
                        material,
                        settings: None,
                        texture: None,
                    },
                );
            }
            let painted = runtime.materials.get_mut(&owner).unwrap();
            let mut transform = *world.get::<Transform>(face).unwrap();
            transform.translation.y -= 0.005;
            world
                .get_mut::<Transform>(painted.plate)
                .unwrap()
                .set_if_neq(transform);
            painted
                .geometry
                .update(world, owner, face, body, painted.plate);
            if world
                .get::<MeshMaterial3d<StandardMaterial>>(body)
                .unwrap()
                .0
                != painted.material
            {
                world
                    .entity_mut(body)
                    .insert(MeshMaterial3d(painted.material.clone()));
            }
            let [r, g, b, a] = settings.color;
            let alpha = a * opacity;
            let desired = StandardMaterial {
                base_color: Color::srgba(r, g, b, alpha),
                base_color_texture: texture.clone(),
                uv_transform: bevy::math::Affine2::from_scale(Vec2::from_array(settings.repeat)),
                perceptual_roughness: settings.roughness,
                metallic: settings.metallic,
                emissive: Color::srgb(r, g, b).to_linear() * settings.emissive,
                unlit: !settings.lit,
                alpha_mode: if alpha < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            };
            let current = world
                .resource::<Assets<StandardMaterial>>()
                .get(&painted.material)
                .unwrap();
            if current.base_color != desired.base_color
                || painted.settings.as_ref() != Some(&settings)
                || painted.texture != texture
            {
                *world
                    .resource_mut::<Assets<StandardMaterial>>()
                    .get_mut(&painted.material)
                    .unwrap() = desired;
            }
            shadows(
                world,
                body,
                settings.cast_shadows && alpha > 0.0,
                settings.receive_shadows,
            );
            shadows(
                world,
                painted.plate,
                settings.cast_shadows && alpha > 0.0,
                settings.receive_shadows,
            );
            painted.settings = Some(settings);
            painted.texture = texture;
            let settings = painted.settings.as_ref().unwrap().clone();
            overlay::update(world, owner, painted, &settings, opacity);
        }
    }
    let (lights, shadow_size) = lighting::synchronize(world, &mut runtime);
    let stale: Vec<_> = runtime
        .lights
        .keys()
        .copied()
        .filter(|owner| !lights.contains(owner))
        .collect();
    for owner in stale {
        let entity = runtime.lights.remove(&owner).unwrap();
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    runtime.textures.retain(|key, texture| {
        if used_textures.contains(key) {
            return true;
        }
        world.resource_mut::<Assets<Image>>().remove(texture.id());
        false
    });
    textures::retain(world, &used_textures);
    runtime.texture_keys = used_textures;
    if shadow_size > 0 {
        let mut map = world.get_resource_or_init::<PointLightShadowMap>();
        runtime.shadow_size.get_or_insert(map.size);
        if map.size != shadow_size {
            map.size = shadow_size;
        }
    } else if let Some(size) = runtime.shadow_size.take() {
        world.get_resource_or_init::<PointLightShadowMap>().size = size;
    }
    world.insert_resource(runtime);
}
