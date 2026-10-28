use super::*;
use crate::{host::DesktopAction, native::actions::Action};

fn fixture(path: Option<PathBuf>) -> (App, Entity) {
    let (mut app, root) = super::super::tests::fixture(path);
    let mut entry = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    for id in [31, 7] {
        entry.id = id;
        entry.name = format!("Named {id}");
        entry.center = [id as f64 * 10.0, -13.0];
        entry.zoom = 0.75;
        app.world_mut()
            .get_mut::<Workspaces>(root)
            .unwrap()
            .entries
            .push(entry.clone());
    }
    (app, root)
}

#[test]
fn numeric_reselection_is_optional_and_identity_activation_does_not_toggle() {
    let (mut app, root) = fixture(None);
    switch(app.world_mut(), root, 31);
    assert_eq!(numeric_destination(app.world(), root, 31), 31);
    let mut settings = crate::native::shortcuts::configuration::LayoutSettings::default();
    settings.workspace_back_and_forth = true;
    app.world_mut().insert_resource(settings);
    assert_eq!(numeric_destination(app.world(), root, 31), 1);
    for _ in 0..20 {
        switch_output(app.world_mut(), root, 31);
    }
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
    DesktopAction::Workspace(2).apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
    DesktopAction::Workspace(1).apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
}

#[test]
fn history_keeps_identity_after_reorder_and_retires_deleted_or_replaced_canvas_targets() {
    let (mut app, root) = fixture(None);
    let mut settings = crate::native::shortcuts::configuration::LayoutSettings::default();
    settings.workspace_back_and_forth = true;
    app.world_mut().insert_resource(settings);
    switch(app.world_mut(), root, 31);
    reorder(app.world_mut(), root, 1).unwrap();
    assert_eq!(numeric_destination(app.world(), root, 31), 1);
    assert!(remove(app.world_mut(), root, 1));
    assert_eq!(numeric_destination(app.world(), root, 31), 31);
    switch(app.world_mut(), root, 7);
    assert_eq!(numeric_destination(app.world(), root, 7), 31);
    app.world_mut().get_mut::<Workspaces>(root).unwrap().canvas_id = nucleus::new_uid("canvas");
    assert_eq!(numeric_destination(app.world(), root, 7), 7);
}

#[test]
fn failed_selection_keeps_history_and_a_fresh_session_starts_empty() {
    let (mut app, root) = fixture(None);
    let mut settings = crate::native::shortcuts::configuration::LayoutSettings::default();
    settings.workspace_back_and_forth = true;
    app.world_mut().insert_resource(settings);
    switch(app.world_mut(), root, 31);
    assert!(!switch(app.world_mut(), root, u64::MAX));
    assert_eq!(numeric_destination(app.world(), root, 31), 1);
    let (mut fresh, owner) = fixture(None);
    fresh.world_mut().insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
        workspace_back_and_forth: true,
        ..Default::default()
    });
    switch_output(fresh.world_mut(), owner, 31);
    assert_eq!(numeric_destination(fresh.world(), owner, 31), 31);
}

#[test]
fn output_histories_stay_independent_and_a_disconnected_view_cannot_recover_old_history() {
    use crate::host::{OutputId, OutputViewport, Rectangle};
    use crate::native::layout::desktop::HostViewport;
    let (mut app, root) = fixture(None);
    app.world_mut().insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
        workspace_back_and_forth: true,
        ..Default::default()
    });
    let view = |id, workspace| {
        let bounds = Rectangle::new([0.0; 2], [1000.0, 700.0]).unwrap();
        OutputViewport { rotation: Default::default(), id: OutputId(id), workspace, bounds, usable: Some(bounds), physical_size: [1000, 700],
            raster: crate::host::raster::RasterMapping { origin: bounds.origin(), scale: [1.0; 2] } }
    };
    let first = view(1, 1);
    let second = view(2, 7);
    let mut views = crate::native::output_views::OutputViews::new(vec![first.clone(), second.clone()], first.id).unwrap();
    views.canvas = Some(root);
    app.world_mut().insert_resource(views);
    app.world_mut().entity_mut(root).insert(HostViewport(first.clone()));
    switch(app.world_mut(), root, 31);
    app.world_mut().entity_mut(root).insert(HostViewport(second.clone()));
    switch_output(app.world_mut(), root, 7);
    switch(app.world_mut(), root, 1);
    assert_eq!(numeric_destination(app.world(), root, 1), 7);
    app.world_mut().entity_mut(root).insert(HostViewport(first));
    switch_output(app.world_mut(), root, 31);
    assert_eq!(numeric_destination(app.world(), root, 31), 1);
    app.world_mut().resource_mut::<crate::native::output_views::OutputViews>().outputs.retain(|view| view.id == OutputId(1));
    switch(app.world_mut(), root, 1);
    app.world_mut().resource_mut::<crate::native::output_views::OutputViews>().outputs.push(second.clone());
    app.world_mut().entity_mut(root).insert(HostViewport(second));
    assert_eq!(numeric_destination(app.world(), root, 1), 1);
}

