use super::*;
use crate::{
    host::{Direction, Manipulation, WorkspaceLayout},
    native::layout::desktop::manipulation,
};

fn canonical(mut document: Document) -> serde_json::Value {
    document
        .sands
        .sort_by_key(|sand| sand.placement.layout.map(|layout| layout.id));
    serde_json::to_value(document).unwrap()
}

#[test]
fn saving_and_reloading_during_move_or_resize_keeps_only_committed_scene_data() {
    let directory = tempfile::tempdir().unwrap();
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        let path = directory.path().join(format!("{mode:?}.json"));
        let (mut app, root) = tests::fixture(Some(path.clone()));
        app.world_mut().entity_mut(root).insert(ComputedNode {
            size: Vec2::new(1000.0, 700.0),
            inverse_scale_factor: 1.0,
            ..default()
        });
        app.world_mut().get_mut::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .set_mode(mode);
        let target = crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::Square,
            "Original window",
            DVec2::new(-200.0, 0.0),
        );
        let anchor = crate::native::sand_store::spawn_sand(
            app.world_mut(),
            root,
            1,
            SandKind::Square,
            "Unrelated window",
            DVec2::new(200.0, 0.0),
        );
        crate::native::layout::desktop::focus_item(app.world_mut(), target);
        let id = crate::layout::desktop::PlacementId(
            app.world()
                .get::<crate::native::layout::LayoutBox>(target)
                .unwrap()
                .id,
        );
        let anchor = crate::layout::desktop::PlacementId(
            app.world()
                .get::<crate::native::layout::LayoutBox>(anchor)
                .unwrap()
                .id,
        );
        let before = canonical(snapshot(app.world_mut(), root));
        let operation = if mode == WorkspaceLayout::Free {
            Manipulation::Resize
        } else {
            Manipulation::Move
        };
        assert!(manipulation::begin(
            app.world_mut(),
            root,
            target,
            operation,
            [1.0; 2]
        ));
        let slot = (mode == WorkspaceLayout::Tiled).then_some(crate::layout::desktop::DropSlot {
            anchor,
            side: Direction::Right,
        });
        assert!(manipulation::propose(app.world_mut(), [80.0, 50.0], slot));
        assert_eq!(canonical(snapshot(app.world_mut(), root)), before);
        tests::flush(&mut app);
        assert!(manipulation::active(app.world()));
        assert!(app.world().get::<Workspaces>(root).unwrap().error.is_none());
        let saved = read_document(&path).unwrap().unwrap();
        assert_eq!(canonical(saved), before);
        let (mut restored, restored_root) = tests::fixture(Some(path));
        assert_eq!(
            canonical(snapshot(restored.world_mut(), restored_root)),
            before
        );
        assert_eq!(
            restored
                .world()
                .get::<Workspaces>(restored_root)
                .unwrap()
                .entries[0]
                .desktop,
            app.world().get::<Workspaces>(root).unwrap().entries[0].desktop
        );
        let source = app.world().get::<Workspaces>(root).unwrap().entries[0]
            .desktop
            .items()
            .iter()
            .find(|item| item.id == id)
            .unwrap()
            .clone();
        assert!(source.free.rectangle().is_some());
        manipulation::cancel(app.world_mut());
    }
}
