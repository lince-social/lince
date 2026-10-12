use nucleus::canvas::{self as api, authoring::*};
use nucleus::component::ComponentState;
use std::collections::BTreeMap;

#[test]
fn canonical_hash_ignores_object_key_order_and_preserves_sequence_and_text() {
    let first: serde_json::Value =
        serde_json::from_str(r#"{"z":{"b":2,"a":1},"a":["literal",3]}"#).unwrap();
    let second: serde_json::Value =
        serde_json::from_str(r#"{"a":["literal",3],"z":{"a":1,"b":2}}"#).unwrap();
    let reordered: serde_json::Value =
        serde_json::from_str(r#"{"a":[3,"literal"],"z":{"a":1,"b":2}}"#).unwrap();
    let changed: serde_json::Value =
        serde_json::from_str(r#"{"a":["different",3],"z":{"a":1,"b":2}}"#).unwrap();
    assert_eq!(hash(&first).unwrap(), hash(&second).unwrap());
    assert_ne!(hash(&first).unwrap(), hash(&reordered).unwrap());
    assert_ne!(hash(&first).unwrap(), hash(&changed).unwrap());
}

fn fixture() -> (api::State, String, String) {
    let placement = nucleus::new_uid("placement");
    let record = nucleus::new_uid("r");
    let snapshot = api::Snapshot {
        revision: 0,
        active_workspace: 1,
        workspaces: vec![api::Workspace {
            id: 1,
            name: "Main".into(),
            center: [0.0; 2],
            zoom: 1.0,
            view: None,
        }],
        placements: vec![api::Placement {
            id: placement.clone(),
            workspace: 1,
            component: api::Component::Builtin {
                state: ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
            },
            geometry: api::Geometry {
                position: [5.0, 8.0],
                size: [640.0, 400.0],
            },
            selected: false,
            group: None,
            metadata: Default::default(),
        }],
        next_offset: None,
    };
    (
        api::State::new(snapshot, vec![]).unwrap(),
        placement,
        record,
    )
}

fn request(state: &api::State, mutation: api::Mutation) -> api::Request {
    api::Request::Mutate {
        request_id: nucleus::new_uid("request"),
        expected_revision: state.snapshot.revision,
        mutation,
    }
}

#[test]
fn atomic_patch_receipts_export_and_recovery_preserve_record_bindings() {
    let (mut state, placement, record) = fixture();
    let patch = request(
        &state,
        api::Mutation::Patch {
            placement: placement.clone(),
            changes: vec![
                Change::InsertPart {
                    part: "user/tools".into(),
                    definition: AddedPart {
                        parent: "root".into(),
                        component: None,
                        size: [100.0; 2],
                    },
                },
                Change::SetPart {
                    part: "root".into(),
                    edit: PartEdit {
                        tokens: BTreeMap::from([("Width".into(), TokenValue::Number(720.0))]),
                        ..Default::default()
                    },
                },
            ],
        },
    );
    let first = state.handle(&patch).unwrap();
    assert_eq!(state.handle(&patch).unwrap(), first);
    assert_eq!(
        state.snapshot.placements[0].component.records(),
        vec![record.as_str()]
    );
    assert_eq!(state.snapshot.placements[0].geometry.size[0], 720.0);
    let mut restored: api::State =
        serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.handle(&patch).unwrap(), first);
    let api::Response::Export {
        document,
        hash: exported,
        revision,
    } = restored
        .handle(&api::Request::Export {
            placement,
            name: "Workflow".into(),
        })
        .unwrap()
    else {
        panic!("export");
    };
    assert_eq!(revision, 1);
    assert_eq!(hash(&document).unwrap(), exported);
    assert_eq!(edits(&document.component).size, Some([720.0, 400.0]));
}

#[test]
fn ownership_cycles_and_invalid_dimensions_reject_the_entire_batch() {
    let (mut state, placement, _) = fixture();
    let before = state.snapshot.clone();
    let operation = request(
        &state,
        api::Mutation::Patch {
            placement: placement.clone(),
            changes: vec![
                Change::InsertPart {
                    part: "user/a".into(),
                    definition: AddedPart {
                        parent: "user/b".into(),
                        component: None,
                        size: [100.0; 2],
                    },
                },
                Change::InsertPart {
                    part: "user/b".into(),
                    definition: AddedPart {
                        parent: "user/a".into(),
                        component: None,
                        size: [100.0; 2],
                    },
                },
            ],
        },
    );
    assert!(state.handle(&operation).is_err());
    assert_eq!(state.snapshot, before);
    let operation = request(
        &state,
        api::Mutation::Patch {
            placement,
            changes: vec![Change::SetPart {
                part: "root".into(),
                edit: PartEdit {
                    style: Style {
                        numbers: BTreeMap::from([("flex-grow".into(), -1.0)]),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            }],
        },
    );
    assert!(state.handle(&operation).is_err());
    assert_eq!(state.snapshot, before);
}

#[test]
fn duplicate_rebind_group_remove_and_undo_leave_the_original_intact() {
    let (mut state, placement, record) = fixture();
    let copy = nucleus::new_uid("placement");
    let duplicate = request(
        &state,
        api::Mutation::Duplicate {
            placement: placement.clone(),
            copy: copy.clone(),
            workspace: 1,
            position: [400.0, 300.0],
        },
    );
    state.handle(&duplicate).unwrap();
    let replacement = nucleus::new_uid("r");
    let rebind = request(
        &state,
        api::Mutation::Rebind {
            placement: copy.clone(),
            records: BTreeMap::from([(record.clone(), replacement.clone())]),
        },
    );
    state.handle(&rebind).unwrap();
    assert_eq!(
        state.snapshot.placements[0].component.records(),
        vec![record.as_str()]
    );
    assert_eq!(
        state.snapshot.placements[1].component.records(),
        vec![replacement.as_str()]
    );
    let group = request(
        &state,
        api::Mutation::Group {
            placements: vec![placement.clone(), copy.clone()],
            group: Some("01".repeat(16)),
        },
    );
    state.handle(&group).unwrap();
    let remove = request(&state, api::Mutation::Remove { placement: copy });
    let api::Response::Receipt { receipt } = state.handle(&remove).unwrap() else {
        panic!("receipt");
    };
    let undo = request(
        &state,
        api::Mutation::Undo {
            receipt: receipt.request_id,
        },
    );
    state.handle(&undo).unwrap();
    assert_eq!(state.snapshot.placements.len(), 2);
    let stale = api::Request::Mutate {
        request_id: nucleus::new_uid("request"),
        expected_revision: 0,
        mutation: api::Mutation::Remove { placement },
    };
    assert!(state.handle(&stale).is_err());
}

#[test]
fn rebinding_keeps_literal_text_and_action_parameters_unchanged() {
    let first = nucleus::new_uid("r");
    let second = nucleus::new_uid("r");
    let component = api::Component::Composition {
        composition: api::Composition {
            name: "Binding".into(),
            origin: None,
            parts: vec![
                api::Part {
                    id: "record".into(),
                    component: api::Component::Builtin {
                        state: ComponentState::Record {
                            record: first.clone(),
                            mode: Default::default(),
                            start_call: None,
                        },
                    },
                    geometry: api::Geometry {
                        position: [0.0; 2],
                        size: [100.0; 2],
                    },
                    events: Vec::new(),
                },
                api::Part {
                    id: "text".into(),
                    component: api::Component::Builtin {
                        state: ComponentState::Text {
                            text: first.clone(),
                        },
                    },
                    geometry: api::Geometry {
                        position: [0.0; 2],
                        size: [100.0; 2],
                    },
                    events: Vec::new(),
                },
            ],
        },
    };
    let rebound = rebind(
        &component,
        &BTreeMap::from([(first.clone(), second.clone())]),
    )
    .unwrap();
    assert_eq!(rebound.records(), vec![second.as_str()]);
    let api::Component::Composition { composition } = rebound else {
        panic!("composition");
    };
    assert_eq!(
        composition.parts[1].component,
        api::Component::Builtin {
            state: ComponentState::Text { text: first }
        }
    );
}
