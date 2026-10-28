use super::*;

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<CompletedCaptures>().add_systems(Update, finish);
    app
}

fn source(entity: Entity) -> CaptureOrigins {
    CaptureOrigins { contributors: BTreeSet::from([entity]), images: BTreeSet::new(), complete: true }
}

#[test]
fn completion_records_actual_draw_order_and_keeps_sampled_provenance_after_an_input_changes() {
    let mut app = app();
    let mut images = Assets::<Image>::default();
    let input = images.add(Image::default());
    let parent = images.add(Image::default());
    let private = app.world_mut().spawn_empty().id();
    let public = app.world_mut().spawn_empty().id();
    let output = app.world_mut().spawn_empty().id();
    let completed = app.world().resource::<CompletedCaptures>().clone();
    let mut sampled = source(output);
    sampled.images.insert(input.id());
    completed.queue(parent.id(), (0, 0), true, sampled);
    completed.queue(input.id(), (-3, 0), true, source(private));
    app.update();
    let first = completed.snapshot().unwrap();
    assert!(first[&parent.id()].complete);
    assert!(first[&parent.id()].contributors.contains(&private));
    completed.queue(input.id(), (-3, 0), true, source(public));
    app.update();
    let second = completed.snapshot().unwrap();
    assert!(!second[&input.id()].contributors.contains(&private));
    assert!(second[&parent.id()].contributors.contains(&private));
    assert!(!second[&parent.id()].contributors.contains(&public));
    assert!(completed.contains(&parent));
    app.update();
    assert!(Arc::ptr_eq(&second[&parent.id()], &completed.snapshot().unwrap()[&parent.id()]));
}

#[test]
fn a_load_keeps_prior_sources_and_only_an_acknowledged_clear_replaces_them() {
    let mut app = app();
    let mut images = Assets::<Image>::default();
    let image = images.add(Image::default());
    let old = app.world_mut().spawn_empty().id();
    let new = app.world_mut().spawn_empty().id();
    let overlay = app.world_mut().spawn_empty().id();
    let completed = app.world().resource::<CompletedCaptures>().clone();
    completed.queue(image.id(), (0, 0), true, source(old));
    app.update();
    completed.queue(image.id(), (0, 0), false, source(new));
    app.update();
    assert_eq!(completed.snapshot().unwrap()[&image.id()].contributors, BTreeSet::from([old, new]));
    completed.queue(image.id(), (1, 1), false, source(overlay));
    completed.queue(image.id(), (0, 0), true, source(new));
    app.update();
    assert_eq!(completed.snapshot().unwrap()[&image.id()].contributors, BTreeSet::from([new, overlay]));
}

#[test]
fn unknown_and_recursive_inputs_never_become_public_completion_proof() {
    let mut app = app();
    let mut images = Assets::<Image>::default();
    let image = images.add(Image::default());
    let missing = images.add(Image::default());
    let entity = app.world_mut().spawn_empty().id();
    let completed = app.world().resource::<CompletedCaptures>().clone();
    let mut origins = source(entity);
    origins.images.insert(missing.id());
    completed.queue(image.id(), (0, 0), true, origins);
    app.update();
    assert!(!completed.snapshot().unwrap()[&image.id()].complete);
    let mut recursive = source(entity);
    recursive.images.insert(image.id());
    completed.queue(image.id(), (0, 0), true, recursive);
    app.update();
    assert!(!completed.snapshot().unwrap()[&image.id()].complete);
    assert!(completed.contains(&image));
}

#[test]
fn filtered_completion_and_cache_history_are_independent_of_the_normal_owner() {
    let mut app = app();
    let mut images = Assets::<Image>::default();
    let image = images.add(Image::default());
    let private = app.world_mut().spawn_empty().id();
    let public = app.world_mut().spawn_empty().id();
    let normal = app.world().resource::<CompletedCaptures>().clone();
    normal.queue(image.id(), (0, 0), true, source(private));
    app.update();
    let cache = super::super::begin_external_capture(app.world_mut());
    app.world_mut().insert_resource(crate::native::capture::render::ExternalPass);
    let filtered = app.world().resource::<CompletedCaptures>().clone();
    filtered.queue(image.id(), (0, 0), true, source(public));
    app.update();
    assert_eq!(filtered.snapshot().unwrap()[&image.id()].contributors, BTreeSet::from([public]));
    assert_eq!(normal.snapshot().unwrap()[&image.id()].contributors, BTreeSet::from([private]));
    super::super::end_external_capture(app.world_mut(), cache);
    assert_eq!(app.world().resource::<CompletedCaptures>().snapshot().unwrap()[&image.id()].contributors, BTreeSet::from([private]));
}

#[test]
fn receipts_keep_item_identity_instead_of_temporary_mesh_or_glyph_identity() {
    let mut app = app();
    let mut images = Assets::<Image>::default();
    let image = images.add(Image::default());
    let owner = app.world_mut().spawn(crate::native::layout::LayoutBox::new(Vec2::splat(100.0))).id();
    let temporary = app.world_mut().spawn(crate::native::topology::presentation::VisualOwner(owner)).id();
    let mut origins = source(temporary);
    origins.normalize(&BTreeMap::from([(temporary, crate::native::capture::semantic_sources(app.world(), temporary))]));
    let completed = app.world().resource::<CompletedCaptures>().clone();
    completed.queue(image.id(), (0, 0), true, origins);
    app.update();
    app.world_mut().despawn(temporary);
    let origins = completed.snapshot().unwrap();
    assert!(origins[&image.id()].complete);
    assert_eq!(origins[&image.id()].contributors, BTreeSet::from([owner]));
}
