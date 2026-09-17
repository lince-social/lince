use super::*;
use bevy::math::DVec2;
use serde_json::json;

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.add_plugins(MinimalPlugins)
        .init_resource::<Assets<Font>>()
        .init_resource::<crate::theme::Typography>()
        .add_plugins(AccessControlPlugin);
    let root = app
        .world_mut()
        .spawn((
            crate::container::BoxRoot,
            crate::workspace::Workspaces::default(),
        ))
        .id();
    let sand = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::AccessControl,
        "",
        DVec2::ZERO,
    );
    (app, root, sand)
}

fn catalog(world: &mut World) {
    let mut catalog = world.resource_mut::<Catalog>();
    catalog.ready = true;
    catalog.rows = vec![
        json!({"kind":"role", "id":"1", "name":"reader", "revision":1, "permissions":["record:read"]}),
        json!({"kind":"role", "id":"2", "name":"writer", "revision":1, "permissions":["record:read","record:update"]}),
        json!({"kind":"permission_catalog", "keys":["record:read","record:update"]}),
        json!({"kind":"user", "id":"person-1", "username":"alice", "name":"Alice", "role":"reader", "active":true}),
    ];
}

fn set(world: &mut World, field: Entity, value: &str) {
    world
        .get_mut::<EditableText>(field)
        .unwrap()
        .editor
        .set_text(value);
}

fn notice(world: &World, sand: Entity) -> &str {
    let status = world.get::<AccessControlSand>(sand).unwrap().status;
    &world.get::<Text>(status).unwrap().0
}

#[test]
fn user_details_and_role_replacement_are_separate_and_passwords_are_not_saved() {
    let (mut app, _, sand) = fixture();
    catalog(app.world_mut());
    Command::Select("person-1".into()).apply(app.world_mut(), sand);
    let form = app.world().get::<UserForm>(sand).unwrap();
    let password = form.password;
    let name = form.name;
    set(app.world_mut(), password, "private-password");
    set(app.world_mut(), name, "Alice changed");
    Command::ChooseRole("writer".into()).apply(app.world_mut(), sand);
    let (action, _) = mutation(app.world(), sand, &Command::SaveUser).unwrap();
    assert!(
        matches!(action, engine::actions::Action::UpdateUser { name, password, .. } if name == "Alice changed" && password == "private-password")
    );
    let (action, _) = mutation(app.world(), sand, &Command::Assign).unwrap();
    assert!(
        matches!(action, engine::actions::Action::AssignRole { user, role } if user == "person-1" && role == "writer")
    );
    assert!(crate::sand_text::snapshot(app.world(), sand).is_empty());
    assert_eq!(
        app.world().get::<TextColor>(password).unwrap().0,
        Color::NONE
    );
    assert_eq!(
        app.world()
            .get::<bevy::a11y::AccessibilityNode>(password)
            .unwrap()
            .role(),
        accesskit::Role::PasswordInput
    );
    assert!(mutation(app.world(), sand, &Command::ConfirmDelete).is_err());
    Command::Delete.apply(app.world_mut(), sand);
    assert!(
        matches!(mutation(app.world(), sand, &Command::ConfirmDelete).unwrap().0, engine::actions::Action::DeleteUser { user } if user == "person-1")
    );
    Command::Tab(Tab::Roles).apply(app.world_mut(), sand);
    assert!(app.world().get::<EditableText>(password).is_none());
}

#[test]
fn permissions_use_the_catalog_and_pending_changes_cannot_be_submitted_twice() {
    let (mut app, _, sand) = fixture();
    catalog(app.world_mut());
    Command::Tab(Tab::Roles).apply(app.world_mut(), sand);
    Command::Select("1".into()).apply(app.world_mut(), sand);
    assert!(
        matches!(mutation(app.world(), sand, &Command::Permission("record:update".into(), true)).unwrap().0,
        engine::actions::Action::GrantPermission { role, permission } if role == "reader" && permission == "record:update")
    );
    assert!(
        matches!(mutation(app.world(), sand, &Command::Permission("record:read".into(), false)).unwrap().0,
        engine::actions::Action::RevokePermission { role, permission } if role == "reader" && permission == "record:read")
    );
    assert!(
        mutation(
            app.world(),
            sand,
            &Command::Permission("invented:permission".into(), true)
        )
        .is_err()
    );
    app.world_mut()
        .get_mut::<AccessControlSand>(sand)
        .unwrap()
        .pending = Some(("pending".into(), Mutation::Permission));
    Command::Tab(Tab::Users).apply(app.world_mut(), sand);
    assert!(app.world().get::<AccessControlSand>(sand).unwrap().tab == Tab::Roles);
    receive_message(
        app.world_mut(),
        ServerMessage::Error {
            id: "unrelated".into(),
            message: "ignored".into(),
            code: None,
        },
    );
    assert!(
        app.world()
            .get::<AccessControlSand>(sand)
            .unwrap()
            .pending
            .is_some()
    );
    receive_message(
        app.world_mut(),
        ServerMessage::Error {
            id: "pending".into(),
            message: "Permission denied".into(),
            code: None,
        },
    );
    assert!(
        app.world()
            .get::<AccessControlSand>(sand)
            .unwrap()
            .pending
            .is_none()
    );
    assert_eq!(notice(app.world(), sand), "Permission denied");
}

