use super::*;
use std::any::TypeId;

#[test]
fn capture_filters_inherited_ui_mesh_camera_and_shadow_contributors_then_restores_them() {
    let mut world = World::new();
    let mut layout = crate::native::layout::LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let private = world.spawn((layout, Visibility::Visible, InheritedVisibility::VISIBLE, ViewVisibility::VISIBLE)).id();
    let child = world.spawn((Visibility::Inherited, InheritedVisibility::VISIBLE, ViewVisibility::VISIBLE, ChildOf(private))).id();
    let public = world.spawn((crate::native::layout::LayoutBox::new(Vec2::splat(100.0)), Visibility::Visible, InheritedVisibility::VISIBLE, ViewVisibility::VISIBLE)).id();
    let mut visible = VisibleEntities::default();
    visible.get_mut(TypeId::of::<Mesh3d>()).extend([private, child, public]);
    let camera = world.spawn(visible).id();
    let private_camera = world.spawn((Camera::default(), crate::native::topology::presentation::VisualOwner(private))).id();
    let mut cubes = CubemapVisibleEntities::default();
    cubes.get_mut(0).entities = vec![private, public];
    let cube = world.spawn(cubes).id();
    let mesh = world.spawn(VisibleMeshEntities { entities: vec![child, public] }).id();
    let mut cascades = CascadesVisibleEntities::default();
    cascades.entities.insert(camera, vec![VisibleMeshEntities { entities: vec![private, public] }]);
    let cascade = world.spawn(cascades).id();
    let filter = Filter::apply(&mut world, &BTreeSet::new()).unwrap();
    assert!(!world.get::<InheritedVisibility>(child).unwrap().get());
    assert!(!world.get::<ViewVisibility>(private).unwrap().get());
    assert!(!world.get::<Camera>(private_camera).unwrap().is_active);
    assert_eq!(world.get::<VisibleEntities>(camera).unwrap().get(TypeId::of::<Mesh3d>()), [public]);
    assert_eq!(world.get::<CubemapVisibleEntities>(cube).unwrap().get(0).entities, [public]);
    assert_eq!(world.get::<VisibleMeshEntities>(mesh).unwrap().entities, [public]);
    assert_eq!(world.get::<CascadesVisibleEntities>(cascade).unwrap().entities[&camera][0].entities, [public]);
    assert!(world.get::<ViewVisibility>(public).unwrap().get());
    filter.restore(&mut world);
    assert!(world.get::<InheritedVisibility>(child).unwrap().get());
    assert!(world.get::<ViewVisibility>(private).unwrap().get());
    assert!(world.get::<Camera>(private_camera).unwrap().is_active);
    assert_eq!(world.get::<Visibility>(child), Some(&Visibility::Inherited));
    assert_eq!(world.get::<VisibleEntities>(camera).unwrap().get(TypeId::of::<Mesh3d>()), [private, child, public]);
    assert_eq!(world.get::<CubemapVisibleEntities>(cube).unwrap().get(0).entities, [private, public]);
    assert_eq!(world.get::<VisibleMeshEntities>(mesh).unwrap().entities, [child, public]);
}

#[test]
fn transient_private_roots_and_disabled_cached_contributors_are_filtered_without_changing_saved_policy() {
    let mut world = World::new();
    let parent = world.spawn((Visibility::Visible, InheritedVisibility::VISIBLE)).id();
    let layout = crate::native::layout::LayoutBox::new(Vec2::splat(100.0));
    let child = world.spawn((layout, Disabled, Visibility::Visible, InheritedVisibility::VISIBLE, ChildOf(parent))).id();
    let filter = Filter::apply(&mut world, &BTreeSet::from([parent])).unwrap();
    assert!(!world.get::<InheritedVisibility>(child).unwrap().get());
    assert_eq!(world.get::<crate::native::layout::LayoutBox>(child), Some(&layout));
    filter.restore(&mut world);
    assert!(world.get::<InheritedVisibility>(child).unwrap().get());
    assert_eq!(world.get::<crate::native::layout::LayoutBox>(child), Some(&layout));
}

