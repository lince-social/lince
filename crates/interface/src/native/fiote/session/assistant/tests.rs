use super::*;
use cell::FioteService;
use cell::fiote_provider::{Message, Provider, Reply, ToolDefinition};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(super) struct Script {
    pub(super) requests: Mutex<Vec<(String, Vec<Message>)>>,
    blocked: Mutex<Option<String>>,
}

#[async_trait::async_trait]
impl Provider for Script {
    fn validate_content(&self, content: &[nucleus::message::MessagePart]) -> Result<(), String> {
        nucleus::message::validate(content)
    }

    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        _: &[ToolDefinition],
    ) -> Result<Reply, String> {
        self.requests
            .lock()
            .unwrap()
            .push((system.into(), messages.to_vec()));
        let blocked = self.blocked.lock().unwrap().clone();
        if blocked
            .is_some_and(|record| system.contains(&format!("Fiote and Agent Record {record}")))
        {
            std::future::pending::<()>().await;
        }
        Ok(Reply {
            text: "Hello from the native assistant workflow".into(),
            ..Default::default()
        })
    }
}

fn click(world: &mut World, caption: &str) {
    let button = world
        .query::<(
            &crate::native::actions::ActionButton,
            &bevy::a11y::AccessibilityNode,
        )>()
        .iter(world)
        .find(|(_, node)| node.label() == Some(caption))
        .map(|(button, _)| button.clone())
        .unwrap_or_else(|| panic!("Missing {caption}"));
    button.actions.run(world, button.target);
}

pub(super) async fn responses(app: &mut App) {
    for _ in 0..100 {
        let pending = app
            .world_mut()
            .query::<&Entry>()
            .iter(app.world())
            .any(|entry| entry.pending.is_some());
        if !pending {
            return;
        }
        let response = tokio::time::timeout(
            Duration::from_secs(20),
            app.world_mut()
                .non_send_mut::<crate::native::cell_bridge::CellBridge>()
                .incoming
                .recv(),
        )
        .await
        .unwrap()
        .unwrap();
        app.world_mut()
            .write_message(crate::native::cell_bridge::CellMessage(response));
        app.update();
    }
    panic!("Assistant controls did not settle");
}

pub(super) async fn fixture() -> (
    App,
    Entity,
    Arc<cell::fiote::Host>,
    Arc<Script>,
    tempfile::TempDir,
    String,
) {
    fixture_named("Native assistant").await
}

