use super::*;
use crate::canvas::{
    Component, Geometry, Metadata, Mutation, Request, Response, Snapshot, State, Workspace,
};

fn placement() -> Placement {
    Placement {
        id: crate::new_uid("placement"),
        workspace: 1,
        component: Component::Builtin {
            state: crate::component::ComponentState::Record {
                record: crate::new_uid("r"),
                mode: Default::default(),
                start_call: None,
            },
        },
        geometry: Geometry {
            position: [31.0, 47.0],
            size: [100.0; 2],
        },
        selected: false,
        group: None,
        metadata: Metadata::default(),
    }
}

fn hierarchy() -> Vec<Placement> {
    let mut root = placement();
    root.metadata.room = Some(Room::default());
    let mut desk = placement();
    desk.metadata.room_member = Some(Member {
        room: root.id.clone(),
        parent: None,
        local: identity_pose(),
        role: Role::Desk,
    });
    let mut computer = placement();
    computer.metadata.room_member = Some(Member {
        room: root.id.clone(),
        parent: Some(desk.id.clone()),
        local: Pose {
            position: [1.0, 2.0, 3.0],
            ..identity_pose()
        },
        role: Role::Computer,
    });
    let mut task = placement();
    task.metadata.room_member = Some(Member {
        room: root.id.clone(),
        parent: Some(computer.id.clone()),
        local: Pose {
            position: [4.0, 5.0, 6.0],
            ..identity_pose()
        },
        role: Role::Content,
    });
    vec![root, desk, computer, task]
}

fn state() -> State {
    State::new(
        Snapshot {
            revision: 0,
            active_workspace: 1,
            workspaces: vec![Workspace {
                id: 1,
                name: "Main".into(),
                center: [0.0; 2],
                zoom: 1.0,
                view: None,
            }],
            placements: hierarchy(),
            next_offset: None,
        },
        Vec::new(),
    )
    .unwrap()
}

fn mutate(state: &mut State, mutation: Mutation) -> Result<crate::canvas::Receipt, String> {
    match state.handle(&Request::Mutate {
        request_id: crate::new_uid("request"),
        expected_revision: state.snapshot.revision,
        mutation,
    })? {
        Response::Receipt { receipt } => Ok(receipt),
        _ => panic!("Expected receipt"),
    }
}

#[test]
fn room_large_coordinate_receipts_reload_and_undo_keep_local_calibration() {
    let mut state = state();
    let original = state.snapshot.placements.clone();
    let receipt = mutate(
        &mut state,
        Mutation::Move {
            placement: original[0].id.clone(),
            workspace: 1,
            position: [1e9, -1e9],
        },
    )
    .unwrap();
    state.validate().unwrap();
    let mut restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    restored.validate().unwrap();
    for (before, after) in original.iter().zip(&restored.snapshot.placements) {
        assert_eq!(before.metadata.room_member, after.metadata.room_member);
        assert!(
            (after.geometry.position[0] - before.geometry.position[0] - (1e9 - 31.0)).abs() < 1e-6
        );
        assert!(
            (after.geometry.position[1] - before.geometry.position[1] - (-1e9 - 47.0)).abs() < 1e-6
        );
    }
    let saved = restored.snapshot.clone();
    assert!(
        mutate(
            &mut restored,
            Mutation::Move {
                placement: original[0].id.clone(),
                workspace: 1,
                position: [f64::INFINITY, 0.0],
            },
        )
        .is_err()
    );
    assert_eq!(restored.snapshot, saved);
    mutate(
        &mut restored,
        Mutation::Undo {
            receipt: receipt.request_id,
        },
    )
    .unwrap();
    assert_eq!(restored.snapshot.placements, original);
}

