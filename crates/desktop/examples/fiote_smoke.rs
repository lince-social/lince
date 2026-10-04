use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::{a11y::AccessibilityNode, prelude::*, text::EditableText, ui_widgets::Activate};
use lince_desktop::{description::Description, protein_area::Source};
use std::sync::Arc;

#[derive(Resource)]
struct Exercise {
    record: String,
    stage: u8,
    ticks: usize,
    screenshot: String,
}

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .build()
        .unwrap();
    let (cell, record, _directory) = runtime.block_on(async {
        let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
        let directory = tempfile::tempdir().unwrap();
        let record = engine
            .act(
                engine::actions::Action::CreateAgent {
                    head: "Fiote live acceptance".into(),
                    operated_by: None,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        let host = Arc::new(
            cell::fiote::Host::open(engine.clone(), directory.path().join("settings"))
                .await
                .unwrap(),
        );
        let mut config: cell::FioteAgentConfig = serde_json::from_str(
            &std::env::var("LINCE_ACP_TEST_CONFIG").expect("LINCE_ACP_TEST_CONFIG"),
        )
        .unwrap();
        config.directory = directory.path().into();
        engine
            .act(
                engine::actions::Action::CreateThread {
                    target: record.clone(),
                    head: "Real conversation".into(),
                },
                None,
            )
            .await
            .unwrap();
        let cell = cell::CellRuntime {
            commands: Default::default(),
            speech: None,
            store: engine.store.clone(),
            engine,
            lanes: Arc::new(cell::LaneHub::new()),
            wire: Default::default(),
            fiote: Some(host),
            information: None,
        };
        let mut session = cell.local_session();
        for request in [
            cell::fiote_connection::Request::Save {
                profile: cell::fiote_connection::Profile {
                    id: "live-render".into(),
                    name: "Existing AI runtime".into(),
                    connection: cell::fiote_connection::Connection::Harness { config },
                },
            },
            cell::fiote_connection::Request::Select {
                id: "live-render".into(),
                api_key: None,
                password: None,
            },
        ] {
            let replies = session
                .handle(cell::ClientMessage::Fiote {
                    id: "render-profile".into(),
                    request: cell::FioteRequest::Connections {
                        record: record.clone(),
                        request,
                    },
                })
                .await;
            assert!(
                replies
                    .iter()
                    .any(|reply| matches!(reply, cell::ServerMessage::Fiote { .. })),
                "{replies:?}"
            );
        }
        (cell, record, directory)
    });
    let _entered = runtime.enter();
    runtime.spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(180)).await;
        eprintln!("Fiote rendered acceptance timed out");
        std::process::exit(1);
    });
    let mut app = lince_desktop::app::connected_app(cell);
    app.insert_resource(bevy::winit::WinitSettings::continuous());
    app.insert_resource(Exercise {
        record,
        stage: 0,
        ticks: 0,
        screenshot: std::env::args().nth(1).expect("screenshot path"),
    });
    app.add_systems(Update, exercise);
    app.run();
}

fn exercise(world: &mut World) {
    let mut exercise = world.remove_resource::<Exercise>().unwrap();
    exercise.ticks += 1;
    match exercise.stage {
        0 => {
            if let Some(root) = world
                .query_filtered::<Entity, With<lince_desktop::workspace::Workspaces>>()
                .iter(world)
                .next()
            {
                lince_desktop::workspace::create(world, root);
                lince_desktop::full_record::open(world, root, &exercise.record, Source::Local)
                    .unwrap();
                let mut view = world
                    .get_mut::<lince_desktop::canvas::CanvasView>(root)
                    .unwrap();
                view.center = bevy::math::DVec2::new(0.0, 250.0);
                view.zoom = 0.65;
                exercise.stage = 1;
            }
        }
        1 => {
            let input = world
                .query::<(
                    Entity,
                    &EditableText,
                    &AccessibilityNode,
                    &ComputedNode,
                    &InheritedVisibility,
                )>()
                .iter(world)
                .find(|(_, _, node, computed, visible)| {
                    node.label() == Some("Message") && computed.size().x > 0.0 && visible.get()
                })
                .map(|(entity, _, _, _, _)| entity);
            if let Some(input) = input {
                world.get_mut::<EditableText>(input).unwrap().editor.set_text("Hello. Reply with exactly: Hello from the real agent. Do not use tools or change files or records.");
                world
                    .resource_mut::<bevy::input_focus::InputFocus>()
                    .set(input, bevy::input_focus::FocusCause::Navigated);
                let send = world
                    .query::<(Entity, &AccessibilityNode)>()
                    .iter(world)
                    .find(|(_, node)| node.label() == Some("Send message"))
                    .unwrap()
                    .0;
                world.trigger(Activate { entity: send });
                eprintln!("Fiote rendered acceptance activated Send message");
                exercise.stage = 2;
            }
        }
        2 => {
            if world
                .query::<&Description>()
                .iter(world)
                .any(|description| description.source.trim() == "Hello from the real agent.")
            {
                assert!(
                    world
                        .query::<(&EditableText, &AccessibilityNode)>()
                        .iter(world)
                        .any(|(text, node)| node.label() == Some("Message")
                            && text.value().to_string().is_empty())
                );
                eprintln!("Fiote rendered acceptance received the real reply");
                exercise.stage = 3;
                exercise.ticks = 0;
            }
        }
        3 if exercise.ticks > 30 => {
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(exercise.screenshot.clone()))
                .observe(
                    |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
            exercise.stage = 4;
        }
        _ => {}
    }
    world.insert_resource(exercise);
}
