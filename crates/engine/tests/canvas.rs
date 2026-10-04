use engine::{Engine, actions::Action};
use nucleus::canvas::{
    Component, Geometry, Mutation, Placement, Request, Response, Snapshot, State, Workspace,
};
use std::sync::Arc;
use tokio::sync::Mutex;

fn initial() -> State {
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
            placements: Vec::<Placement>::new(),
            next_offset: None,
        },
        vec![],
    )
    .unwrap()
}

#[tokio::test]
async fn canvas_actions_require_a_host_and_exact_acknowledgements() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    assert!(
        engine
            .act(
                Action::Canvas {
                    canvas: None,
                    request: Request::Registry
                },
                None
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("No local canvas")
    );
    let canvas = nucleus::new_uid("canvas");
    let mut receiver = engine
        .register_canvas(canvas.clone(), "Desktop".into(), vec![])
        .unwrap();
    assert!(
        engine
            .register_canvas(canvas.clone(), "Duplicate".into(), vec![])
            .is_err()
    );
    let request_id = nucleus::new_uid("request");
    let action = Action::Canvas {
        canvas: Some(canvas),
        request: Request::Mutate {
            request_id: request_id.clone(),
            expected_revision: 0,
            mutation: Mutation::CreateWorkspace {
                name: "Second".into(),
            },
        },
    };
    let task = tokio::spawn({
        let engine = engine.clone();
        let action = action.clone();
        async move { engine.act(action, None).await }
    });
    let call = receiver.recv().await.unwrap();
    assert!(!task.is_finished());
    call.complete(Ok(Response::Receipt {
        receipt: nucleus::canvas::Receipt {
            request_id: "wrong".into(),
            revision: 1,
            persistence: nucleus::canvas::Persistence::Saved,
            affected_placements: vec![],
            affected_count: 0,
            workspace: Some(2),
        },
    }))
    .unwrap();
    assert!(
        task.await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("invalid mutation acknowledgement")
    );
    drop(receiver);
    assert!(engine.connected_canvases().is_empty());
}

#[tokio::test]
async fn canvas_crud_runs_through_actions_and_deduplicates_without_deleting_records() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let record = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Habits".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let state = Arc::new(Mutex::new(initial()));
    let mut receiver = engine
        .register_canvas(nucleus::new_uid("canvas"), "Desktop".into(), vec![])
        .unwrap();
    let worker = tokio::spawn({
        let state = state.clone();
        async move {
            while let Some(call) = receiver.recv().await {
                if call.is_cancelled() {
                    continue;
                }
                let mut state = state.lock().await;
                let result = state.handle(&call.request);
                call.complete(result).unwrap();
            }
        }
    });
    let placement = nucleus::new_uid("placement");
    let request_id = nucleus::new_uid("request");
    let add = Action::Canvas {
        canvas: None,
        request: Request::Mutate {
            request_id: request_id.clone(),
            expected_revision: 0,
            mutation: Mutation::Add {
                placement: placement.clone(),
                workspace: 1,
                component: Component::Builtin {
                    state: nucleus::component::ComponentState::Record {
                        record: record.clone(),
                        mode: Default::default(),
                        start_call: None,
                    },
                },
                geometry: Geometry {
                    position: [0.0, 0.0],
                    size: [200.0, 150.0],
                },
            },
        },
    };
    let first = engine.act(add.clone(), None).await.unwrap().data;
    assert_eq!(engine.act(add, None).await.unwrap().data, first);
    assert_eq!(state.lock().await.snapshot.placements.len(), 1);
    let wrong = Action::Canvas {
        canvas: None,
        request: Request::Mutate {
            request_id,
            expected_revision: 0,
            mutation: Mutation::Remove {
                placement: placement.clone(),
            },
        },
    };
    assert!(
        engine
            .act(wrong, None)
            .await
            .unwrap_err()
            .to_string()
            .contains("different parameters")
    );
    let remove = Action::Canvas {
        canvas: None,
        request: Request::Mutate {
            request_id: nucleus::new_uid("request"),
            expected_revision: 1,
            mutation: Mutation::Remove { placement },
        },
    };
    engine.act(remove, None).await.unwrap();
    assert!(state.lock().await.snapshot.placements.is_empty());
    assert!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .is_some()
    );
    worker.abort();
}

#[tokio::test]
async fn fiote_scope_can_be_revoked_before_canvas_application() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let mut receiver = engine
        .register_canvas(nucleus::new_uid("canvas"), "Desktop".into(), vec![])
        .unwrap();
    let agent = nucleus::new_uid("r");
    let thread = nucleus::new_uid("r");
    let task = tokio::spawn({
        let engine = engine.clone();
        let agent = agent.clone();
        let thread = thread.clone();
        async move {
            engine::operation_origin::fiote(
                &agent,
                &thread,
                engine.act(
                    Action::Canvas {
                        canvas: None,
                        request: Request::Mutate {
                            request_id: nucleus::new_uid("request"),
                            expected_revision: 0,
                            mutation: Mutation::CreateWorkspace {
                                name: "Second".into(),
                            },
                        },
                    },
                    None,
                ),
            )
            .await
        }
    });
    let call = receiver.recv().await.unwrap();
    assert!(call.context.origin.is_some());
    engine.revoke_canvas_requests(&agent, &thread);
    assert!(call.is_cancelled());
    assert!(call.complete(Err("Revoked".into())).is_err());
    assert!(task.await.unwrap().is_err());
}

