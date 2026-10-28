use super::*;
use crate::{layout::desktop::Width, native::actions::Action};

fn fixture(mode: WorkspaceLayout) -> (World, Entity, Entity, Entity) {
    let (mut world, root, native, application) = super::super::tests::fixture();
    world.insert_resource(DefaultLayout(mode));
    world.insert_resource(LayoutSettings {
        gaps: 0,
        ..default()
    });
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_mode(mode);
    world.get_mut::<ComputedNode>(root).unwrap().size.x = 900.0;
    world
        .entity_mut(application)
        .insert(ApplicationOpeningPending);
    (world, root, native, application)
}

fn resolve(world: &mut World) {
    super::super::super::engine::resolve(world);
}

#[test]
fn gaps_apply_to_live_and_restored_models_without_resetting_width_intent_or_identity() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Tiled);
    let mut inactive = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    inactive.id = 2;
    inactive.name = "Keep the authored workspace".into();
    inactive.center = [-1000.0, 650.0];
    inactive.zoom = 0.4;
    inactive.desktop.set_gap(80.0).unwrap();
    world
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .push(inactive);
    resolve(&mut world);
    let inactive = &world.get::<Workspaces>(root).unwrap().entries[1];
    assert_eq!(inactive.desktop.gap(), 0.0);
    assert_eq!(inactive.name, "Keep the authored workspace");
    assert_eq!(inactive.center, [-1000.0, 650.0]);
    assert_eq!(inactive.zoom, 0.4);
    let ids = [native, application].map(|entity| world.get::<Managed>(entity).unwrap().id);
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.gap(), 0.0);
    for entity in [native, application] {
        assert_eq!(world.get::<CanvasItem>(entity).unwrap().size.x, 450.0);
    }
    world.resource_mut::<LayoutSettings>().gaps = 24;
    resolve(&mut world);
    assert_eq!(
        world.get::<Workspaces>(root).unwrap().entries[1]
            .desktop
            .gap(),
        24.0
    );
    for entity in [native, application] {
        assert_eq!(world.get::<CanvasItem>(entity).unwrap().size.x, 414.0);
    }
    let mut restored = world.get::<Workspaces>(root).unwrap().entries[0].clone();
    restored.desktop.set_gap(80.0).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().entries[0] = restored;
    world.remove_resource::<Runtime>();
    resolve(&mut world);
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert_eq!(model.gap(), 24.0);
    assert_eq!(model.items().len(), 2);
    for id in ids {
        assert_eq!(model.item_width(id), Some(Width::Fraction(1, 2)));
    }
    assert_eq!(world.get::<Managed>(application).unwrap().id, ids[1]);
}

#[test]
fn free_application_opening_preserves_normal_width_camera_and_authored_native_geometry() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Free);
    world.resource_mut::<LayoutSettings>().application_opening = ApplicationOpening::Maximized;
    let native_before = *world.get::<CanvasItem>(native).unwrap();
    let view = CanvasView {
        center: bevy::math::DVec2::new(120.0, 180.0),
        zoom: 0.5,
    };
    *world.get_mut::<CanvasView>(root).unwrap() = view;
    let bounds = Rectangle::new([0.0; 2], [900.0, 700.0]).unwrap();
    world.entity_mut(root).insert(HostViewport(OutputViewport { rotation: Default::default(),
        id: OutputId(1),
        bounds,
        usable: Some(Rectangle::new([0.0, 30.0], [900.0, 670.0]).unwrap()),
        physical_size: [900, 700],
        raster: crate::host::raster::RasterMapping {
            origin: [0.0; 2],
            scale: [1.0; 2],
        },
        workspace: 1,
    }));
    resolve(&mut world);
    let id = world.get::<Managed>(application).unwrap().id;
    let native_id = world.get::<Managed>(native).unwrap().id;
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert!(model.maximized(id));
    assert!(!model.maximized(native_id));
    assert_eq!(
        world.get::<CanvasItem>(application).unwrap().size,
        Vec2::new(900.0, 670.0)
    );
    assert_eq!(
        world.get::<CanvasItem>(native).unwrap().size,
        native_before.size
    );
    assert_eq!(
        world.get::<CanvasItem>(native).unwrap().position,
        native_before.position
    );
    focus_item(&mut world, application);
    DesktopAction::ToggleMaximized.apply(&mut world, root);
    for _ in 0..3 {
        resolve(&mut world);
    }
    let item = world.get::<CanvasItem>(application).unwrap();
    assert_eq!(item.size.x, 900.0);
    assert_eq!(item.size.y, 200.0);
    assert_eq!(world.get::<CanvasView>(root).unwrap().zoom, view.zoom);
    assert_eq!(world.get::<CanvasView>(root).unwrap().center, view.center);
    assert_eq!(item.size.x * view.zoom as f32, 450.0);
    assert!(
        !world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .maximized(id)
    );
    world.resource_mut::<LayoutSettings>().default_width = [2, 3];
    resolve(&mut world);
    assert_eq!(world.get::<CanvasItem>(application).unwrap().size.x, 900.0);
    assert!(
        !world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .maximized(id)
    );
}

#[test]
fn opening_preference_skips_dialogs_and_existing_restored_placements() {
    let (mut world, root, native, application) = fixture(WorkspaceLayout::Tiled);
    world.resource_mut::<LayoutSettings>().application_opening = ApplicationOpening::Maximized;
    world
        .entity_mut(application)
        .insert(TransientParent(native));
    resolve(&mut world);
    let id = world.get::<Managed>(application).unwrap().id;
    assert!(
        !world.get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .maximized(id)
    );
    world.entity_mut(application).remove::<TransientParent>();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .toggle_floating(id, Rectangle::new([0.0; 2], [300.0, 200.0]).unwrap())
        .unwrap();
    world.get_mut::<Workspaces>(root).unwrap().entries[0]
        .desktop
        .set_item_width(id, Width::Fraction(2, 3))
        .unwrap();
    let saved = serde_json::to_value(&world.get::<Workspaces>(root).unwrap().entries[0]).unwrap();
    world.get_mut::<Workspaces>(root).unwrap().entries[0] = serde_json::from_value(saved).unwrap();
    world.remove_resource::<Runtime>();
    resolve(&mut world);
    let model = &world.get::<Workspaces>(root).unwrap().entries[0].desktop;
    assert!(!model.maximized(id));
    assert_eq!(model.item_width(id), Some(Width::Fraction(2, 3)));
    assert_eq!(world.get::<Managed>(application).unwrap().id, id);
}
