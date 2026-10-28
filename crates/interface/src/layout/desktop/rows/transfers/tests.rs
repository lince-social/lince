use super::*;
use crate::layout::desktop::tests::{id, item, viewport};

fn workspace(values: &[u8], mode: WorkspaceLayout) -> Workspace {
    let mut workspace = Workspace::new(mode);
    for value in values {
        workspace.insert(item(*value)).unwrap();
    }
    workspace
}

fn sizes(workspace: &mut Workspace) -> BTreeMap<PlacementId, [f64; 2]> {
    workspace
        .solve(viewport(), viewport())
        .unwrap()
        .items
        .into_iter()
        .map(|item| (item.id, item.rectangle.size()))
        .collect()
}

fn rejection(
    mut source: Workspace,
    mut destination: Workspace,
    item: PlacementId,
    kind: TransferKind,
    error: LayoutError,
) {
    let before = (source.clone(), destination.clone());
    let bytes = (
        serde_json::to_vec(&source).unwrap(),
        serde_json::to_vec(&destination).unwrap(),
    );
    assert_eq!(source.transfer_to(&mut destination, item, kind), Err(error));
    assert_eq!((&source, &destination), (&before.0, &before.1));
    assert_eq!(serde_json::to_vec(&source).unwrap(), bytes.0);
    assert_eq!(serde_json::to_vec(&destination).unwrap(), bytes.1);
}

#[test]
fn individual_transfer_preserves_width_intent_geometry_constraints_and_neighbor_sizes() {
    let mut source = workspace(&[1, 2], WorkspaceLayout::Tiled);
    source.set_item_width(id(1), Width::TwoThirds).unwrap();
    source.rows.rows[0].height = Width::Half;
    source.items[0].kind = ItemKind::Application;
    source.items[0].rules.axes[0].min = 400.0;
    source.items[0].rules.axes[0].size = 400.0;
    source.items[0].rules.axes[0].max = 600.0;
    source.set_free_width(id(1), [2, 3], 0.75).unwrap();
    source
        .influence(
            id(1),
            Influence {
                scale: 0.5,
                order: None,
            },
        )
        .unwrap();
    let original = source.items[0].clone();
    let mut destination = workspace(&[9], WorkspaceLayout::Tiled);
    destination.rows.rows[0].height = Width::TwoThirds;
    destination.set_item_width(id(9), Width::Third).unwrap();
    let before = sizes(&mut destination)[&id(9)];
    assert_eq!(
        source
            .transfer_to(&mut destination, id(1), TransferKind::Item)
            .unwrap(),
        [id(1)]
    );
    assert!(!source.items.iter().any(|item| item.id == id(1)));
    assert_eq!(
        destination.items.iter().find(|item| item.id == id(1)),
        Some(&original)
    );
    assert_eq!(source.tiled_rows(), vec![vec![id(2)]]);
    assert_eq!(destination.tiled_rows(), vec![vec![id(9), id(1)]]);
    assert_eq!(destination.rows.rows[0].height, Width::TwoThirds);
    assert_eq!(destination.item_width(id(1)), Some(Width::TwoThirds));
    assert_eq!(destination.item_width(id(9)), Some(Width::Third));
    assert!(!destination.influences.contains_key(&id(1)));
    let after = sizes(&mut destination);
    assert_eq!(after[&id(9)], before);
    assert_eq!(after[&id(1)][0], 600.0);
    assert_eq!(destination.focused(), Some(id(1)));
    assert!(source.valid() && destination.valid());
}

#[test]
fn whole_row_transfer_preserves_order_height_hidden_state_and_destination_row_identity() {
    let mut source = workspace(&[1, 2, 3, 4], WorkspaceLayout::Tiled);
    source.focus(id(3), viewport()).unwrap();
    assert!(source.move_focused(Direction::Down, viewport()));
    source.focus(id(1), viewport()).unwrap();
    source.rows.rows[0].height = Width::Half;
    source.set_item_width(id(1), Width::Third).unwrap();
    source.set_item_width(id(2), Width::TwoThirds).unwrap();
    source.set_item_width(id(4), Width::Full).unwrap();
    source.request_state(
        id(2),
        crate::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        },
    );
    source.set_minimized(id(2), true);
    let remaining = source
        .items
        .iter()
        .find(|item| item.id == id(3))
        .unwrap()
        .clone();
    let mut destination = workspace(&[9], WorkspaceLayout::Tiled);
    let row = destination.rows.rows[0].id;
    let before = sizes(&mut destination)[&id(9)];
    assert_eq!(source.rows.rows[0].id, row);
    assert_eq!(
        source
            .transfer_to(&mut destination, id(1), TransferKind::Row)
            .unwrap(),
        [id(1), id(2), id(4)]
    );
    assert_eq!(source.tiled_rows(), vec![vec![id(3)]]);
    assert_eq!(source.items[0], remaining);
    assert_eq!(
        destination.tiled_rows(),
        vec![vec![id(9)], vec![id(1), id(2), id(4)]]
    );
    assert_eq!(destination.rows.rows[0].id, row);
    assert_ne!(destination.rows.rows[1].id, row);
    assert_eq!(destination.rows.rows[1].height, Width::Half);
    assert_eq!(destination.item_width(id(1)), Some(Width::Third));
    assert_eq!(destination.item_width(id(2)), Some(Width::TwoThirds));
    assert_eq!(destination.item_width(id(4)), Some(Width::Full));
    assert!(destination.minimized(id(2)));
    assert!(destination.minimized_fullscreen.contains(&id(2)));
    assert_eq!(sizes(&mut destination)[&id(9)], before);
    assert_eq!(destination.focused(), Some(id(1)));
    assert!(source.valid() && destination.valid());
}