#[test]
fn room_alignment_undo_restores_world_pose_after_parent_motion_and_keeps_later_resize() {
    let mut state = state();
    let room = state.snapshot.placements[0].id.clone();
    let id = state.snapshot.placements[3].id.clone();
    mutate(
        &mut state,
        Mutation::SetRoomMembers {
            members: [(id.clone(), None)].into(),
        },
    )
    .unwrap();
    let original = state.snapshot.placements[3].clone();
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
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Move {
            placement: room,
            workspace: 1,
            position: [1e9, -1e9],
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Resize {
            placement: id,
            size: [200.0, 300.0],
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::UndoChanges {
            receipt: receipt.request_id,
        },
    )
    .unwrap();
    let restored = &state.snapshot.placements[3];
    assert!(restored.metadata.room_member.is_none());
    assert_eq!(restored.geometry.position, original.geometry.position);
    assert_eq!(restored.metadata.spatial, original.metadata.spatial);
    assert_eq!(restored.geometry.size, [200.0, 300.0]);
    state.validate().unwrap();
}

#[test]
fn room_canvas_moves_project_nested_geometry_and_world_moves_update_member_locals() {
    let mut state = state();
    let original = state.snapshot.placements.clone();
    let receipt = mutate(
        &mut state,
        Mutation::Move {
            placement: original[0].id.clone(),
            workspace: 1,
            position: [131.0, 247.0],
        },
    )
    .unwrap();
    for (before, after) in original.iter().zip(&state.snapshot.placements).skip(1) {
        assert_eq!(
            after.geometry.position,
            [
                before.geometry.position[0] + 100.0,
                before.geometry.position[1] + 200.0
            ]
        );
        assert_eq!(after.metadata.room_member, before.metadata.room_member);
    }
    mutate(
        &mut state,
        Mutation::Undo {
            receipt: receipt.request_id,
        },
    )
    .unwrap();
    assert_eq!(state.snapshot.placements, original);
    let child_id = original[3].id.clone();
    mutate(
        &mut state,
        Mutation::Move {
            placement: child_id,
            workspace: 1,
            position: [490.0, 650.0],
        },
    )
    .unwrap();
    let local = state.snapshot.placements[3].metadata.room_member.clone();
    assert_ne!(local, original[3].metadata.room_member);
    assert_eq!(
        state.snapshot.placements[3].geometry.position,
        [490.0, 650.0]
    );
    mutate(
        &mut state,
        Mutation::Move {
            placement: original[0].id.clone(),
            workspace: 1,
            position: [131.0, 247.0],
        },
    )
    .unwrap();
    assert_eq!(
        state.snapshot.placements[3].geometry.position,
        [590.0, 850.0]
    );
    assert_eq!(state.snapshot.placements[3].metadata.room_member, local);
}

#[test]
fn undo_room_local_edits_after_parent_motion_preserves_later_size_and_depth_edits() {
    let mut state = state();
    let original = state.snapshot.placements[3].clone();
    let root_id = state.snapshot.placements[0].id.clone();
    let mut metadata = original.metadata.clone();
    metadata.room_member.as_mut().unwrap().local.position[0] += 25.0;
    let local = mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: original.id.clone(),
            metadata,
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Resize {
            placement: original.id.clone(),
            size: [200.0; 2],
        },
    )
    .unwrap();
    let mut metadata = state.snapshot.placements[3].metadata.clone();
    metadata.spatial.depth = Some(45.0);
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: original.id.clone(),
            metadata,
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Move {
            placement: root_id,
            workspace: 1,
            position: [131.0, 247.0],
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::UndoChanges {
            receipt: local.request_id,
        },
    )
    .unwrap();
    let child = &state.snapshot.placements[3];
    assert_eq!(child.metadata.room_member, original.metadata.room_member);
    assert_eq!(
        child.geometry.position,
        [
            original.geometry.position[0] + 100.0,
            original.geometry.position[1] + 200.0
        ]
    );
    assert_eq!(child.geometry.size, [200.0; 2]);
    assert_eq!(child.metadata.spatial.depth, Some(45.0));
}

