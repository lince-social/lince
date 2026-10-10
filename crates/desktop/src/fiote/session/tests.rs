use super::*;

pub(super) fn fixture_status(record: &str) -> FioteStatus {
    FioteStatus {
        connections: Default::default(),
        activations: Vec::new(),
        questions: Vec::new(),
        usage: Vec::new(),
        agent_session: None,
        session: None,
        behavior: Default::default(),
        instructions: Vec::new(),
        instruction_error: None,
        fiotes: Vec::new(),
        tasks: Vec::new(),
        agent: Some(cell::FioteAgentConfig {
            require_vault: false,
            additional_directories: Vec::new(),
            command: "test-agent".into(),
            args: vec![],
            directory: std::env::current_dir().unwrap(),
            environment: Default::default(),
            session_meta: Default::default(),
            options: Default::default(),
        }),
        agent_info: Some(
            serde_json::json!({"providers":[{"providerId":"a","name":"First"},{"providerId":"b","name":"Second"}],"selectedProvider":{"fields":[{"key":"KEY","label":"Key","secret":true}]}}),
        ),
        agent_activity: vec![],
        record: record.into(),
        settings: FioteSettings {
            enabled: true,
            ..Default::default()
        },
        has_key: false,
        running: vec![],
        vault_exists: true,
        locked: true,
        login_url: None,
        login_pending: false,
        providers: vec![],
        requires_credential: false,
        provider_diagnostics: Vec::new(),
        tool_connections: vec![],
    }
}

fn panel_fixture(step: Step) -> (App, Entity, FioteStatus) {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<crate::cell_bridge::CellMessage>()
        .add_plugins(Plugin);
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    let binding = RecordBinding {
        area: parent,
        uid: nucleus::new_uid("r"),
        source: Source::Local,
    };
    populate(world, parent, binding.clone());
    let owner = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .next()
        .unwrap()
        .0;
    let saved = fixture_status(&binding.uid);
    world.get_mut::<Panel>(owner).unwrap().saved = Some(saved.clone());
    show(world, owner, step);
    (app, owner, saved)
}

#[test]
fn saved_node_connection_opens_native_setup_instead_of_sending() {
    let (mut app, owner, mut saved) = panel_fixture(Step::Closed);
    let world = app.world_mut();
    saved.agent.as_mut().unwrap().command = "node".into();
    saved.connections.check = Some(cell::fiote_connection::Check {
        profile: "old-node".into(),
        ready: false,
        stage: cell::fiote_communication::check::Stage::Handshake,
        detail: "Cannot find codex-acp/dist/index.js".into(),
        capabilities: Default::default(),
        settings: serde_json::json!({}),
    });
    let binding = world.get::<Panel>(owner).unwrap().binding.clone();
    let mut panel = world.get_mut::<Panel>(owner).unwrap();
    panel.automatic = true;
    panel.saved = Some(saved);
    assert!(!ready(world, &binding));
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Connections);
    for label in [
        "Continue with ChatGPT",
        "New native API / local connection",
        "Disconnect Node.js connection",
    ] {
        assert!(
            world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0 == label),
            "{label}"
        );
    }
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.contains("Cannot find codex-acp"))
    );
}

