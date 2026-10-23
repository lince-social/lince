use super::*;

fn id(value: u8) -> PlacementId {
    PlacementId([value; 16])
}

fn item(value: u8) -> Item {
    Item {
        id: id(value),
        kind: ItemKind::Native,
        rules: Rules::fixed([300.0, 200.0]),
        free: Geometry {
            origin: [20.0, 30.0],
            size: [300.0, 200.0],
        },
        floating: false,
    }
}

fn viewport() -> Rectangle {
    Rectangle::new([0.0, 24.0], [1000.0, 700.0]).unwrap()
}

#[test]
fn minimizing_preserves_placement_and_removes_items_from_every_layout_solution() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Row, WorkspaceLayout::Column, WorkspaceLayout::Grid, WorkspaceLayout::ScrollingColumns] {
        let mut workspace = workspace(3);
        workspace.set_mode(mode);
        workspace.focus(id(1), viewport()).unwrap();
        let columns = workspace.columns.clone();
        let free = workspace.item(id(2)).unwrap().free;
        assert!(workspace.set_minimized(id(2), true));
        assert!(workspace.valid());
        let solution = workspace.solve(viewport(), viewport()).unwrap();
        assert_eq!(solution.items.len(), 2);
        assert!(solution.items.iter().all(|item| item.id != id(2)));
        assert_eq!(workspace.columns, columns);
        assert_eq!(workspace.item(id(2)).unwrap().free, free);
        assert!(workspace.set_minimized(id(2), false));
        assert_eq!(workspace.solve(viewport(), viewport()).unwrap().items.len(), 3);
        assert_eq!(workspace.columns, columns);
        assert!(workspace.valid());
    }
}

#[test]
fn navigation_and_movement_skip_minimized_columns_and_rows() {
    let mut workspace = workspace(3);
    workspace.set_minimized(id(2), true);
    workspace.focus(id(1), viewport()).unwrap();
    assert!(workspace.focus_direction(Direction::Right, viewport()).unwrap());
    assert_eq!(workspace.focused(), Some(id(3)));
    assert!(workspace.move_focused(Direction::Left));
    assert_eq!(workspace.columns.iter().map(|column| column.items[0]).collect::<Vec<_>>(), vec![id(3), id(2), id(1)]);
    assert!(workspace.stack_focused(Direction::Right));
    assert_eq!(workspace.columns.last().unwrap().items, vec![id(1), id(3)]);
    workspace.set_minimized(id(1), true);
    assert!(!workspace.move_focused(Direction::Up));
    assert!(!workspace.stack_focused(Direction::Left));
    assert!(workspace.valid());
}

#[test]
fn restore_and_focus_preserve_fullscreen_and_closing_discards_minimized_state() {
    let mut workspace = workspace(2);
    workspace.focus(id(2), viewport()).unwrap();
    workspace.toggle_fullscreen();
    workspace.set_minimized(id(2), true);
    assert_eq!(workspace.fullscreen(), None);
    assert_ne!(workspace.focused(), Some(id(2)));
    workspace.focus(id(2), viewport()).unwrap();
    assert_eq!(workspace.fullscreen(), Some(id(2)));
    assert!(!workspace.minimized(id(2)));
    workspace.set_minimized(id(2), true);
    workspace.remove(id(2));
    assert!(workspace.valid());
    workspace.insert(item(2)).unwrap();
    assert!(!workspace.minimized(id(2)));
    assert_eq!(workspace.fullscreen(), None);
}

#[test]
fn all_minimized_items_allow_empty_layouts_and_explicit_restore_keeps_focus() {
    let mut workspace = workspace(2);
    workspace.set_minimized(id(1), true);
    workspace.set_minimized(id(2), true);
    assert_eq!(workspace.focused(), None);
    assert!(workspace.solve(viewport(), viewport()).unwrap().items.is_empty());
    assert!(!workspace.focus_direction(Direction::Right, viewport()).unwrap());
    workspace.request_state(id(2), crate::host::WindowRequest { minimized: Some(false), ..Default::default() });
    assert_eq!(workspace.focused(), None);
    assert_eq!(workspace.solve(viewport(), viewport()).unwrap().items.len(), 1);
    assert!(workspace.focus_direction(Direction::Right, viewport()).unwrap());
    assert_eq!(workspace.focused(), Some(id(2)));
    assert!(workspace.valid());
}