#[tokio::test]
async fn configuring_an_existing_user_placement_preserves_its_container() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let state = Arc::new(Mutex::new(initial()));
    let mut receiver = engine
        .register_canvas(nucleus::new_uid("canvas"), "Desktop".into(), vec![])
        .unwrap();
    let worker = tokio::spawn({
        let state = state.clone();
        async move {
            while let Some(call) = receiver.recv().await {
                let result = state.lock().await.handle(&call.request);
                call.complete(result).unwrap();
            }
        }
    });
    let placement = nucleus::new_uid("placement");
    let component = Component::Builtin {
        state: nucleus::component::ComponentState::Text {
            text: "My panel".into(),
        },
    };
    engine
        .act(
            Action::Canvas {
                canvas: None,
                request: Request::Mutate {
                    request_id: nucleus::new_uid("request"),
                    expected_revision: 0,
                    mutation: Mutation::Add {
                        placement: placement.clone(),
                        workspace: 1,
                        component: component.clone(),
                        geometry: Geometry {
                            position: [0.0, 0.0],
                            size: [200.0, 150.0],
                        },
                    },
                },
            },
            None,
        )
        .await
        .unwrap();
    let agent = nucleus::new_uid("r");
    let thread = nucleus::new_uid("r");
    engine::operation_origin::fiote(
        &agent,
        &thread,
        engine.act(
            Action::Canvas {
                canvas: None,
                request: Request::Mutate {
                    request_id: nucleus::new_uid("request"),
                    expected_revision: 1,
                    mutation: Mutation::Configure {
                        placement,
                        component,
                    },
                },
            },
            None,
        ),
    )
    .await
    .unwrap();
    assert!(
        state.lock().await.snapshot.placements[0]
            .component
            .origin()
            .is_none()
    );
    worker.abort();
}

#[tokio::test]
async fn canvas_targets_are_explicit_and_nonlocal_actors_are_refused() {
    let engine = Engine::open_memory().await.unwrap();
    let _first = engine
        .register_canvas(nucleus::new_uid("canvas"), "First".into(), vec![])
        .unwrap();
    let _second = engine
        .register_canvas(nucleus::new_uid("canvas"), "Second".into(), vec![])
        .unwrap();
    assert!(
        engine
            .act(
                Action::Canvas {
                    canvas: None,
                    request: Request::Registry
                },
                None
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("more than one")
    );
    let action = Action::InspectCanvases;
    let result = engine.act(action, Some(nucleus::new_uid("r"))).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn registered_native_compositions_save_with_platform_owned_provenance() {
    let engine = Engine::open_memory().await.unwrap();
    store::organs::ensure_local(&engine.store.pool, "http://canvas.test")
        .await
        .unwrap();
    let descriptor = nucleus::canvas::Descriptor {
        kind: "clock".into(),
        name: "Clock".into(),
        description: "Existing native clock".into(),
        settings: std::collections::BTreeMap::new(),
        max_bindings: 1,
        placeable: true,
        configurable: true,
        composable: true,
        unavailable_reason: None,
    };
    let _receiver = engine
        .register_canvas(
            nucleus::new_uid("canvas"),
            "Desktop".into(),
            vec![descriptor],
        )
        .unwrap();
    let missing = nucleus::canvas::Document::encode(
        "Missing".into(),
        Component::Native {
            kind: "clock".into(),
            settings: std::collections::BTreeMap::new(),
            bindings: vec![nucleus::new_uid("r")],
        },
    )
    .unwrap();
    assert!(
        engine
            .act(
                Action::CreateCustomComponent {
                    head: "Missing".into(),
                    body: missing,
                },
                None
            )
            .await
            .is_err()
    );
    let component = Component::Composition {
        composition: nucleus::canvas::Composition {
            name: "Daily".into(),
            origin: None,
            parts: vec![nucleus::canvas::Part {
                id: "clock".into(),
                geometry: Geometry {
                    position: [0.0, 0.0],
                    size: [100.0, 100.0],
                },
                component: Component::Native {
                    kind: "clock".into(),
                    settings: std::collections::BTreeMap::new(),
                    bindings: vec![],
                },
                events: vec![],
            }],
        },
    };
    let agent = nucleus::new_uid("r");
    let thread = nucleus::new_uid("r");
    let body = nucleus::canvas::Document::encode("Daily".into(), component).unwrap();
    let result = engine::operation_origin::fiote(
        &agent,
        &thread,
        engine.act(
            Action::CreateCustomComponent {
                head: "Daily".into(),
                body,
            },
            None,
        ),
    )
    .await
    .unwrap();
    let uid = result.created.unwrap();
    let row = store::records::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let document = nucleus::canvas::Document::decode(&row.body).unwrap();
    assert_eq!(document.component.origin().unwrap().agent, agent);
    assert_eq!(document.component.origin().unwrap().thread, thread);
    let mut unprotected = document.component;
    if let Component::Composition { composition } = &mut unprotected {
        composition.origin = None;
    }
    let body = nucleus::canvas::Document::encode("Daily".into(), unprotected).unwrap();
    let result = engine
        .act(
            Action::EditRecordText {
                target: uid,
                head: None,
                body: Some(body),
            },
            None,
        )
        .await;
    assert!(result.is_err());
}