#[test]
fn a_temporary_laboratory_destination_never_supplies_or_replaces_numeric_history() {
    let (mut app, root) = fixture(None);
    app.world_mut().insert_resource(crate::native::shortcuts::configuration::LayoutSettings {
        workspace_back_and_forth: true,
        ..Default::default()
    });
    switch(app.world_mut(), root, 31);
    assert_eq!(numeric_destination(app.world(), root, 31), 1);
    app.world_mut().insert_resource(crate::native::laboratory::Laboratory::scoped_for_test(root, 7, 31));
    let previous = capture(app.world(), root).unwrap();
    switch_output(app.world_mut(), root, 7);
    previous.remember(app.world_mut());
    assert!(capture(app.world(), root).is_none());
    assert!(!numbers(app.world(), root).contains(&7));
    switch_output(app.world_mut(), root, 31);
    assert_eq!(numeric_destination(app.world(), root, 31), 1);
    app.world_mut().remove_resource::<crate::native::laboratory::Laboratory>();
    DesktopAction::Workspace(2).apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 1);
}

#[test]
fn adjacent_commands_follow_authored_order_and_stop_at_edges_without_creating_workspaces() {
    let (mut app, root) = fixture(None);
    assert_eq!(numbers(app.world(), root), vec![1, 31, 7]);
    for offset in [0, 2, -2, isize::MIN, isize::MAX] {
        assert!(destination(app.world(), root, offset).is_err());
        assert!(reorder(app.world_mut(), root, offset).is_err());
    }
    assert!(
        destination(app.world(), root, -1)
            .unwrap_err()
            .contains("first")
    );
    let before = *app.world().get::<CanvasView>(root).unwrap();
    DesktopAction::PreviousWorkspace.apply(app.world_mut(), root);
    let current = app.world().get::<CanvasView>(root).unwrap();
    assert_eq!((current.center, current.zoom), (before.center, before.zoom));
    DesktopAction::NextWorkspace.apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
    assert_eq!(
        app.world().get::<CanvasView>(root).unwrap().center,
        DVec2::new(310.0, -13.0)
    );
    DesktopAction::NextWorkspace.apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 7);
    assert!(
        destination(app.world(), root, 1)
            .unwrap_err()
            .contains("last")
    );
    DesktopAction::NextWorkspace.apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 7);
    DesktopAction::PreviousWorkspace.apply(app.world_mut(), root);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
    assert_eq!(numbers(app.world(), root), vec![1, 31, 7]);
}

#[test]
fn reordering_preserves_authored_models_content_camera_active_identity_and_editor_focus() {
    let (mut app, root) = fixture(None);
    switch(app.world_mut(), root, 31);
    let sand = crate::native::sand_store::spawn_sand(
        app.world_mut(),
        root,
        31,
        SandKind::EditableText,
        "Retained",
        DVec2::new(64.0, -32.0),
    );
    let editor = app
        .world()
        .get::<StoredSand>(sand)
        .unwrap()
        .content
        .unwrap();
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("Keep this text");
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(editor, bevy::input_focus::FocusCause::Navigated);
    let camera = *app.world().get::<CanvasView>(root).unwrap();
    let mut expected = super::super::snapshot(app.world_mut(), root);
    expected.workspaces.swap(0, 1);
    DesktopAction::MoveWorkspaceEarlier.apply(app.world_mut(), root);
    assert_eq!(numbers(app.world(), root), vec![31, 1, 7]);
    assert_eq!(
        serde_json::to_value(super::super::snapshot(app.world_mut(), root)).unwrap(),
        serde_json::to_value(expected).unwrap()
    );
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
    assert_eq!(app.world().get::<WorkspaceMember>(sand).unwrap().0, 31);
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(editor));
    let current = app.world().get::<CanvasView>(root).unwrap();
    assert_eq!((current.center, current.zoom), (camera.center, camera.zoom));
    let before = serde_json::to_vec(&super::super::snapshot(app.world_mut(), root)).unwrap();
    assert!(reorder(app.world_mut(), root, -1).is_err());
    assert_eq!(
        serde_json::to_vec(&super::super::snapshot(app.world_mut(), root)).unwrap(),
        before
    );
    DesktopAction::MoveWorkspaceLater.apply(app.world_mut(), root);
    assert_eq!(numbers(app.world(), root), vec![1, 31, 7]);
}

