use super::*;
use cell::{ClientMessage, ServerMessage};

struct Transport {
    requests: tokio::sync::mpsc::Receiver<ClientMessage>,
    session: cell::Session,
    creates: usize,
    extensions: usize,
}

impl Transport {
    fn attach(app: &mut App, runtime: &cell::CellRuntime) -> Self { Self::with_pressure(app, runtime, false) }

    fn with_pressure(app: &mut App, runtime: &cell::CellRuntime, blocked: bool) -> Self {
        let (sender, requests) = tokio::sync::mpsc::channel(64);
        if blocked {
            for index in 0..64 { sender.try_send(ClientMessage::Unsubscribe { id: format!("startup-occupied-{index}") }).unwrap(); }
        }
        app.add_systems(PostStartup, move |mut bridge: NonSendMut<crate::native::cell_bridge::CellBridge>| { bridge.outgoing = sender.clone(); });
        app.update();
        Self { requests, session: runtime.local_session(), creates: 0, extensions: 0 }
    }

    async fn handle(&mut self, request: ClientMessage) -> Vec<ServerMessage> {
        match &request {
            ClientMessage::Act { action: engine::actions::Action::CreateRecordDraft { .. }, .. } => self.creates += 1,
            ClientMessage::Act { action: engine::actions::Action::SetExtension { .. }, .. } => self.extensions += 1,
            _ => {}
        }
        self.session.handle(request).await
    }

    fn deliver(app: &mut App, messages: Vec<ServerMessage>) {
        for message in messages { app.world_mut().write_message(CellMessage(message)); }
        app.update();
    }

    async fn action(&mut self, app: &mut App) -> ClientMessage {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.update();
            if let Ok(request) = self.requests.try_recv() {
                if matches!(request, ClientMessage::Act { .. }) { return request; }
                let messages = self.handle(request).await;
                Self::deliver(app, messages);
            } else { tokio::time::sleep(Duration::from_millis(2)).await; }
            assert!(Instant::now() < deadline, "The real opening editor did not send its Cell mutation");
        }
    }

    async fn until(&mut self, app: &mut App, predicate: impl Fn(&World) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            app.update();
            if predicate(app.world()) { return; }
            if let Ok(request) = self.requests.try_recv() {
                let messages = self.handle(request).await;
                Self::deliver(app, messages);
            } else {
                Self::deliver(app, self.session.refresh().await);
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            assert!(Instant::now() < deadline, "The controlled real Cell/editor exchange did not settle");
        }
    }
}

fn draft(app: &mut App, root: Entity, title: &str) -> Entity {
    editor::open(app.world_mut(), root, None, None, Some("org.example.Editor".into()));
    let owner = shortcuts::keyboard_owner(app.world()).unwrap();
    let state = app.world().get::<Editor>(owner).unwrap();
    let name = state.name.unwrap();
    let width = width(app, owner);
    set(app.world_mut(), name, title);
    set(app.world_mut(), width, "1/2");
    app.update();
    EditChange::Save.apply(app.world_mut(), owner);
    assert!(app.world().get::<Editor>(owner).unwrap().pending);
    owner
}

fn width(app: &mut App, owner: Entity) -> Entity {
    let world = app.world_mut();
    world.query::<(Entity, &Name)>().iter(world).find(|(entity, name)| {
        if name.as_str() != "Initial width (blank inherits)" { return false; }
        let mut current = Some(*entity);
        for _ in 0..crate::native::layout::MAX_DEPTH {
            let Some(entity) = current else { return false; };
            if entity == owner { return true; }
            current = world.get::<ChildOf>(entity).map(ChildOf::parent);
        }
        false
    }).unwrap().0
}

fn proposed(app: &App, owner: Entity) -> String { app.world().get::<Editor>(owner).unwrap().uid.clone().unwrap() }

async fn created(transport: &mut Transport, app: &mut App, owner: Entity) -> Vec<ServerMessage> {
    let request = transport.action(app).await;
    assert!(matches!(&request, ClientMessage::Act { action: engine::actions::Action::CreateRecordDraft { draft }, .. } if draft.uid == proposed(app, owner)));
    let messages = transport.handle(request).await;
    assert!(messages.iter().any(|message| matches!(message, ServerMessage::ActionOk { created: Some(uid), .. } if uid == &proposed(app, owner))));
    messages
}

async fn unrelated(engine: &engine::Engine) -> String {
    engine.act(engine::actions::Action::CreateRecord {
        slug: None, kind: nucleus::RecordKind::Plain, head: "Unrelated control".into(), body: "Keep this body".into(), quantity: 0.0,
    }, None).await.unwrap().created.unwrap()
}

