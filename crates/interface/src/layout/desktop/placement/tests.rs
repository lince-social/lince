use super::*;
use crate::layout::desktop::tests::{id, item, viewport};

fn workspace() -> Workspace {
    let mut model = Workspace::new(WorkspaceLayout::Tiled);
    for value in 1..=4 {
        model.insert(item(value)).unwrap();
    }
    model.focus(id(2), viewport()).unwrap();
    model
}

#[test]
fn explicit_row_placement_preserves_sizes_focus_scroll_and_authored_homes() {
    let mut model = workspace();
    model.set_item_width(id(2), Width::TwoThirds).unwrap();
    model.scroll_to([100.0, 0.0], viewport()).unwrap();
    let before = model.clone();
    assert!(
        model
            .place_near(
                id(4),
                DropSlot {
                    anchor: id(2),
                    side: Direction::Up
                }
            )
            .unwrap()
    );
    assert_eq!(
        model.tiled_rows(),
        vec![vec![id(4)], vec![id(1), id(2), id(3)]]
    );
    assert_eq!(model.items(), before.items());
    assert_eq!(model.focused(), before.focused());
    assert_eq!(model.scroll(), before.scroll());
    assert_eq!(model.item_width(id(2)), before.item_width(id(2)));
    assert!(model.valid());
    assert!(
        model
            .place_near(
                id(4),
                DropSlot {
                    anchor: id(1),
                    side: Direction::Right
                }
            )
            .unwrap()
    );
    assert_eq!(model.tiled_rows(), vec![vec![id(1), id(4), id(2), id(3)]]);
    assert_eq!(model.items(), before.items());
    let placed = model.clone();
    assert!(
        !model
            .place_near(
                id(4),
                DropSlot {
                    anchor: id(4),
                    side: Direction::Down
                }
            )
            .unwrap()
    );
    assert_eq!(model, placed);
}

#[test]
fn invalid_or_unavailable_placements_reject_atomically() {
    for state in 0..4 {
        let mut model = workspace();
        match state {
            0 => model.set_mode(WorkspaceLayout::Free),
            1 => {
                model.set_minimized(id(1), true);
            }
            2 => {
                model.toggle_floating(id(2), viewport()).unwrap();
            }
            _ => {
                model.request_state(
                    id(3),
                    crate::host::WindowRequest {
                        fullscreen: Some(true),
                        ..Default::default()
                    },
                );
            }
        }
        let before = model.clone();
        assert_eq!(
            model.place_near(
                id(1),
                DropSlot {
                    anchor: id(2),
                    side: Direction::Left
                }
            ),
            Err(LayoutError::InvalidState)
        );
        assert_eq!(model, before);
    }
    let mut model = workspace();
    let before = model.clone();
    assert_eq!(
        model.place_near(
            id(8),
            DropSlot {
                anchor: id(2),
                side: Direction::Left
            }
        ),
        Err(LayoutError::MissingItem)
    );
    assert_eq!(model, before);
    assert_eq!(
        model.place_near(
            id(1),
            DropSlot {
                anchor: id(8),
                side: Direction::Right
            }
        ),
        Err(LayoutError::MissingItem)
    );
    assert_eq!(model, before);
}
