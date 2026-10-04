use engine::{
    Engine,
    actions::Action,
    workspace_sync::{Change, Client, Command, Element, Request},
};
use nucleus::{
    canvas::{Component, Geometry},
    component::ComponentState,
};
use serde_json::{Value, json};

async fn act(
    engine: &Engine,
    command: Command,
    actor: Option<&str>,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    engine
        .act(
            Action::Workspace {
                request: Request {
                    client: Client::default(),
                    command,
                },
            },
            actor.map(str::to_owned),
        )
        .await
}

fn policy(quantity: bool) -> Value {
    json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":if quantity { vec![json!({"operation":"update","selector":{"kind_eq":"plain"},"properties":["quantity"],"assertions_add":[],"assertions_remove":[]})] } else { vec![] }}})
}

async fn fixture() -> (Engine, String, String, String) {
    let engine = Engine::open_memory().await.unwrap();
    let record = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Work".into(),
                body: "".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Guest",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&engine.store.pool, "Editor")
        .await
        .unwrap();
    for key in ["record:read", "workspace:read", "workspace:update"] {
        let (subject, operation) = key.split_once(':').unwrap();
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, operation)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["Editor".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let workspace = act(
        &engine,
        Command::Create {
            name: "Joint work".into(),
            policy: policy(true),
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap();
    (engine, workspace, record, person)
}

async fn revision(engine: &Engine, workspace: &str) -> i64 {
    act(
        engine,
        Command::Inspect {
            workspace: workspace.into(),
            permitted_view: false,
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap()["revision"]
        .as_i64()
        .unwrap()
}

async fn propose(engine: &Engine, workspace: &str, change: Change, actor: Option<&str>) -> Value {
    act(
        engine,
        Command::Propose {
            workspace: workspace.into(),
            request_id: nucleus::new_uid("request"),
            base_revision: revision(engine, workspace).await,
            change,
        },
        actor,
    )
    .await
    .unwrap()
    .data
    .unwrap()
}

fn element(component: ComponentState, position: [f64; 2], size: [f64; 2]) -> Element {
    Element {
        id: nucleus::new_uid("placement"),
        component: Component::Builtin { state: component },
        geometry: Geometry { position, size },
    }
}

#[tokio::test]
async fn shared_area_assignments_cannot_delegate_local_fiote_authority() {
    let (engine, workspace, record, _) = fixture().await;
    engine
        .act(
            Action::ConfigureFiote {
                target: record.clone(),
                prompt_parent: None,
                run_assigned: true,
            },
            None,
        )
        .await
        .unwrap();
    let area = element(
        ComponentState::Area {
            immunity: Default::default(),
            strength: 0,
        },
        [0.0; 2],
        [100.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: area.clone(),
        },
        None,
    )
    .await;
    let pending = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision: revision(&engine, &workspace).await,
            change: Change::Area {
                element: area.id,
                changes: engine::area_transition::RecordChanges {
                    assign: vec![record],
                    ..Default::default()
                },
            },
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(pending["state"], "pending");
    let before = revision(&engine, &workspace).await;
    let error = act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: before,
            approve: true,
        },
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Fiote"), "{error}");
    assert_eq!(revision(&engine, &workspace).await, before);
    let pending: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM fiote_activation")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(pending, 0);
}

#[tokio::test]
async fn presentation_is_shared_but_area_effects_never_lend_authority() {
    let (engine, workspace, record, person) = fixture().await;
    let area = element(
        ComponentState::Area {
            immunity: Default::default(),
            strength: 0,
        },
        [0.0; 2],
        [100.0; 2],
    );
    let record_element = element(
        ComponentState::Record {
            record: record.clone(),
            mode: Default::default(),
            start_call: None,
        },
        [200.0, 0.0],
        [30.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: area.clone(),
        },
        None,
    )
    .await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: record_element.clone(),
        },
        None,
    )
    .await;
    let pending = propose(
        &engine,
        &workspace,
        Change::Area {
            element: area.id.clone(),
            changes: engine::area_transition::RecordChanges {
                quantity: Some("+=1".into()),
                ..Default::default()
            },
        },
        None,
    )
    .await;
    assert_eq!(pending["state"], "pending");
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    let prior = revision(&engine, &workspace).await;
    let request = Command::Propose {
        workspace: workspace.clone(),
        request_id: nucleus::new_uid("request"),
        base_revision: prior,
        change: Change::Move {
            element: record_element.id.clone(),
            position: [0.0; 2],
        },
    };
    assert!(act(&engine, request.clone(), Some(&person)).await.is_err());
    assert_eq!(revision(&engine, &workspace).await, prior);
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::zero()
    );
    let role = store::auth::role_by_name(&engine.store.pool, "Editor")
        .await
        .unwrap()
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let result = act(&engine, request.clone(), Some(&person)).await.unwrap();
    assert_eq!(result.data.as_ref().unwrap()["state"], "applied");
    let replay = act(&engine, request, Some(&person)).await.unwrap();
    assert_eq!(replay.data, result.data);
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::one()
    );
    let snapshot = engine
        .workspace_view(Some(&person), &workspace, &Client::default(), false)
        .await
        .unwrap();
    assert_eq!(
        snapshot["layout"]["elements"][1]["geometry"]["position"],
        json!([0.0, 0.0])
    );
}

