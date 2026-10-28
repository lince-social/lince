use super::*;
use crate::native::topology::appearance;
use bevy::light::{AmbientLight, NotShadowCaster, NotShadowReceiver};

#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
pub(super) struct Parameters {
    pub parameters: Vec4,
    pub repeat_opacity: Vec4,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            parameters: Vec4::new(0.65, 0.0, 0.0, 0.0),
            repeat_opacity: Vec4::new(1.0, 1.0, 1.0, 0.0),
        }
    }
}

struct Styled {
    material: Handle<MeshMaterial>,
    original: Handle<MeshMaterial>,
    cast: bool,
    receive: bool,
}

#[derive(Resource, Default)]
struct Styles {
    materials: HashMap<Entity, Styled>,
    cameras: HashMap<Entity, Option<AmbientLight>>,
    light: Option<Entity>,
}

pub(super) fn install(app: &mut App) {
    app.init_resource::<Styles>().add_systems(
        PostUpdate,
        update
            .after(crate::native::topology::presentation::synchronize)
            .after(crate::native::shader_effects::PlaceSpatialEffects)
            .before(bevy::camera::CameraUpdateSystems)
            .before(bevy::transform::TransformSystems::Propagate),
    );
}

fn shadows(world: &mut World, entity: Entity, cast: bool, receive: bool) {
    if cast {
        world.entity_mut(entity).remove::<NotShadowCaster>();
    } else if world.get::<NotShadowCaster>(entity).is_none() {
        world.entity_mut(entity).insert(NotShadowCaster);
    }
    if receive {
        world.entity_mut(entity).remove::<NotShadowReceiver>();
    } else if world.get::<NotShadowReceiver>(entity).is_none() {
        world.entity_mut(entity).insert(NotShadowReceiver);
    }
}

fn same(a: &MeshMaterial, b: &MeshMaterial) -> bool {
    a.inputs == b.inputs
        && a.parameters == b.parameters
        && a.geometry == b.geometry
        && a.key == b.key
        && a.lighting == b.lighting
        && a.texture == b.texture
}

fn release(world: &mut World, entity: Entity, style: Styled) {
    if let Some(current) = world
        .get::<MeshMaterial3d<MeshMaterial>>(entity)
        .map(|handle| handle.0.clone())
    {
        let base = if current == style.material {
            style.original.clone()
        } else {
            current
        };
        let volume = world
            .resource::<Assets<MeshMaterial>>()
            .get(&base)
            .is_some_and(|material| material.key.volume);
        if world.resource::<Assets<MeshMaterial>>().contains(base.id()) {
            world.entity_mut(entity).insert(MeshMaterial3d(base));
            shadows(world, entity, style.cast && !volume, style.receive);
        } else {
            world
                .entity_mut(entity)
                .remove::<MeshMaterial3d<MeshMaterial>>();
        }
    }
    world
        .resource_mut::<Assets<MeshMaterial>>()
        .remove(style.material.id());
}

