use super::super::tests::{id, item, viewport};
use super::*;

fn fixture(mode: WorkspaceLayout) -> Workspace {
    let mut model = Workspace::new(mode);
    for value in 1..=3 {
        model.insert(item(value)).unwrap();
    }
    model
}

#[test]
fn prospective_moves_preserve_live_state_and_other_window_sizes_until_commit() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        let model = fixture(mode);
        let original = model.clone();
        let gesture =
            Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
        let (next, _) = gesture
            .propose(
                &model,
                [120.0, -40.0],
                Some(DropSlot {
                    anchor: id(3),
                    side: Direction::Right,
                }),
            )
            .unwrap();
        assert_eq!(model, original);
        for value in [2, 3] {
            assert_eq!(next.item(id(value)), model.item(id(value)));
        }
        if mode == WorkspaceLayout::Tiled {
            assert_eq!(next.tiled_rows(), vec![vec![id(2), id(3), id(1)]]);
        } else {
            let moved = next.item(id(1)).unwrap().free;
            assert_eq!(moved.origin, [140.0, -10.0]);
            assert_eq!(moved.size, [300.0, 200.0]);
        }
    }
}

#[test]
fn tiled_insertion_preserves_width_intent_and_creates_only_the_previewed_row() {
    let model = fixture(WorkspaceLayout::Tiled);
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    let (next, _) = gesture
        .propose(
            &model,
            [0.0, 1.0],
            Some(DropSlot {
                anchor: id(3),
                side: Direction::Up,
            }),
        )
        .unwrap();
    assert_eq!(next.tiled_rows(), vec![vec![id(1)], vec![id(2), id(3)]]);
    for value in 1..=3 {
        assert_eq!(next.item_width(id(value)), model.item_width(id(value)));
    }
    assert!(next.valid());
}

#[test]
fn resize_clamps_clients_and_preserves_the_opposite_edge_and_other_items() {
    let mut model = fixture(WorkspaceLayout::Free);
    let mut rules = model.item(id(1)).unwrap().rules;
    rules.axes[0].min = 200.0;
    rules.axes[0].max = 350.0;
    model.configure(id(1), rules).unwrap();
    let gesture =
        Gesture::begin(&model, id(1), Manipulation::Resize, [0.0; 2], viewport()).unwrap();
    let (next, geometry) = gesture.propose(&model, [-1000.0, 1000.0], None).unwrap();
    assert_eq!(geometry.size[0], 350.0);
    assert_eq!(geometry.origin[0] + geometry.size[0], 320.0);
    assert_eq!(next.item(id(2)), model.item(id(2)));
    assert!(next.valid());
}

#[test]
fn explicit_resize_edges_preserve_inactive_axes_and_ignore_the_initial_hit_quadrant() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        for horizontal in -1..=1 {
            for vertical in -1..=1 {
                let edges = [horizontal, vertical];
                if edges == [0; 2] {
                    continue;
                }
                let mut model = fixture(mode);
                if mode == WorkspaceLayout::Tiled {
                    let rectangle = geometry(&model, id(1)).rectangle().unwrap();
                    model.toggle_floating(id(1), rectangle).unwrap();
                }
                let original = model.item(id(1)).unwrap().free;
                let gesture = Gesture::begin_with_edges(
                    &model,
                    id(1),
                    Manipulation::Resize,
                    [0.1; 2],
                    viewport(),
                    Some(edges),
                )
                .unwrap();
                assert_eq!(gesture.axes(), edges.map(|edge| edge != 0));
                let delta = [30.0, 20.0];
                let (next, result) = gesture.propose(&model, delta, None).unwrap();
                for axis in 0..2 {
                    assert_eq!(
                        result.size[axis],
                        original.size[axis] + delta[axis] * f64::from(edges[axis])
                    );
                    assert_eq!(
                        result.origin[axis],
                        original.origin[axis] + if edges[axis] < 0 { delta[axis] } else { 0.0 }
                    );
                    if edges[axis] == 0 {
                        assert_eq!(
                            next.item(id(1)).unwrap().rules.axes[axis],
                            model.item(id(1)).unwrap().rules.axes[axis]
                        );
                    }
                }
                assert_eq!(next.item(id(2)), model.item(id(2)));
                assert!(next.valid());
            }
        }
    }
}

