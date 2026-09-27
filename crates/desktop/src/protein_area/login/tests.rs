use super::*;
use serde_json::json;

fn remote(app: &mut App, owner: Entity) -> tokio::sync::mpsc::Receiver<ClientMessage> {
    let config = Config {
        source: Source::Organ("remote-organ".into()),
        ..Config::default()
    };
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein = Some(config.clone());
    let (outgoing, requests) = tokio::sync::mpsc::channel(8);
    let (responses, incoming) = tokio::sync::mpsc::channel(8);
    let task = tokio::spawn(async move {
        let _responses = responses;
        std::future::pending::<()>().await;
    });
    app.world_mut().resource_mut::<Runtime>().sessions.insert(
        "remote-organ".into(),
        sessions::Session::connected(Remote {
            outgoing,
            incoming,
            task,
        }),
    );
    app.world_mut().resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(config.clone()),
            subscription: Some("remote-query".into()),
            remote: Some("remote-organ".into()),
            status: "Connecting".into(),
            pending: VecDeque::from([ClientMessage::Subscribe {
                id: "remote-query".into(),
                protein: config.query().unwrap(),
            }]),
            ..default()
        },
    );
    sync(app.world_mut());
    requests
}

fn receive(world: &mut World, owner: Entity, message: ServerMessage) {
    let organ = world.resource::<Runtime>().areas[&owner]
        .remote
        .clone()
        .unwrap();
    sessions::receive(world, &organ, message);
}

fn panel(world: &mut World, owner: Entity) -> Entity {
    world
        .query::<(Entity, &Form)>()
        .iter(world)
        .find(|(_, form)| form.area == owner && form.canvas)
        .unwrap()
        .0
}

fn credentials(world: &mut World, entity: Entity, username: &str, password: &str) {
    let form = world.get::<Form>(entity).unwrap();
    let (user, secret) = (form.username, form.password);
    world
        .get_mut::<EditableText>(user)
        .unwrap()
        .editor
        .set_text(username);
    world
        .get_mut::<EditableText>(secret)
        .unwrap()
        .editor
        .set_text(password);
}

fn hello(app: &mut App, owner: Entity) {
    receive(
        app.world_mut(),
        owner,
        ServerMessage::LiveHello {
            login_required: true,
        },
    );
    sync(app.world_mut());
}

fn authenticated(app: &mut App, owner: Entity) {
    receive(
        app.world_mut(),
        owner,
        ServerMessage::SessionAuthenticated {
            id: "interface-auth".into(),
            session_id: "session".into(),
            person: "person".into(),
            key_id: "key".into(),
        },
    );
}

#[tokio::test]
async fn locked_area_logs_in_without_edit_mode_and_clears_results_on_disconnect() {
    let (mut app, root, owner) = super::super::tests::fixture();
    let mut requests = remote(&mut app, owner);
    let entity = panel(app.world_mut(), owner);
    assert!(
        app.world()
            .get::<crate::edit_mode::EditMode>(root)
            .is_none()
    );
    hello(&mut app, owner);
    app.update();
    assert!(requests.try_recv().is_err());
    assert!(!app.world().resource::<Runtime>().areas[&owner].ready);
    let fields = app.world().get::<Form>(entity).unwrap().fields;
    assert_eq!(
        app.world().get::<Node>(fields).unwrap().display,
        Display::Flex
    );
    credentials(app.world_mut(), entity, "alice", "secret");
    let count = app.world().entities().len();
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(app.world().entities().len(), count);
    let password = app.world().get::<Form>(entity).unwrap().password;
    assert_eq!(value(app.world(), password).unwrap(), "secret");
    Command::Login.apply(app.world_mut(), entity);
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::LiveLogin { username, password } if username == "alice" && password == "secret")
    );
    assert!(value(app.world(), password).unwrap().is_empty());
    Command::Login.apply(app.world_mut(), entity);
    assert!(requests.try_recv().is_err());
    assert_eq!(
        app.world().get::<Node>(fields).unwrap().display,
        Display::None
    );
    authenticated(&mut app, owner);
    app.update();
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::Subscribe { id, .. } if id == "remote-query")
    );
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "remote-query".into(),
            rows: vec![json!({"uid":"remote-record", "head":"Remote task", "body":""})],
        },
    );
    app.update();
    assert_eq!(
        app.world().get::<Node>(entity).unwrap().display,
        Display::None
    );
    let row = app.world().resource::<Runtime>().areas[&owner].row_entities["remote-record"];
    assert_eq!(
        app.world().get::<RecordBinding>(row).unwrap().source,
        Source::Organ("remote-organ".into())
    );
    let config = app
        .world()
        .get::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .clone();
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: "connection".into(),
            message: "Session expired".into(),
            code: None,
        },
    );
    app.update();
    assert!(app.world().get_entity(row).is_err());
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .order
            .is_empty()
    );
    assert_eq!(
        app.world().get::<Node>(entity).unwrap().display,
        Display::Flex
    );
    assert_eq!(
        app.world().get::<InfluenceArea>(owner).unwrap().protein,
        config
    );
    let status = app.world().get::<Form>(entity).unwrap().status;
    assert_eq!(
        app.world().get::<Text>(status).unwrap().0,
        "Session expired"
    );
    hello(&mut app, owner);
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "remote-query".into(),
            rows: vec![json!({"uid":"late-record"})],
        },
    );
    assert!(
        app.world().resource::<Runtime>().areas[&owner]
            .data
            .is_empty()
    );
}