#[tokio::test]
async fn admission_separate_views_revocation_and_review_are_explicit() {
    let (engine, workspace, record, person) = fixture().await;
    let record_element = element(
        ComponentState::Record {
            record: record.clone(),
            mode: Default::default(),
            start_call: None,
        },
        [200.0, 0.0],
        [30.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: record_element,
        },
        None,
    )
    .await;
    let role = store::auth::role_by_name(&engine.store.pool, "Editor")
        .await
        .unwrap()
        .unwrap();
    store::role_policies::set(
        &engine.store.pool,
        role,
        &json!({"read":{"any":[]},"grants":[]}),
        0,
    )
    .await
    .unwrap();
    let error = engine
        .workspace_view(Some(&person), &workspace, &Client::default(), false)
        .await
        .unwrap_err();
    assert_eq!(error.code(), Some("workspace_missing_read_access"));
    let separate = engine
        .workspace_view(Some(&person), &workspace, &Client::default(), true)
        .await
        .unwrap();
    assert_eq!(separate["separate_view"], true);
    assert_ne!(separate["uid"], workspace);
    assert_eq!(separate["layout"]["elements"], json!([]));
    assert_eq!(separate["can_edit"], false);
    propose(
        &engine,
        &workspace,
        Change::Rename {
            name: "Current".into(),
        },
        None,
    )
    .await;
    let stale = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision: 1,
            change: Change::Rename {
                name: "Draft".into(),
            },
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(stale["state"], "pending");
    let mut restricted = policy(false);
    restricted["required_capabilities"] = json!(["workspace:access_control"]);
    let pending = propose(
        &engine,
        &workspace,
        Change::Policy { policy: restricted },
        None,
    )
    .await;
    assert!(
        act(
            &engine,
            Command::Review {
                workspace: workspace.clone(),
                proposal: pending["proposal"].as_str().unwrap().into(),
                request_id: nucleus::new_uid("request"),
                expected_revision: revision(&engine, &workspace).await,
                approve: true
            },
            Some(&person)
        )
        .await
        .is_err()
    );
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        engine
            .workspace_view(Some(&person), &workspace, &Client::default(), true)
            .await
            .is_err()
    );
    let incompatible = Client {
        schema: 99,
        ..Default::default()
    };
    assert_eq!(
        engine
            .workspace_view(None, &workspace, &incompatible, false)
            .await
            .unwrap_err()
            .code(),
        Some("workspace_client_incompatible")
    );
}