#[test]
fn floating_transfer_rebases_its_home_and_restores_both_rectangles_after_persistence() {
    let mut source = workspace(&[1, 2], WorkspaceLayout::Tiled);
    source.set_item_width(id(1), Width::TwoThirds).unwrap();
    source.set_free_width(id(1), [1, 2], 0.8).unwrap();
    let normal = source.items[0].clone();
    let floating = Rectangle::new([500.0, 600.0], [444.0, 333.0]).unwrap();
    source.toggle_floating(id(1), floating).unwrap();
    source.set_free_width(id(1), [1, 3], 1.25).unwrap();
    let free = source.items[0].clone();
    let mut destination = workspace(&[9, 10], WorkspaceLayout::Tiled);
    destination.focus(id(10), viewport()).unwrap();
    assert!(destination.move_focused(Direction::Down, viewport()));
    let before = sizes(&mut destination);
    source
        .transfer_to(&mut destination, id(1), TransferKind::Item)
        .unwrap();
    assert_eq!(
        destination.items.iter().find(|item| item.id == id(1)),
        Some(&free)
    );
    let encoded = serde_json::to_vec(&destination).unwrap();
    let mut destination: Workspace = serde_json::from_slice(&encoded).unwrap();
    assert!(destination.valid());
    destination.toggle_floating(id(1), floating).unwrap();
    assert_eq!(
        destination.tiled_rows(),
        vec![vec![id(9)], vec![id(10), id(1)]]
    );
    assert_eq!(destination.item_width(id(1)), Some(Width::TwoThirds));
    let restored = destination
        .items
        .iter()
        .find(|item| item.id == id(1))
        .unwrap();
    assert_eq!(restored.free, normal.free);
    assert_eq!(restored.free_width, normal.free_width);
    let after = sizes(&mut destination);
    for id in [id(9), id(10)] {
        assert_eq!(after[&id], before[&id]);
    }
    destination
        .toggle_floating(id(1), normal.free.rectangle().unwrap())
        .unwrap();
    assert_eq!(
        destination.items.iter().find(|item| item.id == id(1)),
        Some(&free)
    );
    assert!(source.valid() && destination.valid());
}

#[test]
fn normal_and_fullscreen_incoming_windows_keep_their_state_without_inheriting_destination_fullscreen()
 {
    let mut source = workspace(&[1, 2], WorkspaceLayout::Tiled);
    source.set_item_width(id(1), Width::Third).unwrap();
    source.request_state(
        id(1),
        crate::host::WindowRequest {
            maximized: Some(true),
            ..Default::default()
        },
    );
    let normal = source.items[0].free;
    let mut destination = workspace(&[9], WorkspaceLayout::Tiled);
    destination.request_state(
        id(9),
        crate::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        },
    );
    source
        .transfer_to(&mut destination, id(1), TransferKind::Item)
        .unwrap();
    assert_eq!(destination.fullscreen(), None);
    assert_eq!(destination.focused(), Some(id(1)));
    assert!(destination.maximized(id(1)));
    assert_eq!(destination.item_width(id(1)), Some(Width::Third));
    assert_eq!(
        destination
            .items
            .iter()
            .find(|item| item.id == id(1))
            .unwrap()
            .free,
        normal
    );
    source.request_state(
        id(2),
        crate::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        },
    );
    source
        .transfer_to(&mut destination, id(2), TransferKind::Item)
        .unwrap();
    assert_eq!(source.fullscreen(), None);
    assert_eq!(destination.fullscreen(), Some(id(2)));
    assert_eq!(destination.focused(), Some(id(2)));
    assert!(source.valid() && destination.valid());
}

