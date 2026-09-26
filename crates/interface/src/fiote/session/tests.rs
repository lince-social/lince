use super::*;

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
    let saved = FioteStatus {
        session: None,
        behavior: Default::default(),
        instructions: Vec::new(),
        instruction_error: None,
        fiotes: Vec::new(),
        tasks: Vec::new(),
        agent: Some(cell::FioteAgentConfig {
            require_vault: false,
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
        record: binding.uid.clone(),
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
        tool_connections: vec![],
    };
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

async fn deliver(app: &mut App) {
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
    assert!(app.world().get::<Panel>(owner).unwrap().step == Step::Agent);
    show(app.world_mut(), owner, Step::Providers);
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
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Enter);
    keyboard(app.world_mut());
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .reset_all();
    assert_eq!(app.world().get::<Panel>(owner).unwrap().selection, 1);
    assert!(app.world().get::<Panel>(owner).unwrap().provider.is_some());
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
    assert!(task_panel.step == Step::Agent);
    let query = serde_json::from_value(serde_json::json!({"source":"record","where":[{"uid_eq":task}],"fields":["kind"],"limit":1})).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    assert_eq!(rows[0]["kind"], "plain");
}