#[test]
fn browser_login_link_and_copy_feedback_survive_stale_acp_status_updates() {
    let (mut app, owner, mut saved) = panel_fixture(Step::Browser);
    let world = app.world_mut();
    let url = "https://login.example/authorize?state=fixture&code_challenge=challenge";
    saved.login_pending = true;
    world.get_mut::<Panel>(owner).unwrap().step = Step::Credentials;
    apply_status(world, owner, saved.clone());
    let status = world.get::<Panel>(owner).unwrap().status;
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Browser);
    assert_eq!(
        world.get::<Text>(status).unwrap().0,
        "Finish signing in in your browser."
    );
    saved.login_url = Some(url.into());
    saved.agent_info.as_mut().unwrap()["connectionCheck"] = serde_json::json!({
        "ready":false, "detail":"Cannot find codex-acp/dist/index.js"
    });
    saved.agent_info.as_mut().unwrap()["deviceCode"] = serde_json::json!({
        "verificationUri":"https://agent.example/login"
    });
    world.get_mut::<Panel>(owner).unwrap().saved = Some(saved.clone());
    show(world, owner, Step::Browser);
    let content = world.get::<Panel>(owner).unwrap().content;
    let status = world.get::<Panel>(owner).unwrap().status;
    let children: Vec<_> = world.get::<Children>(content).unwrap().iter().collect();
    let copy = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == "Copy login link")
        .unwrap()
        .0;
    let button = world.get::<ChildOf>(copy).unwrap().parent();
    world
        .resource_mut::<InputFocus>()
        .set(button, FocusCause::Pressed);
    let action = world
        .get::<crate::actions::ActionButton>(button)
        .unwrap()
        .clone();
    action.actions.run(world, action.target);
    let feedback = world.get::<Text>(status).unwrap().0.clone();
    assert!(feedback.starts_with("Could not copy the login link."));
    for index in 0..3 {
        saved.agent_info.as_mut().unwrap()["poll"] = index.into();
        apply_status(world, owner, saved.clone());
        assert!(world.get::<Panel>(owner).unwrap().step == Step::Browser);
        assert_eq!(
            world
                .get::<Children>(content)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            children
        );
        assert_eq!(world.get::<Text>(status).unwrap().0, feedback);
        assert_eq!(world.resource::<InputFocus>().get(), Some(button));
        assert!(world.query::<&Text>().iter(world).any(|text| text.0 == url));
        assert!(
            !world
                .query::<&Text>()
                .iter(world)
                .any(|text| text.0.contains("Cannot find codex-acp"))
        );
    }
    saved.login_url = Some("https://login.example/authorize?state=new".into());
    apply_status(world, owner, saved);
    assert!(!world.entities().contains(button));
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "https://login.example/authorize?state=new")
    );
}

#[test]
fn laboratory_browser_shows_the_copyable_native_url_from_the_connection_picker() {
    let (mut app, owner, saved) = panel_fixture(Step::Connections);
    let world = app.world_mut();
    let area = world.get::<Panel>(owner).unwrap().binding.area;
    world.get_mut::<Panel>(owner).unwrap().saved = Some(saved);
    let url = "https://login.example/authorize?state=laboratory";
    laboratory_browser(world, area, url);
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Browser);
    assert_eq!(world.get::<Panel>(owner).unwrap().saved.as_ref().unwrap().login_url.as_deref(), Some(url));
    assert!(world.query::<&Text>().iter(world).any(|text| text.0 == "Copy login link"));
}

#[test]
fn unchanged_connection_polls_keep_controls_and_focus() {
    let (mut app, owner, mut saved) = panel_fixture(Step::Connections);
    let world = app.world_mut();
    saved.connections.check = Some(cell::fiote_connection::Check {
        profile: "fixture".into(),
        ready: false,
        stage: cell::fiote_communication::check::Stage::Handshake,
        detail: "Cannot find codex-acp/dist/index.js".into(),
        capabilities: Default::default(),
        settings: serde_json::json!({}),
    });
    apply_status(world, owner, saved.clone());
    let content = world.get::<Panel>(owner).unwrap().content;
    let children: Vec<_> = world.get::<Children>(content).unwrap().iter().collect();
    let label = world
        .query::<(Entity, &Text)>()
        .iter(world)
        .find(|(_, text)| text.0 == "Continue with ChatGPT")
        .unwrap()
        .0;
    let button = world.get::<ChildOf>(label).unwrap().parent();
    world
        .resource_mut::<InputFocus>()
        .set(button, FocusCause::Pressed);
    for _ in 0..3 {
        apply_status(world, owner, saved.clone());
        assert_eq!(
            world
                .get::<Children>(content)
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            children
        );
        assert_eq!(world.resource::<InputFocus>().get(), Some(button));
    }
    saved.connections.check.as_mut().unwrap().detail = "New check result".into();
    update_connections(world, &saved);
    assert!(!world.entities().contains(button));
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.contains("New check result"))
    );
}

