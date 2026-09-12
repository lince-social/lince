use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit},
    window::{PrimaryWindow, WindowCloseRequested, WindowCreated},
};
use lince_interface::{
    app::{CellHandle, connected_app},
    record_view::RecordEditor,
    tray::{TrayMessage, TraySender},
    wake::WakeSignal,
};
use std::sync::Arc;

#[derive(Resource)]
struct AsyncRuntime(tokio::runtime::Handle);

#[derive(Resource)]
struct Exercise {
    stage: u8,
    uid: String,
    capture_wait: u8,
    editor: Option<Entity>,
    path: String,
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("provide a screenshot output path");
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
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: "A real Record".into(),
                    body: "Retained body".into(),
                    quantity: 0.0,
                },
                None,
            )
            .await
            .unwrap()
            .created
            .unwrap();
        (
            cell::CellRuntime {
                store: engine.store.clone(),
                engine,
                lanes: Arc::new(cell::LaneHub::new()),
                wire: Default::default(),
                information: None,
            },
            uid,
        )
    });
    let _guard = runtime.enter();
    let mut app = connected_app(cell);
    app.insert_resource(AsyncRuntime(runtime.handle().clone()));
    app.insert_resource(Exercise {
        stage: 0,
        uid,
        capture_wait: 6,
        editor: None,
        path,
    })
    .add_systems(
        Update,
        exercise
            .after(lince_interface::record_view::ReceiveRecords)
            .after(lince_interface::tray::HandleTray),
    );
    runtime.spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;
        eprintln!("Frontend smoke timed out");
        std::process::exit(1);
    });
    app.run();
}

fn exercise(
    mut test: ResMut<Exercise>,
    mut editors: Query<(Entity, &mut EditableText, &RecordEditor)>,
    windows: Query<Entity, (With<PrimaryWindow>, With<Window>)>,
    mut close: MessageWriter<WindowCloseRequested>,
    mut created: MessageReader<WindowCreated>,
    tray: Res<TraySender>,
    wake: Res<WakeSignal>,
    cell: Res<CellHandle>,
    async_runtime: Res<AsyncRuntime>,
    mut commands: Commands,
) {
    let newly_created = created.read().count() > 0;
    let Some((editor, mut text, record)) = editors
        .iter_mut()
        .find(|(_, _, record)| record.uid == test.uid)
    else {
        return;
    };
    match test.stage {
        0 => {
            println!("Frontend smoke: Records loaded");
            assert_eq!(record.confirmed, "A real Record");
            test.editor = Some(editor);
            text.queue_edit(TextEdit::SelectAll);
            text.queue_edit(TextEdit::Insert("First automatic edit".into()));
            tray.send(TrayMessage::Available(true));
            test.stage = 1;
            wake.ring();
        }
        1 => {
            text.queue_edit(TextEdit::SelectAll);
            text.queue_edit(TextEdit::Insert("Saved through the Cell".into()));
            test.stage = 2;
            wake.ring();
        }
        2 if record.pending.is_none() && record.confirmed == "Saved through the Cell" => {
            println!("Frontend smoke: Save acknowledged, closing window");
            close.write(WindowCloseRequested {
                window: windows.single().unwrap(),
            });
            test.stage = 3;
            wake.ring();
        }
        3 if windows.is_empty() => {
            println!("Frontend smoke: window closed, making external edit");
            let runtime = cell.0.clone();
            let uid = record.uid.clone();
            async_runtime.0.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                runtime
                    .engine
                    .act(
                        engine::actions::Action::EditRecordText {
                            target: uid,
                            head: Some("Updated while closed".into()),
                            body: None,
                        },
                        None,
                    )
                    .await
                    .unwrap();
            });
            test.stage = 4;
        }
        4 if record.confirmed == "Updated while closed" => {
            println!("Frontend smoke: closed view woke with external edit");
            assert_eq!(test.editor, Some(editor));
            tray.send(TrayMessage::Show);
            test.stage = 5;
        }
        5 if newly_created && !windows.is_empty() => {
            println!("Frontend smoke: window restored, capturing");
            commands.insert_resource(bevy::winit::WinitSettings::continuous());
            assert_eq!(test.editor, Some(editor));
            assert_eq!(text.value().to_string(), "Updated while closed");
            test.stage = 6;
        }
        6 if test.capture_wait > 0 => {
            test.capture_wait -= 1;
        }
        6 => {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(test.path.clone()))
                .observe(|_: On<ScreenshotCaptured>, tray: Res<TraySender>| {
                    println!("Frontend smoke passed: real Records, Save Action, live update while closed, restored window and tray Quit.");
                    tray.send(TrayMessage::Quit);
                });
            test.stage = 7;
        }
        _ => {}
    }
}
