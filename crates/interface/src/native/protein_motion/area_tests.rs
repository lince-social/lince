use super::*;
use crate::native::{
    area::{AreaShape, spawn_area},
    canvas::ItemPreview,
    workspace::Workspaces,
    workspace_config,
};

fn sand(world: &mut World, root: Entity, workspace: u64, x: f64) -> Entity {
    world
        .spawn((
            CanvasItem {
                position: DVec2::new(x, 0.0),
                size: Vec2::new(100.0, 40.0),
            },
            ChildOf(root),
            WorkspaceMember(workspace),
        ))
        .id()
}

#[test]
fn ordinary_area_uses_drag_preview_wakes_neighbors_and_cools_in_place() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    assert!(workspace_config::set_physics(&mut world, root, 1, true));
    let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(800.0));
    area.graph_layout = Some(Settings {
        center: 0.0,
        repulsion: 0.0,
        ..Default::default()
    });
    let owner = spawn_area(&mut world, root, 1, area).unwrap();
    let held = sand(&mut world, root, 1, -250.0);
    let neighbor = sand(&mut world, root, 1, 0.0);
    let linked = sand(&mut world, root, 1, 275.0);
    let outside = sand(&mut world, root, 1, 700.0);
    let other_workspace = sand(&mut world, root, 2, 0.0);
    world.spawn(ArrowSand {
        from: held,
        to: linked,
        label: "related".into(),
    });
    for _ in 0..620 {
        prepare(&mut world);
    }
    assert_eq!(world.resource::<Motion>().states[&owner].alpha, 0.0);
    let mut preview = *world.get::<CanvasItem>(held).unwrap();
    preview.position.x = -90.0;
    world.entity_mut(held).insert(ItemPreview(preview));
    prepare(&mut world);
    let forces = &world.resource::<Motion>().forces;
    assert_eq!(forces[&held], DVec3::ZERO);
    assert!(forces[&neighbor].x > 0.0);
    assert!(forces[&linked].x < 0.0);
    assert!(!forces.contains_key(&other_workspace));
    assert!(!forces.contains_key(&outside));
    assert_eq!(world.get::<CanvasItem>(held).unwrap().position.x, -250.0);
    for _ in 0..620 {
        remember(&mut world);
        prepare(&mut world);
    }
    let evaluations = world.resource::<Motion>().evaluations;
    prepare(&mut world);
    assert_eq!(world.resource::<Motion>().evaluations, evaluations);
    world.entity_mut(held).remove::<ItemPreview>();
    prepare(&mut world);
    assert!(world.resource::<Motion>().evaluations > evaluations);
}

#[test]
fn loading_legacy_protein_motion_promotes_saved_tuning_to_the_area() {
    let mut world = World::new();
    let root = world.spawn(Workspaces::default()).id();
    let mut area = InfluenceArea::new(AreaShape::Circle, DVec2::ZERO, DVec2::splat(800.0));
    area.protein = Some(crate::native::relation_castle::config());
    let mut saved = serde_json::to_value(area).unwrap();
    saved["protein"]["motion"] = serde_json::json!({"center":3.0, "repulsion":75.0, "cooling":0.8});
    let restored = spawn_area(&mut world, root, 1, serde_json::from_value(saved).unwrap()).unwrap();
    let area = world.get::<InfluenceArea>(restored).unwrap();
    let layout = area.graph_layout.unwrap();
    assert_eq!(
        (layout.center, layout.repulsion, layout.cooling),
        (3.0, 75.0, 0.8)
    );
    assert_eq!(layout.link_spacing, Settings::default().link_spacing);
    assert!(area.protein.as_ref().unwrap().motion.is_none());
    assert_eq!(
        serde_json::from_value::<InfluenceArea>(serde_json::to_value(area).unwrap()).unwrap(),
        *area
    );
}

#[test]
fn dragging_a_relation_card_pulls_its_link_before_the_drop() {
    use bevy::time::TimeUpdateStrategy;
    let mut app = App::new();
    crate::native::laboratory::isolate(app.world_mut());
    app.add_plugins((
        MinimalPlugins,
        crate::native::physics::WorkspacePhysicsPlugin,
    ))
    .init_resource::<topology::physics::Runtime>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(
        std::time::Duration::from_secs_f64(1.0 / 60.0),
    ));
    app.finish();
    let world = app.world_mut();
    let root = world.spawn(Workspaces::default()).id();
    let owner = crate::native::relation_castle::spawn(world, root).unwrap();
    let settings = world
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .graph_layout
        .unwrap();
    let dragged = sand(world, root, 1, 0.0);
    let neighbor = sand(world, root, 1, 100.0 + settings.link_spacing);
    let outsider = sand(world, root, 1, 800.0);
    for (entity, uid) in [(dragged, "dragged"), (neighbor, "neighbor")] {
        world.entity_mut(entity).insert(RecordBinding {
            area: owner,
            uid: uid.into(),
            source: crate::native::protein_area::Source::Local,
        });
    }
    world.spawn(ArrowSand {
        from: dragged,
        to: neighbor,
        label: "related".into(),
    });
    // Start at rest and move only the presentation used by native drag previews.
    let mut preview = *world.get::<CanvasItem>(dragged).unwrap();
    preview.position.x = -500.0;
    world.entity_mut(dragged).insert(ItemPreview(preview));
    for _ in 0..30 {
        app.update();
    }
    let world = app.world();
    assert!(world.get::<CanvasItem>(neighbor).unwrap().position.x < 150.0);
    assert_eq!(
        crate::native::canvas::presented(world, dragged)
            .unwrap()
            .position
            .x,
        -500.0
    );
    assert_eq!(world.get::<CanvasItem>(outsider).unwrap().position.x, 800.0);
}
