#![cfg(feature = "native-runtime")]

use bevy::{math::DVec2, prelude::*, time::TimeUpdateStrategy};
use lince_sandbox::{
    area::{
        AreaShape, InfluenceArea, Property, PropertyRule, ReachMode, RecordProperties, spawn_area,
    },
    canvas::CanvasItem,
    physics::WorkspacePhysicsPlugin,
    topology::{Spatial, physics::Runtime},
    workspace::{WorkspaceMember, Workspaces},
    workspace_config,
};
use std::time::Duration;

fn position_after_steps(batch: u32) -> DVec2 {
    let mut app = App::new();
    let step = Duration::from_nanos(8_333_333);
    app.add_plugins((MinimalPlugins, WorkspacePhysicsPlugin))
        .init_resource::<Runtime>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(step));
    app.finish();
    let world = app.world_mut();
    let root = world.spawn(Workspaces::default()).id();
    assert!(workspace_config::set_physics(world, root, 1, true));
    let mut area = InfluenceArea::new(AreaShape::Square, DVec2::ZERO, DVec2::splat(900.0));
    area.strength = 100_000.0;
    area.attraction_enabled = true;
    area.reach.mode = ReachMode::Unlimited;
    area.rules.push(PropertyRule {
        property: Property::Quantity,
        value: "-1".into(),
    });
    spawn_area(world, root, 1, area).unwrap();
    let sand = world
        .spawn((
            CanvasItem {
                position: DVec2::new(150.0, 0.0),
                size: Vec2::splat(20.0),
            },
            Spatial::default(),
            WorkspaceMember(1),
            ChildOf(root),
            RecordProperties(serde_json::json!({"quantity":-1}).into()),
        ))
        .id();
    app.update();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(step * batch));
    for _ in 0..96 / batch {
        app.update();
    }
    app.world().get::<CanvasItem>(sand).unwrap().position
}

#[test]
fn area_trajectory_is_independent_of_render_frame_batching() {
    let reference = position_after_steps(1);
    assert!(reference.x.abs() < 125.0);
    for batch in [2, 4, 8] {
        let actual = position_after_steps(batch);
        assert!(
            actual.distance(reference) < 1e-6,
            "batch {batch}: {actual:?} vs {reference:?}"
        );
    }
}
