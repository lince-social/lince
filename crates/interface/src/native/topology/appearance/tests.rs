use super::super::presentation::Surface;
use super::*;

#[test]
fn mesh_textures_share_the_surface_quota_and_release_after_the_last_lease() {
    let (mut world, _) = scene(0);
    let mut runtime = Runtime::default();
    let keys: Vec<_> = (0..model::MAX_TEXTURES)
        .map(|index| model::Texture::Asset {
            id: format!("{index:032x}"),
        })
        .collect();
    let mut handles = Vec::new();
    for key in &keys {
        let handle = world.resource_mut::<Assets<Image>>().add(Image::default());
        runtime.textures.insert(key.clone(), handle.clone());
        runtime.texture_keys.insert(key.clone());
        handles.push(handle);
    }
    world.insert_resource(runtime);
    assert!(mesh_texture(&mut world, &model::Texture::Tiles).is_none());
    for (key, handle) in keys.iter().zip(&handles) {
        assert_eq!(mesh_texture(&mut world, key).as_ref(), Some(handle));
    }
    update(&mut world);
    assert_eq!(
        world.resource::<Runtime>().textures.len(),
        model::MAX_TEXTURES
    );
    assert!(world.resource::<Runtime>().mesh_textures.is_empty());
    update(&mut world);
    assert!(world.resource::<Runtime>().textures.is_empty());
    assert!(
        handles
            .iter()
            .all(|handle| !world.resource::<Assets<Image>>().contains(handle.id()))
    );
    assert!(mesh_texture(&mut world, &model::Texture::Tiles).is_some());
    assert_eq!(world.resource::<Runtime>().textures.len(), 1);
}

fn scene(count: usize) -> (World, Vec<Entity>) {
    let mut world = World::new();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<StandardMaterial>>();
    world.init_resource::<crate::native::tokens::ThemeSettings>();
    let mut owners = Vec::new();
    for _ in 0..count {
        let visual = world
            .spawn((Transform::default(), Visibility::Inherited))
            .id();
        let original = world
            .resource_mut::<Assets<StandardMaterial>>()
            .add(StandardMaterial::default());
        let cube = world.resource_mut::<Assets<Mesh>>().add(Cuboid::default());
        let rectangle = world
            .resource_mut::<Assets<Mesh>>()
            .add(Rectangle::default());
        let body = world
            .spawn((
                Mesh3d(cube),
                MeshMaterial3d(original.clone()),
                Transform::default(),
                ChildOf(visual),
            ))
            .id();
        let face = world
            .spawn((
                Mesh3d(rectangle),
                MeshMaterial3d(original.clone()),
                Transform::default(),
                ChildOf(visual),
            ))
            .id();
        let camera = world.spawn_empty().id();
        owners.push(
            world
                .spawn((
                    Surface {
                        camera,
                        image: Handle::default(),
                        visual,
                        body,
                        face,
                        size: Vec2::splat(100.0),
                        pixels: UVec2::splat(100),
                        density: 1.0,
                        material: original,
                        uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE),
                        visible: true,
                    },
                    Material(model::Material {
                        lit: true,
                        texture: model::Texture::Tiles,
                        ..default()
                    }),
                    Light(model::Light {
                        shadows: model::ShadowQuality::High,
                        ..default()
                    }),
                ))
                .id(),
        );
    }
    (world, owners)
}

#[test]
fn lighting_budgets_are_bounded_and_hidden_owners_release_resources() {
    let (mut world, owners) = scene(12);
    update(&mut world);
    let runtime = world.resource::<Runtime>();
    assert_eq!(runtime.lights.len(), model::MAX_LIGHTS);
    assert_eq!(runtime.materials.len(), 12);
    assert_eq!(runtime.textures.len(), 1);
    assert_eq!(
        world
            .query::<&PointLight>()
            .iter(&world)
            .filter(|light| light.shadow_maps_enabled)
            .count(),
        model::MAX_SHADOW_LIGHTS
    );
    assert_eq!(world.resource::<PointLightShadowMap>().size, 1024);
    for owner in owners {
        world.get_mut::<Surface>(owner).unwrap().visible = false;
    }
    update(&mut world);
    let runtime = world.resource::<Runtime>();
    assert!(runtime.lights.is_empty());
    assert!(runtime.materials.is_empty());
    assert!(runtime.textures.is_empty());
    assert_eq!(world.query::<&PointLight>().iter(&world).count(), 0);
    assert_eq!(world.resource::<Assets<Image>>().len(), 0);
    assert_eq!(world.resource::<Assets<StandardMaterial>>().len(), 12);
}