#[test]
fn explicit_row_edges_resolve_the_same_shared_boundary_regardless_of_hit_position() {
    let mut model = fixture(WorkspaceLayout::Tiled);
    model.focus(id(3), viewport()).unwrap();
    model.move_focused(Direction::Down, viewport());
    model.set_row_height(id(1), Width::Pixels(250.0)).unwrap();
    model.set_row_height(id(3), Width::Pixels(300.0)).unwrap();
    for (target, edges) in [(id(1), [0, 1]), (id(3), [0, -1])] {
        let gesture = Gesture::begin_with_edges(
            &model,
            target,
            Manipulation::Resize,
            [1.0, 0.5],
            viewport(),
            Some(edges),
        )
        .unwrap();
        assert_eq!(gesture.axes(), [false, true]);
        let (next, _) = gesture.propose(&model, [800.0, 40.0], None).unwrap();
        assert_eq!(next.row_height(id(1)), Some(Width::Pixels(290.0)));
        assert_eq!(next.row_height(id(3)), model.row_height(id(3)));
        for value in 1..=3 {
            assert_eq!(next.item_width(id(value)), model.item_width(id(value)));
        }
    }
    let gesture = Gesture::begin_with_edges(
        &model,
        id(1),
        Manipulation::Resize,
        [1.0; 2],
        viewport(),
        Some([0, -1]),
    )
    .unwrap();
    assert_eq!(gesture.axes(), [false; 2]);
    assert_eq!(gesture.propose(&model, [40.0; 2], None).unwrap().0, model);
}

#[test]
fn invalid_or_move_only_explicit_edges_are_rejected_without_changing_the_model() {
    let model = fixture(WorkspaceLayout::Free);
    for edges in [[0, 0], [2, 1], [-1, -2], [i8::MIN, i8::MAX]] {
        assert!(
            Gesture::begin_with_edges(
                &model,
                id(1),
                Manipulation::Resize,
                [0.5; 2],
                viewport(),
                Some(edges),
            )
            .is_err()
        );
    }
    assert!(
        Gesture::begin_with_edges(
            &model,
            id(1),
            Manipulation::Move,
            [0.5; 2],
            viewport(),
            Some([1, 0]),
        )
        .is_err()
    );
}

#[test]
fn target_or_role_changes_reject_stale_proposals_without_overwriting_other_changes() {
    let model = fixture(WorkspaceLayout::Free);
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    let mut current = model.clone();
    current.insert_inactive(item(4)).unwrap();
    let (next, _) = gesture.propose(&current, [1.0, 2.0], None).unwrap();
    assert!(next.item(id(4)).is_some());
    current.remove(id(1));
    let original = current.clone();
    assert!(gesture.propose(&current, [1.0, 2.0], None).is_err());
    assert_eq!(current, original);
}

fn geometry(model: &Workspace, target: PlacementId) -> Geometry {
    let rectangle = model
        .clone()
        .solve(viewport(), viewport())
        .unwrap()
        .items
        .into_iter()
        .find(|item| item.id == target)
        .unwrap()
        .rectangle;
    Geometry {
        origin: rectangle.origin(),
        size: rectangle.size(),
    }
}

#[test]
fn no_motion_preserves_complete_layout_and_maximize_state() {
    for mode in [WorkspaceLayout::Free, WorkspaceLayout::Tiled] {
        for maximized in [false, true] {
            for operation in [Manipulation::Move, Manipulation::Resize] {
                let mut model = fixture(mode);
                model.request_state(
                    id(1),
                    crate::host::WindowRequest {
                        maximized: Some(maximized),
                        ..Default::default()
                    },
                );
                let gesture =
                    Gesture::begin(&model, id(1), operation, [0.75; 2], viewport()).unwrap();
                let (next, preview) = gesture
                    .propose(
                        &model,
                        [0.0; 2],
                        Some(DropSlot {
                            anchor: id(3),
                            side: Direction::Down,
                        }),
                    )
                    .unwrap();
                assert_eq!(next, model);
                assert_eq!(preview, geometry(&model, id(1)));
                let restarted: Workspace =
                    serde_json::from_slice(&serde_json::to_vec(&next).unwrap()).unwrap();
                assert_eq!(restarted, model);
            }
        }
    }
}

#[test]
fn a_tiled_move_without_a_destination_keeps_the_original_layout() {
    let mut model = fixture(WorkspaceLayout::Tiled);
    model.focus(id(1), viewport()).unwrap();
    model.toggle_maximized();
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    for slot in [
        None,
        Some(DropSlot {
            anchor: id(1),
            side: Direction::Up,
        }),
    ] {
        let (next, preview) = gesture.propose(&model, [20.0; 2], slot).unwrap();
        assert_eq!(next, model);
        assert_eq!(preview, geometry(&model, id(1)));
    }
}