#[tokio::test]
async fn independent_edits_merge_and_workspace_audiences_limit_topology_authority() {
    let (engine, workspace, _, person) = fixture().await;
    let first = element(
        ComponentState::Text { text: "One".into() },
        [0.0; 2],
        [100.0; 2],
    );
    let second = element(
        ComponentState::Text { text: "Two".into() },
        [200.0, 0.0],
        [100.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: first.clone(),
        },
        None,
    )
    .await;
    let accepted = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision: 1,
            change: Change::Add { element: second },
        },
        Some(&person),
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(accepted["state"], "applied");
    let base = revision(&engine, &workspace).await;
    propose(
        &engine,
        &workspace,
        Change::Move {
            element: first.id.clone(),
            position: [10.0, 0.0],
        },
        None,
    )
    .await;
    let conflict = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision: base,
            change: Change::Move {
                element: first.id,
                position: [20.0, 0.0],
            },
        },
        Some(&person),
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(conflict["state"], "pending");
    let mut restricted = policy(false);
    restricted["editors"] = json!({"everyone":false,"actors":[],"roles":[]});
    let pending = propose(
        &engine,
        &workspace,
        Change::Policy { policy: restricted },
        None,
    )
    .await;
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    let view = engine
        .workspace_view(Some(&person), &workspace, &Client::default(), false)
        .await
        .unwrap();
    assert_eq!(view["can_edit"], false);
    assert!(
        act(
            &engine,
            Command::Propose {
                workspace,
                request_id: nucleus::new_uid("request"),
                base_revision: view["revision"].as_i64().unwrap(),
                change: Change::Rename {
                    name: "Denied".into()
                }
            },
            Some(&person)
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn durable_history_receipts_and_inert_personal_drafts_survive_reopening() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("host.sqlite").display()
    );
    let engine = Engine::open(&url).await.unwrap();
    let workspace = act(
        &engine,
        Command::Create {
            name: "Persistent".into(),
            policy: policy(false),
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap();
    let request = Command::Propose {
        workspace: workspace.clone(),
        request_id: nucleus::new_uid("request"),
        base_revision: 1,
        change: Change::Rename {
            name: "Committed".into(),
        },
    };
    let accepted = act(&engine, request.clone(), None).await.unwrap().data;
    let draft = nucleus::new_uid("draft");
    act(
        &engine,
        Command::SaveDraft {
            uid: draft.clone(),
            host: None,
            workspace: workspace.clone(),
            base_revision: 1,
            change: Change::Rename {
                name: "Offline".into(),
            },
        },
        None,
    )
    .await
    .unwrap();
    let reopened = Engine::open(&url).await.unwrap();
    assert_eq!(act(&reopened, request, None).await.unwrap().data, accepted);
    assert_eq!(revision(&reopened, &workspace).await, 2);
    let drafts = act(&reopened, Command::Drafts, None)
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(drafts["drafts"][0]["uid"], draft);
    assert_eq!(drafts["drafts"][0]["base_revision"], 1);
    let history = act(
        &reopened,
        Command::History {
            workspace,
            before: None,
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(history["changes"].as_array().unwrap().len(), 1);
    act(&reopened, Command::DiscardDraft { uid: draft }, None)
        .await
        .unwrap();
    assert_eq!(
        act(&reopened, Command::Drafts, None)
            .await
            .unwrap()
            .data
            .unwrap()["drafts"],
        json!([])
    );
}

#[tokio::test]
async fn record_creation_and_deletion_require_review_and_workspace_grants() {
    let (engine, workspace, _, _) = fixture().await;
    let mut broad = policy(true);
    for operation in ["create", "delete"] {
        broad["ceiling"]["grants"].as_array_mut().unwrap().push(json!({"operation":operation,"selector":{"kind_eq":"plain"},"properties":["kind","organ","head","body","quantity","slug"],"assertions_add":[],"assertions_remove":[]}));
    }
    let pending = propose(&engine, &workspace, Change::Policy { policy: broad }, None).await;
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    let draft = engine::record_creation::Draft {
        head: "New work".into(),
        quantity: "2".into(),
        ..Default::default()
    };
    let pending = propose(
        &engine,
        &workspace,
        Change::CreateRecord {
            draft: draft.clone(),
            placement: nucleus::new_uid("placement"),
            geometry: Geometry {
                position: [200.0, 0.0],
                size: [100.0; 2],
            },
        },
        None,
    )
    .await;
    assert!(
        store::records::get(&engine.store.pool, &draft.uid)
            .await
            .unwrap()
            .is_none()
    );
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::records::get(&engine.store.pool, &draft.uid)
            .await
            .unwrap()
            .unwrap()
            .head,
        "New work"
    );
    let pending = propose(
        &engine,
        &workspace,
        Change::DeleteRecord {
            record: draft.uid.clone(),
        },
        None,
    )
    .await;
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        store::records::get(&engine.store.pool, &draft.uid)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        engine
            .workspace_view(None, &workspace, &Client::default(), false)
            .await
            .unwrap()["layout"]["elements"],
        json!([])
    );
}

#[tokio::test]
async fn workspace_ceiling_rolls_back_data_and_topology_together() {
    let (engine, workspace, record, _) = fixture().await;
    let mut bounded = policy(true);
    bounded["ceiling"]["grants"][0]["selector"] = json!({"quantity_lte":"0"});
    let pending = propose(
        &engine,
        &workspace,
        Change::Policy { policy: bounded },
        None,
    )
    .await;
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    let area = element(
        ComponentState::Area {
            immunity: Default::default(),
            strength: 0,
        },
        [0.0; 2],
        [100.0; 2],
    );
    let record_element = element(
        ComponentState::Record {
            record: record.clone(),
            mode: Default::default(),
            start_call: None,
        },
        [200.0, 0.0],
        [30.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: area.clone(),
        },
        None,
    )
    .await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: record_element.clone(),
        },
        None,
    )
    .await;
    let pending = propose(
        &engine,
        &workspace,
        Change::Area {
            element: area.id,
            changes: engine::area_transition::RecordChanges {
                quantity: Some("+=1".into()),
                ..Default::default()
            },
        },
        None,
    )
    .await;
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    let prior = revision(&engine, &workspace).await;
    assert!(
        act(
            &engine,
            Command::Propose {
                workspace: workspace.clone(),
                request_id: nucleus::new_uid("request"),
                base_revision: prior,
                change: Change::Move {
                    element: record_element.id,
                    position: [0.0; 2]
                }
            },
            None
        )
        .await
        .is_err()
    );
    assert_eq!(revision(&engine, &workspace).await, prior);
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::zero()
    );
}

async fn review(
    engine: &Engine,
    workspace: &str,
    proposal: &str,
    approve: bool,
) -> Result<engine::actions::ActionOutcome, engine::EngineError> {
    act(
        engine,
        Command::Review {
            workspace: workspace.into(),
            proposal: proposal.into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(engine, workspace).await,
            approve,
        },
        None,
    )
    .await
}

