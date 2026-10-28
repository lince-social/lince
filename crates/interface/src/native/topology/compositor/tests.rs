use super::*;

#[test]
fn shadow_uses_the_actual_triangle_boundary_and_expands_it_without_mutating_source_geometry() {
    let mesh = Mesh::from(Rectangle::new(200.0, 100.0));
    let VertexAttributeValues::Float32x3(before) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().clone() else { panic!("Expected source positions"); };
    let (shadow, parameters, _) = shape(&mesh, Vec2::new(200.0, 100.0), 12.0).unwrap();
    assert_eq!(parameters.bounds, Vec4::new(-36.0, -36.0, 236.0, 136.0));
    assert_eq!(parameters.style.z, 4.0);
    let VertexAttributeValues::Float32x3(after) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!("Expected retained source positions"); };
    assert_eq!(after, &before);
    let VertexAttributeValues::Float32x3(positions) = shadow.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() else { panic!("Expected actual shadow vertices"); };
    let low = positions.iter().map(|point| Vec3::from_array(*point)).fold(Vec3::splat(f32::INFINITY), Vec3::min);
    let high = positions.iter().map(|point| Vec3::from_array(*point)).fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
    assert!((low.x + 136.0).abs() < 0.001 && (high.x - 136.0).abs() < 0.001);
    assert!((low.y + 86.0).abs() < 0.001 && (high.y - 86.0).abs() < 0.001);
}

#[test]
fn clipped_holes_keep_both_boundaries_while_nonfinite_and_excess_geometry_is_refused() {
    let points = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]];
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points.map(|[x, y]| [x * 200.0, y * 100.0, 0.0]).to_vec())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, points.to_vec())
        .with_inserted_indices(Indices::U32(vec![0, 1, 5, 0, 5, 4, 1, 2, 6, 1, 6, 5, 2, 3, 7, 2, 7, 6, 3, 0, 4, 3, 4, 7]));
    let (_, parameters, _) = shape(&mesh, Vec2::new(200.0, 100.0), 8.0).unwrap();
    assert_eq!(parameters.style.z, 8.0);
    let perimeter = parameters.edges[..8].iter().map(|edge| Vec2::new(edge.z - edge.x, edge.w - edge.y).length()).sum::<f32>();
    assert!((perimeter - 720.0).abs() < 0.001);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[f32::NAN, 0.0]; 8]);
    assert!(shape(&mesh, Vec2::new(200.0, 100.0), 8.0).is_none());
    let mut mesh = Mesh::from(Rectangle::new(200.0, 100.0));
    mesh.insert_indices(Indices::U16(vec![0; 6147]));
    assert!(shape(&mesh, Vec2::new(200.0, 100.0), 8.0).is_none());
}

#[test]
fn optional_shadow_obeys_existing_budget_and_owner_layers_then_releases_with_its_surface() {
    let mut world = World::new();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<StandardMaterial>>();
    world.init_resource::<Assets<ShadowMaterial>>();
    world.insert_resource(crate::native::shader_effects::ShaderSettings {
        compositor: crate::native::shader_effects::compositor::Settings { shadow_radius: 12.0, ..Default::default() }, ..Default::default()
    });
    world.insert_resource(crate::native::topology::surface_budget::Diagnostics { granted_bytes: crate::native::topology::surface_budget::BYTE_BUDGET, ..Default::default() });
    let owner = world.spawn_empty().id();
    let mesh = world.resource_mut::<Assets<Mesh>>().add(Rectangle::new(200.0, 100.0));
    let material = world.resource_mut::<Assets<StandardMaterial>>().add(StandardMaterial::default());
    let layers = RenderLayers::layer(5);
    let camera = world.spawn((Camera::default(), crate::native::topology::stacking::OrderedCamera, layers.clone())).id();
    let face = world.spawn((SurfaceFace, Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), VisualOwner(owner), layers.clone(), Transform::default())).id();
    world.entity_mut(owner).insert((crate::native::topology::presentation::DisplayedImage(Handle::default()), Surface {
        camera: face, image: Handle::default(), visual: face, body: face, face, size: Vec2::new(200.0, 100.0),
        pixels: UVec2::new(200, 100), density: 1.0, material, uv: Rect::from_corners(Vec2::ZERO, Vec2::ONE), visible: true,
    }));
    synchronize(&mut world);
    assert!(world.get::<Decoration>(face).is_none());
    assert_eq!(world.resource::<Status>().shadow_limited, 1);
    assert!(world.resource::<Status>().shadow_limits.contains(&Limitation::Budget));
    world.resource_mut::<crate::native::topology::surface_budget::Diagnostics>().granted_bytes = 0;
    synchronize(&mut world);
    let shadow = world.get::<Decoration>(face).unwrap().entity;
    assert_eq!(world.get::<VisualOwner>(shadow).unwrap().0, owner);
    assert_eq!(world.get::<ChildOf>(shadow).unwrap().parent(), face);
    assert_eq!(world.get::<RenderLayers>(shadow), Some(&layers));
    assert!(!world.get::<Pickable>(shadow).unwrap().is_hoverable);
    assert_eq!(world.get::<Mesh3d>(face).unwrap().0, mesh);
    assert_eq!(world.get::<Surface>(owner).unwrap().size, Vec2::new(200.0, 100.0));
    world.get_mut::<Camera>(camera).unwrap().is_active = false;
    synchronize(&mut world);
    assert!(world.get_entity(shadow).is_err());
    assert!(world.get::<Decoration>(face).is_none());
    world.get_mut::<Camera>(camera).unwrap().is_active = true;
    world.entity_mut(camera).insert(RenderLayers::layer(6));
    synchronize(&mut world);
    assert!(world.get::<Decoration>(face).is_none());
    world.entity_mut(camera).insert(layers);
    synchronize(&mut world);
    crate::native::shader_effects::set_compositor_presentation_active(&mut world, false);
    synchronize(&mut world);
    assert!(world.get::<Decoration>(face).is_none());
    assert_eq!(world.resource::<crate::native::shader_effects::ShaderSettings>().compositor.shadow_radius, 12.0);
    crate::native::shader_effects::set_compositor_presentation_active(&mut world, true);
    synchronize(&mut world);
    let visual = world.spawn(Visibility::Hidden).id();
    world.entity_mut(face).insert(ChildOf(visual));
    synchronize(&mut world);
    assert!(world.get::<Decoration>(face).is_none());
    *world.get_mut::<Visibility>(visual).unwrap() = Visibility::Visible;
    synchronize(&mut world);
    let shadow = world.get::<Decoration>(face).unwrap().entity;
    world.get_mut::<Surface>(owner).unwrap().visible = false;
    synchronize(&mut world);
    assert!(world.get_entity(shadow).is_err());
    assert!(world.get::<Decoration>(face).is_none());
}
