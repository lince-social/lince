use super::*;
use bevy::{
    input::{
        ButtonState, InputPlugin, InputSystems,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{InputFocusSystems, dispatch_focused_input},
    math::DVec2,
};

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    crate::laboratory::isolate(app.world_mut());
    app.init_resource::<Assets<Font>>()
        .init_resource::<Typography>()
        .add_plugins((
            InputPlugin,
            crate::workspace::WorkspacePlugin,
            crate::edit_mode::EditModePlugin,
        ))
        .add_systems(
            PreUpdate,
            dispatch_focused_input::<KeyboardInput>
                .in_set(InputFocusSystems::Dispatch)
                .after(InputSystems),
        );
    let root = app.world_mut().spawn(crate::container::BoxRoot).id();
    let window = app
        .world_mut()
        .spawn((
            Window::default(),
            bevy::window::PrimaryWindow,
            crate::actions::WindowActionTarget(root),
        ))
        .id();
    app.update();
    (app, root, window)
}

fn key(app: &mut App, window: Entity, key_code: KeyCode, state: ButtonState) {
    app.world_mut().write_message(KeyboardInput {
        key_code,
        state,
        window,
        logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified),
        text: None,
        repeat: false,
    });
    app.update();
}

fn enter(app: &mut App, sand: Entity, text: &str) {
    let input = app.world().get::<OperationSand>(sand).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .editor
        .set_text(text);
    app.update();
}

fn notice(app: &App, sand: Entity) -> &str {
    let status = app.world().get::<OperationSand>(sand).unwrap().status;
    &app.world().get::<Text>(status).unwrap().0
}

#[test]
fn ctrl_k_focuses_one_popup_and_escape_restores_focus() {
    let (mut app, root, window) = fixture();
    let previous = app.world_mut().spawn(ChildOf(root)).id();
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(previous, FocusCause::Navigated);
    key(&mut app, window, KeyCode::ControlLeft, ButtonState::Pressed);
    key(&mut app, window, KeyCode::KeyK, ButtonState::Pressed);
    let sand = app.world().get::<Popup>(root).unwrap().panel;
    let input = app.world().get::<OperationSand>(sand).unwrap().input;
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(input));
    OpenOperation.apply(app.world_mut(), root);
    assert_eq!(
        app.world_mut()
            .query::<&OperationSand>()
            .iter(app.world())
            .count(),
        1
    );
    key(
        &mut app,
        window,
        KeyCode::ControlLeft,
        ButtonState::Released,
    );
    key(&mut app, window, KeyCode::Escape, ButtonState::Pressed);
    assert!(app.world().get::<Popup>(root).is_none());
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(previous));
}

#[test]
fn suggestions_narrow_complete_and_refresh_without_mutating_records() {
    let (mut app, root, window) = fixture();
    app.world_mut()
        .write_message(CellMessage(ServerMessage::Snapshot {
            id: RECORDS.into(),
            rows: vec![
                serde_json::json!({"uid":"one", "slug":"apple", "head":"Apple"}),
                serde_json::json!({"uid":"two", "slug":"apricot", "head":"Apricot"}),
            ],
        }));
    OpenOperation.apply(app.world_mut(), root);
    let sand = app.world().get::<Popup>(root).unwrap().panel;
    enter(&mut app, sand, "@ap");
    assert_eq!(
        app.world().get::<OperationSand>(sand).unwrap().items.len(),
        2
    );
    key(&mut app, window, KeyCode::ArrowDown, ButtonState::Pressed);
    key(&mut app, window, KeyCode::Tab, ButtonState::Pressed);
    app.update();
    assert_eq!(
        app.world().get::<OperationSand>(sand).unwrap().query,
        "@apricot"
    );
    assert!(
        app.world()
            .get::<OperationSand>(sand)
            .unwrap()
            .pending
            .is_none()
    );
    enter(&mut app, sand, "/he");
    assert_eq!(
        app.world().get::<OperationSand>(sand).unwrap().items[0].0,
        "/help"
    );
    OperationAction::Submit.apply(app.world_mut(), sand);
    assert!(app.world().get::<Popup>(root).is_some());
    enter(&mut app, sand, "@apple");
    app.world_mut()
        .write_message(CellMessage(ServerMessage::Update {
            id: RECORDS.into(),
            rows: vec![],
        }));
    app.update();
    assert!(
        app.world()
            .get::<OperationSand>(sand)
            .unwrap()
            .items
            .is_empty()
    );
    OperationAction::Submit.apply(app.world_mut(), sand);
    assert!(notice(&app, sand).contains("exact @slug"));
    enter(&mut app, sand, "/help");
    OperationAction::Submit.apply(app.world_mut(), sand);
    assert!(app.world().get::<Popup>(root).is_none());
    assert!(
        app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Cheat sheet")
    );
    EditAction::General.apply(app.world_mut(), root);
    assert!(
        !app.world_mut()
            .query::<&Text>()
            .iter(app.world())
            .any(|text| text.0 == "Cheat sheet")
    );
}