pub(super) async fn fixture_named(
    name: &str,
) -> (
    App,
    Entity,
    Arc<cell::fiote::Host>,
    Arc<Script>,
    tempfile::TempDir,
    String,
) {
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let record = engine
        .act(
            engine::actions::Action::CreateAgent {
                head: name.into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let script = Arc::new(Script::default());
    let host = Arc::new(
        cell::fiote::Host::open(engine.clone(), directory.path().join("settings"))
            .await
            .unwrap()
            .with_provider(script.clone()),
    );
    let status = host
        .handle(FioteRequest::Inspect {
            record: record.clone(),
        })
        .await
        .unwrap();
    let provider = status
        .providers
        .iter()
        .find(|provider| provider.auth_methods[0].kind == FioteAuthKind::ApiKey)
        .unwrap();
    let status = host
        .handle(FioteRequest::Configure {
            record: record.clone(),
            settings: FioteSettings {
                enabled: true,
                provider: provider.id.clone(),
                model: "fixture-model".into(),
                directory: directory.path().into(),
                ..Default::default()
            },
            api_key: Some(FioteSecret("fixture-key".into())),
            password: Some(FioteSecret("fixture-password".into())),
        })
        .await
        .unwrap();
    let runtime = cell::CellRuntime {
        speech: None,
        commands: Default::default(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: Some(host.clone()),
    };
    let (mut app, panel, _) = super::super::tests::panel_fixture(Step::Closed);
    app.insert_resource(crate::native::app::CellHandle(runtime.clone()));
    app.world_mut()
        .insert_non_send(crate::native::cell_bridge::connect(
            runtime,
            crate::native::wake::WakeSignal::new(|| {}),
        ));
    {
        let mut panel = app.world_mut().get_mut::<Panel>(panel).unwrap();
        panel.binding.uid = record.clone();
        panel.saved = Some(status);
    }
    Open.apply(app.world_mut(), panel);
    app.world_mut().get_mut::<Node>(panel).unwrap().display = Display::Flex;
    responses(&mut app).await;
    let owner = app
        .world_mut()
        .query_filtered::<Entity, With<Entry>>()
        .iter(app.world())
        .next()
        .unwrap();
    (app, owner, host, script, directory, record)
}

#[tokio::test]
async fn assistant_controls_send_real_service_request_with_attachments_and_inspect_receipt() {
    let (mut app, owner, host, script, _directory, record) = fixture().await;
    click(app.world_mut(), "Native assistant");
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Hello through the native controls");
    let part = nucleus::message::MessagePart::Reference {
        name: "Reference".into(),
        uri: "https://example.org/context.txt".into(),
    };
    app.world_mut()
        .get_mut::<crate::native::message_content::Draft>(owner)
        .unwrap()
        .parts
        .push(part.clone());
    click(app.world_mut(), "Send assistant request");
    assert!(app.world().get::<Entry>(owner).unwrap().pending.is_some());
    responses(&mut app).await;
    assert!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string()
            .is_empty()
    );
    assert!(
        app.world()
            .get::<crate::native::message_content::Draft>(owner)
            .unwrap()
            .parts
            .is_empty()
    );
    let receipt = app
        .world()
        .get::<Entry>(owner)
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    assert_eq!(receipt.target, record);
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let status = host
                .handle(FioteRequest::AssistantInspect {
                    request_id: receipt.request_id.clone(),
                })
                .await
                .unwrap();
            if status
                .assistant_request
                .as_ref()
                .is_some_and(|receipt| receipt.state == "completed")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    click(app.world_mut(), "Inspect last request");
    responses(&mut app).await;
    let receipt = app
        .world()
        .get::<Entry>(owner)
        .unwrap()
        .receipt
        .as_ref()
        .unwrap();
    assert_eq!(receipt.state, "completed");
    assert_eq!(
        store::message_content::load(
            &app.world()
                .resource::<crate::native::app::CellHandle>()
                .0
                .store
                .pool,
            receipt.message.as_ref().unwrap()
        )
        .await
        .unwrap(),
        vec![part.clone()]
    );
    assert!(script.requests.lock().unwrap().iter().any(|(system, messages)| system.contains(&record) && messages.iter().any(|message| matches!(message, Message::RichUser { content, .. } if content.contains(&part)))));
}

#[tokio::test]
async fn assistant_controls_preserve_unsent_draft_and_request_identity_on_refusal() {
    let (mut app, owner, host, script, directory, record) = fixture().await;
    click(app.world_mut(), "Native assistant");
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Keep this request");
    app.world_mut().get_mut::<Entry>(owner).unwrap().path =
        Some(directory.path().join("request.json"));
    host.handle(FioteRequest::Configure {
        record,
        settings: FioteSettings::default(),
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    click(app.world_mut(), "Send assistant request");
    responses(&mut app).await;
    let first = app
        .world()
        .get::<Entry>(owner)
        .unwrap()
        .saved
        .clone()
        .unwrap();
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Keep this request"
    );
    click(app.world_mut(), "Send assistant request");
    responses(&mut app).await;
    assert_eq!(
        app.world()
            .get::<Entry>(owner)
            .unwrap()
            .saved
            .as_ref()
            .unwrap()
            .request_id,
        first.request_id
    );
    let persisted: Saved =
        serde_json::from_slice(&std::fs::read(directory.path().join("request.json")).unwrap())
            .unwrap();
    assert_eq!(persisted.request_id, first.request_id);
    assert!(script.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn assistant_control_response_preserves_edits_made_during_send() {
    let (mut app, owner, _host, _script, _directory, _record) = fixture().await;
    click(app.world_mut(), "Native assistant");
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Original request");
    click(app.world_mut(), "Send assistant request");
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("Next request typed while sending");
    responses(&mut app).await;
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "Next request typed while sending"
    );
    assert!(
        app.world()
            .get::<Entry>(owner)
            .unwrap()
            .receipt
            .as_ref()
            .unwrap()
            .message
            .is_some()
    );
}

#[tokio::test]
async fn delegated_stop_control_interrupts_child_and_keeps_parent_selected() {
    let (mut app, owner, host, script, _directory, record) = fixture().await;
    click(app.world_mut(), "Native assistant");
    let input = app.world().get::<Entry>(owner).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text("A parent request with delegated work");
    click(app.world_mut(), "Send assistant request");
    responses(&mut app).await;
    let parent = app
        .world()
        .get::<Entry>(owner)
        .unwrap()
        .receipt
        .clone()
        .unwrap();
    let panel = app.world().get::<Entry>(owner).unwrap().panel;
    let child = host
        .handle(FioteRequest::Create {
            head: "Blocked child".into(),
        })
        .await
        .unwrap()
        .record;
    let settings = host
        .handle(FioteRequest::Inspect {
            record: record.clone(),
        })
        .await
        .unwrap()
        .settings;
    host.handle(FioteRequest::Configure {
        record: child.clone(),
        settings,
        api_key: None,
        password: None,
    })
    .await
    .unwrap();
    *script.blocked.lock().unwrap() = Some(child.clone());
    let delegated = host
        .delegate(
            cell::fiote_assistant::Source {
                fiote: record.clone(),
                author: record.clone(),
                thread: parent.thread.clone().unwrap(),
                message: parent.message.clone().unwrap(),
            },
            cell::fiote_assistant::Delegate {
                request_id: "native-child-stop".into(),
                target: child,
                request: "Wait for cancellation".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(delegated.state, "running");
    let saved = host
        .handle(FioteRequest::Inspect {
            record: record.clone(),
        })
        .await
        .unwrap();
    app.world_mut().get_mut::<Panel>(panel).unwrap().saved = Some(saved);
    super::super::show(app.world_mut(), panel, Step::Manage);
    click(app.world_mut(), "Show delegated work");
    click(app.world_mut(), "Stop delegated Fiote");
    super::super::tests::deliver(&mut app).await;
    assert_eq!(app.world().get::<Panel>(panel).unwrap().binding.uid, record);
    assert_eq!(
        app.world()
            .get::<Panel>(panel)
            .unwrap()
            .saved
            .as_ref()
            .unwrap()
            .record,
        record
    );
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let receipts = host
                .handoffs(parent.thread.as_ref().unwrap())
                .await
                .unwrap();
            if receipts.iter().any(|receipt| {
                receipt.request_id == "native-child-stop" && receipt.state == "interrupted"
            }) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let parent = host
        .handle(FioteRequest::AssistantInspect {
            request_id: parent.request_id,
        })
        .await
        .unwrap()
        .assistant_request
        .unwrap();
    assert_eq!(parent.state, "completed");
    let child_thread = delegated.thread.as_ref().unwrap();
    let store = &app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .store;
    let message = store::records::get(&store.pool, delegated.message.as_ref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let root = app.world().get::<Panel>(panel).unwrap().binding.area;
    app.world_mut()
        .entity_mut(root)
        .insert(crate::native::workspace::Workspaces::default());
    let initial = app
        .world()
        .resource::<crate::native::app::CellHandle>()
        .0
        .engine
        .act(
            engine::actions::Action::CreateThread {
                target: delegated.target.clone(),
                head: "Thread 1".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    click(app.world_mut(), "Show delegated work");
    click(app.world_mut(), "Open delegated conversation");
    let area = app
        .world_mut()
        .query::<(Entity, &crate::native::thread_castle::RequestedThread)>()
        .iter(app.world())
        .find(|(_, requested)| requested.0 == *child_thread)
        .map(|(entity, _)| entity)
        .unwrap();
    let config = app
        .world()
        .get::<crate::native::area::InfluenceArea>(area)
        .unwrap()
        .protein
        .as_ref()
        .unwrap();
    assert_eq!(
        config.draft.query["where"][0]["all"][0]["uid_eq"],
        delegated.target
    );
    let castle = app.world_mut().spawn((Node::default(), ChildOf(area))).id();
    crate::native::thread_castle::populate(
        app.world_mut(),
        castle,
        RecordBinding {
            area,
            uid: delegated.target.clone(),
            source: Source::Local,
        },
        &serde_json::json!({
            "uid":delegated.target,
            "threads":[
                {"uid":initial,"head":"Thread 1","messages":[]},
                {"uid":child_thread,"head":"Delegated result","messages":[{
                    "uid":message.uid,"author":"Parent Fiote","body":message.body
                }]}
            ]
        }),
    );
    assert!(crate::native::thread_castle::laboratory_visible(
        app.world(),
        castle,
        child_thread,
        &message.body,
    ));
    assert!(
        app.world()
            .get::<crate::native::thread_castle::RequestedThread>(area)
            .is_none()
    );
    assert_eq!(app.world().get::<Panel>(panel).unwrap().binding.uid, record);
    click(app.world_mut(), "Open delegated conversation");
    assert!(crate::native::thread_castle::laboratory_visible(
        app.world(),
        castle,
        child_thread,
        &message.body,
    ));
    let areas = app
        .world_mut()
        .query::<&crate::native::area::InfluenceArea>()
        .iter(app.world())
        .filter(|area| area.name == "Fiote Castle")
        .count();
    assert_eq!(areas, 1);
}