#[test]
fn excess_material_surfaces_keep_the_default_capture_and_slots_are_reused() {
    let (mut world, owners) = scene(80);
    update(&mut world);
    assert_eq!(
        world.resource::<Runtime>().materials.len(),
        model::MAX_MATERIALS
    );
    assert_eq!(
        world.query::<&Applied>().iter(&world).count(),
        model::MAX_MATERIALS
    );
    let selected: Vec<_> = world
        .resource::<Runtime>()
        .materials
        .keys()
        .copied()
        .collect();
    for owner in selected {
        world.get_mut::<Surface>(owner).unwrap().visible = false;
    }
    update(&mut world);
    assert_eq!(
        world.resource::<Runtime>().materials.len(),
        owners.len() - model::MAX_MATERIALS
    );
    assert_eq!(
        world.query::<&Applied>().iter(&world).count(),
        owners.len() - model::MAX_MATERIALS
    );
}

#[test]
fn material_updates_keep_content_unlit_and_restore_original_body_on_disable() {
    let (mut world, owners) = scene(1);
    let owner = owners[0];
    let text = world
        .spawn((Text::new("Readable content"), ChildOf(owner)))
        .id();
    let body = world.get::<Surface>(owner).unwrap().body;
    let face = world.get::<Surface>(owner).unwrap().face;
    let original = world
        .get::<MeshMaterial3d<StandardMaterial>>(body)
        .unwrap()
        .0
        .clone();
    update(&mut world);
    let runtime = world.resource::<Runtime>();
    let material = runtime.materials[&owner].material.clone();
    let plate = runtime.materials[&owner].plate;
    assert!(
        !world
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .unwrap()
            .unlit
    );
    assert_eq!(
        world
            .get::<MeshMaterial3d<StandardMaterial>>(face)
            .unwrap()
            .0,
        original
    );
    assert!(world.get::<NotShadowCaster>(face).is_some());
    assert_eq!(
        world
            .get::<bevy::text::TextBackgroundColor>(text)
            .unwrap()
            .0
            .alpha(),
        1.0
    );
    let light_entity = world.resource::<Runtime>().lights[&owner];
    let initial_intensity = world.get::<PointLight>(light_entity).unwrap().intensity;
    world
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&original)
        .unwrap()
        .base_color = Color::linear_rgba(0.35, 0.35, 0.35, 0.35);
    update(&mut world);
    assert_eq!(
        world
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .unwrap()
            .base_color
            .alpha(),
        0.35
    );
    assert!(
        (world.get::<PointLight>(light_entity).unwrap().intensity / initial_intensity - 0.35).abs()
            < 1e-6
    );
    world
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&original)
        .unwrap()
        .base_color
        .set_alpha(0.0);
    update(&mut world);
    assert!(world.resource::<Runtime>().lights.is_empty());
    assert!(world.get::<NotShadowCaster>(body).is_some());
    world
        .resource_mut::<Assets<StandardMaterial>>()
        .get_mut(&original)
        .unwrap()
        .base_color
        .set_alpha(0.35);
    update(&mut world);
    assert!(world.get::<NotShadowCaster>(body).is_none());
    world.get_mut::<Material>(owner).unwrap().0.cast_shadows = false;
    world.get_mut::<Material>(owner).unwrap().0.emissive = 4.0;
    update(&mut world);
    assert!(world.get::<NotShadowCaster>(body).is_some());
    assert!(world.get::<NotShadowCaster>(plate).is_some());
    assert!(
        world
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .unwrap()
            .emissive
            .red
            > 0.0
    );
    world.entity_mut(owner).remove::<(Material, Light)>();
    update(&mut world);
    assert_eq!(
        world
            .get::<MeshMaterial3d<StandardMaterial>>(body)
            .unwrap()
            .0,
        original
    );
    assert!(world.get_entity(plate).is_err());
    assert!(
        world
            .resource::<Assets<StandardMaterial>>()
            .get(&material)
            .is_none()
    );
    assert!(world.get::<bevy::text::TextBackgroundColor>(text).is_none());
}

