use std::{collections::BTreeMap, sync::Arc};

use super::{MAX_PLACEMENTS, MAX_UNDO_RECEIPTS, Mutation, Request, Response, Snapshot, State};
use crate::{
    canvas::{Component, Descriptor, Geometry, Placement, Workspace},
    component::ComponentState,
};
fn state() -> State {
    State::new(
        Snapshot {
            revision: 0,
            active_workspace: 1,
            workspaces: vec![Workspace {
                id: 1,
                name: "Main".into(),
                center: [0.0, 0.0],
                zoom: 1.0,
                view: None,
            }],
            placements: vec![],
            next_offset: None,
        },
        vec![],
    )
    .unwrap()
}
fn mutate(state: &State, mutation: Mutation) -> Request {
    Request::Mutate {
        request_id: crate::new_uid("request"),
        expected_revision: state.snapshot.revision,
        mutation,
    }
}
#[test]
fn registry_refresh_preserves_receipts_and_rejects_invalid_replacements_atomically() {
    let mut state = state();
    let request = mutate(
        &state,
        Mutation::RenameWorkspace {
            workspace: 1,
            name: "Camera".into(),
        },
    );
    let receipt = state.handle(&request).unwrap();
    let descriptor = Descriptor {
        kind: "video".into(),
        name: "Video Castle".into(),
        description: String::new(),
        settings: BTreeMap::new(),
        max_bindings: 0,
        placeable: true,
        configurable: true,
        composable: true,
        unavailable_reason: None,
    };
    state.refresh_registry(vec![descriptor.clone()]).unwrap();
    assert_eq!(state.handle(&request).unwrap(), receipt);
    let before = serde_json::to_value(&state).unwrap();
    assert!(
        state
            .refresh_registry(vec![descriptor.clone(), descriptor])
            .is_err()
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    let Response::Receipt { receipt } = receipt else {
        panic!("expected a receipt, got {receipt:?}");
    };
    let undo = mutate(
        &state,
        Mutation::Undo {
            receipt: receipt.request_id,
        },
    );
    state.handle(&undo).unwrap();
    assert_eq!(state.snapshot.workspaces[0].name, "Main");
}

#[test]
fn crud_persists_receipts_rejects_stale_edits_and_preserves_binding() {
    let mut state = state();
    let placement = crate::new_uid("placement");
    let record = crate::new_uid("r");
    let add = mutate(
        &state,
        Mutation::Add {
            placement: placement.clone(),
            workspace: 1,
            component: Component::Builtin {
                state: ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
            },
            geometry: Geometry {
                position: [1.0, 2.0],
                size: [100.0, 100.0],
            },
        },
    );
    let first = state.handle(&add).unwrap();
    assert_eq!(state.handle(&add).unwrap(), first);
    assert_eq!(state.snapshot.placements.len(), 1);
    let mut restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.handle(&add).unwrap(), first);
    let stale = Request::Mutate {
        request_id: crate::new_uid("request"),
        expected_revision: 0,
        mutation: Mutation::Remove {
            placement: placement.clone(),
        },
    };
    assert!(restored.handle(&stale).is_err());
    let request = mutate(
        &restored,
        Mutation::Move {
            placement: placement.clone(),
            workspace: 1,
            position: [9.0, 9.0],
        },
    );
    restored.handle(&request).unwrap();
    assert_eq!(
        restored.snapshot.placements[0].component.records(),
        vec![record.as_str()]
    );
    let request = mutate(
        &restored,
        Mutation::Remove {
            placement: placement.clone(),
        },
    );
    let response = restored.handle(&request).unwrap();
    let Response::Receipt { receipt } = response else {
        panic!("expected a receipt, got {response:?}");
    };
    assert!(restored.snapshot.placements.is_empty());
    let undo = mutate(
        &restored,
        Mutation::Undo {
            receipt: receipt.request_id,
        },
    );
    restored.handle(&undo).unwrap();
    assert_eq!(restored.snapshot.placements[0].id, placement);
}
#[test]
fn removing_a_workspace_removes_contents_and_keeps_last_workspace() {
    let mut state = state();
    let create = mutate(
        &state,
        Mutation::CreateWorkspace {
            name: "Second".into(),
        },
    );
    state.handle(&create).unwrap();
    let rename = mutate(
        &state,
        Mutation::RenameWorkspace {
            workspace: 1,
            name: "Renamed inactive".into(),
        },
    );
    state.handle(&rename).unwrap();
    assert_eq!(state.snapshot.workspaces[0].name, "Renamed inactive");
    let remove = mutate(&state, Mutation::RemoveWorkspace { workspace: 2 });
    state.handle(&remove).unwrap();
    assert_eq!(state.snapshot.active_workspace, 1);
    let remove = mutate(&state, Mutation::RemoveWorkspace { workspace: 1 });
    assert!(state.handle(&remove).is_err());
}
#[test]
fn generated_compositions_cannot_be_reconfigured_to_drop_protection() {
    let mut state = state();
    let id = crate::new_uid("placement");
    let component = Component::Builtin {
        state: ComponentState::Composition {
            composition: crate::component::Composition {
                name: "Protected".into(),
                origin: Some(crate::component::composition::Origin {
                    agent: crate::new_uid("r"),
                    thread: crate::new_uid("r"),
                }),
                parts: vec![crate::component::Part {
                    settings: Default::default(),
                    id: "text".into(),
                    position: [0, 0],
                    size: [100, 100],
                    component: ComponentState::Text {
                        text: "Inside".into(),
                    },
                    events: vec![],
                }],
            },
        },
    };
    state
        .handle(&mutate(
            &state,
            Mutation::Add {
                placement: id.clone(),
                workspace: 1,
                component,
                geometry: Geometry {
                    position: [0.0, 0.0],
                    size: [100.0, 100.0],
                },
            },
        ))
        .unwrap();
    let change = mutate(
        &state,
        Mutation::Configure {
            placement: id,
            component: Component::Builtin {
                state: ComponentState::Text {
                    text: "Unprotected".into(),
                },
            },
        },
    );
    assert!(state.handle(&change).is_err());
}
#[test]
fn populated_canvas_has_bounded_cost_and_never_replays_evicted_requests() {
    let mut state = state();
    let mut first = None;
    for index in 0..MAX_PLACEMENTS {
        let request = mutate(
            &state,
            Mutation::Add {
                placement: crate::new_uid("placement"),
                workspace: 1,
                component: Component::Builtin {
                    state: ComponentState::Text {
                        text: format!("Panel {index}"),
                    },
                },
                geometry: Geometry {
                    position: [f64::from(u32::try_from(index).unwrap()), 0.0],
                    size: [100.0, 100.0],
                },
            },
        );
        if first.is_none() {
            first = Some(request.clone());
        }
        state.handle(&request).unwrap();
    }
    assert_eq!(state.receipts.len(), MAX_UNDO_RECEIPTS);
    state.validate().unwrap();
    assert!(state.handle(&first.unwrap()).is_err());
    let before = state.snapshot.clone();
    let request = mutate(
        &state,
        Mutation::Add {
            placement: state.snapshot.placements[0].id.clone(),
            workspace: 1,
            component: Component::Builtin {
                state: ComponentState::Text {
                    text: "Duplicate".into(),
                },
            },
            geometry: Geometry {
                position: [0.0, 0.0],
                size: [100.0, 100.0],
            },
        },
    );
    assert!(state.handle(&request).is_err());
    assert_eq!(state.snapshot, before);
}
#[test]
fn inspection_pages_do_not_limit_the_whole_canvas_to_one_page() {
    let mut state = state();
    for _ in 0..300 {
        let snapshot = Arc::make_mut(&mut state.snapshot);
        snapshot.placements.push(Placement {
            id: crate::new_uid("placement"),
            workspace: 1,
            component: Component::Builtin {
                state: ComponentState::Text {
                    text: "Existing panel".into(),
                },
            },
            geometry: Geometry {
                position: [0.0, 0.0],
                size: [100.0, 100.0],
            },
            selected: false,
            group: None,
            metadata: Default::default(),
        });
    }
    state.validate().unwrap();
    let response = state
        .handle(&Request::Inspect {
            workspace: None,
            selected_only: false,
            offset: 0,
            limit: 256,
        })
        .unwrap();
    let Response::Snapshot { snapshot } = response else {
        panic!("expected a snapshot, got {response:?}");
    };
    assert_eq!(snapshot.placements.len(), 256);
    assert_eq!(snapshot.next_offset, Some(256));
    let remove = mutate(
        &state,
        Mutation::Remove {
            placement: state
                .snapshot
                .placements
                .last()
                .expect("the canvas has 300 placements")
                .id
                .clone(),
        },
    );
    state.handle(&remove).unwrap();
    assert_eq!(state.snapshot.placements.len(), 299);
    state
        .handle(&mutate(
            &state,
            Mutation::CreateWorkspace {
                name: "Destination".into(),
            },
        ))
        .unwrap();
    let response = state
        .handle(&mutate(&state, Mutation::RemoveWorkspace { workspace: 1 }))
        .unwrap();
    let Response::Receipt { receipt } = response else {
        panic!("expected a receipt, got {response:?}");
    };
    assert_eq!(receipt.affected_count, 299);
    assert_eq!(receipt.affected_placements.len(), 256);
    assert!(state.snapshot.placements.is_empty());
}
#[test]
fn deleted_workspace_identifiers_are_never_reassigned() {
    let mut state = state();
    state
        .handle(&mutate(
            &state,
            Mutation::CreateWorkspace { name: "Old".into() },
        ))
        .unwrap();
    state
        .handle(&mutate(&state, Mutation::RemoveWorkspace { workspace: 2 }))
        .unwrap();
    let mut restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    restored.validate().unwrap();
    restored
        .handle(&mutate(
            &restored,
            Mutation::CreateWorkspace { name: "New".into() },
        ))
        .unwrap();
    assert_eq!(restored.snapshot.active_workspace, 3);
    assert!(
        restored
            .handle(&mutate(
                &restored,
                Mutation::SwitchWorkspace { workspace: 2 }
            ))
            .is_err()
    );
}