#[tokio::test]
async fn an_acknowledged_creation_retries_the_same_record_after_queue_denial_revision_conflict_permission_denial_or_timeout() {
    for failure in 0..4 {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = provider(engine.clone());
        let control = unrelated(&engine).await;
        let directory = tempfile::tempdir().unwrap();
        let (mut app, root) = fixture(runtime.clone(), directory.path());
        let mut transport = Transport::attach(&mut app, &runtime);
        let owner = draft(&mut app, root, "Retry this identity");
        let uid = proposed(&app, owner);
        let messages = created(&mut transport, &mut app, owner).await;
        if failure == 0 {
            let sender = app.world().get_non_send::<crate::native::cell_bridge::CellBridge>().unwrap().outgoing.clone();
            let mut filled = 0;
            while sender.try_send(ClientMessage::Unsubscribe { id: format!("occupied-{filled}") }).is_ok() { filled += 1; assert!(filled <= 64); }
            assert!(filled > 0);
            Transport::deliver(&mut app, messages);
        } else {
            Transport::deliver(&mut app, messages);
            let request = transport.action(&mut app).await;
            assert!(matches!(&request, ClientMessage::Act { action: engine::actions::Action::SetExtension { target, .. }, .. } if target == &uid));
            match failure {
                1 => {
                    let competitor = Rule { revision: 1, enabled: true, match_all: true,
                        conditions: vec![nucleus::property_rule::PropertyRule { property: nucleus::property_rule::Property::ApplicationId, value: "org.example.Editor".into() }],
                        properties: Properties { width: Some(nucleus::window_opening::Dimension::Fraction([2, 3])), ..Default::default() } };
                    engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: serde_json::to_value(competitor).unwrap() }, None).await.unwrap();
                    let messages = transport.handle(request).await;
                    assert!(messages.iter().any(|message| matches!(message, ServerMessage::Error { message, .. } if message.contains("Reload"))));
                    Transport::deliver(&mut app, messages);
                }
                2 => {
                    let person = engine.act(engine::actions::Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Person,
                        head: "A different authenticated subject".into(), body: String::new(), quantity: 0.0 }, None).await.unwrap().created.unwrap();
                    let mut denied = cell::Session::new(engine.clone(), Arc::new(cell::LaneHub::new()), "opening-denied", Some(person));
                    let messages = denied.handle(request).await;
                    assert!(messages.iter().any(|message| matches!(message, ServerMessage::Error { code: Some(code), .. } if code == "action_intent_required")));
                    Transport::deliver(&mut app, messages);
                    assert!(store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE).await.unwrap().is_none());
                }
                _ => {
                    let deadline = Instant::now() + Duration::from_secs(15);
                    while app.world().get::<Editor>(owner).unwrap().pending {
                        app.update();
                        assert!(Instant::now() < deadline, "The real ten-second save deadline did not retire the pending mutation");
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    assert!(app.world().get::<Editor>(owner).unwrap().message.contains("timed out"));
                    let messages = transport.handle(request).await;
                    assert!(messages.iter().any(|message| matches!(message, ServerMessage::ActionOk { .. })));
                    Transport::deliver(&mut app, messages);
                }
            }
        }
        let state = app.world().get::<Editor>(owner).unwrap();
        assert_eq!(state.uid.as_deref(), Some(uid.as_str()));
        assert!(!state.creating && !state.pending && state.reload);
        assert_eq!(state.form.properties(app.world()).unwrap().width, Some(nucleus::window_opening::Dimension::Fraction([1, 2])));
        assert!(app.world().get::<Text>(state.status).unwrap().0.contains(&uid));
        assert!(app.world().get::<bevy::ui::InteractionDisabled>(state.save).is_some());
        assert!(active_order(app.world()).is_empty());
        EditChange::Reload.apply(app.world_mut(), owner);
        transport.until(&mut app, |world| world.get::<Editor>(owner).is_some_and(|state| !state.reloading && !state.reload && state.loaded)).await;
        let width = width(&mut app, owner);
        set(app.world_mut(), width, "1/2");
        app.update();
        EditChange::Save.apply(app.world_mut(), owner);
        transport.until(&mut app, |world| active_order(world) == vec![uid.clone()] && world.get::<Editor>(owner).is_some_and(|state| !state.pending && !state.needs_link)).await;
        let saved: Rule = serde_json::from_value(store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE).await.unwrap().unwrap()).unwrap();
        assert_eq!(saved.properties.width, Some(nucleus::window_opening::Dimension::Fraction([1, 2])));
        assert_eq!(transport.creates, 1);
        assert_eq!(transport.extensions, if failure == 1 { 2 } else { 1 });
        assert_eq!(store::records::list_all(&engine.store.pool).await.unwrap().iter().filter(|record| record.head == "Retry this identity").count(), 1);
        assert!(store::records::get_extension(&engine.store.pool, &control, RULE_NAMESPACE).await.unwrap().is_none());
        let unchanged = store::records::get(&engine.store.pool, &control).await.unwrap().unwrap();
        assert_eq!(unchanged.head, "Unrelated control");
        assert_eq!(unchanged.body, "Keep this body");
        assert_eq!(active_order(app.world()), vec![uid]);
    }
}