#[test]
fn explicit_application_state_requests_do_not_toggle_or_steal_focus() {
    let mut workspace = workspace(2);
    workspace.focus(id(1), viewport()).unwrap();
    let request = crate::host::WindowRequest {
        minimized: None,
        fullscreen: Some(true),
        maximized: Some(true),
    };
    assert!(workspace.request_state(id(2), request));
    assert!(workspace.request_state(id(2), request));
    assert_eq!(workspace.fullscreen(), Some(id(2)));
    assert!(workspace.maximized(id(2)));
    assert_eq!(workspace.focused(), Some(id(1)));
    assert!(!workspace.request_state(id(3), request));
    assert!(workspace.request_state(
        id(1),
        crate::host::WindowRequest {
            minimized: None,
            fullscreen: Some(false),
            maximized: None
        }
    ));
    assert_eq!(workspace.fullscreen(), Some(id(2)));
    assert!(workspace.request_state(
        id(2),
        crate::host::WindowRequest {
            minimized: None,
            fullscreen: Some(false),
            maximized: Some(false)
        }
    ));
    assert_eq!(workspace.fullscreen(), None);
    assert!(!workspace.maximized(id(2)));
}

fn workspace(count: u8) -> Workspace {
    let mut workspace = Workspace::new(WorkspaceLayout::ScrollingColumns);
    for value in 1..=count {
        workspace.insert(item(value)).unwrap();
    }
    workspace
}

#[test]
fn moving_in_grid_follows_visible_rows_and_columns_without_wrapping() {
    let mut workspace = workspace(5);
    workspace.set_mode(WorkspaceLayout::Grid);
    workspace.focus(id(1), viewport()).unwrap();
    assert!(!workspace.move_focused(Direction::Left));
    assert!(!workspace.move_focused(Direction::Up));
    assert!(workspace.move_focused(Direction::Down));
    assert_eq!(workspace.columns[3].items, [id(1)]);
    assert!(workspace.move_focused(Direction::Right));
    assert_eq!(workspace.columns[4].items, [id(1)]);
    assert!(!workspace.move_focused(Direction::Right));
    assert!(workspace.move_focused(Direction::Up));
    assert_eq!(workspace.columns[1].items, [id(1)]);
    assert_eq!(workspace.focused(), Some(id(1)));
    assert!(workspace.valid());
}

#[test]
fn moving_in_single_column_and_row_uses_their_visible_axis() {
    let mut workspace = workspace(3);
    workspace.set_mode(WorkspaceLayout::Column);
    workspace.focus(id(1), viewport()).unwrap();
    assert!(workspace.move_focused(Direction::Down));
    assert_eq!(workspace.columns[1].items, [id(1)]);
    assert!(!workspace.move_focused(Direction::Right));
    workspace.set_mode(WorkspaceLayout::Row);
    assert!(!workspace.move_focused(Direction::Down));
    assert!(workspace.move_focused(Direction::Right));
    assert_eq!(workspace.columns[2].items, [id(1)]);
    assert!(workspace.valid());
}