#[tokio::test]
async fn moving_an_area_previews_and_commits_all_crossings_once() {
    let (engine, workspace, first, _) = fixture().await;
    let second = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Second".into(),
                body: "".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let area = element(
        ComponentState::Area {
            immunity: Default::default(),
            strength: 0,
        },
        [0.0; 2],
        [100.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: area.clone(),
        },
        None,
    )
    .await;
    for (record, position) in [(&first, [200.0, 0.0]), (&second, [250.0, 0.0])] {
        propose(
            &engine,
            &workspace,
            Change::Add {
                element: element(
                    ComponentState::Record {
                        record: record.clone(),
                        mode: Default::default(),
                        start_call: None,
                    },
                    position,
                    [20.0; 2],
                ),
            },
            None,
        )
        .await;
    }
    let recipe = propose(
        &engine,
        &workspace,
        Change::Area {
            element: area.id.clone(),
            changes: engine::area_transition::RecordChanges {
                quantity: Some("+=1".into()),
                ..Default::default()
            },
        },
        None,
    )
    .await;
    review(
        &engine,
        &workspace,
        recipe["proposal"].as_str().unwrap(),
        true,
    )
    .await
    .unwrap();
    let change = Change::Move {
        element: area.id.clone(),
        position: [225.0, 0.0],
    };
    let preview = act(
        &engine,
        Command::Preview {
            workspace: workspace.clone(),
            change: change.clone(),
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(preview["consequences"].as_array().unwrap().len(), 2);
    assert_eq!(preview["requires_review"], true);
    let pending = propose(&engine, &workspace, change, None).await;
    assert_eq!(pending["state"], "pending");
    for record in [&first, &second] {
        assert_eq!(
            store::records::get(&engine.store.pool, record)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::zero()
        );
    }
    let request = Command::Review {
        workspace: workspace.clone(),
        proposal: pending["proposal"].as_str().unwrap().into(),
        request_id: nucleus::new_uid("request"),
        expected_revision: revision(&engine, &workspace).await,
        approve: true,
    };
    let accepted = act(&engine, request.clone(), None).await.unwrap();
    assert_eq!(
        act(&engine, request, None).await.unwrap().data,
        accepted.data
    );
    for record in [&first, &second] {
        assert_eq!(
            store::records::get(&engine.store.pool, record)
                .await
                .unwrap()
                .unwrap()
                .quantity,
            store::exact::one()
        );
    }
}

#[tokio::test]
async fn review_and_preview_recheck_the_original_author_without_borrowing_manager_authority() {
    let (engine, workspace, record, person) = fixture().await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: element(
                ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
                [200.0, 0.0],
                [20.0; 2],
            ),
        },
        None,
    )
    .await;
    let pending = propose(
        &engine,
        &workspace,
        Change::ChangeRecord {
            record: record.clone(),
            changes: engine::area_transition::RecordChanges {
                quantity: Some("+=1".into()),
                ..Default::default()
            },
        },
        Some(&person),
    )
    .await;
    let proposal = pending["proposal"].as_str().unwrap();
    let preview = Command::PreviewProposal {
        workspace: workspace.clone(),
        proposal: proposal.into(),
    };
    assert!(act(&engine, preview.clone(), None).await.is_err());
    assert!(review(&engine, &workspace, proposal, true).await.is_err());
    let role = store::auth::role_by_name(&engine.store.pool, "Editor")
        .await
        .unwrap()
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let simulated = act(&engine, preview.clone(), None)
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(simulated["original_actor"], person);
    assert_eq!(simulated["consequences"].as_array().unwrap().len(), 1);
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::zero()
    );
    store::auth::revoke(&engine.store.pool, role, permission)
        .await
        .unwrap();
    assert!(review(&engine, &workspace, proposal, true).await.is_err());
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    review(&engine, &workspace, proposal, true).await.unwrap();
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::one()
    );
}

#[tokio::test]
async fn pending_proposals_do_not_survive_revoked_and_regranted_organ_authentication() {
    let (engine, workspace, record, person) = fixture().await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: element(
                ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
                [200.0, 0.0],
                [20.0; 2],
            ),
        },
        None,
    )
    .await;
    store::organs::ensure_local(&engine.store.pool, "")
        .await
        .unwrap();
    let organ = nucleus::new_uid("r");
    let node = "a".repeat(64);
    store::organs::add_contact(&engine.store.pool, &organ, None, "Collaborator", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(&engine.store.pool, &organ, Some(&node))
        .await
        .unwrap();
    store::organs::set_trust(&engine.store.pool, &organ, "known")
        .await
        .unwrap();
    store::logins::grant(&engine.store.pool, &organ, &person)
        .await
        .unwrap();
    let role = store::auth::role_by_name(&engine.store.pool, "Editor")
        .await
        .unwrap()
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "record", "update")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    let login = engine.login_granted(&node).await.unwrap();
    let command = Command::Propose {
        workspace: workspace.clone(),
        request_id: nucleus::new_uid("request"),
        base_revision: revision(&engine, &workspace).await,
        change: Change::ChangeRecord {
            record: record.clone(),
            changes: engine::area_transition::RecordChanges {
                quantity: Some("+=1".into()),
                ..Default::default()
            },
        },
    };
    let result = login
        .run(&engine, true, act(&engine, command, Some(&person)))
        .await
        .unwrap()
        .data
        .unwrap();
    let proposal = result["proposal"].as_str().unwrap();
    let preview = Command::PreviewProposal {
        workspace: workspace.clone(),
        proposal: proposal.into(),
    };
    login.revoke();
    assert!(act(&engine, preview.clone(), None).await.is_ok());
    store::logins::revoke(&engine.store.pool, &organ)
        .await
        .unwrap();
    assert!(act(&engine, preview.clone(), None).await.is_err());
    assert!(review(&engine, &workspace, proposal, true).await.is_err());
    store::logins::grant(&engine.store.pool, &organ, &person)
        .await
        .unwrap();
    assert!(engine.login_granted(&node).await.is_ok());
    assert!(act(&engine, preview, None).await.is_err());
    assert!(review(&engine, &workspace, proposal, true).await.is_err());
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::zero()
    );
    review(&engine, &workspace, proposal, false).await.unwrap();
}