#[tokio::test]
async fn a_real_late_save_cannot_link_rules_or_steal_focus_after_editor_view_cell_or_authentication_replacement() {
    use crate::{host::{OutputId, OutputViewport, Rectangle, raster::RasterMapping}, native::output_views::OutputViews};
    for change in 0..7 {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = provider(engine.clone());
        let control = unrelated(&engine).await;
        let directory = tempfile::tempdir().unwrap();
        let (mut app, root) = fixture(runtime.clone(), directory.path());
        let mut transport = Transport::attach(&mut app, &runtime);
        let bounds = Rectangle::new([0.0; 2], [1000.0, 720.0]).unwrap();
        app.world_mut().insert_resource(OutputViews { canvas: Some(root), focused: OutputId(1), outputs: vec![OutputViewport {
            rotation: Default::default(), id: OutputId(1), bounds, usable: Some(bounds), physical_size: [1000, 720],
            raster: RasterMapping { origin: [0.0; 2], scale: [1.0; 2] }, workspace: 1,
        }] });
        let owner = draft(&mut app, root, "Late original");
        let uid = proposed(&app, owner);
        let messages = created(&mut transport, &mut app, owner).await;
        Transport::deliver(&mut app, messages);
        let request = transport.action(&mut app).await;
        let late = transport.handle(request).await;
        assert!(late.iter().any(|message| matches!(message, ServerMessage::ActionOk { .. })));
        let mut replacement = None;
        let mut replacement_channel = None;
        match change {
            0 => {
                EditChange::Cancel.apply(app.world_mut(), owner);
                editor::open(app.world_mut(), root, None, None, Some("another.application".into()));
                let next = shortcuts::keyboard_owner(app.world()).unwrap();
                let field = app.world().get::<Editor>(next).unwrap().name.unwrap();
                set(app.world_mut(), field, "Replacement draft");
                replacement = Some((next, field));
            }
            1 => {
                let mut spaces = app.world_mut().get_mut::<Workspaces>(root).unwrap();
                let mut workspace = spaces.entries[0].clone();
                workspace.id = 2;
                workspace.name = "Another workspace".into();
                spaces.entries.push(workspace);
                spaces.active = 2;
            }
            2 => { app.world_mut().remove_resource::<OutputViews>(); }
            3 => {
                let challenge = transport.session.initialize_action_intent().await;
                Transport::deliver(&mut app, vec![challenge]);
            }
            4 => {
                let (sender, receiver) = tokio::sync::mpsc::channel(64);
                app.world_mut().get_non_send_mut::<crate::native::cell_bridge::CellBridge>().unwrap().outgoing = sender;
                replacement_channel = Some(receiver);
                app.update();
            }
            5 => { app.world_mut().entity_mut(root).insert(Visibility::Hidden); }
            _ => {
                let next = app.world_mut().spawn((Node::default(), ChildOf(root))).id();
                let field = app.world_mut().spawn((Node::default(), EditableText::default(), ChildOf(next))).id();
                shortcuts::own_keyboard(app.world_mut(), next);
                app.world_mut().resource_mut::<InputFocus>().set(field, FocusCause::Navigated);
                replacement = Some((next, field));
            }
        }
        let focus = app.world().resource::<InputFocus>().get();
        Transport::deliver(&mut app, late);
        for _ in 0..20 { app.update(); }
        assert!(active_order(app.world()).is_empty(), "A late save linked its rule in replacement context {change}");
        assert!(app.world().resource::<Catalog>().pending.is_empty());
        assert_eq!(app.world().resource::<InputFocus>().get(), focus);
        if let Some((next, field)) = replacement {
            assert_eq!(shortcuts::keyboard_owner(app.world()), Some(next));
            assert_eq!(app.world().resource::<InputFocus>().get(), Some(field));
            if change == 0 {
                let state = app.world().get::<Editor>(next).unwrap();
                assert!(state.uid.is_none() && !state.pending);
                assert_eq!(app.world().get::<EditableText>(field).unwrap().value(), "Replacement draft");
            }
        } else {
            let state = app.world().get::<Editor>(owner).unwrap();
            assert_eq!(state.uid.as_deref(), Some(uid.as_str()));
            assert!(!state.pending && state.needs_link);
        }
        assert_eq!(store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE).await.unwrap().unwrap()["revision"], 1);
        assert!(store::records::get_extension(&engine.store.pool, &control, RULE_NAMESPACE).await.unwrap().is_none());
        let unchanged = store::records::get(&engine.store.pool, &control).await.unwrap().unwrap();
        assert_eq!(unchanged.head, "Unrelated control");
        assert_eq!(unchanged.body, "Keep this body");
        assert_eq!(transport.creates, 1);
        assert_eq!(transport.extensions, 1);
        drop(replacement_channel);
    }
}

