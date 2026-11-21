use std::{collections::BTreeMap, sync::Arc};

use super::{TestResult, member, move_to, mutate, placement, room, state, undo, undo_changes};
use crate::{
    canvas::{
        Metadata, Mutation, Pose, Spatial,
        room::{Member, Role, identity_pose},
    },
    new_uid,
};

#[test]
fn removing_room_anchor_detaches_descendants_preserves_world_geometry_and_undo_restores_links()
-> TestResult {
    let mut state = state()?;
    let projected = state.snapshot.placements.clone();
    let [_, desk, remaining @ ..] = projected.as_slice() else {
        return Err("The Room fixture is missing its desk.".into());
    };
    assert_eq!(state.snapshot.revision, 0);
    let receipt = mutate(
        &mut state,
        Mutation::Remove {
            placement: desk.id.clone(),
        },
    )?;
    assert_eq!(receipt.affected_count, 3);
    for old in remaining {
        let current = state
            .snapshot
            .placements
            .iter()
            .find(|item| item.id == old.id)
            .ok_or("The detached descendant is missing from the snapshot.")?;
        assert_eq!(current.geometry, old.geometry);
        assert_eq!(current.metadata.spatial, old.metadata.spatial);
        assert!(current.metadata.room_member.is_none());
    }
    undo(&mut state, receipt)?;
    assert_eq!(state.snapshot.placements, projected);
    state.validate()?;
    Ok(())
}

#[test]
fn room_group_assignment_is_atomic_and_undo_preserves_the_complete_group() -> TestResult {
    let mut state = state()?;
    let mut first = placement();
    let mut second = placement();
    first.group = Some("room-board".into());
    second.group = first.group.clone();
    let ids = [first.id.clone(), second.id.clone()];
    let [first_id, _] = &ids;
    Arc::make_mut(&mut state.snapshot)
        .placements
        .extend([first, second]);
    state.validate()?;
    let before = Arc::clone(&state.snapshot);
    let member = Member {
        room: room(&before.placements).id.clone(),
        parent: None,
        local: identity_pose(),
        role: Role::Content,
    };
    assert!(
        mutate(
            &mut state,
            Mutation::SetRoomMembers {
                members: BTreeMap::from([(first_id.clone(), Some(member.clone()))]),
            }
        )
        .unwrap_err()
        .contains("same room parent")
    );
    assert_eq!(state.snapshot, before);
    let receipt = mutate(
        &mut state,
        Mutation::SetRoomMembers {
            members: ids
                .iter()
                .map(|id| (id.clone(), Some(member.clone())))
                .collect(),
        },
    )?;
    assert_eq!(receipt.affected_count, 2);
    assert!(
        state
            .snapshot
            .placements
            .iter()
            .filter(|item| ids.contains(&item.id))
            .all(|item| item.metadata.room_member.as_ref() == Some(&member))
    );
    undo_changes(&mut state, receipt)?;
    assert_eq!(state.snapshot.placements, before.placements);
    assert!(
        mutate(
            &mut state,
            Mutation::SetRoomMembers {
                members: BTreeMap::new()
            }
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn room_proxy_creation_is_atomic_and_undo_survives_parent_movement() -> TestResult {
    let mut state = state()?;
    let item = placement();
    let mut metadata = Metadata {
        room_member: Some(Member {
            room: room(&state.snapshot.placements).id.clone(),
            parent: None,
            local: Pose {
                position: [10.0, 40.0, 20.0],
                ..identity_pose()
            },
            role: Role::Floor,
        }),
        spatial: Spatial {
            depth: Some(4.0),
            ..Default::default()
        },
        material: Some(Default::default()),
        ..Default::default()
    };
    let receipt = mutate(
        &mut state,
        Mutation::AddWithMetadata {
            placement: item.id.clone(),
            workspace: 1,
            component: item.component.clone(),
            geometry: item.geometry.clone(),
            metadata: metadata.clone(),
        },
    )?;
    let root = room(&state.snapshot.placements).id.clone();
    move_to(&mut state, &root, [1000.0, 2000.0])?;
    undo_changes(&mut state, receipt)?;
    assert!(
        !state
            .snapshot
            .placements
            .iter()
            .any(|placed| placed.id == item.id)
    );
    member(&mut metadata).room = new_uid("placement");
    let before = Arc::clone(&state.snapshot);
    assert!(
        mutate(
            &mut state,
            Mutation::AddWithMetadata {
                placement: item.id,
                workspace: 1,
                component: item.component,
                geometry: item.geometry,
                metadata
            }
        )
        .is_err()
    );
    assert_eq!(state.snapshot, before);
    Ok(())
}