#[test]
fn replaced_surface_and_deleted_visual_recover_without_stale_entities() {
    let (mut world, owners) = scene(1);
    let owner = owners[0];
    update(&mut world);
    let visual = world.get::<Surface>(owner).unwrap().visual;
    world.despawn(visual);
    let replacement = world
        .spawn((Transform::default(), Visibility::Inherited))
        .id();
    let material = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial::default());
    let mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Rectangle::default());
    let body = world
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            ChildOf(replacement),
        ))
        .id();
    let face = world
        .spawn((
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::default(),
            ChildOf(replacement),
        ))
        .id();
    let mut surface = world.get_mut::<Surface>(owner).unwrap();
    surface.visual = replacement;
    surface.body = body;
    surface.face = face;
    update(&mut world);
    let runtime = world.resource::<Runtime>();
    assert_eq!(runtime.materials[&owner].body, body);
    assert_eq!(
        world
            .get::<Transform>(runtime.lights[&owner])
            .unwrap()
            .translation,
        Vec3::new(0.0, 120.0, 0.0)
    );
    world.despawn(owner);
    update(&mut world);
    assert!(world.resource::<Runtime>().lights.is_empty());
    assert!(world.resource::<Runtime>().materials.is_empty());
}

#[test]
fn missing_task_mapping_suspends_driven_light_and_shadow_map_is_restored() {
    let (mut world, owners) = scene(1);
    world.insert_resource(PointLightShadowMap { size: 2048 });
    update(&mut world);
    assert_eq!(world.resource::<PointLightShadowMap>().size, 1024);
    world.get_mut::<Light>(owners[0]).unwrap().0.drive = LightDrive::Progress;
    update(&mut world);
    assert!(world.resource::<Runtime>().lights.is_empty());
    assert_eq!(world.resource::<PointLightShadowMap>().size, 2048);
}

#[test]
fn offscreen_emitters_keep_local_lights_but_inactive_workspaces_and_disable_release_them() {
    let (mut world, owners) = scene(1);
    let owner = owners[0];
    let root = world
        .spawn(crate::native::workspace::Workspaces::default())
        .id();
    world
        .entity_mut(owner)
        .insert((ChildOf(root), crate::native::workspace::WorkspaceMember(1)));
    let visual = world.get::<Surface>(owner).unwrap().visual;
    *world.get_mut::<Transform>(visual).unwrap() = Transform::from_xyz(300.0, 40.0, -10.0)
        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
        .with_scale(Vec3::splat(2.0));
    *world.get_mut::<Visibility>(visual).unwrap() = Visibility::Hidden;
    world.get_mut::<Surface>(owner).unwrap().visible = false;
    update(&mut world);
    let light = world.resource::<Runtime>().lights[&owner];
    assert!(world.get::<ChildOf>(light).is_none());
    assert_eq!(
        *world.get::<Visibility>(light).unwrap(),
        Visibility::Visible
    );
    assert!(
        (world.get::<Transform>(light).unwrap().translation - Vec3::new(60.0, 40.0, -10.0))
            .length()
            < 0.001
    );
    world
        .get_mut::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active = 2;
    update(&mut world);
    assert!(world.resource::<Runtime>().lights.is_empty());
    world
        .get_mut::<crate::native::workspace::Workspaces>(root)
        .unwrap()
        .active = 1;
    update(&mut world);
    assert_eq!(world.resource::<Runtime>().lights.len(), 1);
    world
        .entity_mut(owner)
        .insert(bevy::ecs::entity_disabling::Disabled);
    update(&mut world);
    assert!(world.resource::<Runtime>().lights.is_empty());
}