#[test]
fn pointer_resize_reaches_supported_limits_without_the_keyboard_step_limit() {
    let model = fixture(WorkspaceLayout::Tiled);
    let gesture =
        Gesture::begin(&model, id(1), Manipulation::Resize, [1.0, 0.5], viewport()).unwrap();
    let (large, preview) = gesture.propose(&model, [90_000.0, 0.0], None).unwrap();
    assert_eq!(preview.size[0], geometry(&model, id(1)).size[0] + 90_000.0);
    let (largest, preview) = gesture.propose(&model, [1_000_000.0, 0.0], None).unwrap();
    assert_eq!(preview.size[0], f64::from(LIMIT));
    for next in [large, largest] {
        assert_eq!(next.focused(), model.focused());
        assert_eq!(next.item_width(id(2)), model.item_width(id(2)));
        assert_eq!(next.row_height(id(1)), model.row_height(id(1)));
        assert!(next.valid());
    }
}

#[test]
fn bottom_edge_resizes_its_row_and_top_edge_resizes_the_preceding_shared_boundary() {
    let mut model = fixture(WorkspaceLayout::Tiled);
    model.focus(id(3), viewport()).unwrap();
    model.move_focused(Direction::Down, viewport());
    model.set_row_height(id(1), Width::Pixels(250.0)).unwrap();
    model.set_row_height(id(3), Width::Pixels(300.0)).unwrap();
    let before = geometry(&model, id(3));
    for (target, uv) in [(id(1), [0.5, 1.0]), (id(3), [0.5, 0.0])] {
        let gesture = Gesture::begin(&model, target, Manipulation::Resize, uv, viewport()).unwrap();
        assert_eq!(gesture.axes(), [false, true]);
        let (next, _) = gesture.propose(&model, [1000.0, 40.0], None).unwrap();
        assert_eq!(next.row_height(id(1)), Some(Width::Pixels(290.0)));
        assert_eq!(next.row_height(id(3)), model.row_height(id(3)));
        assert_eq!(geometry(&next, id(3)).size, before.size);
        assert_eq!(geometry(&next, id(3)).origin[1], before.origin[1] + 40.0);
        for value in 1..=3 {
            assert_eq!(next.item_width(id(value)), model.item_width(id(value)));
            assert_eq!(next.item(id(value)), model.item(id(value)));
        }
        assert_eq!(next.focused(), model.focused());
    }
    let gesture =
        Gesture::begin(&model, id(1), Manipulation::Resize, [0.5, 0.0], viewport()).unwrap();
    assert_eq!(gesture.axes(), [false; 2]);
    assert_eq!(gesture.propose(&model, [40.0; 2], None).unwrap().0, model);
}

#[test]
fn shared_row_limits_and_floating_homes_survive_a_committed_resize() {
    let mut model = fixture(WorkspaceLayout::Tiled);
    model.set_row_height(id(1), Width::Pixels(400.0)).unwrap();
    let mut rules = model.item(id(2)).unwrap().rules;
    rules.axes[1].min = 350.0;
    rules.axes[1].size = 350.0;
    model.configure(id(2), rules).unwrap();
    let original = geometry(&model, id(1)).rectangle().unwrap();
    model.toggle_floating(id(1), original).unwrap();
    let gesture =
        Gesture::begin(&model, id(2), Manipulation::Resize, [0.5, 1.0], viewport()).unwrap();
    let (mut next, preview) = gesture.propose(&model, [0.0, -200.0], None).unwrap();
    assert_eq!(preview.size[1], 350.0);
    next.remove(id(2));
    next.remove(id(3));
    next.toggle_floating(id(1), original).unwrap();
    assert_eq!(next.row_height(id(1)), Some(Width::Pixels(200.0)));
    assert!(next.valid());
}

