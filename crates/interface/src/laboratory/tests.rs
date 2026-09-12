use super::*;
use crate::{
    actions::Action,
    canvas::{CanvasItem, CanvasView},
    container::BoxRoot,
    sand_store::{SandKind, spawn_sand},
    workspace::{WorkspaceFile, Workspaces},
};
use bevy::{
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    text::EditableText,
    winit::WinitSettings,
};

fn saved_snapshot(path: &std::path::Path) -> serde_json::Value {
    let mut files: Vec<_> = std::fs::read_dir(path.with_extension("snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    serde_json::from_slice(&std::fs::read(files.last().unwrap()).unwrap()).unwrap()
}

fn fixture() -> (App, Entity) {
    let (mut app, root) = super::headless::stress_app();
    app.world_mut().entity_mut(root).remove::<LaboratoryRoot>();
    app.add_plugins((
        crate::actions::ActionsPlugin,
        crate::workspace::WorkspacePlugin,
        LaboratoryPlugin,
    ));
    app.finish();
    app.cleanup();
    app.update();
    (app, root)
}

#[cfg_attr(test, test)]
fn laboratory_suspends_all_roots_and_restores_drafts_cameras_focus_and_physics() {
    let (mut app, root) = fixture();
    let other = app
        .world_mut()
        .spawn((
            BoxRoot,
            Workspaces::default(),
            Node {
                display: Display::Grid,
                ..default()
            },
        ))
        .id();
    let sand = spawn_sand(
        app.world_mut(),
        root,
        1,
        SandKind::EditableText,
        "unsaved draft",
        DVec2::new(140.0, 40.0),
    );
    let editor = app
        .world()
        .get::<crate::sand_store::StoredSand>(sand)
        .unwrap()
        .content
        .unwrap();
    let before = *app.world().get::<CanvasItem>(sand).unwrap();
    app.world_mut().get_mut::<CanvasView>(root).unwrap().center = DVec2::new(1e9, -1e9);
    crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, FocusCause::Pressed);
    LaboratoryAction::Open.apply(app.world_mut(), root);
    let laboratory = app.world().resource::<Laboratory>().root.unwrap();
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::None
    );
    assert_eq!(
        app.world().get::<Node>(other).unwrap().display,
        Display::None
    );
    assert!(!crate::workspace_config::enabled(app.world(), root, 1));
    assert!(!suspended(app.world(), laboratory));
    assert!(suspended(app.world(), editor));
    assert_eq!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .count(),
        0
    );
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(
        app.world().get::<CanvasItem>(sand).unwrap().position,
        before.position
    );
    LaboratoryAction::Close.apply(app.world_mut(), laboratory);
    assert!(app.world().get_entity(laboratory).is_err());
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::Flex
    );
    assert_eq!(
        app.world().get::<Node>(other).unwrap().display,
        Display::Grid
    );
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    assert_eq!(
        app.world()
            .get::<EditableText>(editor)
            .unwrap()
            .value()
            .to_string(),
        "unsaved draft"
    );
    assert_eq!(
        app.world().get::<CanvasView>(root).unwrap().center,
        DVec2::new(1e9, -1e9)
    );
    assert!(crate::workspace_config::enabled(app.world(), root, 1));
}

#[cfg_attr(test, test)]
fn laboratory_stress_is_temporary_bounded_and_never_saved_as_a_user_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture();
    let path = directory.path().join("interface.json");
    app.insert_resource(WorkspaceFile::new(path.clone()));
    LaboratoryAction::Open.apply(app.world_mut(), root);
    let laboratory = app.world().resource::<Laboratory>().root.unwrap();
    app.world_mut().resource_mut::<Laboratory>().config = StressConfig {
        max_sands: 4,
        batch: 2,
        warmup_frames: 1,
        sample_frames: 3,
        budget_ms: 1000.0,
    };
    LaboratoryAction::Stress.apply(app.world_mut(), laboratory);
    for _ in 0..200 {
        app.update();
        if !app.world().resource::<Laboratory>().reports.is_empty() {
            break;
        }
    }
    let report = &app.world().resource::<Laboratory>().reports[0];
    assert!(report.complete);
    assert_eq!(report.stops.len(), 6);
    assert!(report.measurements.iter().all(|row| row.sands <= 4));
    for kind in SandKind::ALL {
        for physics in [false, true] {
            assert!(
                report
                    .measurements
                    .iter()
                    .any(|row| row.kind == kind && row.physics == physics && row.sands == 4)
            );
        }
    }
    assert!(!path.exists());
    assert!(!path.with_extension("snapshots").exists());
    assert!(
        !directory
            .path()
            .join("workspaces/1/workspace.toml")
            .exists()
    );
    assert_eq!(
        app.world_mut()
            .query::<&super::stress::StressSand>()
            .iter(app.world())
            .count(),
        0
    );
    LaboratoryAction::Close.apply(app.world_mut(), laboratory);
    app.world_mut().write_message(AppExit::Success);
    app.update();
    let saved: serde_json::Value = saved_snapshot(&path);
    assert_eq!(saved["workspaces"].as_array().unwrap().len(), 1);
    assert!(saved["sands"].as_array().unwrap().is_empty());
    assert_eq!(saved["workspaces"][0]["name"], "Home");
}

