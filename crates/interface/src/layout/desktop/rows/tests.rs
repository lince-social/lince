use super::super::tests::{id, item, viewport};
use super::*;

fn tiled(count: u8) -> Workspace {
    let mut workspace = Workspace::new(WorkspaceLayout::Tiled);
    for value in 1..=count {
        workspace.insert(item(value)).unwrap();
    }
    workspace
}

fn solved(workspace: &mut Workspace) -> BTreeMap<PlacementId, Rectangle> {
    workspace
        .solve(viewport(), viewport())
        .unwrap()
        .items
        .into_iter()
        .map(|item| (item.id, item.rectangle))
        .collect()
}

#[test]
fn rows_overflow_without_resizing_neighbors_and_restore_scroll_and_free_geometry() {
    let mut workspace = tiled(3);
    let free: Vec<_> = workspace
        .items()
        .iter()
        .map(|item| (item.id, item.free))
        .collect();
    let before = solved(&mut workspace);
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1), id(2), id(3)]]);
    assert_eq!(before[&id(1)].size()[0], 488.0);
    assert_eq!(before[&id(3)].origin()[0], 1000.0);
    workspace.focus(id(3), viewport()).unwrap();
    assert!(workspace.scroll()[0] > 0.0);
    workspace.insert(item(4)).unwrap();
    workspace.focus(id(4), viewport()).unwrap();
    let after = solved(&mut workspace);
    for id in [id(1), id(2), id(3)] {
        assert_eq!(after[&id].size(), before[&id].size());
    }
    let scroll = workspace.scroll();
    let rows = workspace.tiled_rows();
    workspace.set_mode(WorkspaceLayout::Free);
    assert_eq!(workspace.scroll(), [0.0; 2]);
    for (id, geometry) in free {
        assert_eq!(workspace.item(id).unwrap().free, geometry);
    }
    workspace.set_mode(WorkspaceLayout::Tiled);
    assert_eq!(workspace.scroll(), scroll);
    assert_eq!(workspace.tiled_rows(), rows);
    assert!(workspace.valid());
}

#[test]
fn horizontal_navigation_stays_in_the_row_and_vertical_navigation_uses_presented_centers() {
    let mut workspace = tiled(3);
    workspace.focus(id(1), viewport()).unwrap();
    assert!(
        !workspace
            .focus_direction(Direction::Left, viewport())
            .unwrap()
    );
    assert!(
        workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert_eq!(workspace.focused(), Some(id(2)));
    assert!(workspace.move_focused(Direction::Down, viewport()));
    assert_eq!(
        workspace.tiled_rows(),
        vec![vec![id(1), id(3)], vec![id(2)]]
    );
    workspace.set_item_width(id(1), Width::Third).unwrap();
    assert!(
        workspace
            .focus_direction(Direction::Up, viewport())
            .unwrap()
    );
    assert_eq!(workspace.focused(), Some(id(1)));
    assert!(
        workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert_eq!(workspace.focused(), Some(id(3)));
    assert!(
        !workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert!(
        workspace
            .focus_direction(Direction::Down, viewport())
            .unwrap()
    );
    assert_eq!(workspace.focused(), Some(id(2)));
    assert!(
        !workspace
            .focus_direction(Direction::Down, viewport())
            .unwrap()
    );
    assert!(workspace.scroll()[1] > 0.0);
    assert!(workspace.move_focused(Direction::Up, viewport()));
    assert_eq!(workspace.tiled_rows(), vec![vec![id(2), id(1), id(3)]]);
    assert_eq!(workspace.focused(), Some(id(2)));
    assert!(workspace.move_focused(Direction::Right, viewport()));
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1), id(2), id(3)]]);
    assert!(workspace.valid());
}

#[test]
fn moving_at_vertical_edges_creates_useful_rows_and_removes_empty_rows() {
    let mut workspace = tiled(2);
    workspace.focus(id(1), viewport()).unwrap();
    assert!(workspace.move_focused(Direction::Up, viewport()));
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1)], vec![id(2)]]);
    for _ in 0..20 {
        assert!(!workspace.move_focused(Direction::Up, viewport()));
    }
    assert_eq!(workspace.tiled_rows().len(), 2);
    assert!(workspace.move_focused(Direction::Down, viewport()));
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1), id(2)]]);
    workspace.remove(id(2));
    for direction in [Direction::Up, Direction::Down] {
        assert!(!workspace.move_focused(direction, viewport()));
    }
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1)]]);
    assert!(workspace.valid());
}

#[test]
fn close_prefers_a_visible_neighbor_in_the_same_row_and_minimize_keeps_its_slot() {
    let mut workspace = tiled(4);
    workspace.focus(id(3), viewport()).unwrap();
    workspace.move_focused(Direction::Down, viewport());
    workspace.set_minimized(id(2), true);
    workspace.focus(id(4), viewport()).unwrap();
    workspace.remove(id(4));
    assert_eq!(workspace.focused(), Some(id(1)));
    assert_eq!(
        workspace.tiled_rows(),
        vec![vec![id(1), id(2)], vec![id(3)]]
    );
    assert!(!solved(&mut workspace).contains_key(&id(2)));
    workspace.set_minimized(id(2), false);
    assert!(solved(&mut workspace).contains_key(&id(2)));
    assert_eq!(
        workspace.tiled_rows(),
        vec![vec![id(1), id(2)], vec![id(3)]]
    );
    assert!(workspace.valid());
    let mut middle = tiled(3);
    middle.focus(id(2), viewport()).unwrap();
    middle.remove(id(2));
    assert_eq!(middle.focused(), Some(id(3)));
}

