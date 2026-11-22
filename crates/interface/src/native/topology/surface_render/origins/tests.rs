// non-lean-yet
// Scope: fn app, fn source, fn
//   completion_records_actual_draw_order_and_keeps_sampled_provenance_after_an_input_changes, fn
//   a_load_keeps_prior_sources_and_only_an_acknowledged_clear_replaces_them.
// BeautifulRust (baseline lines):
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 112.

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
fn ui_dependencies_follow_writer_changes_with_unchanged_geometry() {
    let mut app = App::new();
    app.init_resource::<UiOrigins>()
        .init_resource::<ExtractedUiNodes>()
        .add_systems(Update, prepare_ui);
    let owner = app.world_mut().spawn_empty().id();
    let camera = app.world_mut().spawn_empty().id();
    let image = Assets::<Image>::default().reserve_handle();
    let mut sources = bevy::ecs::system::SystemState::<crate::native::capture::Sources>::new(app.world_mut());
    let scopes = sources.get(app.world()).unwrap().snapshot(MAX_LINKS + 1);
    app.world_mut().insert_resource(MaterialInputs {
        topology: Arc::new(CaptureTopology {
            materials: BTreeMap::new(),
            scopes,
        }),
        writers: BTreeMap::new(),
    });
    {
        let mut nodes = app.world_mut().resource_mut::<ExtractedUiNodes>();
        nodes.uinodes.entry(owner.into()).or_insert_with(|| (camera, Default::default())).1.insert(
            owner,
            ExtractedUiNode {
                z_order: 0.0,
                image: image.id(),
                transform: bevy::math::Affine2::IDENTITY,
                clip: None,
                item: ExtractedUiItem::Glyphs { glyphs: Vec::new() },
            },
        );
    }
    app.update();
    assert!(app.world().resource::<UiOrigins>().by_camera[&camera].images.is_empty());
    app.world_mut().resource_mut::<MaterialInputs>().writers.insert(image.id(), true);
    app.update();
    let ui = &app.world().resource::<UiOrigins>().by_camera[&camera];
    assert_eq!(ui.contributors, BTreeSet::from([owner]));
    assert_eq!(ui.images, BTreeSet::from([image.id()]));
    app.world_mut().resource_mut::<MaterialInputs>().writers.clear();
    app.update();
    assert!(app.world().resource::<UiOrigins>().by_camera[&camera].images.is_empty());
}

#[test]
fn client_repaints_retain_topology_and_external_captures_use_a_fresh_snapshot() {
    let mut render = World::new();
    let mut main = bevy::render::MainWorld::default();
    let first = main.spawn_empty().id();
    render.insert_resource(main);
    let mut schedule = bevy::ecs::schedule::Schedule::new(ExtractSchedule);
    schedule.add_systems(extract);
    schedule.run(&mut render);
    let desktop = render.resource::<MaterialInputs>().topology.clone();
    assert!(desktop.scopes.resolve(first).is_some());

    render.insert_resource(RetainCaptureTopology);
    schedule.run(&mut render);
    assert!(Arc::ptr_eq(&desktop, &render.resource::<MaterialInputs>().topology));

    let extra = render.resource_mut::<bevy::render::MainWorld>().spawn_empty().id();
    render.insert_resource(crate::native::capture::render::ExternalPass);
    schedule.run(&mut render);
    let capture = &render.resource::<MaterialInputs>().topology;
    assert!(!Arc::ptr_eq(&desktop, capture));
    assert!(capture.scopes.resolve(extra).is_some());

    render.remove_resource::<crate::native::capture::render::ExternalPass>();
    schedule.run(&mut render);
    assert!(Arc::ptr_eq(&desktop, &render.resource::<MaterialInputs>().topology));

    render.remove_resource::<RetainCaptureTopology>();
    schedule.run(&mut render);
    let updated = &render.resource::<MaterialInputs>().topology;
    assert!(!Arc::ptr_eq(&desktop, updated));
    assert!(updated.scopes.resolve(extra).is_some());
}

#[test]
fn output_reuse_requires_complete_provenance_without_changed_sources() {
    let mut app = app();
    let mut images = Assets::<Image>::default();
    let changed = app.world_mut().spawn_empty().id();
    let unchanged = app.world_mut().spawn_empty().id();
    let input = images.add(Image::default());
    let affected = images.add(Image::default());
    let independent = images.add(Image::default());
    let missing = images.add(Image::default());
    let unknown = images.add(Image::default());
    let completed = app.world().resource::<CompletedCaptures>().clone();
    let mut sampled = source(unchanged);
    sampled.images.insert(input.id());
    completed.queue(input.id(), (-1, 0), true, source(changed));
    completed.queue(affected.id(), (0, 0), true, sampled);
    completed.queue(independent.id(), (0, 0), true, source(unchanged));
    completed.queue(unknown.id(), (0, 0), true, CaptureOrigins::default());
    app.update();
    let images = [affected.id(), independent.id(), missing.id(), unknown.id()];
    assert_eq!(
        completed.independent_images(images.into_iter(), &BTreeSet::from([changed])),
        BTreeSet::from([independent.id()]),
    );
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
    let mut sources = bevy::ecs::system::SystemState::<crate::native::capture::Sources>::new(app.world_mut());
    let snapshot = sources.get(app.world()).unwrap().snapshot(MAX_LINKS + 1);
    origins.normalize(&snapshot);
    let completed = app.world().resource::<CompletedCaptures>().clone();
    completed.queue(image.id(), (0, 0), true, origins);
    app.update();
    app.world_mut().despawn(temporary);
    let origins = completed.snapshot().unwrap();
    assert!(origins[&image.id()].complete);
    assert_eq!(origins[&image.id()].contributors, BTreeSet::from([owner]));
}
