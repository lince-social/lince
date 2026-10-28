use super::*;
use bevy::{core_pipeline::core_3d::TransparentSortingInfo3d, render::render_resource::CachedRenderPipelineId};

fn phase(world: &mut World) -> (SortedRenderPhase<Transparent3d>, Vec<Key>, HashSet<Entity>) {
    let mut phase = SortedRenderPhase::default();
    let mut keys = Vec::new();
    for index in 0..6 {
        let entity = world.spawn_empty().id();
        let key = (entity, entity.into());
        phase.add_retained(Transparent3d {
            sorting_info: TransparentSortingInfo3d::AlwaysOnTop,
            distance: index as f32,
            pipeline: CachedRenderPipelineId::INVALID,
            entity: key,
            draw_function: DrawFunctionId(10 + index),
            batch_range: index..index + 1,
            extra_index: bevy::render::render_phase::PhaseItemExtraIndex::None,
            indexed: index % 2 == 0,
        });
        keys.push(key);
    }
    phase.transient_items.push(keys[4]);
    let quads = HashSet::from([keys[1].0, keys[3].0]);
    (phase, keys, quads)
}

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<ViewSortedRenderPhases<Transparent3d>>()
        .init_resource::<Phases>()
        .add_systems(Update, restore);
    app
}

#[test]
fn unsupported_views_keep_every_original_item_and_restore_only_suppressed_quads_in_order() {
    let mut app = app();
    let (phase, keys, quads) = phase(app.world_mut());
    let view = RetainedViewEntity::new(keys[0].1, None, 0);
    app.world_mut().resource_mut::<ViewSortedRenderPhases<Transparent3d>>().insert(view, phase);
    for _ in 0..20 {
        let removed = {
            let mut phases = app.world_mut().resource_mut::<ViewSortedRenderPhases<Transparent3d>>();
            let phase = phases.get_mut(&view).unwrap();
            let removed = suppress(phase, &quads);
            assert_eq!(phase.items.keys().copied().collect::<Vec<_>>(), vec![keys[0], keys[2], keys[4], keys[5]]);
            assert_eq!(removed.iter().map(|entry| entry.index).collect::<Vec<_>>(), vec![1, 3]);
            removed
        };
        app.world_mut().resource_mut::<Phases>().suppressed.insert(view, removed);
        app.update();
        let phases = app.world().resource::<ViewSortedRenderPhases<Transparent3d>>();
        let phase = &phases[&view];
        assert_eq!(phase.items.keys().copied().collect::<Vec<_>>(), keys);
        assert_eq!(phase.transient_items, vec![keys[4]]);
        for (index, item) in phase.items.values().enumerate() {
            assert_eq!(item.draw_function, DrawFunctionId(10 + index as u32));
            assert_eq!(item.batch_range, index as u32..index as u32 + 1);
            assert_eq!(item.distance, index as f32);
            assert_eq!(item.indexed, index % 2 == 0);
        }
        let retained = app.world().resource::<Phases>();
        assert!(retained.views.is_empty() && retained.suppressed.is_empty());
    }
}

#[test]
fn admitted_phase_restores_draw_functions_and_retained_items_without_changing_another_view() {
    let mut app = app();
    let (phase, keys, quads) = phase(app.world_mut());
    let view = RetainedViewEntity::new(keys[0].1, None, 0);
    let (other, other_keys, _) = self::phase(app.world_mut());
    let other_view = RetainedViewEntity::new(other_keys[0].1, None, 0);
    app.world_mut().resource_mut::<ViewSortedRenderPhases<Transparent3d>>().insert(other_view, other);
    let retained = retain(phase, &quads, DrawFunctionId(99));
    assert_eq!(retained.boundaries, vec![1, 3]);
    assert_eq!(retained.phase.transient_items, vec![keys[4]]);
    for (key, item) in &retained.phase.items {
        if quads.contains(&key.0) { assert_eq!(item.draw_function, DrawFunctionId(99)); }
        else { assert_eq!(item.draw_function, DrawFunctionId(10 + item.distance as u32)); }
    }
    app.world_mut().resource_mut::<Phases>().views.insert(view, retained);
    assert!(!app.world().resource::<ViewSortedRenderPhases<Transparent3d>>().contains_key(&view));
    app.update();
    let phases = app.world().resource::<ViewSortedRenderPhases<Transparent3d>>();
    assert_eq!(phases[&view].items.keys().copied().collect::<Vec<_>>(), keys);
    assert_eq!(phases[&other_view].items.keys().copied().collect::<Vec<_>>(), other_keys);
    for item in phases[&view].items.values() { assert_eq!(item.draw_function, DrawFunctionId(10 + item.distance as u32)); }
    assert!(app.world().resource::<Phases>().views.is_empty());
}
