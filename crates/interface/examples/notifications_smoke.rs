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
            lince_interface::actions![lince_interface::notifications::NotificationAction::Toggle],
        );
    }
    if frame < 12 {
        return;
    }
    match frame {
        18 => {
            let bell = world
                .query::<(Entity, &IconButton)>()
                .iter(world)
                .find(|(_, icon)| icon.label == "Notifications (2)")
                .unwrap()
                .0;
            world
                .resource_mut::<bevy::input_focus::InputFocus>()
                .set(bell, bevy::input_focus::FocusCause::Navigated);
        }
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
            let count = world
                .query::<(Entity, &Text)>()
                .iter(world)
                .find(|(_, text)| text.0 == "2")
                .unwrap()
                .0;
            let badge = world.get::<ChildOf>(count).unwrap().parent();
            let bell = world.get::<ChildOf>(badge).unwrap().parent();
            let badge_node = world.get::<ComputedNode>(badge).unwrap();
            let bell_node = world.get::<ComputedNode>(bell).unwrap();
            assert!((badge_node.size().x - badge_node.size().y).abs() < 0.5);
            assert!((badge_node.size().x / bell_node.size().x - 0.5).abs() < 0.05);
            let badge_position = world.get::<UiGlobalTransform>(badge).unwrap().translation;
            let bell_position = world.get::<UiGlobalTransform>(bell).unwrap().translation;
            assert!(badge_position.x > bell_position.x && badge_position.y < bell_position.y);
            let path = world.resource::<Capture>().path.clone();
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(|_: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
                    capture.saved = true;
                });
        }
        22 | 24 | 26 | 30 | 34 | 36 | 38 => {
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            if frame == 30 {
                assert_eq!(world.resource::<Notifications>().log.snapshot().1.len(), 2);
                assert!(
                    world
                        .query::<&IconButton>()
                        .iter(world)
                        .all(|icon| icon.label != "Close notification toast")
                );
            }
            if frame == 22 || frame == 30 || frame == 34 {
                let button = world
                    .query::<(Entity, &IconButton)>()
                    .iter(world)
                    .find(|(_, icon)| {
                        icon.label
                            == if frame == 22 {
                                "Close notification toast"
                            } else {
                                "Delete all notifications"
                            }
                    })
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
                    state: if frame == 24 || frame == 36 {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                }));
            }
        }
        46 => {
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
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _runtime = runtime.enter();
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
        "Notification smoke passed: restored history, background toast, closing a toast without deleting history, and saved deletion."
    );
}
