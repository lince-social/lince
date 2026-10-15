use nucleus::{
    canvas::{
        Component, Geometry, Metadata, Mutation, Request, Response, Snapshot, State, Workspace,
    },
    component::{ComponentState, RecordMode},
    terrain::{Anchor, SpatialBinding},
};

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
            placements: Vec::new(),
            next_offset: None,
        },
        Vec::new(),
    )
    .unwrap()
}

fn mutate(state: &mut State, mutation: Mutation) -> Result<String, String> {
    match state.handle(&Request::Mutate {
        request_id: nucleus::new_uid("request"),
        expected_revision: state.snapshot.revision,
        mutation,
    })? {
        Response::Receipt { receipt } => Ok(receipt.request_id),
        _ => panic!("Expected receipt"),
    }
}

fn add(state: &mut State) -> String {
    let id = nucleus::new_uid("placement");
    mutate(
        state,
        Mutation::Add {
            placement: id.clone(),
            workspace: 1,
            component: Component::Builtin {
                state: ComponentState::Record {
                    record: nucleus::new_uid("r"),
                    mode: RecordMode::default(),
                    start_call: None,
                },
            },
            geometry: Geometry {
                position: [0.0; 2],
                size: [100.0; 2],
            },
        },
    )
    .unwrap();
    id
}

fn binding(anchor: Anchor) -> Metadata {
    Metadata {
        binding: Some(SpatialBinding {
            anchor,
            position: [10.0, 20.0, 30.0],
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[test]
fn cycles_are_rejected_atomically() {
    let mut state = state();
    let a = add(&mut state);
    let b = add(&mut state);
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: a.clone(),
            metadata: binding(Anchor::Item(b.clone())),
        },
    )
    .unwrap();
    let before = state.snapshot.clone();
    assert!(
        mutate(
            &mut state,
            Mutation::SetMetadata {
                placement: b,
                metadata: binding(Anchor::Item(a))
            }
        )
        .is_err()
    );
    assert_eq!(state.snapshot, before);
}

#[test]
fn removing_anchor_freezes_dependents_and_scoped_undo_restores_them_after_motion() {
    let mut state = state();
    let a = add(&mut state);
    let b = add(&mut state);
    let unrelated = add(&mut state);
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: b.clone(),
            metadata: binding(Anchor::Item(a.clone())),
        },
    )
    .unwrap();
    let receipt = mutate(
        &mut state,
        Mutation::Remove {
            placement: a.clone(),
        },
    )
    .unwrap();
    let dependent = state
        .snapshot
        .placements
        .iter()
        .find(|p| p.id == b)
        .unwrap()
        .metadata
        .binding
        .as_ref()
        .unwrap();
    assert_eq!(dependent.anchor, Anchor::Fixed);
    assert_eq!(dependent.position, [10.0, 20.0, 30.0]);
    let mut live = state.snapshot.clone();
    live.placements
        .iter_mut()
        .find(|p| p.id == unrelated)
        .unwrap()
        .geometry
        .position = [400.0, 500.0];
    state.synchronize(live).unwrap();
    let undo = mutate(&mut state, Mutation::UndoChanges { receipt }).unwrap();
    assert!(state.snapshot.placements.iter().any(|p| p.id == a));
    assert_eq!(
        state
            .snapshot
            .placements
            .iter()
            .find(|p| p.id == b)
            .unwrap()
            .metadata
            .binding
            .as_ref()
            .unwrap()
            .anchor,
        Anchor::Item(a.clone())
    );
    assert_eq!(
        state
            .snapshot
            .placements
            .iter()
            .find(|p| p.id == unrelated)
            .unwrap()
            .geometry
            .position,
        [400.0, 500.0]
    );
    mutate(&mut state, Mutation::UndoChanges { receipt: undo }).unwrap();
    assert!(!state.snapshot.placements.iter().any(|p| p.id == a));
    let restored: State = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    restored.validate().unwrap();
}