#[tokio::test]
async fn forms_cannot_mix_credentials_or_send_them_to_a_changed_source_or_workspace() {
    let (mut app, root, owner) = super::super::tests::fixture();
    let mut requests = remote(&mut app, owner);
    hello(&mut app, owner);
    let entity = panel(app.world_mut(), owner);
    let settings = app.world_mut().spawn(Node::default()).id();
    form(app.world_mut(), settings, owner, "remote-organ", false);
    let other = app
        .world_mut()
        .query::<(Entity, &Form)>()
        .iter(app.world())
        .find(|(_, form)| !form.canvas)
        .unwrap()
        .0;
    credentials(app.world_mut(), entity, "alice", "alice-secret");
    credentials(app.world_mut(), other, "bob", "bob-secret");
    app.world_mut()
        .get_mut::<crate::workspace::Workspaces>(root)
        .unwrap()
        .active = 2;
    Command::Login.apply(app.world_mut(), entity);
    assert!(requests.try_recv().is_err());
    app.world_mut()
        .get_mut::<crate::workspace::Workspaces>(root)
        .unwrap()
        .active = 1;
    Command::Login.apply(app.world_mut(), entity);
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::LiveLogin { username, password } if username == "alice" && password == "alice-secret")
    );
    let password = app.world().get::<Form>(other).unwrap().password;
    assert!(value(app.world(), password).unwrap().is_empty());
    hello(&mut app, owner);
    credentials(app.world_mut(), entity, "alice", "private");
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .source = Source::Organ("different-organ".into());
    Command::Login.apply(app.world_mut(), entity);
    assert!(requests.try_recv().is_err());
    sync(app.world_mut());
    assert!(app.world().get_entity(entity).is_err());
    let replacement = panel(app.world_mut(), owner);
    let password = app.world().get::<Form>(replacement).unwrap().password;
    assert!(value(app.world(), password).unwrap().is_empty());
    Command::Login.apply(app.world_mut(), replacement);
    assert!(requests.try_recv().is_err());
    app.world_mut()
        .get_mut::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .as_mut()
        .unwrap()
        .source = Source::Local;
    sync(app.world_mut());
    assert_eq!(
        app.world_mut().query::<&Form>().iter(app.world()).count(),
        0
    );
}

#[tokio::test]
async fn logging_in_from_one_area_clears_passwords_and_updates_every_form_for_that_organ() {
    let (mut app, root, owner) = super::super::tests::fixture();
    let mut requests = remote(&mut app, owner);
    let config = app
        .world()
        .get::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .clone()
        .unwrap();
    let mut area = InfluenceArea::new(
        crate::area::AreaShape::Square,
        bevy::math::DVec2::ZERO,
        bevy::math::DVec2::splat(500.0),
    );
    area.protein = Some(config.clone());
    let other = crate::area::spawn_area(app.world_mut(), root, 1, area).unwrap();
    start(app.world_mut(), other, config);
    hello(&mut app, owner);
    let first = panel(app.world_mut(), owner);
    let second = panel(app.world_mut(), other);
    credentials(app.world_mut(), first, "alice", "secret");
    credentials(app.world_mut(), second, "bob", "other-secret");
    Command::Login.apply(app.world_mut(), first);
    assert!(
        matches!(requests.try_recv().unwrap(), ClientMessage::LiveLogin { username, password } if username == "alice" && password == "secret")
    );
    Command::Login.apply(app.world_mut(), second);
    assert!(requests.try_recv().is_err());
    for entity in [first, second] {
        let form = app.world().get::<Form>(entity).unwrap();
        assert!(value(app.world(), form.password).unwrap().is_empty());
        assert_eq!(
            app.world().get::<Node>(form.fields).unwrap().display,
            Display::None
        );
        assert_eq!(
            app.world().get::<Text>(form.title).unwrap().0,
            "Logging in…"
        );
    }
    authenticated(&mut app, owner);
    app.update();
    for _ in 0..2 {
        assert!(matches!(
            requests.try_recv().unwrap(),
            ClientMessage::Subscribe { .. }
        ));
    }
    for owner in [owner, other] {
        assert!(app.world().resource::<Runtime>().areas[&owner].ready);
    }
}

