use super::*;
use crate::layout::desktop::tests::{id, item, viewport};

const PRESETS: [[u16; 2]; 4] = [[1, 1], [1, 2], [1, 3], [2, 3]];

#[test]
fn explicitly_replacing_a_free_fraction_keeps_the_new_intent_through_mode_round_trips() {
    let mut workspace = Workspace::new(WorkspaceLayout::Free);
    workspace.insert(item(1)).unwrap();
    let geometry = workspace.item(id(1)).unwrap().free;
    workspace.set_free_width(id(1), [1, 3], 1.0).unwrap();
    workspace.set_mode(WorkspaceLayout::Tiled);
    workspace.set_mode(WorkspaceLayout::Free);
    assert_eq!(workspace.item(id(1)).unwrap().free_width.unwrap().ratio, [1, 3]);
    workspace.clear_free_width(id(1)).unwrap();
    assert_eq!(workspace.item(id(1)).unwrap().free, geometry);
    workspace.set_mode(WorkspaceLayout::Tiled);
    workspace.set_mode(WorkspaceLayout::Free);
    assert!(workspace.item(id(1)).unwrap().free_width.is_none());
}

fn tiled(count: u8) -> Workspace {
    let mut workspace = Workspace::new(WorkspaceLayout::Tiled);
    for index in 1..=count {
        workspace.insert(item(index)).unwrap();
    }
    workspace.focus(id(1), viewport()).unwrap();
    workspace
}

#[test]
fn presets_cycle_per_item_without_changing_neighbors_or_height() {
    let mut workspace = tiled(3);
    let neighbors = [id(2), id(3)].map(|id| workspace.item_width(id).unwrap());
    let rows = workspace.tiled_rows();
    let free = workspace.item(id(1)).unwrap().free;
    for ratio in [[1, 3], [2, 3], [1, 1], [1, 2], [1, 3]] {
        workspace
            .cycle_item_width(&PRESETS, viewport(), 1.0)
            .unwrap();
        assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some(ratio));
        assert_eq!(workspace.focused(), Some(id(1)));
        assert_eq!(workspace.tiled_rows(), rows);
        assert_eq!(workspace.item(id(1)).unwrap().free, free);
        assert_eq!(
            [id(2), id(3)].map(|id| workspace.item_width(id).unwrap()),
            neighbors
        );
    }
}

#[test]
fn fractional_widths_account_for_inner_gaps_and_keep_exact_intent() {
    let mut workspace = tiled(3);
    for index in 1..=3 {
        workspace
            .set_item_width(id(index), Width::Fraction(1, 3))
            .unwrap();
    }
    let solved = workspace.solve(viewport(), viewport()).unwrap();
    let widths: f64 = solved
        .items
        .iter()
        .map(|item| item.rectangle.size()[0])
        .sum();
    assert!((widths + workspace.gap * 4.0 - viewport().size()[0]).abs() < 0.000_001);
    let first = solved.items.iter().find(|item| item.id == id(1)).unwrap();
    let last = solved.items.iter().find(|item| item.id == id(3)).unwrap();
    assert!(
        (last.rectangle.origin()[0] + last.rectangle.size()[0] - first.rectangle.origin()[0]
            + workspace.gap * 2.0
            - viewport().size()[0])
            .abs()
            < 0.000_001
    );
}

#[test]
fn constrained_equal_pixel_sizes_and_maximize_do_not_trap_the_cycle() {
    let mut workspace = tiled(2);
    let mut rules = workspace.item(id(1)).unwrap().rules;
    rules.axes[0].min = 650.0;
    rules.axes[0].size = 650.0;
    workspace.configure(id(1), rules).unwrap();
    workspace.set_item_width(id(1), Width::Full).unwrap();
    for ratio in [[1, 2], [1, 3]] {
        workspace
            .cycle_item_width(&PRESETS, viewport(), 1.0)
            .unwrap();
        assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some(ratio));
        let solved = workspace.solve(viewport(), viewport()).unwrap();
        assert_eq!(
            solved
                .items
                .iter()
                .find(|item| item.id == id(1))
                .unwrap()
                .rectangle
                .size()[0],
            650.0
        );
    }
    workspace.toggle_maximized();
    workspace
        .cycle_item_width(&PRESETS, viewport(), 1.0)
        .unwrap();
    assert!(!workspace.maximized(id(1)));
    assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some([1, 2]));
    workspace
        .set_item_width(id(1), Width::Pixels(755.0))
        .unwrap();
    workspace
        .cycle_item_width(&PRESETS, viewport(), 1.0)
        .unwrap();
    assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some([1, 1]));
    workspace
        .cycle_item_width(&[[2, 4], [1, 1]], viewport(), 1.0)
        .unwrap();
    assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some([2, 4]));
    workspace
        .cycle_item_width(&[[1, 3], [2, 3]], viewport(), 1.0)
        .unwrap();
    assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some([1, 3]));
}

