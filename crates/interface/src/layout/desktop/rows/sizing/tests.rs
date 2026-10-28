use super::*;
use crate::layout::desktop::tests::{id, item, viewport};

const PRESETS: [[u16; 2]; 4] = [[1, 1], [1, 2], [1, 3], [2, 3]];

fn fixture() -> Workspace {
    let mut model = Workspace::new(WorkspaceLayout::Tiled);
    for index in 1..=3 {
        model.insert(item(index)).unwrap();
    }
    model.focus(id(1), viewport()).unwrap();
    model
}

fn size(model: &mut Workspace, id: PlacementId) -> [f64; 2] {
    model
        .solve(viewport(), viewport())
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == id)
        .unwrap()
        .rectangle
        .size()
}

#[test]
fn row_height_cycle_is_exact_shared_and_preserves_individual_widths_and_free_geometry() {
    let mut model = fixture();
    let original = model.items.clone();
    let widths = [id(1), id(2), id(3)].map(|id| model.item_width(id));
    for ratio in [[1, 2], [1, 3], [2, 3], [1, 1], [1, 2]] {
        model.cycle_row_height(&PRESETS).unwrap();
        for index in 1..=3 {
            assert_eq!(model.row_height(id(index)).unwrap().ratio(), Some(ratio));
        }
        assert_eq!([id(1), id(2), id(3)].map(|id| model.item_width(id)), widths);
        assert_eq!(model.items, original);
        let first = size(&mut model, id(1));
        assert_eq!(size(&mut model, id(2)), first);
    }
    let bytes = serde_json::to_vec(&model).unwrap();
    let restored: Workspace = serde_json::from_slice(&bytes).unwrap();
    assert!(restored.valid());
    assert_eq!(restored.row_height(id(1)).unwrap().ratio(), Some([1, 2]));
}

#[test]
fn half_height_rows_fit_with_gaps_and_client_limits_do_not_trap_preset_intent() {
    let mut model = fixture();
    model.move_focused(Direction::Down, viewport());
    assert_eq!(model.tiled_rows().len(), 2);
    for index in 1..=3 {
        model.set_row_height(id(index), Width::Half).unwrap();
    }
    let solution = model.solve(viewport(), viewport()).unwrap();
    let low = solution
        .items
        .iter()
        .map(|item| item.rectangle.origin()[1])
        .fold(f64::INFINITY, f64::min);
    let high = solution
        .items
        .iter()
        .map(|item| item.rectangle.origin()[1] + item.rectangle.size()[1])
        .fold(f64::NEG_INFINITY, f64::max);
    assert!((high - low + 2.0 * model.gap - viewport().size()[1]).abs() < 0.000_001);
    let mut rules = model.item(id(1)).unwrap().rules;
    rules.axes[1].min = 450.0;
    rules.axes[1].size = 450.0;
    model.configure(id(1), rules).unwrap();
    model.set_row_height(id(1), Width::Full).unwrap();
    for ratio in [[1, 2], [1, 3]] {
        model.cycle_row_height(&PRESETS).unwrap();
        assert_eq!(model.row_height(id(1)).unwrap().ratio(), Some(ratio));
        assert_eq!(size(&mut model, id(1))[1], 450.0);
    }
    model.cycle_row_height(&PRESETS).unwrap();
    assert_eq!(model.row_height(id(1)).unwrap().ratio(), Some([2, 3]));
    assert!(size(&mut model, id(1))[1] > 450.0);
}

#[test]
fn manual_axes_preserve_neighbors_other_presets_and_floating_homes() {
    let mut model = fixture();
    model.set_item_width(id(1), Width::Third).unwrap();
    model.set_row_height(id(1), Width::Half).unwrap();
    let before = size(&mut model, id(1));
    let neighbor = size(&mut model, id(2));
    model.resize_focused(0, 64.0, viewport(), 1.0).unwrap();
    assert_eq!(size(&mut model, id(1)), [before[0] + 64.0, before[1]]);
    assert_eq!(size(&mut model, id(2)), neighbor);
    assert_eq!(model.item_width(id(1)).unwrap().ratio(), None);
    assert_eq!(model.row_height(id(1)).unwrap().ratio(), Some([1, 2]));
    model.cycle_item_width(&PRESETS, viewport(), 1.0).unwrap();
    assert_eq!(model.item_width(id(1)).unwrap().ratio(), Some([1, 1]));
    model.resize_focused(1, -64.0, viewport(), 1.0).unwrap();
    assert_eq!(size(&mut model, id(1))[1], before[1] - 64.0);
    assert_eq!(size(&mut model, id(2))[1], before[1] - 64.0);
    assert_eq!(model.item_width(id(1)).unwrap().ratio(), Some([1, 1]));
    assert_eq!(model.row_height(id(1)).unwrap().ratio(), None);
    let rectangle = model
        .solve(viewport(), viewport())
        .unwrap()
        .items
        .iter()
        .find(|item| item.id == id(1))
        .unwrap()
        .rectangle;
    model.toggle_floating(id(1), rectangle).unwrap();
    let floating = model.item(id(1)).unwrap().free;
    model.resize_focused(1, 64.0, viewport(), 0.5).unwrap();
    assert_eq!(
        model.item(id(1)).unwrap().free.size[1],
        floating.size[1] + 128.0
    );
    model.toggle_floating(id(1), rectangle).unwrap();
    assert_eq!(model.item_width(id(1)).unwrap().ratio(), Some([1, 1]));
    assert_eq!(model.row_height(id(1)), model.row_height(id(2)));
    assert_eq!(model.item(id(1)).unwrap().free, item(1).free);
}