#[test]
fn scoped_laboratory_anchors_navigation_to_its_underlying_authored_workspace() {
    let (mut app, root) = fixture(None);
    let mut temporary = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
    temporary.id = u64::MAX;
    let spaces = &mut *app.world_mut().get_mut::<Workspaces>(root).unwrap();
    spaces.entries.insert(1, temporary);
    spaces.active = u64::MAX;
    app.world_mut()
        .insert_resource(crate::native::laboratory::Laboratory::scoped_for_test(
            root,
            u64::MAX,
            31,
        ));
    assert_eq!(numbers(app.world(), root), vec![1, 31, 7]);
    assert_eq!(destination(app.world(), root, -1), Ok(1));
    assert_eq!(destination(app.world(), root, 1), Ok(7));
    let before = numbers(app.world(), root);
    assert!(
        reorder(app.world_mut(), root, 1)
            .unwrap_err()
            .contains("authored")
    );
    assert_eq!(numbers(app.world(), root), before);
    switch_output(app.world_mut(), root, 1);
    reorder(app.world_mut(), root, 1).unwrap();
    assert_eq!(numbers(app.world(), root), vec![31, 1, 7]);
    assert_eq!(
        app.world().get::<Workspaces>(root).unwrap().entries[1].id,
        u64::MAX
    );
}

#[test]
fn prepared_navigation_keeps_the_destination_identity_across_reordering_and_rejects_closure() {
    use crate::native::layout::desktop::transfers;

    let (mut app, root) = fixture(None);
    let id = destination(app.world(), root, 1).unwrap();
    let prepared = transfers::prepare_identity(app.world(), root, id).unwrap();
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .swap(1, 2);
    prepared.commit(app.world_mut()).unwrap();
    switch(app.world_mut(), root, id);
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
    let id = destination(app.world(), root, -1).unwrap();
    let prepared = transfers::prepare_identity(app.world(), root, id).unwrap();
    app.world_mut()
        .get_mut::<Workspaces>(root)
        .unwrap()
        .entries
        .retain(|entry| entry.id != id);
    assert!(prepared.commit(app.world_mut()).is_err());
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().active, 31);
    assert_eq!(numbers(app.world(), root), vec![1, 31]);
}

#[test]
fn reordered_number_mapping_names_notes_and_areas_survive_the_existing_save_and_restart_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.json");
    let (mut app, root) = fixture(Some(path.clone()));
    switch(app.world_mut(), root, 31);
    let sand = crate::native::sand_store::spawn_sand(
        app.world_mut(),
        root,
        7,
        SandKind::EditableText,
        "Research",
        DVec2::new(41.0, -32.0),
    );
    let editor = app
        .world()
        .get::<StoredSand>(sand)
        .unwrap()
        .content
        .unwrap();
    app.world_mut()
        .get_mut::<bevy::text::EditableText>(editor)
        .unwrap()
        .editor
        .set_text("A note on another workspace");
    crate::native::area::spawn_area(
        app.world_mut(),
        root,
        7,
        crate::native::area::InfluenceArea::new(
            crate::native::area::AreaShape::Circle,
            DVec2::ZERO,
            DVec2::splat(200.0),
        ),
    )
    .unwrap();
    reorder(app.world_mut(), root, 1).unwrap();
    assert_eq!(numbers(app.world(), root), vec![1, 7, 31]);
    let saved = super::super::snapshot(app.world_mut(), root);
    super::super::tests::flush(&mut app);
    drop(app);
    let (mut restored, root) = super::super::tests::fixture(Some(path));
    let after = super::super::snapshot(restored.world_mut(), root);
    assert_eq!(numbers(restored.world(), root), vec![1, 7, 31]);
    assert_eq!(restored.world().get::<Workspaces>(root).unwrap().active, 31);
    assert_eq!(
        serde_json::to_value(&after.workspaces).unwrap(),
        serde_json::to_value(&saved.workspaces).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.sands).unwrap(),
        serde_json::to_value(&saved.sands).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&after.areas).unwrap(),
        serde_json::to_value(&saved.areas).unwrap()
    );
    assert_eq!(after.areas.len(), 1);
    assert!(
        serde_json::to_string(&after.sands)
            .unwrap()
            .contains("A note on another workspace")
    );
}