#[test]
fn scoped_undo_preserves_camera_motion_but_rejects_later_authored_changes() {
    let mut state = state();
    let a = add(&mut state);
    let receipt = mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: a.clone(),
            metadata: binding(Anchor::Camera),
        },
    )
    .unwrap();
    let mut live = state.snapshot.clone();
    live.placements[0].geometry.position = [100.0, 200.0];
    live.placements[0]
        .metadata
        .binding
        .as_mut()
        .unwrap()
        .position = [100.0, 20.0, 200.0];
    state.synchronize(live).unwrap();
    mutate(&mut state, Mutation::UndoChanges { receipt }).unwrap();
    assert_eq!(
        state.snapshot.placements[0].geometry.position,
        [100.0, 200.0]
    );
    assert!(state.snapshot.placements[0].metadata.binding.is_none());
    let receipt = mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: a.clone(),
            metadata: binding(Anchor::Camera),
        },
    )
    .unwrap();
    let mut metadata = state.snapshot.placements[0].metadata.clone();
    metadata.binding.as_mut().unwrap().offset[0] = 40.0;
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: a,
            metadata,
        },
    )
    .unwrap();
    let before = state.snapshot.clone();
    assert!(mutate(&mut state, Mutation::UndoChanges { receipt }).is_err());
    assert_eq!(state.snapshot, before);
}

#[test]
fn moving_and_duplicating_a_bound_item_preserves_its_canvas_offset() {
    let mut state = state();
    let id = add(&mut state);
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: id.clone(),
            metadata: binding(Anchor::Camera),
        },
    )
    .unwrap();
    mutate(
        &mut state,
        Mutation::Move {
            placement: id.clone(),
            workspace: 1,
            position: [30.0, 40.0],
        },
    )
    .unwrap();
    assert_eq!(
        state.snapshot.placements[0]
            .metadata
            .binding
            .as_ref()
            .unwrap()
            .offset,
        [30.0, 0.0, 40.0]
    );
    let copy = nucleus::new_uid("placement");
    mutate(
        &mut state,
        Mutation::Duplicate {
            placement: id,
            copy: copy.clone(),
            workspace: 1,
            position: [50.0, 80.0],
        },
    )
    .unwrap();
    assert_eq!(
        state
            .snapshot
            .placements
            .iter()
            .find(|p| p.id == copy)
            .unwrap()
            .metadata
            .binding
            .as_ref()
            .unwrap()
            .offset,
        [50.0, 0.0, 80.0]
    );
}

#[test]
fn orientation_following_offsets_translate_in_the_anchor_frame() {
    let angle = std::f64::consts::FRAC_PI_4;
    let mut binding = SpatialBinding {
        anchor: Anchor::Camera,
        follow_orientation: true,
        rotation: [0.0, angle.sin(), 0.0, angle.cos()],
        ..Default::default()
    };
    binding.translate([0.0, 0.0, -40.0]);
    assert!((binding.offset[0] - 40.0).abs() < 1e-6);
    assert!(binding.offset[2].abs() < 1e-6);
}

#[test]
fn appearance_metadata_roundtrips_and_invalid_updates_are_atomic() {
    use nucleus::canvas::appearance::{Light, Material, ShadowQuality, Texture};
    let mut state = state();
    let id = add(&mut state);
    let metadata = Metadata {
        material: Some(Material {
            lit: true,
            texture: Texture::Asset {
                id: "0123456789abcdef0123456789abcdef".into(),
            },
            ..Default::default()
        }),
        light: Some(Light {
            shadows: ShadowQuality::High,
            ..Default::default()
        }),
        ..Default::default()
    };
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: id.clone(),
            metadata: metadata.clone(),
        },
    )
    .unwrap();
    let restored: State = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    restored.validate().unwrap();
    assert_eq!(restored.snapshot.placements[0].metadata, metadata);
    let before = state.snapshot.clone();
    let mut invalid = Vec::new();
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        let mut draft = metadata.clone();
        draft.material.as_mut().unwrap().color[0] = value;
        invalid.push(draft);
    }
    for value in [f32::NAN, -1.0, 100_001.0] {
        let mut draft = metadata.clone();
        draft.light.as_mut().unwrap().intensity = value;
        invalid.push(draft);
    }
    for value in [
        "../source.png",
        "https://example.com/a",
        "0123456789abcdef0123456789abcdeg",
    ] {
        let mut draft = metadata.clone();
        draft.material.as_mut().unwrap().texture = Texture::Asset { id: value.into() };
        invalid.push(draft);
    }
    for draft in invalid {
        assert!(
            mutate(
                &mut state,
                Mutation::SetMetadata {
                    placement: id.clone(),
                    metadata: draft
                }
            )
            .is_err()
        );
        assert_eq!(state.snapshot, before);
    }
    mutate(
        &mut state,
        Mutation::SetMetadata {
            placement: id,
            metadata: Metadata::default(),
        },
    )
    .unwrap();
    assert!(state.snapshot.placements[0].metadata.material.is_none());
    assert!(state.snapshot.placements[0].metadata.light.is_none());
}