#[test]
fn room_member_resize_undo_preserves_parent_motion_and_detach_undo_rejects_later_world_edits() {
    let mut state = state();
    let original = state.snapshot.placements[3].clone();
    let root_id = state.snapshot.placements[0].id.clone();
    let resize = mutate(
        &mut state,
        Mutation::Resize {
            placement: original.id.clone(),
            size: [200.0; 2],
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Move {
            placement: root_id,
            workspace: 1,
            position: [131.0, 247.0],
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::UndoChanges {
            receipt: resize.request_id,
        },
    )
    .unwrap();
    assert_eq!(
        state.snapshot.placements[3].geometry.size,
        original.geometry.size
    );
    let mut metadata = state.snapshot.placements[3].metadata.clone();
    metadata.room_member = None;
    let detach = mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: original.id.clone(),
            metadata,
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Move {
            placement: original.id,
            workspace: 1,
            position: [400.0, 500.0],
        },
    )
    .unwrap();
    let before = state.snapshot.clone();
    assert!(
        mutate(
            &mut state,
            Mutation::UndoChanges {
                receipt: detach.request_id
            }
        )
        .unwrap_err()
        .contains("world placement")
    );
    assert_eq!(state.snapshot, before);
}

#[test]
fn room_hierarchy_roundtrip_preserves_local_poses_and_rejects_invalid_metadata() {
    let items = hierarchy();
    validate(&items, true).unwrap();
    let restored: Vec<Placement> =
        serde_json::from_str(&serde_json::to_string(&items).unwrap()).unwrap();
    assert_eq!(restored, items);
    let mut metadata = restored[0].metadata.clone();
    metadata.room.as_mut().unwrap().home.rotation = [0.0; 4];
    assert!(!metadata.validate());
    metadata = restored[3].metadata.clone();
    metadata.room_member.as_mut().unwrap().local.position[0] = f64::NAN;
    assert!(!metadata.validate());
    metadata = restored[0].metadata.clone();
    metadata.pinned = Some(crate::canvas::Pinned {
        anchor: [0.5; 2],
        scale: 1.0,
    });
    assert!(!metadata.validate());
    metadata = restored[0].metadata.clone();
    metadata.room_member = restored[3].metadata.room_member.clone();
    assert!(!metadata.validate());
    metadata = restored[3].metadata.clone();
    metadata.follow_slope = true;
    assert!(!metadata.validate());
}

#[test]
fn room_hierarchy_rejects_cycles_foreign_workspaces_and_foreign_rooms() {
    let original = hierarchy();
    let mut items = original.clone();
    items[1].metadata.room_member.as_mut().unwrap().parent = Some(items[2].id.clone());
    assert!(validate(&items, true).unwrap_err().contains("acyclic"));
    items = original.clone();
    items[2].workspace = 2;
    assert!(validate(&items, true).is_err());
    items = original.clone();
    let mut other = placement();
    other.metadata.room = Some(Room::default());
    items[2].metadata.room_member.as_mut().unwrap().room = other.id.clone();
    items.push(other);
    assert!(validate(&items, true).is_err());
    items = original;
    items[1].metadata.room_member.as_mut().unwrap().role = Role::Content;
    assert!(validate(&items, true).is_err());
}

#[test]
fn room_inspection_pages_can_omit_parents_but_complete_state_cannot() {
    let items = hierarchy();
    let page = &items[3..];
    validate(page, false).unwrap();
    assert!(validate(page, true).unwrap_err().contains("missing"));
    let mut items = items;
    items[2].metadata.room_member.as_mut().unwrap().parent = Some(crate::new_uid("placement"));
    assert!(validate(&items, true).unwrap_err().contains("anchor"));
}

#[test]
fn room_limits_bound_parent_depth_rooms_and_members() {
    let mut items = hierarchy();
    for _ in 0..MAX_DEPTH {
        let mut child = placement();
        child.metadata.room_member = Some(Member {
            room: items[0].id.clone(),
            parent: Some(items.last().unwrap().id.clone()),
            local: identity_pose(),
            role: Role::Desk,
        });
        items
            .last_mut()
            .unwrap()
            .metadata
            .room_member
            .as_mut()
            .unwrap()
            .role = Role::Desk;
        items.push(child);
    }
    assert!(validate(&items, true).unwrap_err().contains("eight"));
    let mut items = Vec::new();
    for _ in 0..=MAX_ROOMS {
        let mut root = placement();
        root.metadata.room = Some(Room::default());
        items.push(root);
    }
    assert!(validate(&items, true).unwrap_err().contains("sixteen"));
    items.truncate(1);
    for _ in 0..=MAX_MEMBERS {
        let mut child = placement();
        child.metadata.room_member = Some(Member {
            room: items[0].id.clone(),
            parent: None,
            local: identity_pose(),
            role: Role::Content,
        });
        items.push(child);
    }
    assert!(validate(&items, true).unwrap_err().contains("256"));
}

#[test]
fn removing_room_anchor_detaches_descendants_preserves_world_geometry_and_undo_restores_links() {
    let items = hierarchy();
    let mut state = State::new(
        Snapshot {
            revision: 0,
            active_workspace: 1,
            workspaces: vec![Workspace {
                id: 1,
                name: "Main".into(),
                center: [0.0; 2],
                zoom: 1.0,
                view: None,
            }],
            placements: items.clone(),
            next_offset: None,
        },
        Vec::new(),
    )
    .unwrap();
    let projected = state.snapshot.placements.clone();
    let response = state
        .handle(&Request::Mutate {
            request_id: crate::new_uid("request"),
            expected_revision: 0,
            mutation: Mutation::Remove {
                placement: items[1].id.clone(),
            },
        })
        .unwrap();
    let Response::Receipt { receipt } = response else {
        panic!("Expected receipt")
    };
    assert_eq!(receipt.affected_count, 3);
    for old in &projected[2..] {
        let current = state
            .snapshot
            .placements
            .iter()
            .find(|item| item.id == old.id)
            .unwrap();
        assert_eq!(current.geometry, old.geometry);
        assert_eq!(current.metadata.spatial, old.metadata.spatial);
        assert!(current.metadata.room_member.is_none());
    }
    state
        .handle(&Request::Mutate {
            request_id: crate::new_uid("request"),
            expected_revision: state.snapshot.revision,
            mutation: Mutation::Undo {
                receipt: receipt.request_id,
            },
        })
        .unwrap();
    assert_eq!(state.snapshot.placements, projected);
    state.validate().unwrap();
}

#[test]
fn room_group_assignment_is_atomic_and_undo_preserves_the_complete_group() {
    let mut state = state();
    let mut first = placement();
    let mut second = placement();
    first.group = Some("room-board".into());
    second.group = first.group.clone();
    let ids = [first.id.clone(), second.id.clone()];
    state.snapshot.placements.extend([first, second]);
    state.validate().unwrap();
    let before = state.snapshot.clone();
    let member = Member {
        room: before.placements[0].id.clone(),
        parent: None,
        local: identity_pose(),
        role: Role::Content,
    };
    assert!(
        mutate(
            &mut state,
            Mutation::SetRoomMembers {
                members: BTreeMap::from([(ids[0].clone(), Some(member.clone()))]),
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
    )
    .unwrap();
    assert_eq!(receipt.affected_count, 2);
    assert!(
        state
            .snapshot
            .placements
            .iter()
            .filter(|item| ids.contains(&item.id))
            .all(|item| item.metadata.room_member.as_ref() == Some(&member))
    );
    mutate(
        &mut state,
        Mutation::UndoChanges {
            receipt: receipt.request_id,
        },
    )
    .unwrap();
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
}

#[test]
fn room_proxy_creation_is_atomic_and_undo_survives_parent_movement() {
    let mut state = state();
    let item = placement();
    let mut metadata = Metadata::default();
    metadata.room_member = Some(Member {
        room: state.snapshot.placements[0].id.clone(),
        parent: None,
        local: Pose {
            position: [10.0, 40.0, 20.0],
            ..identity_pose()
        },
        role: Role::Floor,
    });
    metadata.spatial.depth = Some(4.0);
    metadata.material = Some(Default::default());
    let receipt = mutate(
        &mut state,
        Mutation::AddWithMetadata {
            placement: item.id.clone(),
            workspace: 1,
            component: item.component.clone(),
            geometry: item.geometry.clone(),
            metadata: metadata.clone(),
        },
    )
    .unwrap();
    let root = state.snapshot.placements[0].id.clone();
    mutate(
        &mut state,
        Mutation::Move {
            placement: root,
            workspace: 1,
            position: [1000.0, 2000.0],
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::UndoChanges {
            receipt: receipt.request_id,
        },
    )
    .unwrap();
    assert!(
        !state
            .snapshot
            .placements
            .iter()
            .any(|placed| placed.id == item.id)
    );
    metadata.room_member.as_mut().unwrap().room = crate::new_uid("placement");
    let before = state.snapshot.clone();
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
}
