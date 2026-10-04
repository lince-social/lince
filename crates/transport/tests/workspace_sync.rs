use engine::{
    Engine,
    actions::Action,
    workspace_sync::{Change, Client, Command, Request},
};
use std::sync::Arc;
use transport::{
    lane::LaneHub,
    protocol::{ClientMessage, ServerMessage},
    session::Session,
};

#[tokio::test]
async fn two_live_views_receive_host_changes_and_revoke_without_further_snapshots() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let person = store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Viewer",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&engine.store.pool, "Viewer")
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&engine.store.pool, "workspace", "read")
        .await
        .unwrap();
    store::auth::grant(&engine.store.pool, role, permission)
        .await
        .unwrap();
    engine
        .act(
            Action::AssignRoles {
                person: person.clone(),
                roles: vec!["Viewer".into()],
                expected_revision: 0,
            },
            None,
        )
        .await
        .unwrap();
    let workspace = engine.act(Action::Workspace { request: Request { client: Client::default(), command: Command::Create { name:"Joint".into(), policy:serde_json::json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}) } } }, None).await.unwrap().created.unwrap();
    let hub = Arc::new(LaneHub::new());
    let mut first = Session::local(engine.clone(), hub.clone(), "first");
    let mut second = Session::new(engine.clone(), hub, "second", Some(person));
    for session in [&mut first, &mut second] {
        let messages = session
            .handle(ClientMessage::WorkspaceSubscribe {
                id: "shared".into(),
                workspace: workspace.clone(),
                client: Client::default(),
            })
            .await;
        assert!(messages.iter().any(|message| matches!(message, ServerMessage::Workspace { workspace, .. } if workspace["revision"] == 1)));
    }
    engine
        .act(
            Action::Workspace {
                request: Request {
                    client: Client::default(),
                    command: Command::Propose {
                        workspace: workspace.clone(),
                        request_id: nucleus::new_uid("request"),
                        base_revision: 1,
                        change: Change::Rename {
                            name: "Together".into(),
                        },
                    },
                },
            },
            None,
        )
        .await
        .unwrap();
    for session in [&mut first, &mut second] {
        assert!(session.refresh().await.iter().any(|message| matches!(message, ServerMessage::Workspace { workspace, .. } if workspace["revision"] == 2 && workspace["name"] == "Together")));
    }
    store::auth::revoke(&engine.store.pool, role, permission)
        .await
        .unwrap();
    assert!(
        second
            .refresh()
            .await
            .iter()
            .any(|message| matches!(message,ServerMessage::Error { id, .. } if id == "shared"))
    );
    assert!(
        !second
            .refresh()
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::Workspace { .. }))
    );
    assert!(
        first
            .refresh()
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::Workspace { .. }))
    );
    assert!(first.handle(ClientMessage::WorkspaceSubscribe { id:"unsupported".into(), workspace, client:Client { protocol:0, ..Default::default() } }).await.iter().any(|message| matches!(message,ServerMessage::Error { code:Some(code), .. } if code == "workspace_client_incompatible")));
}

#[tokio::test]
async fn workspace_feeds_are_bounded_and_replacement_retains_other_presence() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let mut spaces = Vec::new();
    for index in 0..9 {
        spaces.push(engine.act(Action::Workspace { request:Request { client:Client::default(),command:Command::Create { name:format!("Space {index}"),policy:serde_json::json!({"required_capabilities":[],"ceiling":{"read":{"all":[]},"grants":[]}}) } } },None).await.unwrap().created.unwrap());
    }
    let mut session = Session::local(engine.clone(), Arc::new(LaneHub::new()), "bounded");
    for index in 0..8 {
        assert!(
            session
                .handle(ClientMessage::WorkspaceSubscribe {
                    id: format!("feed-{index}"),
                    workspace: spaces[0].clone(),
                    client: Client::default()
                })
                .await
                .iter()
                .any(|message| matches!(message, ServerMessage::Workspace { .. }))
        );
    }
    assert_eq!(engine.workspace_presence.participants(&spaces[0]).len(), 1);
    assert!(
        !session
            .handle(ClientMessage::WorkspaceSubscribe {
                id: "overflow".into(),
                workspace: spaces[8].clone(),
                client: Client::default()
            })
            .await
            .iter()
            .any(|message| matches!(message, ServerMessage::Workspace { .. }))
    );
    session
        .handle(ClientMessage::WorkspaceSubscribe {
            id: "feed-0".into(),
            workspace: spaces[1].clone(),
            client: Client::default(),
        })
        .await;
    assert_eq!(engine.workspace_presence.participants(&spaces[0]).len(), 1);
    assert_eq!(engine.workspace_presence.participants(&spaces[1]).len(), 1);
    for index in 1..8 {
        session
            .handle(ClientMessage::Unsubscribe {
                id: format!("feed-{index}"),
            })
            .await;
    }
    assert!(
        engine
            .workspace_presence
            .participants(&spaces[0])
            .is_empty()
    );
    drop(session);
    assert!(
        engine
            .workspace_presence
            .participants(&spaces[1])
            .is_empty()
    );
}