#[test]
fn cached_targets_require_public_source_provenance_for_every_writer_even_when_inactive() {
    let mut world = World::new();
    let mut images = Assets::<Image>::default();
    let public_image = images.add(Image::default());
    let private_image = images.add(Image::default());
    let public = world.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    let mut layout = crate::native::layout::LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let private = world.spawn(layout).id();
    let public_camera = world.spawn((Camera { is_active: false, ..default() }, TargetSource(public), RenderTarget::Image(public_image.clone().into()))).id();
    world.spawn((Camera { is_active: false, ..default() }, TargetSource(private), RenderTarget::Image(private_image.clone().into()), Disabled));
    assert_eq!(reusable_targets(&mut world, &BTreeSet::new()), BTreeSet::from([public_image.id()]));
    assert!(reusable_targets(&mut world, &BTreeSet::from([public])).is_empty());
    let unknown = world.spawn((Camera::default(), RenderTarget::Image(public_image.clone().into()))).id();
    assert!(reusable_targets(&mut world, &BTreeSet::new()).is_empty());
    world.despawn(unknown);
    let private_alias = world.spawn((Camera::default(), TargetSource(private), RenderTarget::Image(public_image.clone().into()))).id();
    assert!(reusable_targets(&mut world, &BTreeSet::new()).is_empty());
    world.despawn(private_alias);
    assert_eq!(reusable_targets(&mut world, &BTreeSet::new()), BTreeSet::from([public_image.id()]));
    world.despawn(public);
    assert!(super::super::excluded(&world, public_camera));
    assert!(reusable_targets(&mut world, &BTreeSet::new()).is_empty());
}

#[test]
fn detached_shader_meshes_spheres_and_preview_targets_follow_all_semantic_sources() {
    let mut world = World::new();
    let public = world.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    let mut layout = crate::native::layout::LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let private = world.spawn(layout).id();
    let mesh = world.spawn((crate::native::shader_effects::RecordMesh(private), Visibility::Visible)).id();
    let sphere = world.spawn((crate::native::shader_effects::RecordSphere(private), Visibility::Visible)).id();
    let mixed = world.spawn((crate::native::shader_effects::RecordMesh(public), crate::native::shader_effects::RecordSphere(private), Visibility::Visible)).id();
    let preview = world.spawn((TargetSource(private), Visibility::Visible, Camera::default())).id();
    let public_preview = world.spawn((TargetSource(public), Visibility::Visible, Camera::default())).id();
    let filter = Filter::apply(&mut world, &BTreeSet::new()).unwrap();
    for entity in [mesh, sphere, mixed, preview] { assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Hidden)); }
    assert!(!world.get::<Camera>(preview).unwrap().is_active);
    assert!(world.get::<Camera>(public_preview).unwrap().is_active);
    filter.restore(&mut world);
    for entity in [mesh, sphere, mixed, preview, public_preview] { assert_eq!(world.get::<Visibility>(entity), Some(&Visibility::Visible)); }
    assert!(world.get::<Camera>(preview).unwrap().is_active);
}

#[test]
fn retained_image_sources_keep_a_removed_private_child_out_of_the_public_fast_path() {
    use crate::native::topology::surface_render::{CaptureOrigins, CompletedCaptures};
    use std::{collections::BTreeMap, sync::Arc};
    let mut world = World::new();
    let mut images = Assets::<Image>::default();
    let current = images.add(Image::default());
    let retained = images.add(Image::default());
    let public = world.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    let mut layout = crate::native::layout::LayoutBox::new(Vec2::splat(100.0));
    layout.capture_excluded = true;
    let private = world.spawn((layout, ChildOf(public))).id();
    let camera = world.spawn((Camera { is_active: false, ..default() }, TargetSource(public), RenderTarget::Image(current.clone().into()))).id();
    let completed = CompletedCaptures::default();
    completed.seed(BTreeMap::from([
        (current.id(), Arc::new(CaptureOrigins { contributors: BTreeSet::from([camera, public]), images: BTreeSet::new(), complete: true })),
        (retained.id(), Arc::new(CaptureOrigins { contributors: BTreeSet::from([camera, public, private]), images: BTreeSet::from([current.id()]), complete: true })),
    ]));
    world.insert_resource(completed);
    world.despawn(private);
    let targets = Targets::collect(&mut world, &BTreeSet::new()).unwrap();
    assert!(targets.images.contains(&retained.id()));
    assert!(targets.reusable.contains(&current.id()));
    assert!(!targets.reusable.contains(&retained.id()));
    assert!(targets.private_entities.contains(&private.to_bits()));
    let gate = crate::host::capture::PolicyGate::default();
    gate.publish(crate::host::capture::Policy { private_entities: targets.private_entities.clone(), unresolved_images: targets.unresolved, ..default() });
    assert!(gate.read().unwrap().requires_filter());
}

