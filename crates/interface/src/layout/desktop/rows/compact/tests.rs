use super::*;
use crate::layout::desktop::tests::{id, item, viewport};

fn workspace() -> Workspace {
    let mut model = Workspace::new(WorkspaceLayout::Tiled);
    for value in 1..=4 { model.insert(item(value)).unwrap(); }
    model.focus(id(1), viewport()).unwrap();
    model
}

#[test]
fn compact_members_share_one_slot_and_ungroup_restores_individual_homes_and_sizes() {
    let mut model = workspace();
    model.set_item_width(id(1), Width::Half).unwrap();
    model.set_item_width(id(2), Width::Third).unwrap();
    let before = model.clone();
    model.create_compact_group(id(1), &[id(2), id(3)]).unwrap();
    assert_eq!(model.items(), before.items());
    assert_eq!(model.tiled_rows().iter().flatten().count(), 2);
    assert!(model.compact_hidden(id(2)));
    for _ in 0..20 {
        for active in [id(1), id(2), id(3)] {
            model.select_compact_member(active).unwrap();
            let solution = model.solve(viewport(), viewport()).unwrap();
            assert_eq!(solution.items.len(), 2);
            assert!(solution.items.iter().any(|item| item.id == active));
            assert_eq!(model.item_width(active), Some(Width::Half));
            assert!(model.valid());
        }
    }
    model.dissolve_compact_group(id(3)).unwrap();
    assert_eq!(model.items(), before.items());
    assert_eq!(model.tiled_rows(), before.tiled_rows());
    for item in model.items() { assert_eq!(model.item_width(item.id), before.item_width(item.id)); }
    assert!(model.valid());
}

#[test]
fn incompatible_member_constraints_are_clipped_inside_a_stable_slot_without_moving_neighbours() {
    let mut model = workspace();
    model.item_mut(id(2)).unwrap().rules.axes[0].min = 850.0;
    model.item_mut(id(2)).unwrap().rules.axes[0].size = 850.0;
    model.item_mut(id(2)).unwrap().free.size[0] = 850.0;
    model.create_compact_group(id(1), &[id(2)]).unwrap();
    let first = model.solve(viewport(), viewport()).unwrap();
    let neighbour = *first.items.iter().find(|item| item.id == id(3)).unwrap();
    let slot = model.compact_viewport(id(1), viewport(), &first).unwrap();
    model.select_compact_member(id(2)).unwrap();
    let second = model.solve(viewport(), viewport()).unwrap();
    assert_eq!(*second.items.iter().find(|item| item.id == id(3)).unwrap(), neighbour);
    assert_eq!(model.compact_viewport(id(2), viewport(), &second), Some(slot));
    assert!(second.items.iter().find(|item| item.id == id(2)).unwrap().rectangle.size()[0] >= 850.0);
    assert!(slot[0] < 850.0);
}

#[test]
fn free_mode_keeps_the_relation_dormant_and_returns_to_its_remembered_active_member() {
    let mut model = workspace();
    model.create_compact_group(id(1), &[id(2)]).unwrap();
    model.select_compact_member(id(2)).unwrap();
    model.set_mode(WorkspaceLayout::Free);
    assert!(!model.compact_hidden(id(1)));
    assert_eq!(model.solve(viewport(), viewport()).unwrap().items.len(), 4);
    model.focus(id(1), viewport()).unwrap();
    model.set_mode(WorkspaceLayout::Tiled);
    assert_eq!(model.focused(), Some(id(2)));
    assert!(model.compact_hidden(id(1)));
    assert!(model.valid());
}

