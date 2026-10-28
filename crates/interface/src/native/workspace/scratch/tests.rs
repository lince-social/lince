use super::*;
use crate::{host::WorkspaceLayout, layout::desktop::PlacementId, native::layout::desktop::{self, Managed}};

#[test]
fn a_real_scratch_snapshot_write_failure_keeps_live_content_and_last_good_files_then_retry_reopens_hidden_at_home() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.json");
    let (mut app, root) = super::super::tests::fixture(Some(path.clone()));
    app.world_mut().entity_mut(root).insert(ComputedNode { size: Vec2::new(1000.0, 700.0), inverse_scale_factor: 1.0, ..default() });
    let parent = crate::native::sand_store::spawn_sand(app.world_mut(), root, 1, SandKind::EditableText, "Retained scratch write", DVec2::ZERO);
    let field = app.world().get::<StoredSand>(parent).unwrap().content.unwrap();
    app.world_mut().get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("Last good scratch body");
    desktop::focus_item(app.world_mut(), parent);
    let id = app.world().get::<Managed>(parent).unwrap().id;
    desktop::apply_scratch_action(app.world_mut(), root, crate::host::DesktopAction::MarkScratch).unwrap().unwrap();
    super::super::tests::flush(&mut app);
    assert!(app.world().get::<Workspaces>(root).unwrap().error.is_none());
    let snapshots = path.with_extension("snapshots");
    let backup = directory.path().join("last-good-snapshots");
    let good = std::fs::read_dir(&snapshots).unwrap().map(|entry| {
        let path = entry.unwrap().path();
        (path.file_name().unwrap().to_owned(), std::fs::read(path).unwrap())
    }).collect::<Vec<_>>();
    assert!(!good.is_empty());
    std::fs::rename(&snapshots, &backup).unwrap();
    std::fs::write(&snapshots, b"The selected snapshot path is temporarily unavailable").unwrap();
    app.world_mut().get_mut::<bevy::text::EditableText>(field).unwrap().editor.set_text("Retried scratch body");
    let live = app.world().get::<Workspaces>(root).unwrap().entries[0].desktop.clone();
    super::super::tests::flush(&mut app);
    assert!(app.world().get::<Workspaces>(root).unwrap().error.is_some());
    assert_eq!(app.world().get::<Workspaces>(root).unwrap().entries[0].desktop, live);
    assert_eq!(app.world().get::<bevy::text::EditableText>(field).unwrap().value(), "Retried scratch body");
    for (name, bytes) in &good { assert_eq!(&std::fs::read(backup.join(name)).unwrap(), bytes); }
    std::fs::remove_file(&snapshots).unwrap(); std::fs::rename(&backup, &snapshots).unwrap();
    super::super::tests::flush(&mut app);
    assert!(app.world().get::<Workspaces>(root).unwrap().error.is_none());
    drop(app);
    let (mut restored, restored_root) = super::super::tests::fixture(Some(path));
    let entity = restored.world_mut().query::<(Entity, &LayoutBox, &StoredSand)>().iter(restored.world())
        .find_map(|(entity, layout, _)| (PlacementId(layout.id) == id).then_some(entity)).unwrap();
    let field = restored.world().get::<StoredSand>(entity).unwrap().content.unwrap();
    assert_eq!(restored.world().get::<bevy::text::EditableText>(field).unwrap().value(), "Retried scratch body");
    assert!(restored.world().get::<Workspaces>(restored_root).unwrap().entries[0].desktop.scratch_hidden(id));
}