#[test]
fn unsafe_old_alias_waits_for_a_real_filtered_completion_without_changing_owner_history() {
    use crate::native::topology::surface_render::{CaptureOrigins, CompletedCaptures};
    use std::{collections::BTreeMap, sync::Arc};
    let mut main = World::new();
    let mut render = World::new();
    let mut images = Assets::<Image>::default();
    let alias = images.add(Image::default());
    let public = main.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    let missing = main.spawn_empty().id();
    main.despawn(missing);
    let normal = CompletedCaptures::default();
    normal.seed(BTreeMap::from([(alias.id(), Arc::new(CaptureOrigins { contributors: BTreeSet::from([public, missing]), images: BTreeSet::new(), complete: true }))]));
    main.insert_resource(normal.clone());
    let targets = Targets::collect(&mut main, &BTreeSet::new()).unwrap();
    let safe = CompletedCaptures::default();
    render.insert_resource(safe.clone());
    targets.seed(&mut render);
    assert!(targets.validate(&main, &render, &[alias.id()], &BTreeSet::new()).is_err());
    safe.seed(BTreeMap::from([(alias.id(), Arc::new(CaptureOrigins { contributors: BTreeSet::from([public]), images: BTreeSet::new(), complete: true }))]));
    targets.validate(&main, &render, &[alias.id()], &BTreeSet::new()).unwrap();
    assert!(normal.snapshot().unwrap()[&alias.id()].contributors.contains(&missing));
}

#[test]
fn a_target_reference_without_a_camera_is_neither_a_writer_nor_an_owned_image_to_clear() {
    let mut world = World::new();
    let mut images = Assets::<Image>::default();
    let owned = images.add(Image::default());
    let client = images.add(Image::default());
    let source = world.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    world.spawn((Camera::default(), TargetSource(source), RenderTarget::Image(owned.clone().into())));
    world.spawn(RenderTarget::Image(owned.clone().into()));
    world.spawn(RenderTarget::Image(client.clone().into()));
    let targets = Targets::collect(&mut world, &BTreeSet::new()).unwrap();
    assert!(targets.reusable.contains(&owned.id()));
    assert!(!targets.images.contains(&client.id()));
}

#[test]
fn retired_image_history_can_be_redacted_but_an_eligible_current_writer_still_needs_a_real_filtered_completion() {
    use crate::native::topology::surface_render::{CaptureOrigins, CompletedCaptures};
    use std::{collections::BTreeMap, sync::Arc};
    let mut world = World::new();
    let mut images = Assets::<Image>::default();
    let image = images.add(Image::default());
    let retired = world.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    world.despawn(retired);
    let completed = CompletedCaptures::default();
    completed.seed(BTreeMap::from([(image.id(), Arc::new(CaptureOrigins {
        contributors: BTreeSet::from([retired]), images: BTreeSet::new(), complete: true,
    }))]));
    world.insert_resource(completed.clone());
    let targets = Targets::collect(&mut world, &BTreeSet::new()).unwrap();
    let mut render = World::new();
    render.init_resource::<CompletedCaptures>();
    targets.seed(&mut render);
    targets.validate(&world, &render, &[image.id()], &BTreeSet::new()).unwrap();
    assert!(completed.snapshot().unwrap()[&image.id()].contributors.contains(&retired));
    let public = world.spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    world.spawn((Camera::default(), TargetSource(public), RenderTarget::Image(image.clone().into())));
    let targets = Targets::collect(&mut world, &BTreeSet::new()).unwrap();
    targets.seed(&mut render);
    assert!(targets.validate(&world, &render, &[image.id()], &BTreeSet::new()).is_err());
    assert!(completed.snapshot().unwrap()[&image.id()].contributors.contains(&retired));
}