#[test]
fn disconnected_fiote_login_and_send_offer_generic_routes() {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<crate::cell_bridge::CellMessage>()
        .add_plugins(Plugin);
    let world = app.world_mut();
    let parent = world.spawn(Node::default()).id();
    let binding = RecordBinding {
        area: parent,
        uid: nucleus::new_uid("r"),
        source: Source::Local,
    };
    populate(world, parent, binding.clone());
    let owner = world
        .query::<(Entity, &Panel)>()
        .iter(world)
        .next()
        .unwrap()
        .0;
    let mut saved = fixture_status(&binding.uid);
    saved.agent = None;
    saved.agent_info = None;
    saved.settings.enabled = false;
    let mut panel = world.get_mut::<Panel>(owner).unwrap();
    panel.automatic = true;
    panel.saved = Some(saved);
    assert!(command(world, &binding, "/login"));
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Connections);
    show(world, owner, Step::Closed);
    assert!(!ready(world, &binding));
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Connections);
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "New optional ACP connection")
    );
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "New native API / local connection")
    );
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "External AI tools")
    );
}

#[test]
fn external_login_picker_preserves_fields_during_polling_and_uses_arrow_keys() {
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<crate::cell_bridge::CellMessage>()
        .add_plugins(Plugin);
    let world = app.world_mut();
    let owner = world.spawn_empty().id();
    let status = world.spawn(Text::default()).id();
    let content = world.spawn((Node::default(), ChildOf(owner))).id();
    let binding = RecordBinding {
        area: owner,
        uid: nucleus::new_uid("r"),
        source: Source::Local,
    };
    let saved = fixture_status(&binding.uid);
    world.entity_mut(owner).insert(Panel {
        view_uid: binding.uid.clone(),
        pending_command: None,
        binding: binding.clone(),
        status,
        content,
        step: Step::Closed,
        provider: None,
        method: None,
        labels: vec![],
        selection: 0,
        choices: vec![],
        fields: vec![],
        pending: None,
        saved: Some(saved.clone()),
        poll: std::time::Instant::now(),
        automatic: true,
    });
    assert!(command(world, &binding, "/login"));
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Agent);
    let key = world.get::<Panel>(owner).unwrap().fields[4];
    world
        .get_mut::<EditableText>(key)
        .unwrap()
        .editor
        .set_text("entered-secret");
    apply_status(world, owner, saved.clone());
    assert_eq!(world.get::<Panel>(owner).unwrap().fields[4], key);
    assert_eq!(
        world.get::<EditableText>(key).unwrap().value(),
        "entered-secret"
    );
    assert_eq!(world.get::<Panel>(owner).unwrap().fields.len(), 5);
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Check connection · no tokens")
    );
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Connection not checked")
    );
    let mut failed = saved.clone();
    failed.agent_info.as_mut().unwrap()["connectionCheck"] = serde_json::json!({
        "ready":false,"agent":"Could not start","login":"Not checked","model":"Not checked","session":"Not checked","detail":"Install the agent or choose its full path."
    });
    world.entity_mut(owner).insert(agent::OpenAfterSetup);
    apply_status(world, owner, failed);
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Agent);
    assert!(world.get::<agent::OpenAfterSetup>(owner).is_none());
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0 == "Last check: connection needs attention")
    );
    assert!(
        world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.contains("No model tokens are used"))
    );
    let mut picker = saved.clone();
    picker
        .agent_info
        .as_mut()
        .unwrap()
        .as_object_mut()
        .unwrap()
        .remove("selectedProvider");
    apply_status(world, owner, picker);
    world
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    keyboard(world);
    assert_eq!(world.get::<Panel>(owner).unwrap().selection, 1);
    assert!(
        !world
            .query::<&Text>()
            .iter(world)
            .any(|text| text.0.contains("entered-secret"))
    );
    let mut locked = saved;
    locked.requires_credential = true;
    world.get_mut::<Panel>(owner).unwrap().saved = Some(locked);
    assert!(command(world, &binding, "/login"));
    assert!(world.get::<Panel>(owner).unwrap().step == Step::Agent);
}

pub(super) async fn deliver(app: &mut App) {
    loop {
        let bridge = app
            .world_mut()
            .non_send_mut::<crate::cell_bridge::CellBridge>()
            .into_inner();
        let message =
            tokio::time::timeout(std::time::Duration::from_secs(15), bridge.incoming.recv())
                .await
                .unwrap()
                .unwrap();
        app.world_mut()
            .write_message(crate::cell_bridge::CellMessage(message));
        app.update();
        let panels = app
            .world_mut()
            .query::<&Panel>()
            .iter(app.world())
            .any(|panel| panel.pending.is_some());
        let controls = app
            .world_mut()
            .query::<&ThreadControl>()
            .iter(app.world())
            .any(|control| control.pending.is_some());
        if !panels && !controls {
            break;
        }
    }
}

