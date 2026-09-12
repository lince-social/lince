use bevy::{
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    app::interface_app, container::BoxRoot, icons::IconButton, notifications::Notifications,
};

#[derive(Resource)]
struct Capture {
    path: String,
    saved: bool,
    tooltip_checked: bool,
}

fn check_tooltips(
    tips: Query<(
        &GlobalZIndex,
        &Visibility,
        &Node,
        &ComputedNode,
        &UiGlobalTransform,
    )>,
    mut capture: ResMut<Capture>,
) {
    for (layer, visibility, node, computed, transform) in &tips {
        if layer.0 != 100 || *visibility != Visibility::Inherited || node.display == Display::None {
            continue;
        }
        let (Val::Px(left), Val::Px(top)) = (node.left, node.top) else {
            continue;
        };
        let scale = computed.inverse_scale_factor();
        let expected = Vec2::new(left, top) + computed.size() * scale * 0.5;
        assert!(
            (transform.translation * scale - expected).length() < 0.5,
            "a tooltip must already be in its final position on its first visible frame"
        );
        capture.tooltip_checked = true;
    }
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    if frame == 6 {
        let root = world
            .query_filtered::<Entity, With<BoxRoot>>()
            .single(world)
            .unwrap();
        lince_interface::actions::dispatch(
            world,
            root,
            lince_interface::actions![
                lince_interface::edit_mode::EditAction::Open,
                lince_interface::edit_mode::EditAction::Notifications
            ],
        );
    }
    if frame < 12 {
        return;
    }
    match frame {
        12 => {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == "Could not save the document. Your draft is still here.")
            );
            let log = world.resource::<Notifications>().log.clone();
            std::thread::spawn(move || {
                log.report(
                    "cell::connection",
                    "The connection to this Cell stopped. Reopen Lince to reconnect.",
                );
            })
            .join()
            .unwrap();
        }
        20 => {
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0.contains("connection to this Cell stopped"))
            );
            let path = world.resource::<Capture>().path.clone();
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(|_: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
                    capture.saved = true;
                });
        }
        22 | 30 | 32 | 34 => {
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            if frame == 22 || frame == 30 {
                let button = world
                    .query::<(Entity, &IconButton)>()
                    .iter(world)
                    .find(|(_, icon)| icon.label == "Dismiss all notifications")
                    .unwrap()
                    .0;
                let position = world.get::<UiGlobalTransform>(button).unwrap().translation
                    * world
                        .get::<ComputedNode>(button)
                        .unwrap()
                        .inverse_scale_factor();
                world.write_message(WindowEvent::CursorMoved(CursorMoved {
                    window,
                    position,
                    delta: None,
                }));
            } else {
                world.write_message(WindowEvent::MouseButtonInput(MouseButtonInput {
                    window,
                    button: MouseButton::Left,
                    state: if frame == 32 {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                }));
            }
        }
        42 => {
            assert!(
                world
                    .resource::<Notifications>()
                    .log
                    .snapshot()
                    .1
                    .is_empty(),
                "real click must dismiss the notices"
            );
            assert!(world.resource::<Capture>().saved);
            assert!(world.resource::<Capture>().tooltip_checked);
            world.write_message(AppExit::Success);
        }
        1800 => panic!("Notification smoke timed out"),
        _ => {}
    }
}

fn main() {
    let directory = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("provide an output directory"),
    );
    let history = directory.join("notifications.json");
    let old = cell::Diagnostics::default();
    let journal = cell::DiagnosticJournal::open(history.clone(), old.clone()).unwrap();
    old.report(
        "interface::save",
        "Could not save the document. Your draft is still here.",
    );
    drop(journal);
    let log = cell::Diagnostics::default();
    let journal = cell::DiagnosticJournal::open(history.clone(), log.clone()).unwrap();
    interface_app()
        .insert_resource(Notifications::new(log))
        .insert_resource(Capture {
            path: directory.join("notifications.png").display().to_string(),
            saved: false,
            tooltip_checked: false,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .add_systems(
            PostUpdate,
            check_tooltips.after(lince_interface::time_limit::TimeLimitSystems),
        )
        .run();
    drop(journal);
    let restored = cell::Diagnostics::default();
    let journal = cell::DiagnosticJournal::open(history, restored.clone()).unwrap();
    assert!(
        restored.snapshot().1.is_empty(),
        "dismissal must survive restart"
    );
    drop(journal);
    println!(
        "Notification smoke passed: restored history, background delivery, real dismissal click and saved dismissal."
    );
}
