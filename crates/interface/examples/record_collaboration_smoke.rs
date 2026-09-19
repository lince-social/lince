use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit},
};
use lince_interface::{app::connected_app, record_binding::TextBinding};
use std::sync::Arc;

#[derive(Resource)]
struct Exercise {
    uid: String,
    stage: u8,
    timer: Option<Entity>,
    frames: usize,
    path: String,
}

fn main() {
    let path = std::env::args().nth(1).expect("Screenshot path");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let (cell, uid) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let uid = engine
            .act(
                engine::actions::Action::CreateRecord {
                    slug: Some("collaborative-work".into()),
                    kind: nucleus::RecordKind::Plain,
                    head: "Collaborative work".into(),
                    body: "A shared description with accents and emoji: Olá 👩‍💻".into(),
                    quantity: 2.5,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let thread = engine
            .act(
                engine::actions::Action::CreateThread {
                    target: uid.clone(),
                    head: "Discussion".into(),
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        engine
            .act(
                engine::actions::Action::CreateMessage {
                    thread,
                    body: "The original author stays visible when others edit.".into(),
                    author: None,
                    state: nucleus::MessageState::Finished,
                    parent: None,
                    references: Vec::new(),
                },
                None,
            )
            .await
            .unwrap();
        (
            cell::CellRuntime {
                store: engine.store.clone(),
                engine,
                lanes: Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
                fiote: None,
                information: None,
            },
            uid,
        )
    });
    let _runtime = runtime.enter();
    let participant = cell.clone();
    let shared = uid.clone();
    runtime.spawn(async move {
        let mut session = cell::Session::new(
            participant.engine.clone(),
            participant.lanes.clone(),
            "other-editor",
            None,
        );
        session
            .handle(cell::ClientMessage::CollabJoin {
                id: "join".into(),
                record_uid: shared.clone(),
            })
            .await;
        let doc = loro::LoroDoc::new();
        doc.import(
            &B64.decode(participant.engine.collab_snapshot(&shared).await.unwrap())
                .unwrap(),
        )
        .unwrap();
        let cursor = B64.encode(
            doc.get_text("body")
                .get_cursor(7, Default::default())
                .unwrap()
                .encode(),
        );
        loop {
            session
                .handle(cell::ClientMessage::CollabPresence {
                    record_uid: shared.clone(),
                    property: "body".into(),
                    anchor: cursor.clone(),
                    focus: cursor.clone(),
                })
                .await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    let mut app = connected_app(cell);
    app.insert_resource(Exercise {
        uid,
        stage: 0,
        timer: None,
        frames: 0,
        path,
    });
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    runtime.spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(90)).await;
        eprintln!("Collaboration UI timed out");
        std::process::exit(1);
    });
    app.add_systems(Update, exercise);
    app.run();
}

fn exercise(world: &mut World) {
    let mut test = world.remove_resource::<Exercise>().unwrap();
    test.frames += 1;
    if test.frames % 300 == 0 {
        let labels: Vec<_> = world
            .query::<&Text>()
            .iter(world)
            .map(|text| text.0.clone())
            .filter(|text| !text.is_empty())
            .take(90)
            .collect();
        println!(
            "Collaboration stage {}, frame {}: {:?}",
            test.stage, test.frames, labels
        );
        world
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(test.path.clone()));
    }
    if test.frames > 18000 {
        panic!("Collaboration UI did not finish at stage {}", test.stage);
    }
    if test.stage == 0 {
        if let Some(root) = world
            .query_filtered::<Entity, With<lince_interface::workspace::Workspaces>>()
            .iter(world)
            .next()
        {
            let workspace = world
                .get::<lince_interface::workspace::Workspaces>(root)
                .unwrap()
                .active;
            let full = lince_interface::full_record::open(
                world,
                root,
                &test.uid,
                lince_interface::protein_area::Source::Local,
            )
            .unwrap();
            world
                .get_mut::<lince_interface::area::InfluenceArea>(full)
                .unwrap()
                .center = [-320.0, 0.0];
            world
                .get_mut::<lince_interface::canvas::CanvasItem>(full)
                .unwrap()
                .position = bevy::math::DVec2::new(-320.0, 0.0);
            let card = lince_interface::full_record::open(
                world,
                root,
                &test.uid,
                lince_interface::protein_area::Source::Local,
            )
            .unwrap();
            {
                let mut area = world
                    .get_mut::<lince_interface::area::InfluenceArea>(card)
                    .unwrap();
                area.center = [320.0, -200.0];
                area.protein
                    .as_mut()
                    .unwrap()
                    .bindings
                    .retain(|binding| matches!(binding.property.as_str(), "head" | "body"));
            }
            world
                .get_mut::<lince_interface::canvas::CanvasItem>(card)
                .unwrap()
                .position = bevy::math::DVec2::new(320.0, -200.0);
            test.timer = Some(lince_interface::sand_store::spawn_sand(
                world,
                root,
                workspace,
                lince_interface::sand_store::SandKind::WorkTimer,
                "collaborative-work",
                bevy::math::DVec2::new(350.0, 200.0),
            ));
            test.stage = 1;
        }
    }
    let editor = world
        .query::<(Entity, &TextBinding)>()
        .iter(world)
        .find(|(_, binding)| binding.record.uid == test.uid && binding.property == "head")
        .and_then(|(entity, binding)| binding.status.map(|status| (entity, status)));
    if let Some((editor, status)) = editor {
        match test.stage {
            1 if world
                .get::<Text>(status)
                .is_some_and(|label| label.0 == "Saved")
                && world
                    .query::<&TextBinding>()
                    .iter(world)
                    .filter(|binding| binding.record.uid == test.uid && binding.property == "head")
                    .count()
                    >= 2 =>
            {
                world
                    .resource_mut::<bevy::input_focus::InputFocus>()
                    .set(editor, bevy::input_focus::FocusCause::Navigated);
                let mut text = world.get_mut::<EditableText>(editor).unwrap();
                text.queue_edit(TextEdit::SelectAll);
                text.queue_edit(TextEdit::Insert("Shared title · Olá 👩‍💻".into()));
                test.stage = 2;
            }
            2 if world
                .get::<Text>(status)
                .is_some_and(|label| label.0 == "Saved")
                && world
                    .get::<EditableText>(editor)
                    .unwrap()
                    .value()
                    .to_string()
                    == "Shared title · Olá 👩‍💻" =>
            {
                for (binding, text) in world.query::<(&TextBinding, &EditableText)>().iter(world) {
                    if binding.record.uid == test.uid && binding.property == "head" {
                        assert_eq!(text.value().to_string(), "Shared title · Olá 👩‍💻");
                    }
                }
                if activate_timer(world, test.timer.unwrap(), "Start") {
                    test.stage = 3;
                }
            }
            3 => {
                if activate_timer(world, test.timer.unwrap(), "Pause") {
                    test.stage = 4;
                    test.frames = 0;
                }
            }
            4 if test.frames > 60 => {
                let timer = test.timer.unwrap();
                let label = world
                    .query::<(&ChildOf, &Text)>()
                    .iter(world)
                    .any(|(parent, text)| parent.parent() == timer && text.0 == "Saved");
                if label {
                    let values: Vec<_> = world
                        .query::<&EditableText>()
                        .iter(world)
                        .map(|text| text.value().to_string())
                        .collect();
                    let data: Vec<_> = world
                        .query::<&lince_interface::area::RecordProperties>()
                        .iter(world)
                        .map(|record| record.0.clone())
                        .collect();
                    assert!(
                        values.iter().any(|value| value == "2.5"),
                        "Full Record quantity missing: {values:?}; data: {data:?}"
                    );
                    assert!(
                        world
                            .query::<&Text>()
                            .iter(world)
                            .any(|text| text.0.starts_with("Written by ")),
                        "Message author missing"
                    );
                    world.spawn(Screenshot::primary_window()).observe(save_to_disk(test.path.clone())).observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        println!("Collaboration UI passed: shared title, Full Record fields, thread authors, and timer start/pause.");
                        exit.write(AppExit::Success);
                    });
                    test.stage = 5;
                }
            }
            _ => {}
        }
    }
    world.insert_resource(test);
}

fn activate_timer(world: &mut World, timer: Entity, caption: &str) -> bool {
    if caption == "Pause"
        && !world
            .query::<(&ChildOf, &Text)>()
            .iter(world)
            .any(|(parent, text)| parent.parent() == timer && text.0 == "Saved")
    {
        return false;
    }
    let button = world
        .query::<(Entity, &lince_interface::actions::ActionButton)>()
        .iter(world)
        .find(|(entity, action)| {
            action.target == timer
                && world
                    .get::<Children>(*entity)
                    .into_iter()
                    .flatten()
                    .any(|child| {
                        world
                            .get::<Text>(*child)
                            .is_some_and(|text| text.0 == caption)
                    })
        })
        .map(|(_, action)| action.clone());
    if let Some(button) = button {
        button.actions.run(world, timer);
        true
    } else {
        false
    }
}
