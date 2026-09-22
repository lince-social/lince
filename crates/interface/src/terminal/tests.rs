use super::*;
use crate::sand_panel::tests::{app, connect, settle};

#[tokio::test]
async fn native_terminal_opens_renders_output_and_closes_after_removal() {
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let mut app = app();
    connect(&mut app, engine);
    app.add_plugins(TerminalPlugin);
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(app.world_mut(), owner, owner);
    app.update();
    assert!(app.world().resource::<Sessions>().0.is_empty());
    Command::Open.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        world.get::<TerminalSand>(owner).unwrap().opened
    })
    .await;
    let id = app
        .world()
        .get::<TerminalSand>(owner)
        .unwrap()
        .session
        .clone()
        .unwrap();
    app.world_mut()
        .get_mut::<TerminalSand>(owner)
        .unwrap()
        .input
        .push_back(b"printf '__native_terminal__\\n'\r".to_vec());
    settle(&mut app, |world| {
        world
            .get::<TerminalSand>(owner)
            .unwrap()
            .frame
            .as_ref()
            .is_some_and(|frame| {
                frame.lines.iter().any(|line| {
                    line.iter()
                        .map(|cell| cell.text.as_str())
                        .collect::<String>()
                        .contains("__native_terminal__")
                })
            })
    })
    .await;
    app.world_mut().despawn(owner);
    settle(&mut app, |world| world.resource::<Sessions>().0.is_empty()).await;
    panel::send(
        app.world(),
        ClientMessage::TerminalInput {
            id: id.clone(),
            data_base64: BASE64.encode(b"echo should-not-run\r"),
        },
    )
    .unwrap();
    let mut cursor =
        bevy::ecs::message::MessageCursor::<crate::cell_bridge::CellMessage>::default();
    for _ in 0..200 {
        app.update();
        let messages = app
            .world()
            .resource::<Messages<crate::cell_bridge::CellMessage>>();
        if cursor.read(messages).any(|message| matches!(&message.0, ServerMessage::Error { id: error_id, message, .. } if error_id == &id && message.contains("was not found"))) { return; }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    panic!("Removed terminal still accepted input");
}

#[test]
fn terminal_runs_coalesce_ascii_without_losing_wide_character_positions() {
    let cell = |text: &str, width| vt::Cell {
        text: text.into(),
        width,
        foreground: [220; 3],
        background: [0; 3],
        bold: false,
        underline: false,
    };
    let input = [
        cell("a", 1),
        cell("b", 1),
        cell("界", 2),
        cell(" ", 0),
        cell("c", 1),
    ];
    let runs = runs(&input);
    assert_eq!(runs.len(), 3);
    assert_eq!(
        (runs[0].start, runs[0].width, runs[0].text.as_str()),
        (0, 2, "ab")
    );
    assert_eq!((runs[1].start, runs[1].width), (2, 2));
    assert_eq!(runs[2].start, 4);
}