#[tokio::test]
async fn granted_organ_unlocks_without_a_password_and_reconnect_preserves_the_query() {
    let (mut app, _, owner) = super::super::tests::fixture();
    let mut requests = remote(&mut app, owner);
    let entity = panel(app.world_mut(), owner);
    authenticated(&mut app, owner);
    app.update();
    assert!(matches!(
        requests.try_recv().unwrap(),
        ClientMessage::Subscribe { .. }
    ));
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Snapshot {
            id: "remote-query".into(),
            rows: Vec::new(),
        },
    );
    app.update();
    assert_eq!(
        app.world().get::<Node>(entity).unwrap().display,
        Display::None
    );
    let config = app
        .world()
        .get::<InfluenceArea>(owner)
        .unwrap()
        .protein
        .clone();
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: "connection".into(),
            message: "Disconnected".into(),
            code: None,
        },
    );
    sync(app.world_mut());
    Command::Reconnect.apply(app.world_mut(), entity);
    assert_eq!(
        app.world().get::<InfluenceArea>(owner).unwrap().protein,
        config
    );
    assert_eq!(
        app.world().resource::<Runtime>().areas[&owner].applied,
        config
    );
    assert!(!app.world().resource::<Runtime>().areas[&owner].ready);
    assert!(!app.world().resource::<Runtime>().areas[&owner].login_pending);
    assert_eq!(
        app.world().get::<Node>(entity).unwrap().display,
        Display::Flex
    );
}

#[tokio::test]
async fn password_input_is_masked_not_persisted_and_cleared_after_failed_login() {
    let (mut app, _, owner) = super::super::tests::fixture();
    let mut requests = remote(&mut app, owner);
    hello(&mut app, owner);
    let entity = panel(app.world_mut(), owner);
    credentials(app.world_mut(), entity, "alice", "secret-password");
    let password = app.world().get::<Form>(entity).unwrap().password;
    assert_eq!(
        app.world()
            .get::<bevy::a11y::AccessibilityNode>(password)
            .unwrap()
            .role(),
        accesskit::Role::PasswordInput
    );
    assert_eq!(
        app.world().get::<TextColor>(password).unwrap().0,
        Color::NONE
    );
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .pending_edits = vec![
        bevy::text::TextEdit::Copy,
        bevy::text::TextEdit::Cut,
        bevy::text::TextEdit::Paste,
    ];
    app.update();
    assert_eq!(
        app.world()
            .get::<EditableText>(password)
            .unwrap()
            .pending_edits
            .len(),
        1
    );
    let mask = app
        .world_mut()
        .query::<(&Text, &PasswordMask)>()
        .iter(app.world())
        .find(|(_, mask)| mask.0 == password)
        .unwrap()
        .0;
    assert_eq!(mask.0, "•".repeat("secret-password".len()));
    let stored = serde_json::to_string(app.world().get::<InfluenceArea>(owner).unwrap()).unwrap();
    assert!(!stored.contains("secret-password"));
    assert!(!stored.contains("alice"));
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .pending_paste = Some(bevy::clipboard::ClipboardRead::Pending(
        std::sync::Arc::new(std::sync::Mutex::new(None)),
    ));
    Command::Login.apply(app.world_mut(), entity);
    assert!(requests.try_recv().is_err());
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .pending_paste = None;
    Command::Login.apply(app.world_mut(), entity);
    assert!(matches!(
        requests.try_recv().unwrap(),
        ClientMessage::LiveLogin { .. }
    ));
    receive(
        app.world_mut(),
        owner,
        ServerMessage::Error {
            id: "connection".into(),
            message: "Invalid username or password".into(),
            code: None,
        },
    );
    sync(app.world_mut());
    assert!(!app.world().resource::<Runtime>().areas[&owner].login_pending);
    let status = app.world().get::<Form>(entity).unwrap().status;
    assert_eq!(
        app.world().get::<Text>(status).unwrap().0,
        "Invalid username or password"
    );
    assert!(value(app.world(), password).unwrap().is_empty());
    hello(&mut app, owner);
    let fields = app.world().get::<Form>(entity).unwrap().fields;
    assert_eq!(
        app.world().get::<Node>(fields).unwrap().display,
        Display::Flex
    );
}