#[test]
fn focus_scrolls_only_enough_to_reveal_and_never_wraps() {
    let mut workspace = workspace(4);
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
    let second = workspace.solve(viewport(), viewport()).unwrap();
    assert_eq!(second.scroll[0], 8.0);
    assert!(
        workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    let third = workspace.solve(viewport(), viewport()).unwrap();
    let focused = third.items.iter().find(|item| item.id == id(3)).unwrap();
    assert_eq!(
        focused.rectangle.origin()[0] + focused.rectangle.size()[0],
        992.0
    );
    assert!(
        workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert!(
        !workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert_eq!(workspace.focused(), Some(id(4)));
    assert!(
        !workspace
            .focus_direction(Direction::Down, viewport())
            .unwrap()
    );
}

#[test]
fn impossible_stack_spills_without_violating_client_minimums() {
    let mut workspace = workspace(3);
    for value in 1..=3 {
        let mut rules = item(value).rules;
        rules.axes[1].min = 350.0;
        rules.axes[1].size = 350.0;
        workspace.configure(id(value), rules).unwrap();
    }
    assert!(workspace.stack_focused(Direction::Left));
    let solution = workspace.solve(viewport(), viewport()).unwrap();
    assert_eq!(workspace.columns.len(), 3);
    assert!(
        solution
            .items
            .iter()
            .all(|item| item.rectangle.size()[1] >= 350.0)
    );
    for pair in solution.items.windows(2) {
        assert!(
            pair[0].rectangle.origin()[0] + pair[0].rectangle.size()[0]
                < pair[1].rectangle.origin()[0]
        );
    }
    assert!(workspace.valid());
}

#[test]
fn stacks_share_height_respect_maxima_and_have_directional_focus() {
    let mut workspace = workspace(2);
    assert!(workspace.stack_focused(Direction::Left));
    let mut rules = item(1).rules;
    rules.axes[1].max = 240.0;
    workspace.configure(id(1), rules).unwrap();
    let solution = workspace.solve(viewport(), viewport()).unwrap();
    let first = solution.items.iter().find(|item| item.id == id(1)).unwrap();
    let second = solution.items.iter().find(|item| item.id == id(2)).unwrap();
    assert_eq!(first.rectangle.size()[1], 240.0);
    assert!((second.rectangle.size()[1] - 436.0).abs() < 0.001);
    assert_eq!(first.rectangle.origin()[0], second.rectangle.origin()[0]);
    assert!(
        workspace
            .focus_direction(Direction::Up, viewport())
            .unwrap()
    );
    assert_eq!(workspace.focused(), Some(id(1)));
    assert!(
        !workspace
            .focus_direction(Direction::Up, viewport())
            .unwrap()
    );
    assert!(workspace.move_focused(Direction::Down));
    assert_eq!(workspace.columns[0].items, vec![id(2), id(1)]);
}

#[test]
fn floating_round_trip_and_close_leave_no_duplicate_or_placeholder_tile() {
    let mut workspace = workspace(3);
    let solution = workspace.solve(viewport(), viewport()).unwrap();
    let rectangle = solution
        .items
        .iter()
        .find(|item| item.id == id(3))
        .unwrap()
        .rectangle;
    workspace.toggle_floating(id(3), rectangle).unwrap();
    assert_eq!(workspace.columns.len(), 2);
    let solution = workspace.solve(viewport(), viewport()).unwrap();
    assert!(
        solution
            .items
            .iter()
            .find(|item| item.id == id(3))
            .unwrap()
            .floating
    );
    workspace.toggle_floating(id(3), rectangle).unwrap();
    assert_eq!(workspace.columns.len(), 3);
    workspace.remove(id(2)).unwrap();
    assert_eq!(workspace.items.len(), 2);
    assert_eq!(workspace.columns.len(), 2);
    assert!(workspace.valid());
    workspace.remove(id(3)).unwrap();
    assert_eq!(workspace.focused(), Some(id(1)));
    workspace.remove(id(1)).unwrap();
    assert_eq!(workspace.focused(), None);
    assert!(workspace.valid());
}

#[test]
fn maximize_respects_bars_and_fullscreen_uses_the_output() {
    let mut workspace = workspace(1);
    let output = Rectangle::new([0.0, 0.0], [1000.0, 748.0]).unwrap();
    assert!(workspace.toggle_maximized());
    let solution = workspace.solve(viewport(), output).unwrap();
    assert_eq!(
        solution.items[0].rectangle,
        Rectangle::new([8.0, 32.0], [984.0, 684.0]).unwrap()
    );
    workspace.toggle_fullscreen();
    let solution = workspace.solve(viewport(), output).unwrap();
    assert_eq!(solution.items[0].rectangle, output);
    workspace.toggle_fullscreen();
    assert_eq!(
        workspace.solve(viewport(), output).unwrap().items[0]
            .rectangle
            .origin(),
        [8.0, 32.0]
    );
}

#[test]
fn maximizing_free_and_floating_items_tracks_usable_space_and_restores_geometry() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Row, WorkspaceLayout::Column, WorkspaceLayout::Grid, WorkspaceLayout::ScrollingColumns] {
        for floating in [false, true] {
            let mut workspace = workspace(1);
            workspace.set_mode(mode);
            if floating { workspace.toggle_floating(id(1), item(1).free.rectangle().unwrap()).unwrap(); }
            let before = workspace.solve(viewport(), viewport()).unwrap().items[0].rectangle;
            let free = workspace.item(id(1)).unwrap().free;
            assert!(workspace.toggle_maximized());
            assert!(workspace.maximized(id(1)));
            let with_bar = workspace.solve(viewport(), viewport()).unwrap().items[0];
            assert_eq!(with_bar.rectangle, Rectangle::new([8.0, 32.0], [984.0, 684.0]).unwrap());
            assert!(!with_bar.floating);
            let expanded = Rectangle::new([0.0; 2], [1000.0, 748.0]).unwrap();
            assert_eq!(workspace.solve(expanded, expanded).unwrap().items[0].rectangle, Rectangle::new([8.0; 2], [984.0, 732.0]).unwrap());
            assert!(workspace.toggle_maximized());
            assert!(!workspace.maximized(id(1)));
            assert_eq!(workspace.solve(viewport(), viewport()).unwrap().items[0].rectangle, before);
            assert_eq!(workspace.item(id(1)).unwrap().free, free);
            assert!(workspace.valid());
            workspace.remove(id(1));
            assert!(workspace.valid());
        }
    }
}

#[test]
fn saved_layout_round_trips_and_rejects_corrupt_membership() {
    let mut workspace = workspace(4);
    workspace.stack_focused(Direction::Left);
    workspace.focus(id(3), viewport()).unwrap();
    workspace.cycle_width();
    workspace.solve(viewport(), viewport()).unwrap();
    let encoded = serde_json::to_vec(&workspace).unwrap();
    let restored: Workspace = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored, workspace);
    assert!(restored.valid());
    let mut malformed = restored.clone();
    malformed.columns[0].items.push(id(4));
    assert_eq!(
        malformed.solve(viewport(), viewport()),
        Err(LayoutError::InvalidState)
    );
    let mut malformed = restored;
    malformed.items[0].rules.axes[0].min = f32::NAN;
    assert!(!malformed.valid());
}