fn update(world: &mut World) {
    if !world.contains_resource::<Assets<MeshMaterial>>() {
        return;
    }
    let mut styles = world.remove_resource::<Styles>().unwrap_or_default();
    let mut targets: Vec<_> = world
        .query::<(
            Entity,
            &crate::native::shader_effects::RecordMesh,
            &Visibility,
        )>()
        .iter(world)
        .filter(|(_, _, visibility)| **visibility == Visibility::Visible)
        .map(|(entity, record, _)| (entity, record.0, None))
        .collect();
    if let Some(effects) = world.get_resource::<MeshEffects>() {
        for (preview, state) in &effects.0 {
            if enabled(world, *preview)
                && let Some(entity) = state.preview
                && let Some(owner) =
                    crate::native::shader_effects::appearance_owner(world, *preview)
                && let Some(data) = world.get::<EffectPreview>(*preview)
                && visible(world, output(world, *preview))
            {
                targets.push((entity, owner, Some(data.camera)));
            }
        }
    }
    targets.sort_by_key(|(entity, owner, _)| (owner.to_bits(), entity.to_bits()));
    let mut owners = HashSet::new();
    let mut alive = HashSet::new();
    let mut cameras = HashSet::new();
    let mut lit_preview = false;
    for (entity, owner, camera) in targets {
        if alive.len() >= nucleus::canvas::appearance::MAX_MATERIALS * 2 {
            break;
        }
        let Some((settings, opacity)) = appearance::mesh_appearance(world, owner) else {
            continue;
        };
        if !owners.contains(&owner) && owners.len() >= nucleus::canvas::appearance::MAX_MATERIALS {
            continue;
        }
        let Some(current) = world
            .get::<MeshMaterial3d<MeshMaterial>>(entity)
            .map(|material| material.0.clone())
        else {
            continue;
        };
        let base = styles
            .materials
            .get(&entity)
            .filter(|style| style.material == current)
            .map_or(current, |style| style.original.clone());
        let Some(mut desired) = world
            .resource::<Assets<MeshMaterial>>()
            .get(&base)
            .cloned()
            .filter(|material| !material.key.volume)
        else {
            continue;
        };
        owners.insert(owner);
        alive.insert(entity);
        desired.lighting = Parameters {
            parameters: Vec4::new(
                settings.roughness,
                settings.metallic,
                settings.emissive,
                if settings.lit { 2.0 } else { 1.0 },
            ),
            repeat_opacity: Vec4::new(settings.repeat[0], settings.repeat[1], opacity, 0.0),
        };
        desired.texture = appearance::mesh_texture(world, &settings.texture);
        let style = styles.materials.entry(entity).or_insert_with(|| Styled {
            material: world
                .resource_mut::<Assets<MeshMaterial>>()
                .add(desired.clone()),
            original: base.clone(),
            cast: world.get::<NotShadowCaster>(entity).is_none(),
            receive: world.get::<NotShadowReceiver>(entity).is_none(),
        });
        style.original = base;
        if !world
            .resource::<Assets<MeshMaterial>>()
            .contains(style.material.id())
        {
            style.material = world
                .resource_mut::<Assets<MeshMaterial>>()
                .add(desired.clone());
        } else if world
            .resource::<Assets<MeshMaterial>>()
            .get(&style.material)
            .is_none_or(|material| !same(material, &desired))
        {
            *world
                .resource_mut::<Assets<MeshMaterial>>()
                .get_mut(&style.material)
                .unwrap() = desired;
        }
        if world.get::<MeshMaterial3d<MeshMaterial>>(entity).unwrap().0 != style.material {
            world
                .entity_mut(entity)
                .insert(MeshMaterial3d(style.material.clone()));
        }
        shadows(
            world,
            entity,
            settings.cast_shadows && opacity > 0.0,
            settings.receive_shadows,
        );
        if let Some(camera) = camera.filter(|camera| *camera != Entity::PLACEHOLDER) {
            cameras.insert(camera);
            if !styles.cameras.contains_key(&camera) {
                styles
                    .cameras
                    .insert(camera, world.get::<AmbientLight>(camera).cloned());
                world.entity_mut(camera).insert(AmbientLight {
                    brightness: 30.0,
                    ..default()
                });
            }
            lit_preview |= settings.lit;
        }
    }
    let stale: Vec<_> = styles
        .materials
        .keys()
        .copied()
        .filter(|entity| !alive.contains(entity))
        .collect();
    for entity in stale {
        release(world, entity, styles.materials.remove(&entity).unwrap());
    }
    let stale: Vec<_> = styles
        .cameras
        .keys()
        .copied()
        .filter(|camera| !cameras.contains(camera))
        .collect();
    for camera in stale {
        let original = styles.cameras.remove(&camera).unwrap();
        if let Ok(mut entity) = world.get_entity_mut(camera) {
            if let Some(original) = original {
                entity.insert(original);
            } else {
                entity.remove::<AmbientLight>();
            }
        }
    }
    if lit_preview && styles.light.is_none() {
        styles.light = Some(
            world
                .spawn((
                    DirectionalLight {
                        illuminance: 1000.0,
                        ..default()
                    },
                    Transform::from_xyz(3.0, 4.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
                    RenderLayers::from_layers(
                        &crate::native::render_layers::SHADERS
                            .indices()
                            .collect::<Vec<_>>(),
                    ),
                ))
                .id(),
        );
    } else if !lit_preview
        && let Some(light) = styles.light.take()
        && let Ok(entity) = world.get_entity_mut(light)
    {
        entity.despawn();
    }
    world.insert_resource(styles);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(count: usize) -> (World, Handle<MeshMaterial>, Vec<(Entity, Entity)>) {
        let mut world = World::new();
        world.init_resource::<Assets<MeshMaterial>>();
        let original = world
            .resource_mut::<Assets<MeshMaterial>>()
            .add(MeshMaterial {
                inputs: default(),
                parameters: Vec4::ZERO,
                geometry: Handle::default(),
                lighting: default(),
                texture: None,
                key: MeshShaderKey {
                    vertex: Handle::default(),
                    fragment: Handle::default(),
                    volume: false,
                    offscreen: false,
                },
            });
        let records = (0..count)
            .map(|index| {
                let owner = world
                    .spawn(appearance::Material(
                        nucleus::canvas::appearance::Material {
                            lit: true,
                            roughness: 0.2 + (index % 2) as f32 * 0.6,
                            ..default()
                        },
                    ))
                    .id();
                let entity = world
                    .spawn((
                        crate::native::shader_effects::RecordMesh(owner),
                        MeshMaterial3d(original.clone()),
                        Visibility::Visible,
                    ))
                    .id();
                (owner, entity)
            })
            .collect();
        (world, original, records)
    }

    #[test]
    fn mesh_appearance_instances_keep_shared_programs_and_independent_parameters() {
        let (mut world, original, records) = scene(2);
        update(&mut world);
        let a = world
            .get::<MeshMaterial3d<MeshMaterial>>(records[0].1)
            .unwrap()
            .0
            .clone();
        let b = world
            .get::<MeshMaterial3d<MeshMaterial>>(records[1].1)
            .unwrap()
            .0
            .clone();
        assert_ne!(a, b);
        assert_ne!(a, original);
        let assets = world.resource::<Assets<MeshMaterial>>();
        assert_eq!(assets.get(&original).unwrap().lighting.parameters.w, 0.0);
        assert_eq!(assets.get(&a).unwrap().lighting.parameters.x, 0.2);
        assert_eq!(assets.get(&b).unwrap().lighting.parameters.x, 0.8);
        assert_eq!(
            assets.get(&a).unwrap().key,
            assets.get(&original).unwrap().key
        );
        world
            .get_mut::<appearance::Material>(records[0].0)
            .unwrap()
            .0
            .receive_shadows = false;
        world
            .resource_mut::<Assets<MeshMaterial>>()
            .get_mut(&original)
            .unwrap()
            .inputs
            .quantity = 7.0;
        update(&mut world);
        assert!(world.get::<NotShadowReceiver>(records[0].1).is_some());
        assert!(world.get::<NotShadowReceiver>(records[1].1).is_none());
        assert_eq!(
            world
                .resource::<Assets<MeshMaterial>>()
                .get(&a)
                .unwrap()
                .inputs
                .quantity,
            7.0
        );
        assert_eq!(
            world
                .resource::<Assets<MeshMaterial>>()
                .get(&b)
                .unwrap()
                .inputs
                .quantity,
            7.0
        );
        world
            .entity_mut(records[0].0)
            .remove::<appearance::Material>();
        update(&mut world);
        assert_eq!(
            world
                .get::<MeshMaterial3d<MeshMaterial>>(records[0].1)
                .unwrap()
                .0,
            original
        );
        assert!(world.resource::<Assets<MeshMaterial>>().get(&a).is_none());
        assert!(world.get::<NotShadowReceiver>(records[0].1).is_none());
        world.despawn(records[1].1);
        update(&mut world);
        assert!(world.resource::<Assets<MeshMaterial>>().get(&b).is_none());
        assert_eq!(world.resource::<Assets<MeshMaterial>>().len(), 1);
    }

    #[test]
    fn mesh_appearance_budget_releases_hidden_instances_and_preserves_volume_shadow_exclusion() {
        let (mut world, original, records) = scene(80);
        update(&mut world);
        assert_eq!(
            world.resource::<Styles>().materials.len(),
            nucleus::canvas::appearance::MAX_MATERIALS
        );
        for (_, entity) in &records {
            *world.get_mut::<Visibility>(*entity).unwrap() = Visibility::Hidden;
        }
        update(&mut world);
        assert!(world.resource::<Styles>().materials.is_empty());
        assert_eq!(world.resource::<Assets<MeshMaterial>>().len(), 1);
        let entity = records[0].1;
        *world.get_mut::<Visibility>(entity).unwrap() = Visibility::Visible;
        update(&mut world);
        let mut volume = world
            .resource::<Assets<MeshMaterial>>()
            .get(&original)
            .unwrap()
            .clone();
        volume.key.volume = true;
        let volume = world.resource_mut::<Assets<MeshMaterial>>().add(volume);
        world
            .entity_mut(entity)
            .insert((MeshMaterial3d(volume.clone()), NotShadowCaster));
        update(&mut world);
        assert!(world.resource::<Styles>().materials.is_empty());
        assert!(world.get::<NotShadowCaster>(entity).is_some());
        assert_eq!(
            world.get::<MeshMaterial3d<MeshMaterial>>(entity).unwrap().0,
            volume
        );
    }
}
