use super::*;
use crate::host::WindowRequest;
use super::super::tests::{id, item, viewport};

fn workspace(mode: WorkspaceLayout) -> Workspace {
    let mut workspace = Workspace::new(mode);
    for value in 1..=3 {
        let mut item = item(value);
        if value < 3 { item.kind = ItemKind::Application; }
        workspace.insert(item).unwrap();
    }
    workspace.focus(id(1), viewport()).unwrap();
    workspace
}

#[test]
fn bounded_protocol_presentation_retains_requests_homes_and_neighbour_geometry_for_twenty_cycles() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        for floating in [false, true] {
            for maximized in [false, true] {
                let mut workspace = workspace(mode);
                if floating { workspace.toggle_floating(id(1), Rectangle::new([80.0, 90.0], [350.0, 220.0]).unwrap()).unwrap(); }
                if maximized { workspace.toggle_maximized(); }
                let solution = workspace.solve(viewport(), viewport()).unwrap();
                let original = workspace.clone();
                for _ in 0..20 {
                    assert!(workspace.set_windowed_fullscreen(id(1), true).unwrap());
                    assert!(workspace.protocol_fullscreen(id(1)));
                    assert!(workspace.windowed_fullscreen(id(1)));
                    assert_eq!(workspace.fullscreen(), None);
                    assert!(!workspace.set_windowed_fullscreen(id(1), true).unwrap());
                    assert_eq!(workspace.solve(viewport(), viewport()).unwrap(), solution);
                    assert!(workspace.set_windowed_fullscreen(id(1), false).unwrap());
                    assert!(!workspace.protocol_fullscreen(id(1)));
                    assert_eq!(workspace, original);
                }
            }
        }
    }
}

#[test]
fn output_constraints_and_deliberate_edits_resolve_the_existing_request_without_an_entry_snapshot() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    workspace.set_item_width(id(1), Width::Half).unwrap();
    workspace.set_windowed_fullscreen(id(1), true).unwrap();
    let original = workspace.items().to_vec();
    let output = Rectangle::new([500.0, 0.0], [1200.0, 800.0]).unwrap();
    let usable = Rectangle::new([500.0, 40.0], [1200.0, 760.0]).unwrap();
    let bounded = workspace.solve(usable, output).unwrap();
    assert_eq!(workspace.items(), original.as_slice());
    assert_eq!(workspace.item_width(id(1)), Some(Width::Half));
    workspace.set_windowed_fullscreen(id(1), false).unwrap();
    assert_eq!(workspace.solve(usable, output).unwrap(), bounded);
    workspace.set_windowed_fullscreen(id(1), true).unwrap();
    workspace.set_item_width(id(1), Width::Third).unwrap();
    workspace.set_windowed_fullscreen(id(1), false).unwrap();
    assert_eq!(workspace.item_width(id(1)), Some(Width::Third));
    assert_eq!(workspace.items(), original.as_slice());
}

#[test]
fn client_fullscreen_and_maximize_requests_preserve_bounded_policy_until_an_explicit_exit() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    workspace.set_windowed_fullscreen(id(1), true).unwrap();
    for _ in 0..20 {
        assert!(workspace.request_state(id(1), WindowRequest { fullscreen: Some(true), ..Default::default() }));
        assert_eq!(workspace.fullscreen(), None);
        assert!(workspace.protocol_fullscreen(id(1)));
    }
    workspace.request_state(id(1), WindowRequest { maximized: Some(true), ..Default::default() });
    assert!(workspace.maximized(id(1)));
    assert!(workspace.windowed_fullscreen(id(1)));
    workspace.request_state(id(1), WindowRequest { fullscreen: Some(false), ..Default::default() });
    workspace.request_state(id(1), WindowRequest { fullscreen: Some(false), ..Default::default() });
    assert!(!workspace.protocol_fullscreen(id(1)));
    assert!(workspace.maximized(id(1)));
    workspace.set_windowed_fullscreen(id(1), true).unwrap();
    assert!(workspace.toggle_maximized());
    assert!(!workspace.windowed_fullscreen(id(1)));
    assert!(!workspace.maximized(id(1)));
    workspace.set_windowed_fullscreen(id(1), true).unwrap();
    workspace.toggle_fullscreen();
    assert!(!workspace.windowed_fullscreen(id(1)));
    assert_eq!(workspace.fullscreen(), Some(id(1)));
    assert!(workspace.valid());
}

#[test]
fn bounded_policy_does_not_clear_a_different_windows_fullscreen_intent() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    workspace.set_windowed_fullscreen(id(1), true).unwrap();
    workspace.request_state(id(2), WindowRequest { fullscreen: Some(true), ..Default::default() });
    workspace.request_state(id(1), WindowRequest { fullscreen: Some(true), ..Default::default() });
    assert_eq!(workspace.fullscreen(), Some(id(2)));
    assert!(workspace.windowed_fullscreen(id(1)));
    workspace.set_windowed_fullscreen(id(1), false).unwrap();
    assert_eq!(workspace.fullscreen(), Some(id(2)));
    assert!(workspace.valid());
}

#[test]
fn close_transfer_and_cold_load_never_replay_a_retired_live_protocol_mode() {
    let mut source = workspace(WorkspaceLayout::Tiled);
    source.set_windowed_fullscreen(id(1), true).unwrap();
    let mut target = Workspace::new(WorkspaceLayout::Tiled);
    source.transfer_to(&mut target, id(1), TransferKind::Item).unwrap();
    assert!(!source.windowed_fullscreen(id(1)));
    assert!(target.windowed_fullscreen(id(1)));
    let encoded = serde_json::to_value(&target).unwrap();
    assert!(encoded.get("windowed_fullscreen").is_none());
    let restored: Workspace = serde_json::from_value(encoded).unwrap();
    assert!(!restored.windowed_fullscreen(id(1)));
    assert!(restored.valid());
    let removed = target.remove(id(1)).unwrap();
    target.insert(removed).unwrap();
    assert!(!target.protocol_fullscreen(id(1)));
    assert!(target.valid());
}

#[test]
fn native_missing_and_invalid_targets_leave_the_complete_model_unchanged() {
    let mut workspace = workspace(WorkspaceLayout::Tiled);
    let before = workspace.clone();
    assert_eq!(workspace.set_windowed_fullscreen(id(3), true), Err(LayoutError::InvalidItem));
    assert_eq!(workspace.set_windowed_fullscreen(id(9), true), Err(LayoutError::MissingItem));
    assert_eq!(workspace, before);
    workspace.windowed_fullscreen.insert(id(3));
    assert!(!workspace.valid());
}