#[test]
fn floating_gestures_keep_authored_homes_and_remember_their_own_resize() {
    let mut model = fixture(WorkspaceLayout::Tiled);
    let authored = model.item(id(1)).unwrap().clone();
    let width = model.item_width(id(1));
    let rectangle = geometry(&model, id(1)).rectangle().unwrap();
    model.toggle_floating(id(1), rectangle).unwrap();
    let gesture =
        Gesture::begin(&model, id(1), Manipulation::Resize, [1.0; 2], viewport()).unwrap();
    let (mut next, preview) = gesture.propose(&model, [70.0, 40.0], None).unwrap();
    assert_eq!(
        preview.size,
        [rectangle.size()[0] + 70.0, rectangle.size()[1] + 40.0]
    );
    let resized = next.item(id(1)).unwrap().clone();
    next.toggle_floating(id(1), rectangle).unwrap();
    assert_eq!(next.item(id(1)), Some(&authored));
    assert_eq!(next.item_width(id(1)), width);
    next.toggle_floating(id(1), rectangle).unwrap();
    assert_eq!(next.item(id(1)), Some(&resized));
    assert!(next.valid());
}

#[test]
fn restore_from_maximize_uses_normal_geometry_and_keeps_the_width_preset() {
    let mut model = fixture(WorkspaceLayout::Free);
    model.set_free_width(id(1), [1, 2], 1.0).unwrap();
    let normal = geometry(&model, id(1));
    model.request_state(
        id(1),
        crate::host::WindowRequest {
            maximized: Some(true),
            ..Default::default()
        },
    );
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    let (next, preview) = gesture.propose(&model, [10.0, 20.0], None).unwrap();
    assert_eq!(preview.size, normal.size);
    assert_eq!(
        preview.origin,
        [normal.origin[0] + 10.0, normal.origin[1] + 20.0]
    );
    assert_eq!(next.item(id(1)).unwrap().free_width.unwrap().ratio, [1, 2]);
    assert!(!next.maximized(id(1)));
    assert!(model.maximized(id(1)));
}

#[test]
fn proposals_use_total_motion_and_keep_concurrent_unrelated_changes() {
    let model = fixture(WorkspaceLayout::Free);
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    let (_, first) = gesture.propose(&model, [10.0; 2], None).unwrap();
    let mut current = model.clone();
    current.insert_inactive(item(4)).unwrap();
    current
        .set_free_geometry(
            id(2),
            Geometry {
                origin: [100.0; 2],
                size: [200.0; 2],
            },
        )
        .unwrap();
    current.focus(id(4), viewport()).unwrap();
    let (next, second) = gesture.propose(&current, [20.0; 2], None).unwrap();
    assert_eq!(second.origin, first.origin.map(|value| value + 10.0));
    assert_eq!(next.item(id(2)), current.item(id(2)));
    assert_eq!(next.item(id(4)), current.item(id(4)));
    assert_eq!(next.focused(), current.focused());
}

#[test]
fn changed_target_constraints_and_layout_intent_cancel_stale_proposals() {
    let model = fixture(WorkspaceLayout::Tiled);
    let gesture =
        Gesture::begin(&model, id(1), Manipulation::Resize, [1.0; 2], viewport()).unwrap();
    for change in 0..8 {
        let mut current = model.clone();
        match change {
            0 => {
                let mut rules = current.item(id(1)).unwrap().rules;
                rules.axes[0].max = 600.0;
                current.configure(id(1), rules).unwrap();
            }
            1 => current.set_item_width(id(1), Width::Third).unwrap(),
            2 => current.set_row_height(id(1), Width::Half).unwrap(),
            3 => current.set_gap(0.0).unwrap(),
            4 => {
                current.set_minimized(id(1), true);
            }
            5 => current.set_mode(WorkspaceLayout::Free),
            6 => current
                .influence(
                    id(1),
                    Influence {
                        scale: 2.0,
                        order: None,
                    },
                )
                .unwrap(),
            7 => {
                current.request_state(
                    id(1),
                    crate::host::WindowRequest {
                        maximized: Some(true),
                        ..Default::default()
                    },
                );
            }
            _ => unreachable!(),
        }
        let before = current.clone();
        assert!(gesture.propose(&current, [10.0; 2], None).is_err());
        assert_eq!(current, before);
    }
}

#[test]
fn vanished_or_ineligible_drop_anchors_do_not_publish_partial_reorders() {
    let model = fixture(WorkspaceLayout::Tiled);
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    for state in 0..3 {
        let mut current = model.clone();
        match state {
            0 => {
                current.remove(id(3));
            }
            1 => {
                current.set_minimized(id(3), true);
            }
            2 => {
                let rectangle = geometry(&current, id(3)).rectangle().unwrap();
                current.toggle_floating(id(3), rectangle).unwrap();
            }
            _ => unreachable!(),
        }
        let before = current.clone();
        assert!(
            gesture
                .propose(
                    &current,
                    [10.0; 2],
                    Some(DropSlot {
                        anchor: id(3),
                        side: Direction::Right
                    })
                )
                .is_err()
        );
        assert_eq!(current, before);
    }
}