#[test]
fn autocomplete_is_bounded_and_pending_text_does_not_submit() {
    let mut catalog = Catalog::default();
    for index in 0..10_000 {
        catalog.records.insert(
            format!("record-{index:05}"),
            (index.to_string(), String::new()),
        );
    }
    assert_eq!(suggestions(&catalog, "@record-099").len(), 6);
    assert_eq!(suggestions(&catalog, "@record-09999")[0].0, "@record-09999");
    assert!(suggestions(&catalog, "@absent").is_empty());
    let (mut app, root, _) = fixture();
    OpenOperation.apply(app.world_mut(), root);
    let sand = app.world().get::<Popup>(root).unwrap().panel;
    enter(&mut app, sand, "/help");
    let input = app.world().get::<OperationSand>(sand).unwrap().input;
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .pending_edits
        .push(bevy::text::TextEdit::Insert("x".into()));
    OperationAction::Submit.apply(app.world_mut(), sand);
    assert!(app.world().get::<Popup>(root).is_some());
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .pending_edits
        .clear();
    enter(&mut app, sand, "/he");
    app.world_mut()
        .get_mut::<EditableText>(input)
        .unwrap()
        .queue_edit(bevy::text::TextEdit::ImeSetCompose {
            value: String::new().into(),
            cursor: None,
        });
    OperationAction::Complete.apply(app.world_mut(), sand);
    assert_eq!(
        app.world()
            .get::<EditableText>(input)
            .unwrap()
            .value()
            .to_string(),
        "/help"
    );
}

#[test]
fn operation_sand_survives_workspace_save_and_restore() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("interface.json");
    let (mut app, root, _) = fixture();
    app.world_mut()
        .insert_resource(crate::workspace::WorkspaceFile::new(path.clone()));
    crate::sand_store::spawn_sand(
        app.world_mut(),
        root,
        1,
        crate::sand_store::SandKind::Operation,
        "",
        DVec2::ZERO,
    );
    app.update();
    assert_eq!(
        app.world_mut()
            .query::<&OperationSand>()
            .iter(app.world())
            .count(),
        1
    );
    drop(app);
    let mut restored = App::new();
    crate::laboratory::isolate(restored.world_mut());
    restored
        .init_resource::<Assets<Font>>()
        .init_resource::<Typography>()
        .insert_resource(crate::workspace::WorkspaceFile::new(path.clone()))
        .add_plugins((
            crate::workspace::WorkspacePlugin,
            crate::edit_mode::EditModePlugin,
        ));
    restored.world_mut().spawn(crate::container::BoxRoot);
    restored.update();
    assert_eq!(
        restored
            .world_mut()
            .query::<&OperationSand>()
            .iter(restored.world())
            .count(),
        1
    );
}

#[tokio::test]
async fn slug_submission_sets_zero_through_cell_and_preserves_other_records() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    for (slug, quantity) in [("apple", -7.25), ("apricot", 9.0)] {
        engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some(slug.into()),
                    kind: nucleus::RecordKind::Plain,
                    head: slug.into(),
                    body: "Keep this".into(),
                    quantity,
                },
                None,
            )
            .await
            .unwrap();
    }
    let runtime = cell::CellRuntime {
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        fiote: None,
        information: None,
    };
    let mut bridge = crate::cell_bridge::connect(runtime, crate::wake::WakeSignal::new(|| {}));
    bridge
        .outgoing
        .send(crate::cell_bridge::records_subscription())
        .await
        .unwrap();
    let initial = tokio::time::timeout(std::time::Duration::from_secs(5), bridge.incoming.recv())
        .await
        .unwrap()
        .unwrap();
    let (mut app, root, _) = fixture();
    app.world_mut().insert_non_send(bridge);
    app.world_mut().write_message(CellMessage(initial));
    OpenOperation.apply(app.world_mut(), root);
    let sand = app.world().get::<Popup>(root).unwrap().panel;
    enter(&mut app, sand, "@app");
    OperationAction::Submit.apply(app.world_mut(), sand);
    assert!(
        app.world()
            .get::<OperationSand>(sand)
            .unwrap()
            .pending
            .is_none()
    );
    enter(&mut app, sand, "@apple");
    OperationAction::Submit.apply(app.world_mut(), sand);
    let request = app
        .world()
        .get::<OperationSand>(sand)
        .unwrap()
        .pending
        .clone()
        .unwrap();
    OperationAction::Submit.apply(app.world_mut(), sand);
    assert_eq!(
        app.world().get::<OperationSand>(sand).unwrap().pending,
        Some(request)
    );
    let mut confirmed = false;
    let mut updated = false;
    while !confirmed || !updated {
        let message = {
            let mut bridge = app.world_mut().non_send_mut::<CellBridge>();
            tokio::time::timeout(std::time::Duration::from_secs(5), bridge.incoming.recv())
                .await
                .unwrap()
                .unwrap()
        };
        if let ServerMessage::Update { rows, .. } = &message {
            assert_eq!(
                rows.iter().find(|row| row["slug"] == "apple").unwrap()["quantity"],
                0.0
            );
            assert_eq!(
                rows.iter().find(|row| row["slug"] == "apricot").unwrap()["quantity"],
                9.0
            );
            updated = true;
        }
        confirmed |= matches!(&message, ServerMessage::ActionOk { .. });
        app.world_mut().write_message(CellMessage(message));
        app.update();
    }
    assert!(notice(&app, sand).contains("quantity set to zero"));
    assert!(
        app.world()
            .get::<OperationSand>(sand)
            .unwrap()
            .pending
            .is_none()
    );
    OperationAction::Submit.apply(app.world_mut(), sand);
    app.world_mut()
        .write_message(CellMessage(ServerMessage::Error {
            id: crate::cell_bridge::CONNECTION.into(),
            message: "Disconnected".into(),
            code: None,
        }));
    app.update();
    assert!(notice(&app, sand).contains("not confirmed"));
    assert!(
        app.world()
            .get::<OperationSand>(sand)
            .unwrap()
            .pending
            .is_none()
    );
    assert!(!app.world().resource::<Catalog>().ready);
}