#[test]
fn rounded_material_geometry_is_cached_resized_clipped_and_restored_on_disable() {
    use bevy::mesh::{PrimitiveTopology, VertexAttributeValues};
    let (mut world, owners) = scene(1);
    let owner = owners[0];
    let body = world.get::<Surface>(owner).unwrap().body;
    let face = world.get::<Surface>(owner).unwrap().face;
    let original = world.get::<Mesh3d>(body).unwrap().0.clone();
    world.entity_mut(owner).insert(ComputedNode {
        size: Vec2::splat(100.0),
        border_radius: bevy::ui::ResolvedBorderRadius {
            top_left: 20.0,
            top_right: 20.0,
            bottom_left: 20.0,
            bottom_right: 20.0,
        },
        ..default()
    });
    update(&mut world);
    let plate = world.resource::<Runtime>().materials[&owner].plate;
    let plate_mesh = world.get::<Mesh3d>(plate).unwrap().0.clone();
    let body_mesh = world.get::<Mesh3d>(body).unwrap().0.clone();
    assert_ne!(body_mesh, original);
    let positions = |world: &World, handle: &Handle<Mesh>| {
        let VertexAttributeValues::Float32x3(points) = world
            .resource::<Assets<Mesh>>()
            .get(handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
        else {
            panic!("positions");
        };
        points.clone()
    };
    let before = positions(&world, &plate_mesh);
    assert!(before.len() > 6);
    for points in before.chunks_exact(3) {
        let [a, b, c] = [
            Vec3::from_array(points[0]),
            Vec3::from_array(points[1]),
            Vec3::from_array(points[2]),
        ];
        assert!((b - a).cross(c - a).z >= -1e-6);
    }
    assert!(
        before
            .iter()
            .all(|point| point[0].abs() < 0.499 || point[1].abs() < 0.499)
    );
    assert!(
        positions(&world, &body_mesh)
            .iter()
            .all(|point| point[0].abs() < 0.499 || point[2].abs() < 0.499)
    );
    let mesh_tick = world
        .get_resource_ref::<Assets<Mesh>>()
        .unwrap()
        .last_changed();
    world.increment_change_tick();
    update(&mut world);
    assert_eq!(
        world
            .get_resource_ref::<Assets<Mesh>>()
            .unwrap()
            .last_changed(),
        mesh_tick
    );
    world.get_mut::<ComputedNode>(owner).unwrap().size.x = 200.0;
    update(&mut world);
    assert_eq!(world.get::<Mesh3d>(plate).unwrap().0, plate_mesh);
    assert_ne!(positions(&world, &plate_mesh), before);
    let source = world.get::<Mesh3d>(face).unwrap().0.clone();
    let triangle = Mesh::new(
        PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[-0.5, 0.5, 0.0], [-0.5, -0.5, 0.0], [0.5, 0.5, 0.0]],
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 3])
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]],
    );
    world
        .resource_mut::<Assets<Mesh>>()
        .insert(source.id(), triangle)
        .unwrap();
    update(&mut world);
    let mesh = world.resource::<Assets<Mesh>>().get(&plate_mesh).unwrap();
    let VertexAttributeValues::Float32x2(uvs) = mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap()
    else {
        panic!("uvs");
    };
    assert!(uvs.iter().all(|uv| uv[0] + uv[1] <= 1.00001));
    world.entity_mut(owner).remove::<Material>();
    update(&mut world);
    assert_eq!(world.get::<Mesh3d>(body).unwrap().0, original);
    assert!(world.resource::<Assets<Mesh>>().get(&plate_mesh).is_none());
    assert!(world.resource::<Assets<Mesh>>().get(&body_mesh).is_none());
}