#[test]
fn invalid_input_and_fullscreen_never_begin_or_mutate_a_gesture() {
    let mut model = fixture(WorkspaceLayout::Free);
    for uv in [[f64::NAN, 0.5], [-0.1, 0.5], [0.5, f64::INFINITY]] {
        assert!(Gesture::begin(&model, id(1), Manipulation::Move, uv, viewport()).is_err());
    }
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    for delta in [[f64::NAN, 0.0], [f64::INFINITY; 2], [1_000_000_001.0, 0.0]] {
        assert!(gesture.propose(&model, delta, None).is_err());
    }
    model.toggle_fullscreen();
    assert!(Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).is_err());
    assert!(gesture.propose(&model, [10.0; 2], None).is_err());
}

#[test]
fn world_boundaries_clamp_moves_and_keep_the_opposite_resize_edge_valid() {
    let mut model = fixture(WorkspaceLayout::Free);
    model
        .set_free_geometry(
            id(1),
            Geometry {
                origin: [1_000_000_000.0, -1_000_000_000.0],
                size: [300.0, 200.0],
            },
        )
        .unwrap();
    let move_gesture =
        Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    let (next, preview) = move_gesture
        .propose(&model, [1000.0, -1000.0], None)
        .unwrap();
    assert_eq!(preview.origin, [1_000_000_000.0, -1_000_000_000.0]);
    assert!(next.valid());
    let resize = Gesture::begin(&model, id(1), Manipulation::Resize, [0.0; 2], viewport()).unwrap();
    let (next, preview) = resize.propose(&model, [1000.0, -1000.0], None).unwrap();
    assert_eq!(preview.origin, [1_000_000_000.0, -1_000_000_000.0]);
    assert_eq!(preview.size, [300.0, 200.0]);
    assert!(next.valid());
}

#[test]
fn free_moves_ignore_unrelated_tiling_edits_and_keep_the_latest_row_intent() {
    let model = fixture(WorkspaceLayout::Free);
    let gesture = Gesture::begin(&model, id(1), Manipulation::Move, [0.5; 2], viewport()).unwrap();
    let mut current = model.clone();
    current.set_row_height(id(2), Width::Third).unwrap();
    current.set_item_width(id(2), Width::TwoThirds).unwrap();
    let (next, _) = gesture.propose(&current, [10.0; 2], None).unwrap();
    assert_eq!(next.row_height(id(1)), current.row_height(id(1)));
    assert_eq!(next.item_width(id(2)), current.item_width(id(2)));
}

#[test]
fn current_client_limits_resolve_before_the_gesture_captures_its_effective_edges() {
    let mut model = fixture(WorkspaceLayout::Free);
    model
        .set_free_geometry(
            id(1),
            Geometry {
                origin: [-1_000_000_000.0; 2],
                size: [50.0; 2],
            },
        )
        .unwrap();
    let mut rules = model.item(id(1)).unwrap().rules;
    for axis in &mut rules.axes {
        axis.min = 100.0;
    }
    model.configure(id(1), rules).unwrap();
    let gesture =
        Gesture::begin(&model, id(1), Manipulation::Resize, [0.0; 2], viewport()).unwrap();
    assert_eq!(gesture.original().size, [100.0; 2]);
    let (next, preview) = gesture.propose(&model, [-500.0; 2], None).unwrap();
    assert_eq!(preview.origin, [-1_000_000_000.0; 2]);
    assert_eq!(preview.size, [100.0; 2]);
    assert!(next.valid());
}

#[test]
fn a_replaced_shared_boundary_cancels_without_resizing_an_unrelated_row() {
    let mut model = fixture(WorkspaceLayout::Tiled);
    model.focus(id(3), viewport()).unwrap();
    model.move_focused(Direction::Down, viewport());
    let gesture =
        Gesture::begin(&model, id(3), Manipulation::Resize, [0.5, 0.0], viewport()).unwrap();
    model.focus(id(1), viewport()).unwrap();
    model.move_focused(Direction::Down, viewport());
    let before = model.clone();
    assert!(gesture.propose(&model, [0.0, 10.0], None).is_err());
    assert_eq!(model, before);
}
