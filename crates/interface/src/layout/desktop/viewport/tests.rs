use super::*;
use crate::layout::desktop::tests::{id, item, viewport};

fn tiled() -> Workspace {
    let mut model = Workspace::new(WorkspaceLayout::Tiled);
    for value in 1..=4 {
        model.insert(item(value)).unwrap();
    }
    model.focus(id(1), viewport()).unwrap();
    model.solve(viewport(), viewport()).unwrap();
    model
}

#[test]
fn pan_changes_only_the_view_and_preserves_client_sizes_focus_and_authored_homes() {
    let mut model = tiled();
    let before = model.clone();
    let first = model.solve(viewport(), viewport()).unwrap();
    assert!(model.scroll_to([180.0, 0.0], viewport()).unwrap());
    let moved = model.solve(viewport(), viewport()).unwrap();
    assert_eq!(moved.scroll, [180.0, 0.0]);
    assert_eq!(model.focused(), before.focused());
    assert_eq!(model.items(), before.items());
    assert_eq!(model.tiled_rows(), before.tiled_rows());
    for (old, new) in first.items.iter().zip(&moved.items) {
        assert_eq!(new.id, old.id);
        assert_eq!(new.rectangle.size(), old.rectangle.size());
        assert_eq!(new.rectangle.origin()[0], old.rectangle.origin()[0] - 180.0);
        assert_eq!(new.rectangle.origin()[1], old.rectangle.origin()[1]);
    }
    assert!(!model.scroll_to([180.0, 0.0], viewport()).unwrap());
    assert!(model.scroll_to(before.scroll(), viewport()).unwrap());
    assert_eq!(model.solve(viewport(), viewport()).unwrap(), first);
    assert!(model.valid());
}

#[test]
fn pan_uses_current_usable_bounds_and_keeps_floating_items_fixed() {
    let mut model = tiled();
    model.toggle_floating(id(4), viewport()).unwrap();
    let initial = model.solve(viewport(), viewport()).unwrap();
    let floating = initial.items.iter().find(|item| item.id == id(4)).unwrap();
    model.scroll_to([f64::MAX; 2], viewport()).unwrap();
    let edge = model.solve(viewport(), viewport()).unwrap();
    assert_eq!(edge.scroll[0], edge.extent[0] - viewport().size()[0]);
    assert_eq!(edge.scroll[1], 0.0);
    assert_eq!(
        edge.items.iter().find(|item| item.id == id(4)),
        Some(floating)
    );
    let narrower = Rectangle::new([175.0, 80.0], [600.0, 500.0]).unwrap();
    model.scroll_to([f64::MAX; 2], narrower).unwrap();
    let constrained = model.solve(narrower, narrower).unwrap();
    assert_eq!(constrained.scroll[0], constrained.extent[0] - 600.0);
    assert_eq!(
        constrained.items.iter().find(|item| item.id == id(4)),
        Some(floating)
    );
    model.scroll_to([-100.0; 2], narrower).unwrap();
    assert_eq!(model.scroll(), [0.0; 2]);
}

#[test]
fn invalid_or_unavailable_pan_is_atomic_and_saved_tiled_scroll_survives_free_mode() {
    let mut model = tiled();
    model.scroll_to([210.0, 0.0], viewport()).unwrap();
    for requested in [
        [f64::NAN, 0.0],
        [0.0, f64::INFINITY],
        [f64::NEG_INFINITY, 0.0],
    ] {
        let before = model.clone();
        assert_eq!(
            model.scroll_to(requested, viewport()),
            Err(LayoutError::InvalidState)
        );
        assert_eq!(model, before);
    }
    let saved = serde_json::to_vec(&model).unwrap();
    let mut restored: Workspace = serde_json::from_slice(&saved).unwrap();
    assert_eq!(restored.scroll(), [210.0, 0.0]);
    restored.set_mode(WorkspaceLayout::Free);
    let before = restored.clone();
    assert_eq!(
        restored.scroll_to([50.0, 0.0], viewport()),
        Err(LayoutError::InvalidState)
    );
    assert_eq!(restored, before);
    restored.set_mode(WorkspaceLayout::Tiled);
    assert_eq!(restored.scroll(), [210.0, 0.0]);
    restored.request_state(
        id(1),
        crate::host::WindowRequest {
            fullscreen: Some(true),
            ..Default::default()
        },
    );
    let before = restored.clone();
    assert_eq!(
        restored.scroll_to([50.0, 0.0], viewport()),
        Err(LayoutError::InvalidState)
    );
    assert_eq!(restored, before);
}
