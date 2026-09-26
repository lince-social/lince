use super::*;
use crate::sand_panel::tests::{app, settle};
use serde_json::json;

#[tokio::test]
async fn removing_castle_keeps_run_alive_and_reopening_restores_history() {
    let directory = std::env::temp_dir().join(nucleus::new_uid("castle-test"));
    std::fs::create_dir(&directory).unwrap();
    let engine = std::sync::Arc::new(engine::Engine::open_memory().await.unwrap());
    let script = "printf before; read -r answer; printf 'after:%s\\n' \"$answer\"";
    let uid = engine
        .act(
            engine::actions::Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Command,
                head: "Interactive".into(),
                body: "printf original".into(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let runtime = cell::CellRuntime {
        commands: cell::terminal::commands::CommandHost::new(directory.clone()),
        store: engine.store.clone(),
        engine,
        lanes: std::sync::Arc::new(cell::LaneHub::new()),
        wire: Default::default(),
        information: None,
        fiote: None,
    };
    let mut app = app();
    app.insert_resource(crate::app::CellHandle(runtime.clone()))
        .insert_resource(crate::wake::WakeSignal::new(|| {}))
        .add_plugins((
            crate::cell_bridge::CellBridgePlugin,
            crate::terminal::TerminalPlugin,
            CommandCastlePlugin,
            crate::record_binding::RecordBindingPlugin,
        ));
    let config = crate::protein_area::Config {
        command: Some(Settings {
            cwd: "~".into(),
            show_output: true,
        }),
        ..crate::protein_area::Config::records()
    };
    let data = json!({"kind":"command", "head":"Interactive", "body":"printf original"});
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(
        app.world_mut(),
        owner,
        &config,
        &data,
        RecordBinding {
            area: owner,
            uid: uid.clone(),
            source: Source::Local,
        },
    );
    let castle = app.world().get::<CommandCastle>(owner).unwrap();
    let (head, body, binding) = (castle.head, castle.script, castle.binding.clone());
    for field in [head, body] {
        app.world_mut()
            .entity_mut(field)
            .remove::<(ComputedNode, InheritedVisibility)>();
    }
    settle(&mut app, |world| {
        crate::record_binding::status(world, &binding).as_deref() == Some("Saved")
    })
    .await;
    app.world_mut()
        .get_mut::<EditableText>(head)
        .unwrap()
        .editor
        .set_text("Saved command");
    app.world_mut()
        .get_mut::<EditableText>(body)
        .unwrap()
        .editor
        .set_text(script);
    settle(&mut app, |world| {
        !world
            .get::<crate::record_binding::TextBinding>(body)
            .unwrap()
            .unsaved(script)
    })
    .await;
    assert_eq!(
        runtime.engine.doc_text(&uid).await.unwrap(),
        ("Saved command".into(), script.into())
    );
    Control::Run.apply(app.world_mut(), owner);
    settle(&mut app, |world| {
        !world.get::<CommandCastle>(owner).unwrap().runs.is_empty()
    })
    .await;
    let run = app.world().get::<CommandCastle>(owner).unwrap().runs[0].clone();
    assert_eq!(
        app.world_mut()
            .query::<&crate::terminal::TerminalSand>()
            .iter(app.world())
            .count(),
        1
    );
    app.world_mut().despawn(owner);
    app.update();
    runtime
        .commands
        .request(
            &runtime.engine,
            Request::Input {
                command: uid.clone(),
                run: run.id.clone(),
                data_base64: "cmVjb25uZWN0ZWQK".into(),
            },
        )
        .await
        .unwrap();
    let reopened = app.world_mut().spawn(Node::default()).id();
    populate(
        app.world_mut(),
        reopened,
        &config,
        &data,
        RecordBinding {
            area: reopened,
            uid,
            source: Source::Local,
        },
    );
    settle(&mut app, |world| {
        world
            .get::<CommandCastle>(reopened)
            .unwrap()
            .runs
            .first()
            .is_some_and(|run| run.finished_ms.is_some())
    })
    .await;
    assert_eq!(
        app.world().get::<CommandCastle>(reopened).unwrap().runs[0].id,
        run.id
    );
    Control::Toggle(run.id).apply(app.world_mut(), reopened);
    let terminal = app
        .world_mut()
        .query_filtered::<Entity, With<crate::terminal::TerminalSand>>()
        .single(app.world())
        .unwrap();
    settle(&mut app, |world| {
        crate::terminal::displayed_text(world, terminal).contains("after:reconnected")
    })
    .await;
    app.world_mut().despawn(reopened);
    drop(app);
    drop(runtime);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn settings_survive_castle_configuration_roundtrip() {
    let config = crate::protein_area::Config {
        command: Some(Settings {
            cwd: "~/projects".into(),
            show_output: false,
        }),
        ..crate::protein_area::Config::records()
    };
    assert!(config.valid());
    let restored: crate::protein_area::Config =
        serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
    assert_eq!(restored.command, config.command);
    assert_eq!(Settings::default().cwd, "~");
}

#[test]
fn remote_command_has_no_local_execution_controls() {
    let mut app = app();
    let owner = app.world_mut().spawn(Node::default()).id();
    populate(
        app.world_mut(),
        owner,
        &crate::protein_area::Config::records(),
        &json!({"head":"Remote", "body":"printf untouched"}),
        RecordBinding {
            area: owner,
            uid: nucleus::new_uid("r"),
            source: Source::Organ(nucleus::new_uid("o")),
        },
    );
    assert!(app.world().get::<CommandCastle>(owner).is_none());
    assert_eq!(
        app.world_mut()
            .query::<&EditableText>()
            .iter(app.world())
            .count(),
        0
    );
}