#[test]
fn free_width_uses_the_invocation_scale_survives_restart_and_resolves_output_changes() {
    let mut workspace = tiled(2);
    workspace.set_mode(WorkspaceLayout::Free);
    let before = workspace.item(id(1)).unwrap().free;
    workspace
        .cycle_item_width(&PRESETS, viewport(), 0.5)
        .unwrap();
    let after = workspace.item(id(1)).unwrap().free;
    assert_eq!(after.size, [1968.0, before.size[1]]);
    assert_eq!(
        center_of(after.rectangle().unwrap()),
        center_of(before.rectangle().unwrap())
    );
    let serialized = serde_json::to_vec(&workspace).unwrap();
    let mut workspace: Workspace = serde_json::from_slice(&serialized).unwrap();
    assert!(workspace.valid());
    let smaller = Rectangle::new([0.0, 30.0], [800.0, 600.0]).unwrap();
    workspace.solve(smaller, smaller).unwrap();
    assert_eq!(workspace.item(id(1)).unwrap().free.size[0], 1568.0);
    let mut moved = workspace.item(id(1)).unwrap().free;
    moved.origin[0] += 123.0;
    workspace.set_free_geometry(id(1), moved).unwrap();
    assert!(workspace.item(id(1)).unwrap().free_width.is_some());
    moved.size[0] = 330.0;
    workspace.set_free_geometry(id(1), moved).unwrap();
    assert!(workspace.item(id(1)).unwrap().free_width.is_none());
    workspace.cycle_item_width(&PRESETS, smaller, 1.0).unwrap();
    assert_eq!(
        workspace.item(id(1)).unwrap().free_width.unwrap().ratio,
        [1, 1]
    );
}

#[test]
fn floating_width_does_not_replace_its_saved_tiled_intent() {
    let mut workspace = tiled(2);
    let current = workspace.solve(viewport(), viewport()).unwrap().items[0].rectangle;
    workspace.toggle_floating(id(1), current).unwrap();
    workspace
        .cycle_item_width(&PRESETS, viewport(), 1.0)
        .unwrap();
    assert_eq!(
        workspace.item(id(1)).unwrap().free_width.unwrap().ratio,
        [1, 1]
    );
    let current = workspace.item(id(1)).unwrap().free.rectangle().unwrap();
    workspace.toggle_floating(id(1), current).unwrap();
    assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some([1, 2]));
    workspace
        .cycle_item_width(&PRESETS, viewport(), 1.0)
        .unwrap();
    assert_eq!(workspace.item_width(id(1)).unwrap().ratio(), Some([1, 3]));
}

#[test]
fn invalid_lists_and_fullscreen_leave_all_state_unchanged() {
    let mut workspace = tiled(2);
    for presets in [
        vec![],
        vec![[0, 1]],
        vec![[1, 0]],
        vec![[2, 1]],
        vec![[1, 2], [2, 4]],
        vec![[1, 1]; 33],
    ] {
        let before = workspace.clone();
        assert_eq!(
            workspace.cycle_item_width(&presets, viewport(), 1.0),
            Err(LayoutError::InvalidState)
        );
        assert_eq!(workspace, before);
    }
    workspace.toggle_fullscreen();
    let before = workspace.clone();
    assert_eq!(
        workspace.cycle_item_width(&PRESETS, viewport(), 1.0),
        Err(LayoutError::InvalidState)
    );
    assert_eq!(workspace, before);
}