#[test]
fn large_user_lists_are_paged_and_idle_frames_preserve_the_editor() {
    let (mut app, _, sand) = fixture();
    catalog(app.world_mut());
    app.world_mut().resource_mut::<Catalog>().rows.extend((0..10_000).map(|i| json!({"kind":"user", "id":format!("user-{i}"),"username":format!("user{i}"),"name":format!("User {i}"),"role":"reader"})));
    editor(app.world_mut(), sand);
    list(app.world_mut(), sand);
    let editor_entity = app.world().get::<AccessControlSand>(sand).unwrap().editor;
    let input = app.world().get::<UserForm>(sand).unwrap().username;
    set(app.world_mut(), input, "unsaved");
    let count = app.world().entities().len();
    app.world_mut()
        .get_mut::<AccessControlSand>(sand)
        .unwrap()
        .dirty = false;
    for _ in 0..30 {
        app.update();
    }
    assert_eq!(app.world().entities().len(), count);
    assert!(
        count < 150,
        "Only one page should create UI entities, got {count}"
    );
    assert_eq!(
        app.world().get::<AccessControlSand>(sand).unwrap().editor,
        editor_entity
    );
    assert_eq!(value(app.world(), input).unwrap(), "unsaved");
    Command::Page(true).apply(app.world_mut(), sand);
    app.update();
    assert_eq!(app.world().get::<AccessControlSand>(sand).unwrap().page, 1);
    assert_eq!(value(app.world(), input).unwrap(), "unsaved");
}

