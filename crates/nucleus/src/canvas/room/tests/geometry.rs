use std::sync::Arc;

use super::{
    TestResult, member, move_to, mutate, room, roundtrip, state, task, undo, undo_changes,
};
use crate::canvas::{
    Mutation, Pose,
    room::{Member, Role, identity_pose},
};

#[test]
fn room_large_coordinate_receipts_reload_and_undo_keep_local_calibration() -> TestResult {
    let mut state = state()?;
    let original = state.snapshot.placements.clone();
    let receipt = move_to(&mut state, &room(&original).id, [1e9, -1e9])?;
    state.validate()?;
    let mut restored = roundtrip(&state)?;
    restored.validate()?;
    for (before, after) in original.iter().zip(&restored.snapshot.placements) {
        assert_eq!(before.metadata.room_member, after.metadata.room_member);
        let [before_x, before_z] = before.geometry.position;
        let [after_x, after_z] = after.geometry.position;
        assert!((after_x - before_x - (1e9 - 31.0)).abs() < 1e-6);
        assert!((after_z - before_z - (-1e9 - 47.0)).abs() < 1e-6);
    }
    let saved = Arc::clone(&restored.snapshot);
    assert!(move_to(&mut restored, &room(&original).id, [f64::INFINITY, 0.0]).is_err());
    assert_eq!(restored.snapshot, saved);
    undo(&mut restored, receipt)?;
    assert_eq!(restored.snapshot.placements, original);
    Ok(())
}

#[test]
fn room_alignment_undo_restores_world_pose_after_parent_motion_and_keeps_later_resize() -> TestResult
{
    let mut state = state()?;
    let room = room(&state.snapshot.placements).id.clone();
    let id = task(&state.snapshot.placements).id.clone();
    mutate(
        &mut state,
        Mutation::SetRoomMembers {
            members: [(id.clone(), None)].into(),
        },
    )?;
    let original = task(&state.snapshot.placements).clone();
    let receipt = mutate(
        &mut state,
        Mutation::SetRoomMembers {
            members: [(
                id.clone(),
                Some(Member {
                    room: room.clone(),
                    parent: None,
                    local: Pose {
                        position: [80.0, 40.0, -60.0],
                        ..identity_pose()
                    },
                    role: Role::Content,
                }),
            )]
            .into(),
        },
    )?;
    move_to(&mut state, &room, [1e9, -1e9])?;
    mutate(
        &mut state,
        Mutation::Resize {
            placement: id,
            size: [200.0, 300.0],
        },
    )?;
    undo_changes(&mut state, receipt)?;
    let restored = task(&state.snapshot.placements);
    assert!(restored.metadata.room_member.is_none());
    assert_eq!(restored.geometry.position, original.geometry.position);
    assert_eq!(restored.metadata.spatial, original.metadata.spatial);
    assert_eq!(restored.geometry.size, [200.0, 300.0]);
    state.validate()?;
    Ok(())
}

#[test]
fn room_canvas_moves_project_nested_geometry_and_world_moves_update_member_locals() -> TestResult {
    let mut state = state()?;
    let original = state.snapshot.placements.clone();
    let receipt = move_to(&mut state, &room(&original).id, [131.0, 247.0])?;
    for (before, after) in original.iter().zip(&state.snapshot.placements).skip(1) {
        let [x, z] = before.geometry.position;
        assert_eq!(after.geometry.position, [x + 100.0, z + 200.0]);
        assert_eq!(after.metadata.room_member, before.metadata.room_member);
    }
    undo(&mut state, receipt)?;
    assert_eq!(state.snapshot.placements, original);
    let child_id = task(&original).id.clone();
    move_to(&mut state, &child_id, [490.0, 650.0])?;
    let local = task(&state.snapshot.placements)
        .metadata
        .room_member
        .clone();
    assert_ne!(local, task(&original).metadata.room_member);
    assert_eq!(
        task(&state.snapshot.placements).geometry.position,
        [490.0, 650.0]
    );
    move_to(&mut state, &room(&original).id, [131.0, 247.0])?;
    assert_eq!(
        task(&state.snapshot.placements).geometry.position,
        [590.0, 850.0]
    );
    assert_eq!(task(&state.snapshot.placements).metadata.room_member, local);
    Ok(())
}

#[test]
fn undo_room_local_edits_after_parent_motion_preserves_later_size_and_depth_edits() -> TestResult {
    let mut state = state()?;
    let original = task(&state.snapshot.placements).clone();
    let root_id = room(&state.snapshot.placements).id.clone();
    let mut metadata = original.metadata.clone();
    let [x, ..] = &mut member(&mut metadata).local.position;
    *x += 25.0;
    let local = mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: original.id.clone(),
            metadata,
        },
    )?;
    mutate(
        &mut state,
        Mutation::Resize {
            placement: original.id.clone(),
            size: [200.0; 2],
        },
    )?;
    let mut metadata = task(&state.snapshot.placements).metadata.clone();
    metadata.spatial.depth = Some(45.0);
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: original.id.clone(),
            metadata,
        },
    )?;
    move_to(&mut state, &root_id, [131.0, 247.0])?;
    undo_changes(&mut state, local)?;
    let child = task(&state.snapshot.placements);
    assert_eq!(child.metadata.room_member, original.metadata.room_member);
    let [x, z] = original.geometry.position;
    assert_eq!(child.geometry.position, [x + 100.0, z + 200.0]);
    assert_eq!(child.geometry.size, [200.0; 2]);
    assert_eq!(child.metadata.spatial.depth, Some(45.0));
    Ok(())
}

#[test]
fn room_member_resize_undo_preserves_parent_motion_and_detach_undo_rejects_later_world_edits()
-> TestResult {
    let mut state = state()?;
    let original = task(&state.snapshot.placements).clone();
    let root_id = room(&state.snapshot.placements).id.clone();
    let resize = mutate(
        &mut state,
        Mutation::Resize {
            placement: original.id.clone(),
            size: [200.0; 2],
        },
    )?;
    move_to(&mut state, &root_id, [131.0, 247.0])?;
    undo_changes(&mut state, resize)?;
    assert_eq!(
        task(&state.snapshot.placements).geometry.size,
        original.geometry.size
    );
    let mut metadata = task(&state.snapshot.placements).metadata.clone();
    metadata.room_member = None;
    let detach = mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: original.id.clone(),
            metadata,
        },
    )?;
    move_to(&mut state, &original.id, [400.0, 500.0])?;
    let before = Arc::clone(&state.snapshot);
    assert!(
        undo_changes(&mut state, detach)
            .unwrap_err()
            .contains("world placement")
    );
    assert_eq!(state.snapshot, before);
    Ok(())
}