#[tokio::test]
async fn configured_startup_waits_for_a_real_snapshot_or_its_terminal_deadline_and_ignores_a_retired_late_reply() {
    for case in 0..3 {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let runtime = provider(engine.clone());
        let uid = unrelated(&engine).await;
        let payload = Rule { revision: 1, enabled: true, match_all: true,
            conditions: vec![nucleus::property_rule::PropertyRule { property: nucleus::property_rule::Property::ApplicationId, value: "org.example.Editor".into() }],
            properties: Properties { width: Some(nucleus::window_opening::Dimension::Fraction([1, 2])), ..Default::default() } };
        engine.act(engine::actions::Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: serde_json::to_value(payload).unwrap() }, None).await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let mut settings = World::new();
        settings.insert_resource(crate::native::workspace::WorkspaceFile::new(directory.path().join("interface.json")));
        configuration::load(&mut settings);
        configuration::commit(&mut settings, Settings::default(), LayoutSettings { opening_rules: vec![uid.clone()], ..Default::default() }).unwrap();
        drop(settings);
        let (mut app, _) = fixture(runtime.clone(), directory.path());
        let mut transport = Transport::with_pressure(&mut app, &runtime, case == 1);
        assert_eq!(active_order(app.world()), vec![uid.clone()]);
        assert!(!super::super::super::ready(app.world()));
        if case == 1 {
            let feed = &app.world().resource::<Catalog>().rules;
            assert!(feed.error.is_some() && feed.deadline.is_some() && feed.retry.is_some());
        }
        if case == 2 {
            let limit = Instant::now() + Duration::from_secs(5);
            let request = loop {
                app.update();
                if let Ok(request) = transport.requests.try_recv() {
                    if matches!(&request, ClientMessage::Subscribe { protein, .. } if protein.include.extension.as_ref().is_some_and(|extension| extension.namespace == RULE_NAMESPACE)) { break request; }
                    let messages = transport.handle(request).await;
                    Transport::deliver(&mut app, messages);
                } else { tokio::time::sleep(Duration::from_millis(2)).await; }
                assert!(Instant::now() < limit);
            };
            let late = transport.handle(request).await;
            assert!(late.iter().any(|message| matches!(message, ServerMessage::Snapshot { rows, .. } if rows.iter().any(|row| row["uid"] == uid))));
            let load_deadline = app.world().resource::<Catalog>().rules.deadline.unwrap();
            let limit = load_deadline + Duration::from_secs(5);
            while !super::super::super::ready(app.world()) {
                app.update();
                assert!(Instant::now() < limit, "A configured startup did not settle by its bounded load deadline");
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert!(Instant::now() >= load_deadline);
            let feed = &app.world().resource::<Catalog>().rules;
            assert!(feed.blocked && feed.error.as_ref().unwrap().contains("timed out"));
            assert!(feed.deadline.is_none() && feed.retry.is_none());
            Transport::deliver(&mut app, late);
            assert!(!app.world().resource::<Catalog>().rules.entries.contains_key(&uid));
            catalog::refresh(app.world_mut());
        }
        transport.until(&mut app, |world| world.resource::<Catalog>().rules.entries.get(&uid).is_some_and(|entry| entry.payload.is_ok())).await;
        assert!(super::super::super::ready(app.world()));
        assert_eq!(super::super::super::capture(app.world(), &serde_json::json!({"app_id":"org.example.Editor"}), None).matched.unwrap().uid, uid);
        for _ in 0..20 { app.update(); }
        let catalog = app.world().resource::<Catalog>();
        assert!(catalog.clock.is_none() && catalog.pending.is_empty());
        assert!(catalog.rules.ready && catalog.rules.error.is_none());
    }
}