#[tokio::test]
async fn editing_a_concurrently_removed_element_retains_a_rejectable_conflict() {
    let (engine, workspace, _, _) = fixture().await;
    let item = element(
        ComponentState::Text {
            text: "Draft".into(),
        },
        [0.0; 2],
        [100.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: item.clone(),
        },
        None,
    )
    .await;
    let base_revision = revision(&engine, &workspace).await;
    propose(
        &engine,
        &workspace,
        Change::Remove {
            element: item.id.clone(),
        },
        None,
    )
    .await;
    let conflict = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision,
            change: Change::Move {
                element: item.id,
                position: [10.0; 2],
            },
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(conflict["state"], "pending");
    let proposal = conflict["proposal"].as_str().unwrap();
    assert!(review(&engine, &workspace, proposal, true).await.is_err());
    assert_eq!(
        review(&engine, &workspace, proposal, false)
            .await
            .unwrap()
            .data
            .unwrap()["state"],
        "rejected"
    );
}

#[test]
fn workspace_presence_and_client_contract_are_bounded() {
    let presence = engine::workspace_sync::Presence::default();
    for participant in 0..64 {
        assert!(presence.join("joint", &participant.to_string()).unwrap());
    }
    assert!(!presence.join("joint", "0").unwrap());
    assert!(presence.join("joint", "overflow").is_err());
    assert!(presence.leave(Some("joint"), "0"));
    assert!(presence.join("joint", "replacement").unwrap());
    assert_eq!(presence.participants("joint").len(), 64);
    for client in [
        Client {
            schema: 0,
            ..Default::default()
        },
        Client {
            lince_version: "0.6.9".into(),
            ..Default::default()
        },
        Client {
            features: vec![],
            ..Default::default()
        },
    ] {
        assert!(client.validate().is_err());
    }
}

