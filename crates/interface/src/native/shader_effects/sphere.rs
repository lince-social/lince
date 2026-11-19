use super::*;
use bevy::{asset::embedded_asset, render::render_resource::AsBindGroup, shader::ShaderRef};

#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct SphereMaterial {
    #[texture(0)]
    #[sampler(1)]
    image: Handle<Image>,
}

impl Material for SphereMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://lince_interface/native/shader_effects/sphere.wesl".into()
    }
}

#[derive(Component)]
struct SphereEffect {
    owner: Entity,
    material: Handle<SphereMaterial>,
}

#[derive(Component)]
pub struct RecordSphere(pub Entity);

#[derive(Resource)]
struct Geometry(Handle<Mesh>);

#[derive(Default)]
struct PlacementScratch {
    existing: HashMap<Entity, Entity>,
    floating: Vec<(Entity, Entity, Entity)>,
}

pub(super) fn install(app: &mut App) {
    if app.world().contains_resource::<AssetServer>() {
        embedded_asset!(app, "sphere.wesl");
        app.add_plugins(MaterialPlugin::<SphereMaterial>::default())
            .add_systems(
                PostUpdate,
                update
                    .in_set(super::PlaceSpatialEffects)
                    .after(crate::native::topology::presentation::synchronize)
                    .before(bevy::camera::CameraUpdateSystems)
                    .before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

fn update(world: &mut World) {
    world
        .run_system_cached(place_floating)
        .expect("place current floating shader effects");
    world
        .run_system_cached(update_surfaces)
        .expect("place current Surface shader effects");
}

fn hide(world: &mut World, entity: Entity) {
    if let Some(mut visibility) = world.get_mut::<Visibility>(entity)
        && *visibility != Visibility::Hidden
    {
        *visibility = Visibility::Hidden;
        nucleus::performance::add("shader-placement-publications", 1);
    }
}

fn output_color(world: &mut World, output: Entity, color: Color) {
    if let Some(mut image) = world.get_mut::<ImageNode>(output)
        && image.color != color
    {
        image.color = color;
        nucleus::performance::add("shader-placement-publications", 1);
    }
}

fn place_floating(
    world: &mut World,
    effects: &mut QueryState<(Entity, &'static SphereEffect)>,
    floats: &mut QueryState<(Entity, &'static Floating)>,
    mut scratch: Local<PlacementScratch>,
) {
    let _placement = nucleus::performance::Span::new("shader-floating-placement");
    let PlacementScratch { existing, floating } = &mut *scratch;
    existing.clear();
    existing.extend(
        effects
            .iter(world)
            .map(|(entity, effect)| (effect.owner, entity)),
    );
    existing.retain(|owner, entity| {
        if world.get::<Floating>(*owner).is_none() {
            let material = world.get::<SphereEffect>(*entity).unwrap().material.id();
            world
                .resource_mut::<Assets<SphereMaterial>>()
                .remove(material);
            world.despawn(*entity);
            false
        } else {
            true
        }
    });
    floating.clear();
    floating.extend(
        floats
            .iter(world)
            .map(|(entity, effect)| (entity, effect.record, effect.preview)),
    );
    if existing.is_empty() && floating.is_empty() {
        *scratch = PlacementScratch::default();
        return;
    }
    for (owner, record, preview) in floating.drain(..) {
        let Some(data) = world.get::<RecordData>(record) else {
            continue;
        };
        let Some(config) = crate::native::protein_area::configuration_ref(world, data.binding.area)
        else {
            continue;
        };
        let settings = placement::effective_ref(world, &data.binding, &config.shader_placement);
        let (size, sphere) = (settings.size, settings.sphere);
        let Some(root) = root(world, owner) else {
            continue;
        };
        let spatial = crate::native::topology::view::spatial(world, root);
        let showing = !disabled(world)
            && world
                .get::<shader::EffectPreview>(preview)
                .is_some_and(|preview| !preview.disabled)
            && world
                .get::<crate::native::topology::presentation::Surface>(owner)
                .is_some_and(|surface| surface.visible);
        let visible = spatial && sphere && showing;
        if let Some(body) = world
            .get::<crate::native::topology::presentation::Surface>(owner)
            .map(|surface| surface.body)
        {
            hide(world, body);
        }
        let mesh_position = crate::native::topology::position(world, owner).unwrap_or(DVec3::ZERO)
            - crate::native::topology::presentation::origin(world, root);
        let mesh_transform = Transform {
            translation: mesh_position.as_vec3(),
            rotation: crate::native::topology::spatial(world, owner)
                .rotation()
                .as_quat(),
            scale: Vec3::splat(size),
        };
        if shader::spatial::place(
            world,
            preview,
            owner,
            record,
            mesh_transform,
            spatial && showing,
        ) {
            if let Some(entity) = existing.get(&owner) {
                let material = world.get::<SphereEffect>(*entity).unwrap().material.id();
                world
                    .resource_mut::<Assets<SphereMaterial>>()
                    .remove(material);
                world.despawn(*entity);
            }
            if spatial
                && let Some(visual) = world
                    .get::<crate::native::topology::presentation::Surface>(owner)
                    .map(|surface| surface.visual)
            {
                hide(world, visual);
            }
            let output = shader::output(world, preview);
            output_color(
                world,
                output,
                if spatial { Color::NONE } else { Color::WHITE },
            );
            continue;
        }
        if (!showing || spatial && sphere)
            && let Some(visual) = world
                .get::<crate::native::topology::presentation::Surface>(owner)
                .map(|surface| surface.visual)
        {
            hide(world, visual);
        }
        let output = shader::output(world, preview);
        let Some(image) = world
            .get::<ImageNode>(output)
            .map(|node| node.image.clone())
        else {
            continue;
        };
        output_color(
            world,
            output,
            if spatial && sphere {
                Color::NONE
            } else {
                Color::WHITE
            },
        );
        let entity = if let Some(entity) = existing.get(&owner) {
            *entity
        } else {
            if !visible {
                continue;
            }
            if !world.contains_resource::<Geometry>() {
                let mesh = world
                    .resource_mut::<Assets<Mesh>>()
                    .add(Sphere::new(0.5).mesh().uv(32, 20));
                world.insert_resource(Geometry(mesh));
            }
            let mesh = world.resource::<Geometry>().0.clone();
            let material = world
                .resource_mut::<Assets<SphereMaterial>>()
                .add(SphereMaterial {
                    image: image.clone(),
                });
            world
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material.clone()),
                    SphereEffect { owner, material },
                    shader::extent::SurfaceConsumer {
                        image: image.clone(),
                        surface: owner,
                    },
                    RecordSphere(record),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::picking::Pickable::IGNORE,
                    crate::native::inspection::InspectionExcluded,
                ))
                .id()
        };
        let material = world.get::<SphereEffect>(entity).unwrap().material.id();
        if world
            .resource::<Assets<SphereMaterial>>()
            .get(material)
            .is_some_and(|material| material.image != image)
        {
            world
                .resource_mut::<Assets<SphereMaterial>>()
                .get_mut(material)
                .unwrap()
                .image = image.clone();
        }
        let consumer = shader::extent::SurfaceConsumer {
            image,
            surface: owner,
        };
        if world.get::<shader::extent::SurfaceConsumer>(entity) != Some(&consumer) {
            world.entity_mut(entity).insert(consumer);
        }
        let rotation = crate::native::topology::spatial(world, record)
            .rotation()
            .as_quat();
        let transform = Transform {
            translation: mesh_position.as_vec3(),
            rotation,
            scale: Vec3::splat(size),
        };
        if world.get::<Transform>(entity) != Some(&transform) {
            world.entity_mut(entity).insert(transform);
            nucleus::performance::add("shader-placement-publications", 1);
        }
        let visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if world.get::<Visibility>(entity) != Some(&visibility) {
            world.entity_mut(entity).insert(visibility);
            nucleus::performance::add("shader-placement-publications", 1);
        }
        if world
            .get::<RecordSphere>(entity)
            .is_none_or(|current| current.0 != record)
        {
            world.entity_mut(entity).insert(RecordSphere(record));
            nucleus::performance::add("shader-placement-publications", 1);
        }
    }
}

fn update_surfaces(
    world: &mut World,
    backgrounds: &mut QueryState<
        (
            Entity,
            &'static Background,
            &'static crate::native::canvas::CanvasItem,
        ),
        With<Surface>,
    >,
    mut surfaces: Local<Vec<(Entity, Entity, Vec2)>>,
) {
    let _placement = nucleus::performance::Span::new("shader-surface-placement");
    surfaces.clear();
    surfaces.extend(
        backgrounds
            .iter(world)
            .map(|(owner, background, item)| (owner, background.preview, item.size)),
    );
    if surfaces.is_empty() {
        *surfaces = Vec::new();
        return;
    }
    for (owner, preview, size) in surfaces.drain(..) {
        let Some(root) = root(world, owner) else {
            continue;
        };
        let spatial = crate::native::topology::view::spatial(world, root);
        let showing = world
            .get::<crate::native::topology::presentation::Surface>(owner)
            .is_some_and(|surface| surface.visible)
            && crate::native::topology::presentation::displayed(world, owner)
            && !disabled(world)
            && world
                .get::<shader::EffectPreview>(preview)
                .is_some_and(|preview| !preview.disabled);
        let placement = crate::native::topology::spatial(world, owner);
        let position = crate::native::topology::position(world, owner).unwrap_or(DVec3::ZERO)
            - crate::native::topology::presentation::origin(world, root);
        let transform = Transform {
            translation: position.as_vec3(),
            rotation: placement.rotation().as_quat(),
            scale: Vec3::new(size.x, placement.depth(size) as f32, size.y),
        };
        let placed =
            shader::spatial::place(world, preview, owner, owner, transform, spatial && showing);
        if placed
            && spatial
            && showing
            && let Some(visual) = world
                .get::<crate::native::topology::presentation::Surface>(owner)
                .map(|surface| surface.visual)
        {
            hide(world, visual);
        }
        let output = shader::output(world, preview);
        output_color(
            world,
            output,
            if placed && spatial && showing {
                Color::NONE
            } else {
                Color::WHITE
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sphere_follows_record_switches_views_and_releases_material() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<SphereMaterial>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<ShaderSettings>();
        let root = world
            .spawn((
                Node::default(),
                crate::native::workspace::Workspaces::default(),
                crate::native::topology::view::View {
                    spatial: true,
                    ..default()
                },
            ))
            .id();
        let config = Config {
            shader_effects: true,
            shader_placement: FloatingSettings {
                sphere: true,
                ..default()
            },
            ..Config::records()
        };
        let mut area = crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Circle,
            DVec2::ZERO,
            DVec2::splat(1000.0),
        );
        area.protein = Some(config.clone());
        let area = world.spawn((area, ChildOf(root))).id();
        let row = world
            .spawn((
                Node::default(),
                ChildOf(root),
                crate::native::canvas::CanvasItem {
                    position: DVec2::new(100.0, 200.0),
                    size: Vec2::new(320.0, 200.0),
                },
            ))
            .id();
        record(
            &mut world,
            row,
            &config,
            &serde_json::json!({"quantity":"-3", "shader_effect":Extension::single(1, Effect::default())}),
            Some(RecordBinding {
                area,
                uid: nucleus::new_uid("r"),
                source: Source::Local,
            }),
        );
        update_presentations(&mut world);
        let owner = world
            .query_filtered::<Entity, With<Floating>>()
            .single(&world)
            .unwrap();
        let preview = world.get::<Floating>(owner).unwrap().preview;
        let output = shader::output(&world, preview);
        world.entity_mut(output).insert(ImageNode::default());
        let panel = world.spawn(Visibility::Visible).id();
        world
            .entity_mut(owner)
            .insert(crate::native::topology::presentation::Surface {
                camera: Entity::PLACEHOLDER,
                image: default(),
                visual: panel,
                body: Entity::PLACEHOLDER,
                face: Entity::PLACEHOLDER,
                size: Vec2::splat(120.0),
                pixels: UVec2::splat(256),
                density: 1.0,
                material: default(),
                uv: Rect::default(),
                visible: true,
            });
        update(&mut world);
        let sphere = world
            .query_filtered::<Entity, With<SphereEffect>>()
            .single(&world)
            .unwrap();
        let expected = crate::native::topology::position(&world, owner).unwrap()
            - crate::native::topology::presentation::origin(&world, root);
        assert_eq!(
            world.get::<Transform>(sphere).unwrap().translation,
            expected.as_vec3()
        );
        assert_eq!(world.get::<Visibility>(sphere), Some(&Visibility::Visible));
        assert_eq!(world.get::<ImageNode>(output).unwrap().color, Color::NONE);
        assert_eq!(world.get::<Visibility>(panel), Some(&Visibility::Hidden));
        let mesh = &world.get::<Mesh3d>(sphere).unwrap().0;
        assert!(
            world
                .resource::<Assets<Mesh>>()
                .get(mesh)
                .unwrap()
                .count_vertices()
                > 100
        );
        world
            .get_mut::<crate::native::canvas::CanvasItem>(row)
            .unwrap()
            .position
            .x += 300.0;
        update_presentations(&mut world);
        update(&mut world);
        assert_eq!(
            world.get::<Transform>(sphere).unwrap().translation.x,
            expected.x as f32 + 300.0
        );
        world
            .get_mut::<crate::native::topology::view::View>(root)
            .unwrap()
            .spatial = false;
        update(&mut world);
        assert_eq!(world.get::<Visibility>(sphere), Some(&Visibility::Hidden));
        assert_eq!(world.get::<ImageNode>(output).unwrap().color, Color::WHITE);
        world.despawn(owner);
        update(&mut world);
        assert_eq!(world.query::<&SphereEffect>().iter(&world).count(), 0);
        assert_eq!(world.resource::<Assets<SphereMaterial>>().len(), 0);
        let encoded = serde_json::to_value(&config.shader_placement).unwrap();
        assert_eq!(
            serde_json::from_value::<FloatingSettings>(encoded).unwrap(),
            config.shader_placement
        );
    }
}
