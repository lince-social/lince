use super::*;
use super::super::tests::{id, item, viewport};

fn workspace(mode: WorkspaceLayout) -> Workspace {
    let mut workspace = Workspace::new(mode);
    for value in 1..=3 { workspace.insert(item(value)).unwrap(); }
    workspace.focus(id(1), viewport()).unwrap();
    workspace
}

#[test]
fn marking_does_not_move_or_hide_and_twenty_reveals_keep_one_item_and_restore_its_home() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        for floating in [false, true] {
            let mut home = workspace(mode);
            if floating { home.toggle_floating(id(1), viewport()).unwrap(); }
            home.set_item_width(id(1), Width::Third).unwrap();
            let original = home.items().iter().find(|item| item.id == id(1)).unwrap().clone();
            let original_width = home.item_width(id(1));
            let before = home.solve(viewport(), viewport()).unwrap();
            home.mark_scratch(id(1), 1).unwrap();
            assert_eq!(home.solve(viewport(), viewport()).unwrap(), before);
            assert!(home.scratch(id(1)).unwrap().shown());
            let mut work = Workspace::new(mode);
            work.insert(item(4)).unwrap();
            for _ in 0..20 {
                home.hide_scratch(id(1)).unwrap();
                assert!(!home.minimized(id(1)));
                assert!(!home.solve(viewport(), viewport()).unwrap().items.iter().any(|item| item.id == id(1)));
                assert_eq!(home.focus(id(1), viewport()), Err(LayoutError::InvalidState));
                home.transfer_to(&mut work, id(1), TransferKind::Item).unwrap();
                assert!(work.scratch_hidden(id(1)));
                work.show_scratch(id(1), original.free).unwrap();
                assert!(work.items().iter().find(|item| item.id == id(1)).unwrap().floating);
                assert_eq!(home.items().iter().chain(work.items()).filter(|item| item.id == id(1)).count(), 1);
                work.hide_scratch(id(1)).unwrap();
                work.hide_scratch(id(1)).unwrap();
                work.transfer_to(&mut home, id(1), TransferKind::Item).unwrap();
            }
            home.unmark_scratch(id(1), true).unwrap();
            assert_eq!(home.items().iter().find(|item| item.id == id(1)), Some(&original));
            assert_eq!(home.item_width(id(1)), original_width);
            assert!(home.scratch(id(1)).is_none());
            assert!(home.valid());
            assert!(work.valid());
        }
    }
}

#[test]
fn scratch_and_groups_are_exclusive_and_failed_actions_preserve_the_model() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    workspace.mark_scratch(id(1), 1).unwrap();
    let marked = workspace.clone();
    assert!(workspace.create_compact_group(id(1), &[id(2)]).is_err());
    assert_eq!(workspace, marked);
    workspace.create_compact_group(id(2), &[id(3)]).unwrap();
    let grouped = workspace.clone();
    assert_eq!(workspace.mark_scratch(id(2), 1), Err(LayoutError::InvalidState));
    assert_eq!(workspace, grouped);
    assert_eq!(workspace.show_scratch(id(1), Geometry { origin: [f64::NAN, 0.0], size: [100.0, 100.0] }), Err(LayoutError::InvalidItem));
    assert_eq!(workspace, grouped);
}

#[test]
fn normalized_native_save_starts_hidden_at_home_and_never_persists_the_reveal_state() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    workspace.mark_scratch(id(1), 1).unwrap();
    workspace.show_scratch(id(1), item(1).free).unwrap();
    workspace.normalize_scratch(id(1), true).unwrap();
    assert!(workspace.scratch_hidden(id(1)));
    let restored: Workspace = serde_json::from_slice(&serde_json::to_vec(&workspace).unwrap()).unwrap();
    assert!(restored.valid());
    assert!(restored.scratch_hidden(id(1)));
    assert_eq!(restored.scratch(id(1)).unwrap().home_workspace(), 1);
    assert!(!restored.items().iter().find(|item| item.id == id(1)).unwrap().floating);
}

#[test]
fn hidden_client_requests_cannot_reveal_scratch_or_take_fullscreen_and_close_retires_it() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    workspace.mark_scratch(id(1), 1).unwrap();
    workspace.hide_scratch(id(1)).unwrap();
    workspace.request_state(id(1), crate::host::WindowRequest { fullscreen: Some(true), maximized: Some(true), minimized: Some(false) });
    assert!(workspace.scratch_hidden(id(1)));
    assert_ne!(workspace.focused(), Some(id(1)));
    assert_eq!(workspace.fullscreen(), None);
    assert!(workspace.valid());
    workspace.remove(id(1)).unwrap();
    assert!(workspace.scratch(id(1)).is_none());
    assert!(workspace.valid());
}
