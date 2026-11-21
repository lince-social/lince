// non-lean-yet
// Scope: fn key, fn terminal_input, fn
//   typing_and_timer_advance_during_an_ordered_private_command_stream.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;
use crate::native::{
    app::CellHandle,
    cell_bridge::CellBridgePlugin,
    record_binding::RecordBindingPlugin,
    terminal::{TerminalPlugin, TerminalSand, displayed_text},
    work_timer::{self, WorkTimerPlugin},
};
use bevy::{
    input::{
        ButtonState, InputPlugin,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputDispatchPlugin, InputFocus, InputFocusPlugin},
    text::{FontCx, LayoutCx},
    window::{Ime, PrimaryWindow},
};

fn key(app: &mut App, window: Entity, code: KeyCode, value: &str) {
    for state in [ButtonState::Pressed, ButtonState::Released] {
        app.world_mut().write_message(KeyboardInput {
            key_code: code,
            logical_key: Key::Character(value.into()),
            text: state.is_pressed().then(|| value.into()),
            state,
            repeat: false,
            window,
        });
    }
    app.update();
}

fn terminal_input(world: &World, terminal: Entity, uid: &str, run: &Run, data: &str) {
    use base64::Engine as _;
    panel::send(
        world,
        terminal,
        ClientMessage::Command {
            id: nucleus::new_uid("test-stdin"),
            request: Request::Input {
                command: uid.into(),
                run: run.id.clone(),
                data_base64: base64::engine::general_purpose::STANDARD.encode(data),
            },
        },
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typing_and_timer_advance_during_an_ordered_private_command_stream() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let script = "printf 'stage:one\\n'; read -r first; printf 'stage:two:%s\\n' \"$first\"; read -r second; printf 'stage:three:%s\\n' \"$second\"; exit 7";
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Command,
                head: "Private stream".into(),
                body: script.into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let commands = cell::terminal::commands::CommandHost::new(directory.path().into());
    let cell = cell::CellRuntime {
        commands: commands.clone(),
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
        speech: None,
    };
    let driver = Driver::new();
    let mut app = app();
    crate::native::laboratory::isolate(app.world_mut());
    app.insert_resource(CellHandle(cell))
        .insert_resource(driver.signal())
        .init_resource::<FontCx>()
        .init_resource::<LayoutCx>()
        .init_resource::<UiScale>()
        .init_resource::<bevy::clipboard::Clipboard>()
        .add_message::<Ime>()
        .add_plugins((
            MinimalPlugins,
            InputPlugin,
            InputFocusPlugin,
            InputDispatchPlugin,
        ))
        .add_plugins((
            bevy::ui_widgets::TextInputPlugin,
            crate::controls::editing::TextEditingPlugin,
            CellBridgePlugin,
            CommandCastlePlugin,
            TerminalPlugin,
            RecordBindingPlugin,
            WorkTimerPlugin,
        ))
        .add_systems(
            PostUpdate,
            bevy::text::apply_text_edits.in_set(bevy::text::EditableTextSystems),
        );
    let font = Font::from_bytes(
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../institute/assets/fonts/Lato/Lato-Regular.ttf"
        ))
        .to_vec(),
    );
    let mut fonts = app.world_mut().resource_mut::<FontCx>();
    fonts.collection.register_fonts(font.data, None);
    fonts.set_sans_serif_family("Lato").unwrap();

    let window = app
        .world_mut()
        .spawn((Window::default(), PrimaryWindow))
        .id();
    let input = crate::native::sand::text_editor("", app.world().resource(), 0);
    let input = app.world_mut().spawn(input).id();
    let timer = app.world_mut().spawn(Node::default()).id();
    work_timer::populate(app.world_mut(), timer, None, &serde_json::Value::Null, None);
    let owner = app.world_mut().spawn(Node::default()).id();
    let config = crate::native::protein_area::Config {
        command: Some(Settings {
            cwd: directory.path().display().to_string(),
            show_output: true,
        }),
        ..crate::native::protein_area::Config::records()
    };
    let record = RecordBinding {
        area: owner,
        uid: uid.clone(),
        source: Source::Local,
    };
    populate(
        app.world_mut(),
        owner,
        &config,
        &json!({"kind":"command", "head":"Private stream", "body":script}),
        record,
    );
    app.update();
    Control::Run.apply(app.world_mut(), owner);
    driver.pending.store(true, Ordering::Release);
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            !world.get::<CommandCastle>(owner).unwrap().runs.is_empty()
        })
        .await;
    let run = app.world().get::<CommandCastle>(owner).unwrap().runs[0].clone();
    let terminal = app
        .world_mut()
        .query_filtered::<Entity, With<TerminalSand>>()
        .single(app.world())
        .unwrap();
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            displayed_text(world, terminal).contains("stage:one")
        })
        .await;
    assert!(displayed_text(app.world(), terminal).contains("stage:one"));

    assert!(has_text(app.world(), timer, "Total 00:00:00"));
    app.world_mut()
        .resource_mut::<InputFocus>()
        .set(input, FocusCause::Pressed);
    work_timer::Toggle.apply(app.world_mut(), timer);
    key(&mut app, window, KeyCode::KeyA, "a");
    assert_eq!(app.world().get::<EditableText>(input).unwrap().value(), "a");
    assert_eq!(work_timer::running(app.world(), timer), Some(true));
    assert!(!displayed_text(app.world(), terminal).contains("stage:two"));

    terminal_input(app.world(), terminal, &uid, &run, "alpha\n");
    key(&mut app, window, KeyCode::KeyB, "β");
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            displayed_text(world, terminal).contains("stage:two:alpha")
                && !has_text(world, timer, "Total 00:00:00")
        })
        .await;
    assert_eq!(
        app.world().get::<EditableText>(input).unwrap().value(),
        "aβ"
    );
    assert_eq!(app.world().resource::<InputFocus>().get(), Some(input));
    assert!(!has_text(app.world(), timer, "Total 00:00:00"));
    let intermediate = displayed_text(app.world(), terminal);
    assert!(
        intermediate.find("stage:one").unwrap() < intermediate.find("stage:two:alpha").unwrap()
    );
    assert!(!intermediate.contains("stage:three"));
    assert!(
        app.world().get::<CommandCastle>(owner).unwrap().runs[0]
            .finished_ms
            .is_none()
    );

    work_timer::Toggle.apply(app.world_mut(), timer);
    assert_eq!(work_timer::running(app.world(), timer), Some(false));
    assert!(work_timer::stopped_log(app.world(), timer));
    terminal_input(app.world(), terminal, &uid, &run, "omega\n");
    driver
        .pump(&mut app, Duration::from_secs(5), |world| {
            displayed_text(world, terminal).contains("stage:three:omega")
                && world.get::<CommandCastle>(owner).unwrap().runs[0].exit_code == Some(7)
        })
        .await;
    let output = displayed_text(app.world(), terminal);
    let markers: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("stage:"))
        .collect();
    assert_eq!(
        markers,
        ["stage:one", "stage:two:alpha", "stage:three:omega"]
    );
    assert_eq!(
        app.world().get::<CommandCastle>(owner).unwrap().runs[0].exit_code,
        Some(7)
    );
    assert_eq!(
        app.world().get::<EditableText>(input).unwrap().value(),
        "aβ"
    );
    app.world_mut().despawn(owner);
    app.update();
    assert!(app.world().get_entity(terminal).is_err());
    commands.shutdown().await;
}