async fn until(app: &mut App, condition: impl Fn(&World) -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            app.update();
            if condition(app.world()) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[test]
fn role_updates_preserve_drafts_and_confirmation_revision() {
    let (mut app, _, sand) = fixture();
    catalog(app.world_mut());
    Command::Tab(Tab::Roles).apply(app.world_mut(), sand);
    Command::Select("1".into()).apply(app.world_mut(), sand);
    let field = app.world().get::<RoleForm>(sand).unwrap().0;
    set(app.world_mut(), field, "my draft");
    app.world_mut().resource_mut::<Catalog>().rows[0]["revision"] = json!(2);
    app.world_mut().resource_mut::<Catalog>().rows[0]["name"] = json!("someone else's edit");
    refresh_role(app.world_mut(), sand);
    assert_eq!(value(app.world(), field).unwrap(), "my draft");
    assert!(matches!(
        mutation(app.world(), sand, &Command::RenameRole).unwrap().0,
        engine::actions::Action::RenameRole {
            role: 1,
            expected_revision: 1,
            ..
        }
    ));
    assert!(mutation(app.world(), sand, &Command::ConfirmDeleteRole).is_err());
    editor(app.world_mut(), sand);
    assert_eq!(app.world().get::<RoleEdit>(sand).unwrap().revision, 2);
    Command::DeleteRole.apply(app.world_mut(), sand);
    app.world_mut().resource_mut::<Catalog>().rows[0]["revision"] = json!(3);
    refresh_role(app.world_mut(), sand);
    assert!(matches!(
        mutation(app.world(), sand, &Command::ConfirmDeleteRole)
            .unwrap()
            .0,
        engine::actions::Action::DeleteRole {
            role: 1,
            expected_revision: 2
        }
    ));
    Command::CancelDeleteRole.apply(app.world_mut(), sand);
    refresh_role(app.world_mut(), sand);
    assert_eq!(app.world().get::<RoleEdit>(sand).unwrap().revision, 3);
}

async fn saved(app: &mut App, sand: Entity) {
    until(app, |world| {
        world
            .get::<AccessControlSand>(sand)
            .unwrap()
            .pending
            .is_none()
            && world.resource::<Catalog>().ready
    })
    .await;
    assert!(
        notice(app.world(), sand).starts_with("Saved."),
        "{}",
        notice(app.world(), sand)
    );
}

#[tokio::test]
async fn existing_backend_handles_user_lifecycle_role_replacement_and_permissions() {
    let (mut app, root, sand) = fixture();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine: engine.clone(),
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
    };
    app.insert_resource(crate::app::CellHandle(runtime))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins(crate::cell_bridge::CellBridgePlugin);
    until(&mut app, |world| world.resource::<Catalog>().ready).await;
    for name in ["reader", "writer"] {
        Command::Tab(Tab::Roles).apply(app.world_mut(), sand);
        let field = app.world().get::<RoleForm>(sand).unwrap().0;
        set(app.world_mut(), field, name);
        Command::CreateRole.apply(app.world_mut(), sand);
        saved(&mut app, sand).await;
    }
    let role_id = app
        .world()
        .resource::<Catalog>()
        .rows
        .iter()
        .find(|r| r["kind"] == "role" && r["name"] == "reader")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    Command::Select(role_id).apply(app.world_mut(), sand);
    Command::Permission("record:read".into(), true).apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert!(app.world().resource::<Catalog>().rows.iter().any(|r| {
        r["name"] == "reader"
            && r["permissions"]
                .as_array()
                .unwrap()
                .contains(&json!("record:read"))
    }));
    Command::Permission("record:read".into(), false).apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert!(
        app.world()
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|r| r["name"] == "reader" && r["permissions"].as_array().unwrap().is_empty())
    );
    Command::Tab(Tab::Users).apply(app.world_mut(), sand);
    let form = app.world().get::<UserForm>(sand).unwrap();
    let (username, name, password) = (form.username, form.name, form.password);
    set(app.world_mut(), username, "alice");
    set(app.world_mut(), name, "Alice");
    set(app.world_mut(), password, "a-long-test-password");
    Command::ChooseRole("reader".into()).apply(app.world_mut(), sand);
    Command::SaveUser.apply(app.world_mut(), sand);
    assert!(value(app.world(), password).unwrap().is_empty());
    saved(&mut app, sand).await;
    let uid = app
        .world()
        .get::<UserForm>(sand)
        .unwrap()
        .uid
        .clone()
        .unwrap();
    let login =
        || engine::private_password::PasswordInput::new(b"a-long-test-password".to_vec()).unwrap();
    assert!(engine.login_password("alice", login(), None).await.is_ok());
    let name = app.world().get::<UserForm>(sand).unwrap().name;
    set(app.world_mut(), name, "Alice updated");
    Command::SaveUser.apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert!(engine.login_password("alice", login(), None).await.is_ok());
    Command::ChooseRole("writer".into()).apply(app.world_mut(), sand);
    Command::Assign.apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert!(
        app.world()
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|r| r["id"] == uid && r["name"] == "Alice updated" && r["role"] == "writer")
    );
    let writer = app
        .world()
        .resource::<Catalog>()
        .rows
        .iter()
        .find(|row| row["kind"] == "role" && row["name"] == "writer")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    Command::Tab(Tab::Roles).apply(app.world_mut(), sand);
    Command::Select(writer.clone()).apply(app.world_mut(), sand);
    let field = app.world().get::<RoleForm>(sand).unwrap().0;
    set(app.world_mut(), field, "contributors");
    Command::RenameRole.apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert_eq!(
        app.world()
            .get::<AccessControlSand>(sand)
            .unwrap()
            .selected
            .as_deref(),
        Some(writer.as_str())
    );
    assert!(
        app.world()
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|row| row["id"] == uid && row["role"] == "contributors")
    );
    Command::DeleteRole.apply(app.world_mut(), sand);
    Command::ConfirmDeleteRole.apply(app.world_mut(), sand);
    until(&mut app, |world| {
        world
            .get::<AccessControlSand>(sand)
            .unwrap()
            .pending
            .is_none()
    })
    .await;
    assert!(notice(app.world(), sand).contains("still assigned"));
    Command::Tab(Tab::Users).apply(app.world_mut(), sand);
    Command::Select(uid.clone()).apply(app.world_mut(), sand);
    Command::ChooseRole("reader".into()).apply(app.world_mut(), sand);
    Command::Assign.apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    Command::Tab(Tab::Roles).apply(app.world_mut(), sand);
    Command::Select(writer.clone()).apply(app.world_mut(), sand);
    Command::DeleteRole.apply(app.world_mut(), sand);
    Command::ConfirmDeleteRole.apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert!(
        !app.world()
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|row| row["kind"] == "role" && row["id"] == writer)
    );
    Command::Tab(Tab::Users).apply(app.world_mut(), sand);
    Command::Select(uid.clone()).apply(app.world_mut(), sand);
    Command::Delete.apply(app.world_mut(), sand);
    Command::ConfirmDelete.apply(app.world_mut(), sand);
    saved(&mut app, sand).await;
    assert!(
        !app.world()
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|r| r["kind"] == "user" && r["id"] == uid)
    );
    assert!(engine.login_password("alice", login(), None).await.is_err());
    engine
        .act(
            engine::actions::Action::CreateRole {
                name: "admin".into(),
            },
            None,
        )
        .await
        .unwrap();
    let admin = engine
        .act(
            engine::actions::Action::CreateUser {
                username: "recovery".into(),
                name: "Recovery".into(),
                password: "recovery-test-password".into(),
                role: "admin".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    Command::Refresh.apply(app.world_mut(), sand);
    until(&mut app, |world| {
        world
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|row| row["id"] == admin)
    })
    .await;
    Command::Select(admin.clone()).apply(app.world_mut(), sand);
    Command::Delete.apply(app.world_mut(), sand);
    Command::ConfirmDelete.apply(app.world_mut(), sand);
    until(&mut app, |world| {
        world
            .get::<AccessControlSand>(sand)
            .unwrap()
            .pending
            .is_none()
    })
    .await;
    assert!(!notice(app.world(), sand).starts_with("Saved."));
    assert!(
        app.world()
            .resource::<Catalog>()
            .rows
            .iter()
            .any(|row| row["id"] == admin)
    );
    assert!(
        engine
            .login_password(
                "recovery",
                engine::private_password::PasswordInput::new(b"recovery-test-password".to_vec())
                    .unwrap(),
                None
            )
            .await
            .is_ok()
    );
    let other = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::AccessControl,
        "",
        DVec2::ZERO,
    );
    let subscription = app.world().resource::<Catalog>().subscription.clone();
    app.update();
    assert_eq!(app.world().resource::<Catalog>().subscription, subscription);
    app.world_mut().entity_mut(sand).despawn();
    app.world_mut().entity_mut(other).despawn();
    app.update();
    assert!(app.world().resource::<Catalog>().subscription.is_none());
}