#[test]
fn resizing_a_stack_item_changes_its_share_without_overlapping_neighbors() {
    let mut workspace = workspace(2);
    workspace.stack_focused(Direction::Left);
    workspace.resize_focused_height(200.0);
    let solution = workspace.solve(viewport(), viewport()).unwrap();
    let first = solution
        .items
        .iter()
        .find(|item| item.id == id(1))
        .unwrap()
        .rectangle;
    let second = solution
        .items
        .iter()
        .find(|item| item.id == id(2))
        .unwrap()
        .rectangle;
    assert!(second.size()[1] > first.size()[1] * 1.9);
    assert_eq!(
        first.origin()[1] + first.size()[1] + 8.0,
        second.origin()[1]
    );
    assert!((second.origin()[1] + second.size()[1] - 716.0).abs() < 0.001);
}

#[test]
fn hostile_constraints_and_tiny_outputs_produce_finite_valid_requests() {
    for mode in [
        WorkspaceLayout::Free,
        WorkspaceLayout::Row,
        WorkspaceLayout::Column,
        WorkspaceLayout::Grid,
        WorkspaceLayout::ScrollingColumns,
    ] {
        let mut workspace = Workspace::new(mode);
        for value in 1..=32 {
            let mut item = item(value);
            item.rules.axes[0].min = f32::from(value) * 11.0;
            item.rules.axes[0].size = item.rules.axes[0].min.max(300.0);
            item.rules.axes[1].max = 200.0 + f32::from(value);
            workspace.insert(item).unwrap();
        }
        let tiny = Rectangle::new([-200.0, 500.0], [12.0, 8.0]).unwrap();
        let solution = workspace.solve(tiny, tiny).unwrap();
        assert_eq!(solution.items.len(), 32);
        for solved in solution.items {
            let item = workspace.item(solved.id).unwrap();
            for axis in 0..2 {
                assert!(solved.rectangle.size()[axis] >= f64::from(item.rules.axes[axis].min));
                assert!(solved.rectangle.size()[axis] <= f64::from(item.rules.axes[axis].max));
                assert!(solved.rectangle.origin()[axis].is_finite());
            }
        }
    }
}