#[test]
fn close_detach_reorder_and_slot_transfer_keep_each_placement_once() {
    let mut model = workspace();
    model.create_compact_group(id(1), &[id(2), id(3)]).unwrap();
    model.reorder_compact_member(id(2), Direction::Right).unwrap();
    assert_eq!(model.compact_group(id(1)).unwrap().members, [id(1), id(3), id(2)]);
    model.remove(id(1)).unwrap();
    assert_eq!(model.compact_group(id(3)).unwrap().active, id(3));
    assert_eq!(model.focused(), Some(id(3)));
    let mut destination = Workspace::new(WorkspaceLayout::Tiled);
    let moved = model.transfer_to(&mut destination, id(3), TransferKind::Item).unwrap();
    assert_eq!(moved, [id(3), id(2)]);
    assert_eq!(destination.compact_groups().len(), 1);
    assert!(model.compact_groups().is_empty());
    assert!(model.valid() && destination.valid());
    destination.detach_compact_member(id(3)).unwrap();
    assert!(destination.compact_groups().is_empty());
    assert_eq!(destination.items().len(), 2);
    assert_eq!(destination.tiled_rows().iter().flatten().count(), 2);
    assert!(destination.valid());
}

#[test]
fn native_group_persistence_and_invalid_members_preserve_the_single_existing_owner() {
    let mut model = workspace();
    let before = model.clone();
    for members in [vec![id(1)], vec![id(2), id(2)], vec![id(9)]] {
        assert!(model.create_compact_group(id(1), &members).is_err());
        assert_eq!(model, before);
    }
    model.create_compact_group(id(1), &[id(2)]).unwrap();
    model.add_compact_members(id(1), &[id(3)]).unwrap();
    model.select_compact_member(id(2)).unwrap();
    let restored: Workspace = serde_json::from_slice(&serde_json::to_vec(&model).unwrap()).unwrap();
    assert_eq!(restored, model);
    assert!(restored.valid());
    let before = model.clone();
    assert!(model.toggle_floating(id(2), viewport()).is_err());
    assert!(model.add_compact_members(id(1), &[id(3)]).is_err());
    assert_eq!(model, before);
}

#[test]
fn a_hidden_members_fullscreen_request_cannot_blank_the_active_slot_or_take_over() {
    let mut model = workspace();
    model.create_compact_group(id(1), &[id(2)]).unwrap();
    let before = model.clone();
    assert!(model.request_state(id(2), crate::host::WindowRequest { fullscreen: Some(true), ..Default::default() }));
    assert_eq!(model, before);
    assert_eq!(model.solve(viewport(), viewport()).unwrap().items.len(), 3);
    model.set_mode(WorkspaceLayout::Free);
    model.focus(id(2), viewport()).unwrap();
    model.toggle_fullscreen();
    model.set_mode(WorkspaceLayout::Tiled);
    assert_eq!(model.compact_group(id(2)).unwrap().active, id(2));
    assert_eq!(model.solve(viewport(), viewport()).unwrap().items[0].id, id(2));
    assert!(model.valid());
}

#[test]
fn compact_resize_uses_slot_intent_instead_of_an_oversized_members_minimum() {
    let mut model = workspace();
    model.item_mut(id(2)).unwrap().rules.axes[0].min = 850.0;
    model.item_mut(id(2)).unwrap().rules.axes[0].size = 850.0;
    model.item_mut(id(2)).unwrap().free.size[0] = 850.0;
    model.create_compact_group(id(1), &[id(2)]).unwrap();
    model.select_compact_member(id(2)).unwrap();
    let before = model.solve(viewport(), viewport()).unwrap();
    let slot = model.compact_viewport(id(2), viewport(), &before).unwrap();
    model.resize_focused(0, 10.0, viewport(), 1.0).unwrap();
    let resized = model.solve(viewport(), viewport()).unwrap();
    let next = model.compact_viewport(id(2), viewport(), &resized).unwrap();
    assert!((next[0] - slot[0] - 10.0).abs() < 0.001);
    assert_eq!(next[1], slot[1]);
    assert!(next[0] < 850.0);
    model.set_row_height(id(2), Width::Pixels(1.0)).unwrap();
    let minimum = model.solve(viewport(), viewport()).unwrap();
    assert!(model.compact_viewport(id(2), viewport(), &minimum).unwrap()[1] >= 1.0);
    model.toggle_maximized();
    let maximized = model.solve(viewport(), viewport()).unwrap();
    assert!(model.compact_viewport(id(2), viewport(), &maximized).is_some());
    assert!(maximized.items.iter().find(|item| item.id == id(2)).unwrap().rectangle.origin()[1] >= viewport().origin()[1] + COMPACT_HEADER_HEIGHT);
}