#[test]
fn ordinary_native_save_normalizes_scratch_home_and_startup_restores_hidden_content() {
    let directory = tempfile::tempdir().unwrap();
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        let path = directory.path().join(format!("scratch-{mode:?}.json"));
        let (mut app, root) = super::super::tests::fixture(Some(path.clone()));
        app.world_mut().entity_mut(root).insert(ComputedNode { size: Vec2::new(1000.0, 700.0), inverse_scale_factor: 1.0, ..default() });
        app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0].desktop.set_mode(mode);
        let entity = crate::native::sand_store::spawn_sand(app.world_mut(), root, 1, SandKind::Square, "Retained scratch content", DVec2::new(-120.0, 80.0));
        desktop::focus_item(app.world_mut(), entity);
        let id = app.world().get::<Managed>(entity).unwrap().id;
        let original = app.world().get::<Workspaces>(root).unwrap().entries[0].desktop.items().iter().find(|item| item.id == id).unwrap().clone();
        desktop::apply_scratch_action(app.world_mut(), root, crate::host::DesktopAction::MarkScratch).unwrap().unwrap();
        let child = crate::native::sand_store::spawn_sand(app.world_mut(), root, 1, SandKind::Square, "Retained owned editor", DVec2::new(-80.0, 120.0));
        app.world_mut().entity_mut(child).insert(desktop::TransientParent(entity));
        crate::native::layout::resolve_for_test(app.world_mut());
        let child_id = app.world().get::<Managed>(child).unwrap().id;
        let mut workspace = app.world().get::<Workspaces>(root).unwrap().entries[0].clone();
        workspace.id = 2; workspace.name = "Work".into(); workspace.desktop = crate::layout::desktop::Workspace::new(mode);
        app.world_mut().get_mut::<Workspaces>(root).unwrap().entries.push(workspace);
        crate::native::workspace::switch_output(app.world_mut(), root, 2);
        desktop::apply_scratch_action(app.world_mut(), root, crate::host::DesktopAction::ShowScratch).unwrap().unwrap();
        let live = app.world().get::<Workspaces>(root).unwrap().entries.clone();
        let document = snapshot(app.world_mut(), root);
        assert!(document.validate(), "{:?}", document.validation_error());
        assert_eq!(app.world().get::<WorkspaceMember>(entity), Some(&WorkspaceMember(2)));
        assert_eq!(app.world().get::<Workspaces>(root).unwrap().entries.iter().map(|space| &space.desktop).collect::<Vec<_>>(), live.iter().map(|space| &space.desktop).collect::<Vec<_>>());
        let saved = document.sands.iter().find(|saved| saved.placement.layout.is_some_and(|layout| layout.id == id.0)).unwrap();
        let saved_texts = serde_json::to_value(&saved.texts).unwrap();
        let saved_child = document.sands.iter().find(|saved| saved.placement.layout.is_some_and(|layout| layout.id == child_id.0)).unwrap();
        assert_eq!(saved_child.workspace, 1);
        assert_eq!(saved_child.placement.transient_parent, Some(id.0));
        let child_texts = serde_json::to_value(&saved_child.texts).unwrap();
        assert_eq!(saved.workspace, 1);
        assert_eq!(saved.position, std::array::from_fn(|axis| original.free.origin[axis] + original.free.size[axis] * 0.5));
        assert!(document.workspaces[0].desktop.scratch_hidden(id));
        super::super::tests::flush(&mut app);
        assert!(app.world().get::<Workspaces>(root).unwrap().error.is_none());
        drop(app);
        let (mut restored, restored_root) = super::super::tests::fixture(Some(path.clone()));
        let spaces = restored.world().get::<Workspaces>(restored_root).unwrap();
        assert!(spaces.entries.iter().find(|space| space.id == 1).unwrap().desktop.scratch_hidden(id));
        let restored_entity = restored.world_mut().query::<(Entity, &LayoutBox)>().iter(restored.world()).find(|(_, layout)| PlacementId(layout.id) == id).map(|(entity, _)| entity).unwrap();
        assert_eq!(restored.world().get::<WorkspaceMember>(restored_entity), Some(&WorkspaceMember(1)));
        assert!(restored.world().get::<StoredSand>(restored_entity).is_some());
        assert_eq!(restored.world().get::<CanvasItem>(restored_entity).unwrap().position.to_array(), saved.position);
        assert_eq!(restored.world().get::<Workspaces>(restored_root).unwrap().entries.iter().flat_map(|space| space.desktop.items()).filter(|item| item.id == id).count(), 1);
        restored.world_mut().entity_mut(restored_root).insert(ComputedNode { size: Vec2::new(1000.0, 700.0), inverse_scale_factor: 1.0, ..default() });
        crate::native::layout::resolve_for_test(restored.world_mut());
        let child = restored.world_mut().query::<(Entity, &LayoutBox)>().iter(restored.world()).find(|(_, layout)| PlacementId(layout.id) == child_id).map(|(entity, _)| entity).unwrap();
        assert_eq!(restored.world().get::<desktop::TransientParent>(child).map(|parent| parent.0), Some(restored_entity));
        assert!(!desktop::visible_item(restored.world(), restored_root, child));
        let reopened = snapshot(restored.world_mut(), restored_root);
        assert_eq!(serde_json::to_value(&reopened.sands.iter().find(|saved| saved.placement.layout.is_some_and(|layout| layout.id == id.0)).unwrap().texts).unwrap(), saved_texts);
        assert_eq!(serde_json::to_value(&reopened.sands.iter().find(|saved| saved.placement.layout.is_some_and(|layout| layout.id == child_id.0)).unwrap().texts).unwrap(), child_texts);
        assert!(restored.world_mut().despawn(restored_entity));
        super::super::tests::flush(&mut restored);
        assert!(restored.world().get::<Workspaces>(restored_root).unwrap().error.is_none());
        let orphaned = snapshot(restored.world_mut(), restored_root);
        let retained = orphaned.sands.iter().find(|saved| saved.placement.layout.is_some_and(|layout| layout.id == child_id.0)).unwrap();
        assert!(retained.placement.transient_parent.is_none());
        assert_eq!(serde_json::to_value(&retained.texts).unwrap(), child_texts);
        drop(restored);
        let (mut reopened, root) = super::super::tests::fixture(Some(path));
        let child = reopened.world_mut().query::<(Entity, &LayoutBox)>().iter(reopened.world())
            .find_map(|(entity, layout)| (PlacementId(layout.id) == child_id).then_some(entity)).unwrap();
        assert!(reopened.world().get::<desktop::TransientParent>(child).is_none());
        assert!(reopened.world().get::<Workspaces>(root).unwrap().entries.iter().all(|workspace|
            workspace.desktop.scratch(id).is_none() && workspace.desktop.items().iter().all(|item| item.id != id)));
    }
}