#[test]
fn minimized_transfer_preserves_bookkeeping_without_stealing_destination_focus() {
    let mut source = workspace(&[1, 2], WorkspaceLayout::Tiled);
    source.request_state(
        id(1),
        crate::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        },
    );
    source.set_minimized(id(1), true);
    let mut destination = workspace(&[9], WorkspaceLayout::Tiled);
    destination.request_state(
        id(9),
        crate::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        },
    );
    source
        .transfer_to(&mut destination, id(1), TransferKind::Item)
        .unwrap();
    assert!(destination.minimized(id(1)));
    assert!(destination.minimized_fullscreen.contains(&id(1)));
    assert_eq!(destination.focused(), Some(id(9)));
    assert_eq!(destination.fullscreen(), Some(id(9)));
    assert!(!source.minimized_fullscreen.contains(&id(1)));
    assert!(source.valid() && destination.valid());
}

#[test]
fn transfers_between_free_and_tiled_preserve_requested_width_and_can_cycle_it() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        let mut source = workspace(&[1, 2], WorkspaceLayout::Free);
        source.set_item_width(id(1), Width::TwoThirds).unwrap();
        source.set_free_width(id(1), [2, 3], 0.75).unwrap();
        let original = source.items[0].clone();
        let mut destination = workspace(&[9], mode);
        source
            .transfer_to(&mut destination, id(1), TransferKind::Item)
            .unwrap();
        assert_eq!(source.mode(), WorkspaceLayout::Free);
        assert_eq!(destination.mode(), mode);
        assert_eq!(
            destination.items.iter().find(|item| item.id == id(1)),
            Some(&original)
        );
        destination
            .cycle_item_width(&[[1, 1], [1, 2], [1, 3], [2, 3]], viewport(), 0.75)
            .unwrap();
        if mode == WorkspaceLayout::Tiled {
            assert_eq!(destination.item_width(id(1)), Some(Width::Fraction(1, 1)));
        } else {
            assert_eq!(
                destination
                    .items
                    .iter()
                    .find(|item| item.id == id(1))
                    .unwrap()
                    .free_width
                    .unwrap()
                    .ratio,
                [1, 1]
            );
        }
        assert!(source.valid() && destination.valid());
    }
}

#[test]
fn invalid_duplicate_missing_and_non_tiled_row_transfers_are_atomic() {
    rejection(
        workspace(&[1, 2], WorkspaceLayout::Tiled),
        workspace(&[2, 9], WorkspaceLayout::Tiled),
        id(1),
        TransferKind::Row,
        LayoutError::DuplicateItem,
    );
    rejection(
        workspace(&[1, 2], WorkspaceLayout::Tiled),
        workspace(&[9], WorkspaceLayout::Tiled),
        id(99),
        TransferKind::Item,
        LayoutError::MissingItem,
    );
    let mut source = workspace(&[1, 2], WorkspaceLayout::Tiled);
    source.gap = -1.0;
    rejection(
        source,
        workspace(&[9], WorkspaceLayout::Tiled),
        id(1),
        TransferKind::Item,
        LayoutError::InvalidState,
    );
    let mut destination = workspace(&[9], WorkspaceLayout::Tiled);
    destination.gap = -1.0;
    rejection(
        workspace(&[1, 2], WorkspaceLayout::Tiled),
        destination,
        id(1),
        TransferKind::Item,
        LayoutError::InvalidState,
    );
    rejection(
        workspace(&[1, 2], WorkspaceLayout::Free),
        workspace(&[9], WorkspaceLayout::Tiled),
        id(1),
        TransferKind::Row,
        LayoutError::InvalidState,
    );
    let mut source = workspace(&[1, 2], WorkspaceLayout::Tiled);
    source.toggle_floating(id(1), viewport()).unwrap();
    rejection(
        source,
        workspace(&[9], WorkspaceLayout::Tiled),
        id(1),
        TransferKind::Row,
        LayoutError::InvalidState,
    );
}

#[test]
fn a_full_destination_rejects_transfer_without_changing_either_workspace() {
    let mut destination = Workspace::new(WorkspaceLayout::Tiled);
    for value in 0..MAX_ITEMS {
        let mut item = item(9);
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&(value as u64).to_le_bytes());
        bytes[15] = 255;
        item.id = PlacementId(bytes);
        destination.insert(item).unwrap();
    }
    rejection(
        workspace(&[1, 2], WorkspaceLayout::Tiled),
        destination,
        id(1),
        TransferKind::Item,
        LayoutError::TooManyItems,
    );
}