#[tokio::test]
async fn native_provider_credentials_remain_separate_from_agent_login() {
    let directory = tempfile::tempdir().unwrap();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let record = engine
        .act(
            engine::actions::Action::CreateAgent {
                head: "Fiote".into(),
                operated_by: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let host = std::sync::Arc::new(
        cell::fiote::Host::open(engine.clone(), directory.path().join("settings"))
            .await
            .unwrap(),
    );
    let runtime = cell::CellRuntime {
        speech: None,
        commands: Default::default(),
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: Some(host),
    };
    let bridge = crate::cell_bridge::connect(runtime, crate::wake::WakeSignal::new(|| {}));
    let mut app = App::new();
    app.init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<crate::cell_bridge::CellMessage>()
        .add_plugins(Plugin);
    app.world_mut().insert_non_send(bridge);
    let root = app.world_mut().spawn_empty().id();
    let binding = RecordBinding {
        area: root,
        uid: record.clone(),
        source: Source::Local,
    };
    populate(app.world_mut(), root, binding.clone());
    deliver(&mut app).await;
    assert!(command(app.world_mut(), &binding, "/login"));
    let owner = app
        .world_mut()
        .query_filtered::<Entity, With<Panel>>()
        .single(app.world())
        .unwrap();
    assert!(app.world().get::<Panel>(owner).unwrap().step == Step::Connections);
    deliver(&mut app).await;
    let status = app.world().get::<Panel>(owner).unwrap().status;
    for message in [
        FioteRequest::Inspect {
            record: record.clone(),
        },
        FioteRequest::BrowserPoll {
            record: record.clone(),
        },
    ] {
        app.world_mut().get_mut::<Text>(status).unwrap().0 = "Stable login feedback".into();
        request(app.world_mut(), owner, message);
        assert!(app.world().get::<Panel>(owner).unwrap().pending.is_some());
        assert_eq!(
            app.world().get::<Text>(status).unwrap().0,
            "Stable login feedback"
        );
        deliver(&mut app).await;
    }
    let login = app
        .world_mut()
        .query::<(
            &crate::actions::ActionButton,
            &bevy::a11y::AccessibilityNode,
        )>()
        .iter(app.world())
        .find(|(_, node)| node.label() == Some("Other native providers"))
        .map(|(button, _)| button.clone())
        .unwrap();
    login.actions.run(app.world_mut(), login.target);
    assert!(app.world().get::<Panel>(owner).unwrap().step == Step::Providers);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowUp);
    keyboard(app.world_mut());
    let panel = app.world().get::<Panel>(owner).unwrap();
    assert_eq!(panel.selection, panel.choices.len() - 1);
    let button = app
        .world()
        .get::<ChildOf>(panel.choices[panel.selection])
        .unwrap()
        .parent();
    let list = app.world().get::<ChildOf>(button).unwrap().parent();
    assert!(app.world().get::<ScrollPosition>(list).unwrap().0.y > 0.0);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    keyboard(app.world_mut());
    assert_eq!(app.world().get::<ScrollPosition>(list).unwrap().0.y, 0.0);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::ArrowDown);
    keyboard(app.world_mut());
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    assert_eq!(app.world().get::<Panel>(owner).unwrap().selection, 1);
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    keyboard(app.world_mut());
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    assert!(app.world().get::<Panel>(owner).unwrap().provider.is_some());
    if app.world().get::<Panel>(owner).unwrap().step == Step::Methods {
        Choose(0).apply(app.world_mut(), owner);
    }
    let fields = app.world().get::<Panel>(owner).unwrap().fields.clone();
    for (index, value) in [
        (0, "test-model"),
        (1, "private-api-key"),
        (2, "vault-password"),
    ] {
        app.world_mut()
            .get_mut::<EditableText>(fields[index])
            .unwrap()
            .editor
            .set_text(value);
    }
    Continue.apply(app.world_mut(), owner);
    for index in [1, 2] {
        assert!(
            app.world()
                .get::<EditableText>(fields[index])
                .unwrap()
                .value()
                .to_string()
                .is_empty()
        );
    }
    deliver(&mut app).await;
    let panel = app.world().get::<Panel>(owner).unwrap();
    assert!(
        panel.step == Step::Closed,
        "{}",
        app.world().get::<Text>(panel.status).unwrap().0
    );
    assert!(panel.saved.as_ref().unwrap().has_key);
    assert!(command(app.world_mut(), &binding, "/lock"));
    deliver(&mut app).await;
    assert!(!ready(app.world_mut(), &binding));
    show(app.world_mut(), owner, Step::Unlock);
    let password = app.world().get::<Panel>(owner).unwrap().fields[0];
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .editor
        .set_text("vault-password");
    Continue.apply(app.world_mut(), owner);
    deliver(&mut app).await;
    assert!(ready(app.world_mut(), &binding));
    let thread = engine
        .act(
            engine::actions::Action::CreateThread {
                target: record.clone(),
                head: "External session".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    thread_controls(app.world_mut(), root, &thread, &binding);
    let control = app
        .world_mut()
        .query_filtered::<Entity, With<ThreadControl>>()
        .single(app.world())
        .unwrap();
    AgentTools.apply(app.world_mut(), control);
    deliver(&mut app).await;
    let connection = app
        .world()
        .get::<ThreadControl>(control)
        .unwrap()
        .connection
        .clone()
        .unwrap();
    assert!(connection.url.starts_with("http://127.0.0.1:"));
    assert!(!connection.token.0.is_empty());
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains(&connection.token.0))
    );
    AgentTools.apply(app.world_mut(), control);
    deliver(&mut app).await;
    assert!(
        app.world()
            .get::<ThreadControl>(control)
            .unwrap()
            .connection
            .is_none()
    );
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0.contains("private-api-key") || text.0.contains("vault-password"))
    );
    management::Open.apply(app.world_mut(), owner);
    deliver(&mut app).await;
    let panel = app.world().get::<Panel>(owner).unwrap();
    assert!(panel.step == Step::Manage);
    let saved = panel.saved.as_ref().unwrap();
    assert_eq!(saved.instructions.len(), 2);
    assert!(saved.behavior.run_assigned);
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Selected Fiote")
    );
    request(
        app.world_mut(),
        owner,
        FioteRequest::Behavior {
            record: record.clone(),
            prompt_parent: None,
            run_assigned: false,
        },
    );
    deliver(&mut app).await;
    let saved = app
        .world()
        .get::<Panel>(owner)
        .unwrap()
        .saved
        .as_ref()
        .unwrap();
    assert!(!saved.behavior.run_assigned);
    assert!(saved.behavior.prompt_parent.is_none());
    management::Refresh.apply(app.world_mut(), control);
    deliver(&mut app).await;
    let text = app
        .world()
        .get::<ThreadControl>(control)
        .unwrap()
        .instructions;
    assert!(
        app.world()
            .get::<Text>(text)
            .unwrap()
            .0
            .contains("revision")
    );
    let task = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Plain,
                head: "Task view".into(),
                body: "Task description".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act(
            engine::actions::Action::SetExtension {
                target: thread.clone(),
                namespace: "lince.fiote-task".into(),
                fds: serde_json::json!({"fiote":record,"task":task}),
            },
            None,
        )
        .await
        .unwrap();
    let task_binding = RecordBinding {
        uid: task.clone(),
        ..binding.clone()
    };
    populate(app.world_mut(), root, task_binding.clone());
    deliver(&mut app).await;
    assert!(thread_command(
        app.world_mut(),
        &task_binding,
        &thread,
        "/login"
    ));
    deliver(&mut app).await;
    let task_panel = app
        .world_mut()
        .query::<&Panel>()
        .iter(app.world())
        .find(|panel| panel.view_uid == task)
        .unwrap();
    assert_eq!(task_panel.binding.uid, record);
    assert!(task_panel.step == Step::Connections);
    let query = serde_json::from_value(serde_json::json!({"source":"record","where":[{"uid_eq":task}],"fields":["kind"],"limit":1})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows[0]["kind"], "plain");
}