#[test]
fn free_resize_preserves_center_scale_and_only_clears_horizontal_intent_on_horizontal_change() {
    let mut model = fixture();
    model.set_mode(WorkspaceLayout::Free);
    model.cycle_item_width(&PRESETS, viewport(), 0.5).unwrap();
    let original = model.item(id(1)).unwrap().free;
    let center = center_of(original.rectangle().unwrap());
    let neighbor = model.item(id(2)).unwrap().clone();
    model.resize_focused(1, -30.0, viewport(), 0.5).unwrap();
    assert_eq!(
        model.item(id(1)).unwrap().free.size,
        [original.size[0], original.size[1] - 60.0]
    );
    assert!(model.item(id(1)).unwrap().free_width.is_some());
    model.resize_focused(0, -30.0, viewport(), 0.5).unwrap();
    let item = model.item(id(1)).unwrap();
    assert_eq!(
        item.free.size,
        [original.size[0] - 60.0, original.size[1] - 60.0]
    );
    assert_eq!(center_of(item.free.rectangle().unwrap()), center);
    assert!(item.free_width.is_none());
    assert_eq!(model.item(id(2)).unwrap(), &neighbor);
    model
        .solve(Rectangle::new([0.0; 2], [500.0; 2]).unwrap(), viewport())
        .unwrap();
    assert_eq!(
        model.item(id(1)).unwrap().free.size,
        [original.size[0] - 60.0, original.size[1] - 60.0]
    );
}

#[test]
fn invalid_requests_and_fullscreen_leave_the_complete_model_unchanged() {
    let mut model = fixture();
    for (axis, delta, scale) in [
        (2, 64.0, 1.0),
        (0, f64::NAN, 1.0),
        (0, 0.0, 1.0),
        (1, 10_001.0, 1.0),
        (0, 64.0, 0.0),
    ] {
        let before = model.clone();
        assert!(
            model
                .resize_focused(axis, delta, viewport(), scale)
                .is_err()
        );
        assert_eq!(model, before);
    }
    for presets in [vec![], vec![[1, 0]], vec![[1, 2], [2, 4]]] {
        let before = model.clone();
        assert!(model.cycle_row_height(&presets).is_err());
        assert_eq!(model, before);
    }
    model.toggle_fullscreen();
    let before = model.clone();
    assert!(model.resize_focused(0, 64.0, viewport(), 1.0).is_err());
    assert!(model.cycle_row_height(&PRESETS).is_err());
    assert_eq!(model, before);
    model.toggle_fullscreen();
    model.clear_focus();
    let before = model.clone();
    assert!(model.resize_focused(0, 64.0, viewport(), 1.0).is_err());
    assert!(model.cycle_row_height(&PRESETS).is_err());
    assert_eq!(model, before);
}

#[test]
fn changing_a_row_updates_the_floating_home_used_after_its_last_tiled_member_closes() {
    let mut model = fixture();
    let rectangle = model.solve(viewport(), viewport()).unwrap().items[0].rectangle;
    model.toggle_floating(id(1), rectangle).unwrap();
    model.set_row_height(id(2), Width::Third).unwrap();
    model.remove(id(2));
    model.remove(id(3));
    model.toggle_floating(id(1), rectangle).unwrap();
    assert_eq!(model.row_height(id(1)), Some(Width::Third));
    assert!(model.valid());
}

#[test]
fn floating_and_authored_sizing_restore_independently_while_client_limits_remain_current() {
    let mut model = fixture();
    let mut rules = model.item(id(1)).unwrap().rules;
    rules.axes[0].sizing = crate::layout::Sizing::Fit;
    model.configure(id(1), rules).unwrap();
    let rectangle = model.solve(viewport(), viewport()).unwrap().items[0].rectangle;
    model.toggle_floating(id(1), rectangle).unwrap();
    model.resize_focused(0, 64.0, viewport(), 1.0).unwrap();
    assert_eq!(
        model.item(id(1)).unwrap().rules.axes[0].sizing,
        crate::layout::Sizing::Fixed
    );
    let mut limits = model.item(id(1)).unwrap().rules;
    limits.axes[0].min = 500.0;
    limits.axes[0].size = 500.0;
    model.configure(id(1), limits).unwrap();
    model.toggle_floating(id(1), rectangle).unwrap();
    let restored = model.item(id(1)).unwrap().rules.axes[0];
    assert_eq!(restored.sizing, crate::layout::Sizing::Fit);
    assert_eq!(restored.size, 500.0);
    assert_eq!(restored.min, 500.0);
    model.toggle_floating(id(1), rectangle).unwrap();
    let floating = model.item(id(1)).unwrap().rules.axes[0];
    assert_eq!(floating.sizing, crate::layout::Sizing::Fixed);
    assert_eq!(floating.min, 500.0);
    assert!(model.valid());
    let bytes = serde_json::to_vec(&model).unwrap();
    let restarted: Workspace = serde_json::from_slice(&bytes).unwrap();
    assert!(restarted.valid());
    assert_eq!(restarted, model);
}