#[tokio::test]
async fn manager_snapshots_and_history_omit_policy_references_to_hidden_records() {
    let (engine, workspace, visible, person) = fixture().await;
    let hidden = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Hidden".into(),
                body: "".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: element(
                ComponentState::Record {
                    record: visible.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
                [200.0, 0.0],
                [20.0; 2],
            ),
        },
        None,
    )
    .await;
    let mut ceiling = policy(true);
    ceiling["ceiling"]["read"] = json!({"any":[{"uid_eq":visible},{"uid_eq":hidden}]});
    let pending = propose(
        &engine,
        &workspace,
        Change::Policy { policy: ceiling },
        None,
    )
    .await;
    review(
        &engine,
        &workspace,
        pending["proposal"].as_str().unwrap(),
        true,
    )
    .await
    .unwrap();
    let role = store::auth::role_by_name(&engine.store.pool, "Editor")
        .await
        .unwrap()
        .unwrap();
    let permission =
        store::auth::ensure_permission(&engine.store.pool, "workspace", "access_control")
            .await
            .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine
        .act(
            Action::SetRolePolicy {
                role: "Editor".into(),
                expected_revision: 0,
                policy: json!({"read":{"uid_eq":visible},"grants":[]}),
            },
            None,
        )
        .await
        .unwrap();
    let snapshot = engine
        .workspace_view(Some(&person), &workspace, &Client::default(), false)
        .await
        .unwrap();
    assert_eq!(snapshot["can_review"], true);
    assert_eq!(snapshot["policy_unavailable"], true);
    assert!(snapshot["policy"].is_null());
    assert!(!serde_json::to_string(&snapshot).unwrap().contains(&hidden));
    let history = act(
        &engine,
        Command::History {
            workspace,
            before: None,
        },
        Some(&person),
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert!(!serde_json::to_string(&history).unwrap().contains(&hidden));
    assert!(
        history["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|change| change["change"]["operation"] != "policy")
    );
}

#[tokio::test]
async fn reviewers_can_accept_an_editors_proposal_without_topology_editing_permission() {
    let (engine, workspace, _, editor) = fixture().await;
    let reviewer = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Reviewer",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&engine.store.pool, "Review only")
        .await
        .unwrap();
    for key in ["workspace:read", "workspace:access_control", "record:read"] {
        let (subject, operation) = key.split_once(':').unwrap();
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, operation)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    engine
        .act(
            Action::AssignRoles {
                person: reviewer.clone(),
                roles: vec!["Review only".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    propose(
        &engine,
        &workspace,
        Change::Rename {
            name: "Current".into(),
        },
        None,
    )
    .await;
    let pending = act(
        &engine,
        Command::Propose {
            workspace: workspace.clone(),
            request_id: nucleus::new_uid("request"),
            base_revision: 1,
            change: Change::Rename {
                name: "Reviewed".into(),
            },
        },
        Some(&editor),
    )
    .await
    .unwrap()
    .data
    .unwrap();
    let approved = act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        Some(&reviewer),
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(approved["state"], "applied");
    let view = engine
        .workspace_view(Some(&reviewer), &workspace, &Client::default(), false)
        .await
        .unwrap();
    assert_eq!(view["can_edit"], false);
    assert_eq!(view["can_review"], true);
    let policy_change = propose(
        &engine,
        &workspace,
        Change::Policy {
            policy: policy(true),
        },
        Some(&reviewer),
    )
    .await;
    let proposal = policy_change["proposal"].as_str().unwrap();
    assert!(
        act(
            &engine,
            Command::PreviewProposal {
                workspace: workspace.clone(),
                proposal: proposal.into()
            },
            Some(&reviewer)
        )
        .await
        .is_ok()
    );
    let approved = act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: proposal.into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        Some(&reviewer),
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(approved["state"], "applied");
    let view = engine
        .workspace_view(Some(&reviewer), &workspace, &Client::default(), false)
        .await
        .unwrap();
    assert!(
        act(
            &engine,
            Command::Propose {
                workspace,
                request_id: nucleus::new_uid("request"),
                base_revision: view["revision"].as_i64().unwrap(),
                change: Change::Rename {
                    name: "Not an editor".into()
                }
            },
            Some(&reviewer)
        )
        .await
        .is_err()
    );
}

async fn approve_change(engine: &Engine, workspace: &str, change: Change) {
    let proposal = propose(engine, workspace, change, None).await;
    assert_eq!(proposal["state"], "pending");
    act(
        engine,
        Command::Review {
            workspace: workspace.into(),
            proposal: proposal["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(engine, workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn declared_composed_controls_recheck_actor_ceiling_and_policy_suspension() {
    let (engine, workspace, record, person) = fixture().await;
    let action = json!({"action":"workspace-record","record":record,"changes":{"quantity":"+=1"}});
    let control = Element {
        id: nucleus::new_uid("placement"),
        geometry: Geometry {
            position: [0.0; 2],
            size: [400.0; 2],
        },
        component: Component::Composition {
            composition: nucleus::canvas::Composition {
                name: "Controls".into(),
                origin: None,
                parts: vec![nucleus::canvas::Part {
                    id: "counter".into(),
                    geometry: Geometry {
                        position: [0.0; 2],
                        size: [100.0; 2],
                    },
                    component: Component::Builtin {
                        state: ComponentState::Button {
                            label: "Increment".into(),
                            action: action.clone(),
                        },
                    },
                    events: vec![],
                }],
            },
        },
    };
    approve_change(
        &engine,
        &workspace,
        Change::Add {
            element: control.clone(),
        },
    )
    .await;
    let invoke = Change::Invoke {
        element: control.id.clone(),
        path: vec!["counter".into()],
        event: None,
        action: action.clone(),
    };
    let command = || Command::Propose {
        workspace: workspace.clone(),
        request_id: nucleus::new_uid("request"),
        base_revision: 2,
        change: invoke.clone(),
    };
    assert!(act(&engine, command(), Some(&person)).await.is_err());
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::zero()
    );
    engine
        .act(
            Action::GrantPermission {
                role: "Editor".into(),
                permission: "record:update".into(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        act(&engine, command(), Some(&person))
            .await
            .unwrap()
            .data
            .unwrap()["state"],
        "applied"
    );
    let changed_action =
        json!({"action":"workspace-record","record":record,"changes":{"quantity":"+=100"}});
    assert!(
        act(
            &engine,
            Command::Propose {
                workspace: workspace.clone(),
                request_id: nucleus::new_uid("request"),
                base_revision: 3,
                change: Change::Invoke {
                    element: control.id.clone(),
                    path: vec!["counter".into()],
                    event: None,
                    action: changed_action
                }
            },
            Some(&person)
        )
        .await
        .is_err()
    );
    approve_change(
        &engine,
        &workspace,
        Change::Policy {
            policy: policy(true),
        },
    )
    .await;
    assert!(
        act(
            &engine,
            Command::Propose {
                workspace: workspace.clone(),
                request_id: nucleus::new_uid("request"),
                base_revision: 4,
                change: invoke
            },
            Some(&person)
        )
        .await
        .is_err()
    );
    approve_change(
        &engine,
        &workspace,
        Change::Configure {
            element: control.id.clone(),
            component: control.component.clone(),
        },
    )
    .await;
    let applied = propose(
        &engine,
        &workspace,
        Change::Invoke {
            element: control.id,
            path: vec!["counter".into()],
            event: None,
            action,
        },
        Some(&person),
    )
    .await;
    assert_eq!(applied["state"], "applied");
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        nucleus::DecimalValue::parse_inferred("2").unwrap()
    );
}

#[tokio::test]
async fn shared_text_is_previewed_atomically_and_retained_in_collaborative_documents() {
    let (engine, workspace, record, _) = fixture().await;
    approve_change(&engine, &workspace, Change::Policy { policy: json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[{"operation":"update","selector":{"kind_eq":"plain"},"properties":["head","body","slug","unit",{"extension":{"namespace":"audit","field":"color"}}],"assertions_add":[],"assertions_remove":[]}]}}) }).await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: element(
                ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
                [0.0; 2],
                [100.0; 2],
            ),
        },
        None,
    )
    .await;
    let prior = engine.doc_text(&record).await.unwrap();
    let edit = Change::EditRecord {
        record: record.clone(),
        edits: vec![
            engine::workspace_sync::RecordEdit::Text {
                head: Some("Shared head".into()),
                body: Some("Shared body".into()),
            },
            engine::workspace_sync::RecordEdit::Slug {
                slug: Some("shared-editor".into()),
            },
            engine::workspace_sync::RecordEdit::Extension {
                namespace: "audit".into(),
                value: json!({"color":"green"}),
            },
        ],
    };
    let preview = act(
        &engine,
        Command::Preview {
            workspace: workspace.clone(),
            change: edit.clone(),
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(preview["consequences"][0]["after"]["head"], "Shared head");
    assert_eq!(engine.doc_text(&record).await.unwrap(), prior);
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .head,
        prior.0
    );
    approve_change(&engine, &workspace, edit).await;
    assert_eq!(
        engine.doc_text(&record).await.unwrap(),
        ("Shared head".into(), "Shared body".into())
    );
    engine
        .write_record_text(&record, Some("Ordinary edit"), None)
        .await
        .unwrap();
    assert_eq!(
        engine.doc_text(&record).await.unwrap(),
        ("Ordinary edit".into(), "Shared body".into())
    );
    let refused = Change::EditRecord {
        record: record.clone(),
        edits: vec![
            engine::workspace_sync::RecordEdit::Text {
                head: Some("Must roll back".into()),
                body: None,
            },
            engine::workspace_sync::RecordEdit::Extension {
                namespace: "audit".into(),
                value: json!({"unauthorized":"bad"}),
            },
        ],
    };
    let proposal = propose(&engine, &workspace, refused, None).await;
    assert!(
        act(
            &engine,
            Command::Review {
                workspace: workspace.clone(),
                proposal: proposal["proposal"].as_str().unwrap().into(),
                request_id: nucleus::new_uid("request"),
                expected_revision: revision(&engine, &workspace).await,
                approve: true
            },
            None
        )
        .await
        .is_err()
    );
    assert_eq!(engine.doc_text(&record).await.unwrap().0, "Ordinary edit");
    assert_eq!(
        store::records::get_extension(&engine.store.pool, &record, "audit")
            .await
            .unwrap()
            .unwrap(),
        json!({"color":"green"})
    );
}

#[tokio::test]
async fn publish_reports_every_rejected_element_and_never_silently_omits_it() {
    let (engine, _, record, _) = fixture().await;
    let valid = element(
        ComponentState::Text {
            text: "Local".into(),
        },
        [12.0, 24.0],
        [100.0; 2],
    );
    let unsupported = Element {
        id: nucleus::new_uid("placement"),
        geometry: valid.geometry.clone(),
        component: Component::Native {
            kind: "external-program".into(),
            settings: Default::default(),
            bindings: vec![],
        },
    };
    let forbidden = element(
        ComponentState::Button {
            label: "Write text".into(),
            action: json!({"action":"edit-record-text","target":record,"head":"unauthorized"}),
        },
        [0.0; 2],
        [100.0; 2],
    );
    let layout = json!({"elements":[valid,unsupported,forbidden],"areas":{}});
    let report = act(
        &engine,
        Command::ValidateImport {
            name: "Imported".into(),
            policy: policy(true),
            layout: layout.clone(),
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(report["ready"], false);
    assert_eq!(report["rejected"].as_array().unwrap().len(), 2);
    assert!(
        act(
            &engine,
            Command::Publish {
                name: "Imported".into(),
                policy: policy(true),
                layout
            },
            None
        )
        .await
        .is_err()
    );
    let before: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM shared_workspace")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(before, 1);
    let layout = json!({"elements":[valid],"areas":{}});
    let published = act(
        &engine,
        Command::Publish {
            name: "Imported".into(),
            policy: policy(false),
            layout,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap();
    let snapshot = act(
        &engine,
        Command::Inspect {
            workspace: published,
            permitted_view: false,
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(
        snapshot["layout"]["elements"][0]["geometry"]["position"],
        json!([12.0, 24.0])
    );
    assert!(snapshot["layout"].get("camera").is_none());
}

#[tokio::test]
async fn history_pages_are_disjoint_and_keep_a_cursor_when_rows_are_filtered() {
    let (engine, workspace, _, _) = fixture().await;
    for index in 0..70 {
        propose(
            &engine,
            &workspace,
            Change::Rename {
                name: format!("Name {index}"),
            },
            None,
        )
        .await;
    }
    let first = act(
        &engine,
        Command::History {
            workspace: workspace.clone(),
            before: None,
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(first["changes"].as_array().unwrap().len(), 64);
    let second = act(
        &engine,
        Command::History {
            workspace,
            before: Some(first["next_cursor"].as_str().unwrap().into()),
        },
        None,
    )
    .await
    .unwrap()
    .data
    .unwrap();
    assert_eq!(second["changes"].as_array().unwrap().len(), 6);
    assert!(second["next_cursor"].is_null());
    let first_ids = first["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|change| change["uid"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        second["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|change| !first_ids.contains(change["uid"].as_str().unwrap()))
    );
}

#[tokio::test]
async fn identity_refinement_and_restoration_use_separate_declared_authority() {
    use engine::workspace_sync::RecordEdit;
    let (engine, workspace, record, person) = fixture().await;
    let predicate = store::concepts::ensure(&engine.store.pool, "workspace-link")
        .await
        .unwrap();
    let identity = store::concepts::ensure(&engine.store.pool, "workspace-identity")
        .await
        .unwrap();
    let target = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "Target",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let rules = vec![
        json!({"predicate_uid":predicate,"target":"unary","role":"ordinary","properties":[]}),
        json!({"predicate_uid":predicate,"target":{"record":target},"role":"ordinary","properties":[]}),
        json!({"predicate_uid":identity,"target":"unary","role":"identity","properties":[]}),
    ];
    let all_properties = json!([
        "kind", "organ", "head", "body", "quantity", "slug", "unit", "place"
    ]);
    let grants = ["update", "delete", "restore"].into_iter().map(|operation| json!({"operation":operation,"selector":{"kind_eq":"plain"},"properties":all_properties,"assertions_add":rules,"assertions_remove":rules})).collect::<Vec<_>>();
    approve_change(&engine, &workspace, Change::Policy { policy: json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":grants}}) }).await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: element(
                ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
                [0.0; 2],
                [100.0; 2],
            ),
        },
        None,
    )
    .await;
    approve_change(
        &engine,
        &workspace,
        Change::EditRecord {
            record: record.clone(),
            edits: vec![
                RecordEdit::Assert {
                    predicate: predicate.clone(),
                    object: None,
                    quantity: None,
                    unit: None,
                },
                RecordEdit::Identity {
                    predicate: Some(identity.clone()),
                },
            ],
        },
    )
    .await;
    approve_change(
        &engine,
        &workspace,
        Change::EditRecord {
            record: record.clone(),
            edits: vec![RecordEdit::Refine {
                predicate,
                object: target.clone(),
            }],
        },
    )
    .await;
    let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM record_assertion WHERE subject_uid=? AND object_uid=? AND retracted_at IS NULL").bind(&record).bind(&target).fetch_one(&engine.store.pool).await.unwrap();
    assert_eq!(count, 1);
    approve_change(
        &engine,
        &workspace,
        Change::DeleteRecord {
            record: record.clone(),
        },
    )
    .await;
    engine
        .act(
            Action::GrantPermission {
                role: "Editor".into(),
                permission: "record:update".into(),
            },
            None,
        )
        .await
        .unwrap();
    let restore = Change::RestoreRecord {
        record: record.clone(),
        slug: None,
        placement: nucleus::new_uid("placement"),
        geometry: Geometry {
            position: [200.0, 0.0],
            size: [100.0; 2],
        },
    };
    let pending = propose(&engine, &workspace, restore, Some(&person)).await;
    act(
        &engine,
        Command::Review {
            workspace: workspace.clone(),
            proposal: pending["proposal"].as_str().unwrap().into(),
            request_id: nucleus::new_uid("request"),
            expected_revision: revision(&engine, &workspace).await,
            approve: true,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        act(
            &engine,
            Command::Inspect {
                workspace,
                permitted_view: false
            },
            Some(&person)
        )
        .await
        .unwrap()
        .data
        .unwrap()["layout"]["elements"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn history_pages_bound_large_record_drafts_without_skipping_changes() {
    let (engine, workspace, record, _) = fixture().await;
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: element(
                ComponentState::Record {
                    record: record.clone(),
                    mode: Default::default(),
                    start_call: None,
                },
                [0.0; 2],
                [100.0; 2],
            ),
        },
        None,
    )
    .await;
    for index in 0..14 {
        propose(
            &engine,
            &workspace,
            Change::EditRecord {
                record: record.clone(),
                edits: vec![engine::workspace_sync::RecordEdit::Text {
                    head: Some(format!("Draft {index}")),
                    body: Some("x".repeat(131072)),
                }],
            },
            None,
        )
        .await;
    }
    let mut before = None;
    let mut ids = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let page = act(
            &engine,
            Command::History {
                workspace: workspace.clone(),
                before,
            },
            None,
        )
        .await
        .unwrap()
        .data
        .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() <= 1048576);
        for change in page["changes"].as_array().unwrap() {
            assert!(ids.insert(change["uid"].as_str().unwrap().to_owned()));
        }
        pages += 1;
        before = page["next_cursor"].as_str().map(str::to_owned);
        if before.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 15);
    assert!(pages >= 2);
}

#[tokio::test]
async fn composed_record_crossings_use_local_part_coordinates_and_current_actor() {
    let (engine, workspace, record, person) = fixture().await;
    let area = element(
        ComponentState::Area {
            immunity: Default::default(),
            strength: 0,
        },
        [0.0; 2],
        [100.0; 2],
    );
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: area.clone(),
        },
        None,
    )
    .await;
    approve_change(
        &engine,
        &workspace,
        Change::Area {
            element: area.id,
            changes: engine::area_transition::RecordChanges {
                quantity: Some("+=1".into()),
                ..Default::default()
            },
        },
    )
    .await;
    let group = Element {
        id: nucleus::new_uid("placement"),
        geometry: Geometry {
            position: [300.0, 0.0],
            size: [400.0; 2],
        },
        component: Component::Composition {
            composition: nucleus::canvas::Composition {
                name: "Group".into(),
                origin: None,
                parts: vec![nucleus::canvas::Part {
                    id: "record".into(),
                    geometry: Geometry {
                        position: [-100.0, 0.0],
                        size: [30.0; 2],
                    },
                    component: Component::Builtin {
                        state: ComponentState::Record {
                            record: record.clone(),
                            mode: Default::default(),
                            start_call: None,
                        },
                    },
                    events: vec![],
                }],
            },
        },
    };
    propose(
        &engine,
        &workspace,
        Change::Add {
            element: group.clone(),
        },
        None,
    )
    .await;
    let change = Change::Move {
        element: group.id,
        position: [100.0, 0.0],
    };
    assert!(
        act(
            &engine,
            Command::Propose {
                workspace: workspace.clone(),
                request_id: nucleus::new_uid("request"),
                base_revision: revision(&engine, &workspace).await,
                change: change.clone()
            },
            Some(&person)
        )
        .await
        .is_err()
    );
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::zero()
    );
    propose(&engine, &workspace, change, None).await;
    assert_eq!(
        store::records::get(&engine.store.pool, &record)
            .await
            .unwrap()
            .unwrap()
            .quantity
            .to_string(),
        "1"
    );
}