#[cfg_attr(test, test)]
fn stop_restores_idle_updates_and_exit_saves_only_the_original_scene() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture();
    let path = directory.path().join("interface.json");
    app.insert_resource(WorkspaceFile::new(path.clone()));
    app.insert_resource(crate::theme::idle_settings());
    LaboratoryAction::Open.apply(app.world_mut(), root);
    LaboratoryAction::Stress.apply(app.world_mut(), root);
    assert!(matches!(
        app.world().resource::<WinitSettings>().focused_mode,
        bevy::winit::UpdateMode::Continuous
    ));
    LaboratoryAction::Stop.apply(app.world_mut(), root);
    assert!(!matches!(
        app.world().resource::<WinitSettings>().focused_mode,
        bevy::winit::UpdateMode::Continuous
    ));
    app.world_mut().write_message(AppExit::Success);
    app.update();
    assert!(!active(app.world()));
    let saved: serde_json::Value = saved_snapshot(&path);
    assert_eq!(saved["workspaces"][0]["name"], "Home");
    assert_eq!(
        app.world().get::<Node>(root).unwrap().display,
        Display::Flex
    );
}

#[cfg_attr(test, test)]
fn stress_stops_at_a_slow_baseline_and_aborts_severe_slowdowns_during_warmup() {
    for emergency in [false, true] {
        let (mut app, root) = super::headless::stress_app();
        let config = StressConfig {
            max_sands: 32768,
            batch: 64,
            warmup_frames: if emergency { 600 } else { 1 },
            sample_frames: 3,
            budget_ms: 20.0,
        };
        let mut run = super::stress::StressRun::new(config, false, Vec2::new(1000.0, 800.0));
        for _ in 0..40 {
            run.advance(app.world_mut(), root, if emergency { 1500.0 } else { 25.0 });
            if run.report.complete {
                break;
            }
        }
        assert!(run.report.complete);
        assert_eq!(run.report.measurements.len(), 6);
        assert!(run.report.measurements.iter().all(|row| !row.within_budget
            && row.sands == 0
            && row.emergency_stop == emergency
            && row.visible_sands.is_none()));
    }
}

#[cfg_attr(test, test)]
fn resource_snapshot_preserves_physics_counts_and_exports_without_draft_contents() {
    let directory = tempfile::tempdir().unwrap();
    let (mut app, root) = fixture();
    app.insert_resource(WorkspaceFile::new(directory.path().join("interface.json")));
    let sand = spawn_sand(app.world_mut(), root, 1, SandKind::Square, "", DVec2::ZERO);
    crate::workspace_config::set_physics(app.world_mut(), root, 1, true);
    app.update();
    LaboratoryAction::Open.apply(app.world_mut(), root);
    let snapshot = &app.world().resource::<Laboratory>().resources;
    let row = snapshot
        .sands
        .iter()
        .find(|row| row.entity == sand.to_string())
        .unwrap();
    assert_eq!(row.physics_bodies, 1);
    assert_eq!(row.awake_bodies, 1);
    assert!(!row.suspended);
    for _ in 0..3 {
        app.update();
    }
    assert!(crate::physics::resource_usage(app.world_mut()).is_empty());
    assert_eq!(
        app.world()
            .resource::<Laboratory>()
            .resources
            .sands
            .iter()
            .find(|row| row.entity == sand.to_string())
            .unwrap()
            .physics_bodies,
        1
    );
    LaboratoryAction::Export.apply(app.world_mut(), root);
    let path = app
        .world()
        .resource::<Laboratory>()
        .status
        .strip_prefix("Results saved to ")
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(report["schema"], 2);
    assert!(report["graphics"]["name"].is_null());
    assert_eq!(
        report["sand_resources_before_suspension"]["sands"][0]["physics_bodies"],
        1
    );
    LaboratoryAction::Close.apply(app.world_mut(), root);
}

crate::laboratory_cases! {
    laboratory_suspends_all_roots_and_restores_drafts_cameras_focus_and_physics,
    laboratory_stress_is_temporary_bounded_and_never_saved_as_a_user_workspace,
    stop_restores_idle_updates_and_exit_saves_only_the_original_scene,
    stress_stops_at_a_slow_baseline_and_aborts_severe_slowdowns_during_warmup,
    resource_snapshot_preserves_physics_counts_and_exports_without_draft_contents,
}
