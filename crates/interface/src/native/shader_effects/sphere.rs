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
        "embedded://lince_interface/native/shader_effects/sphere.wgsl".into()
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

pub(super) fn install(app: &mut App) {
    if app.world().contains_resource::<AssetServer>() {
        embedded_asset!(app, "sphere.wgsl");
        app.add_plugins(MaterialPlugin::<SphereMaterial>::default())
            .add_systems(
                PostUpdate,
                update
                    .in_set(super::PlaceSpatialEffects)
                    .before(bevy::transform::TransformSystems::Propagate),
            );
    }
}

fn update(world: &mut World) {
    let existing: HashMap<_, _> = world
        .query::<(Entity, &SphereEffect)>()
        .iter(world)
        .map(|(entity, effect)| (effect.owner, entity))
        .collect();
    for (owner, entity) in &existing {
        if world.get::<Floating>(*owner).is_none() {
            let material = world.get::<SphereEffect>(*entity).unwrap().material.id();
            world
                .resource_mut::<Assets<SphereMaterial>>()
                .remove(material);
            world.despawn(*entity);
        }
    }
    let floating: Vec<_> = world
        .query::<(Entity, &Floating)>()
        .iter(world)
        .map(|(entity, effect)| (entity, effect.record, effect.preview))
        .collect();
    for (owner, record, preview) in floating {
        let Some(data) = world.get::<RecordData>(record) else {
            continue;
        };
        let Some(config) = crate::native::protein_area::configuration(world, data.binding.area)
        else {
            continue;
        };
        let settings = placement::effective(world, &data.binding, &config.shader_placement);
        let Some(root) = root(world, owner) else {
            continue;
        };
        let spatial = world
            .get::<crate::native::topology::view::View>(root)
            .is_some_and(|view| view.spatial);
        let showing = !disabled(world)
            && world
                .get::<shader::EffectPreview>(preview)
                .is_some_and(|preview| !preview.disabled)
            && world
                .get::<crate::native::topology::presentation::Surface>(owner)
                .is_some_and(|surface| surface.visible);
        let visible = spatial && settings.sphere && showing;
        if let Some(body) = world
            .get::<crate::native::topology::presentation::Surface>(owner)
            .map(|surface| surface.body)
            && let Some(mut visibility) = world.get_mut::<Visibility>(body)
        {
            *visibility = Visibility::Hidden;
        }
        let mesh_position = crate::native::topology::position(world, owner).unwrap_or(DVec3::ZERO)
            - crate::native::topology::presentation::origin(world, root);
        let mesh_transform = Transform {
            translation: mesh_position.as_vec3(),
            rotation: crate::native::topology::spatial(world, owner)
                .rotation()
                .as_quat(),
            scale: Vec3::splat(settings.size),
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
                && let Some(mut visibility) = world.get_mut::<Visibility>(visual)
            {
                *visibility = Visibility::Hidden;
            }
            let output = shader::output(world, preview);
            world.get_mut::<ImageNode>(output).unwrap().color =
                if spatial { Color::NONE } else { Color::WHITE };
            continue;
        }
        if (!showing || spatial && settings.sphere)
            && let Some(visual) = world
                .get::<crate::native::topology::presentation::Surface>(owner)
                .map(|surface| surface.visual)
            && let Some(mut visibility) = world.get_mut::<Visibility>(visual)
        {
            *visibility = Visibility::Hidden;
        }
        let output = shader::output(world, preview);
        let Some(image) = world
            .get::<ImageNode>(output)
            .map(|node| node.image.clone())
        else {
            continue;
        };
        world.get_mut::<ImageNode>(output).unwrap().color = if spatial && settings.sphere {
            Color::NONE
        } else {
            Color::WHITE
        };
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
                .add(SphereMaterial { image });
            world
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(material.clone()),
                    SphereEffect { owner, material },
                    RecordSphere(record),
                    Transform::default(),
                    Visibility::Hidden,
                    bevy::picking::Pickable::IGNORE,
                    crate::native::inspection::InspectionExcluded,
                ))
                .id()
        };
        let position = crate::native::topology::position(world, owner).unwrap_or(DVec3::ZERO)
            - crate::native::topology::presentation::origin(world, root);
        let rotation = crate::native::topology::spatial(world, record)
            .rotation()
            .as_quat();
        world.entity_mut(entity).insert((
            Transform {
                translation: position.as_vec3(),
                rotation,
                scale: Vec3::splat(settings.size),
            },
            if visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            },
        ));
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