#[test]
fn floating_remembers_row_slot_width_free_pose_and_the_last_floating_rectangle() {
    let mut workspace = tiled(3);
    workspace
        .set_item_width(id(2), Width::Fraction(1, 3))
        .unwrap();
    workspace.focus(id(2), viewport()).unwrap();
    let free = workspace.item(id(2)).unwrap().free;
    let rectangle = solved(&mut workspace)[&id(2)];
    workspace.toggle_floating(id(2), rectangle).unwrap();
    let floating = Geometry {
        origin: [900.0, 800.0],
        size: [400.0, 300.0],
    };
    workspace.set_free_geometry(id(2), floating).unwrap();
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1), id(3)]]);
    assert!(workspace.valid());
    let encoded = serde_json::to_vec(&workspace).unwrap();
    workspace = serde_json::from_slice(&encoded).unwrap();
    workspace
        .toggle_floating(id(2), floating.rectangle().unwrap())
        .unwrap();
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1), id(2), id(3)]]);
    assert_eq!(workspace.item_width(id(2)), Some(Width::Fraction(1, 3)));
    assert_eq!(workspace.item(id(2)).unwrap().free, free);
    let rectangle = solved(&mut workspace)[&id(2)];
    workspace.toggle_floating(id(2), rectangle).unwrap();
    assert_eq!(workspace.item(id(2)).unwrap().free, floating);
    workspace.set_mode(WorkspaceLayout::Free);
    let authored = Geometry {
        origin: [60.0, 70.0],
        ..floating
    };
    workspace.set_free_geometry(id(2), authored).unwrap();
    workspace.set_mode(WorkspaceLayout::Tiled);
    workspace
        .toggle_floating(id(2), authored.rectangle().unwrap())
        .unwrap();
    assert_eq!(workspace.item(id(2)).unwrap().free, authored);
    assert!(workspace.valid());
}

#[test]
fn retiling_reconstructs_a_removed_row_and_deleted_windows_leave_no_home() {
    let mut workspace = tiled(2);
    workspace.focus(id(2), viewport()).unwrap();
    workspace.move_focused(Direction::Down, viewport());
    let rectangle = solved(&mut workspace)[&id(2)];
    workspace.toggle_floating(id(2), rectangle).unwrap();
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1)]]);
    workspace.toggle_floating(id(2), rectangle).unwrap();
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1)], vec![id(2)]]);
    workspace.toggle_floating(id(2), rectangle).unwrap();
    workspace.remove(id(2));
    workspace.insert(item(2)).unwrap();
    assert_eq!(workspace.tiled_rows(), vec![vec![id(1), id(2)]]);
    assert!(workspace.valid());
}

#[test]
fn row_constraints_and_area_scale_do_not_overlap_the_next_row_or_resize_neighbors() {
    let mut workspace = tiled(3);
    workspace.focus(id(3), viewport()).unwrap();
    workspace.move_focused(Direction::Down, viewport());
    let before = solved(&mut workspace);
    workspace
        .influence(
            id(1),
            Influence {
                scale: 1.5,
                order: None,
            },
        )
        .unwrap();
    let scaled = solved(&mut workspace);
    assert_eq!(scaled[&id(2)].size(), before[&id(2)].size());
    assert!(
        scaled[&id(3)].origin()[1] >= scaled[&id(1)].origin()[1] + scaled[&id(1)].size()[1] + 8.0
    );
    let constrained = Rules {
        axes: [
            super::super::super::Axis {
                min: 600.0,
                max: 650.0,
                size: 625.0,
                ..super::super::super::Axis::fixed(625.0)
            },
            item(2).rules.axes[1],
        ],
        ..item(2).rules
    };
    workspace.configure(id(2), constrained).unwrap();
    assert_eq!(solved(&mut workspace)[&id(2)].size()[0], 600.0);
    assert!(workspace.valid());
}

#[test]
fn row_membership_and_fractions_survive_serialization_and_reject_corruption() {
    let mut workspace = tiled(3);
    workspace
        .set_item_width(id(1), Width::Fraction(2, 3))
        .unwrap();
    workspace.focus(id(2), viewport()).unwrap();
    workspace.move_focused(Direction::Down, viewport());
    let encoded = serde_json::to_value(&workspace).unwrap();
    let decoded: Workspace = serde_json::from_value(encoded.clone()).unwrap();
    assert!(decoded.valid());
    assert_eq!(decoded, workspace);
    let mut corrupt = encoded;
    corrupt["rows"]["rows"][0]["items"][1]["id"] =
        corrupt["rows"]["rows"][0]["items"][0]["id"].clone();
    let decoded: Workspace = serde_json::from_value(corrupt).unwrap();
    assert!(!decoded.valid());
    for width in [
        Width::Fraction(1, 0),
        Width::Fraction(0, 1),
        Width::Fraction(2, 1),
    ] {
        let before = workspace.clone();
        assert_eq!(
            workspace.set_item_width(id(1), width),
            Err(LayoutError::InvalidItem)
        );
        assert_eq!(workspace, before);
    }
}

#[test]
fn free_focus_never_applies_tiled_scroll_to_authored_or_floating_positions() {
    let mut workspace = tiled(2);
    workspace.set_mode(WorkspaceLayout::Free);
    let geometry = Geometry {
        origin: [12000.0, -4000.0],
        size: [400.0, 300.0],
    };
    workspace.set_free_geometry(id(2), geometry).unwrap();
    workspace
        .toggle_floating(id(2), geometry.rectangle().unwrap())
        .unwrap();
    workspace.focus(id(2), viewport()).unwrap();
    assert_eq!(workspace.scroll(), [0.0; 2]);
    assert_eq!(
        solved(&mut workspace)[&id(2)],
        geometry.rectangle().unwrap()
    );
}