#[test]
fn workspace_restart_restores_the_sand_without_credentials_or_drafts() {
    fn open(path: std::path::PathBuf) -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<bevy::input_focus::InputFocus>()
            .insert_resource(crate::workspace::WorkspaceFile::new(path))
            .add_plugins((crate::workspace::WorkspacePlugin, AccessControlPlugin));
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        app.update();
        (app, root)
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.json");
    let (mut app, root) = open(path.clone());
    let sand = crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::AccessControl,
        "",
        DVec2::new(10.0, 20.0),
    );
    catalog(app.world_mut());
    editor(app.world_mut(), sand);
    let form = app.world().get::<UserForm>(sand).unwrap();
    let (username, password) = (form.username, form.password);
    set(app.world_mut(), username, "unsaved-secret-user");
    set(app.world_mut(), password, "unsaved-secret-password");
    app.world_mut().write_message(bevy::app::AppExit::Success);
    app.update();
    drop(app);
    let mut snapshots: Vec<_> = std::fs::read_dir(path.with_extension("snapshots"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    snapshots.sort();
    assert!(!snapshots.is_empty());
    for snapshot in &snapshots {
        assert!(
            !std::fs::read_to_string(snapshot)
                .unwrap()
                .contains("unsaved-secret")
        );
    }
    assert!(
        std::fs::read_to_string(snapshots.last().unwrap())
            .unwrap()
            .contains("AccessControl")
    );
    let (mut app, _) = open(path);
    let (stored, item) = app.world_mut().query_filtered::<(&crate::sand_store::StoredSand, &crate::canvas::CanvasItem), With<AccessControlSand>>().single(app.world()).unwrap();
    assert_eq!(stored.kind, crate::sand_store::SandKind::AccessControl);
    assert_eq!(item.position, DVec2::new(10.0, 20.0));
    assert!(!app.world().resource::<Catalog>().ready);
}

#[test]
fn password_copy_and_cut_are_blocked_and_disconnect_clears_the_field() {
    let (mut app, _, sand) = fixture();
    catalog(app.world_mut());
    editor(app.world_mut(), sand);
    let password = app.world().get::<UserForm>(sand).unwrap().password;
    set(app.world_mut(), password, "private-password");
    app.world_mut()
        .get_mut::<EditableText>(password)
        .unwrap()
        .pending_edits = vec![
        bevy::text::TextEdit::Copy,
        bevy::text::TextEdit::Cut,
        bevy::text::TextEdit::Paste,
    ];
    app.update();
    let field = app.world().get::<EditableText>(password).unwrap();
    assert_eq!(field.pending_edits.len(), 1);
    assert!(matches!(
        field.pending_edits[0],
        bevy::text::TextEdit::Paste
    ));
    receive_message(
        app.world_mut(),
        ServerMessage::Error {
            id: crate::cell_bridge::CONNECTION.into(),
            message: "closed".into(),
            code: None,
        },
    );
    assert!(value(app.world(), password).unwrap().is_empty());
    assert!(!app.world().resource::<Catalog>().ready);
}