#[test]
fn directional_focus_switches_fullscreen_content_without_wrapping() {
    let mut workspace = workspace(2);
    workspace.focus(id(1), viewport()).unwrap();
    workspace.toggle_fullscreen();
    assert!(
        workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert_eq!(workspace.fullscreen(), Some(id(2)));
    assert_eq!(
        workspace.solve(viewport(), viewport()).unwrap().items.len(),
        1
    );
    assert!(
        !workspace
            .focus_direction(Direction::Right, viewport())
            .unwrap()
    );
    assert_eq!(workspace.fullscreen(), Some(id(2)));
}

#[test]
fn invalid_floating_geometry_does_not_partially_change_membership() {
    let mut workspace = workspace(1);
    let before = workspace.clone();
    let enormous = Rectangle::new([0.0; 2], [1_000_000.0; 2]).unwrap();
    assert_eq!(
        workspace.toggle_floating(id(1), enormous),
        Err(LayoutError::InvalidItem)
    );
    assert_eq!(workspace, before);
    assert!(workspace.valid());
}

#[test]
fn area_size_and_order_inputs_use_the_solver_without_compounding() {
    let mut workspace = workspace(3);
    workspace
        .influence(
            id(1),
            Influence {
                scale: 1.5,
                order: Some(2),
            },
        )
        .unwrap();
    workspace
        .influence(
            id(3),
            Influence {
                scale: 1.0,
                order: Some(0),
            },
        )
        .unwrap();
    let first = workspace.solve(viewport(), viewport()).unwrap();
    assert_eq!(
        first.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![id(3), id(2), id(1)]
    );
    assert_eq!(
        first
            .items
            .iter()
            .find(|item| item.id == id(1))
            .unwrap()
            .rectangle
            .size()[0],
        738.0
    );
    for _ in 0..30 {
        assert_eq!(workspace.solve(viewport(), viewport()).unwrap(), first);
    }
    workspace.influence(id(1), Influence::default()).unwrap();
    let restored = workspace.solve(viewport(), viewport()).unwrap();
    assert_eq!(
        restored
            .items
            .iter()
            .find(|item| item.id == id(1))
            .unwrap()
            .rectangle
            .size()[0],
        492.0
    );
    assert_eq!(workspace.item(id(1)).unwrap().rules.axes[0].size, 300.0);
}

#[test]
fn area_inputs_are_transient_and_grid_constraints_keep_columns_aligned() {
    let mut workspace = workspace(4);
    workspace.set_mode(WorkspaceLayout::Grid);
    workspace
        .influence(
            id(1),
            Influence {
                scale: 1.5,
                order: None,
            },
        )
        .unwrap();
    let solution = workspace.solve(viewport(), viewport()).unwrap();
    assert_eq!(
        solution.items[0].rectangle.origin()[0],
        solution.items[2].rectangle.origin()[0]
    );
    assert_eq!(
        solution.items[1].rectangle.origin()[0],
        solution.items[3].rectangle.origin()[0]
    );
    assert!(
        solution.items[0].rectangle.origin()[0] + solution.items[0].rectangle.size()[0]
            < solution.items[1].rectangle.origin()[0]
    );
    let saved = serde_json::to_vec(&workspace).unwrap();
    let restored: Workspace = serde_json::from_slice(&saved).unwrap();
    assert!(restored.influences.is_empty());
    assert!(restored.valid());
}
