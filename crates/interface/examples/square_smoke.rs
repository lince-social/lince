use bevy::{
    diagnostic::FrameCount,
    input::{
        ButtonState,
        keyboard::{Key, KeyboardInput},
    },
    input_focus::{FocusCause, InputFocus},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::{EditableText, TextEdit, TextLayoutInfo},
    ui::widget::TextScroll,
    ui_widgets::Activate,
    window::PrimaryWindow,
    winit::WinitSettings,
};
use lince_interface::canvas::{CanvasItem, CanvasView};
use lince_interface::canvas_controls::{CanvasAction, CanvasControl};
use lince_interface::{
    app::interface_app,
    container::BoxRoot,
    effect::{SendBoxEvent, SquareToggled},
    sand::{InBox, Square, button, text_editor},
    theme::{PURPLE, Typography},
};

#[derive(Component, Default)]
struct TestEvents {
    received_events: u64,
}

fn setup(mut commands: Commands, typography: Res<Typography>) {
    let workspace = commands.spawn((BoxRoot, TestEvents::default())).id();
    let reply = commands
        .spawn((
            Square,
            InBox(workspace),
            CanvasItem {
                position: DVec2::new(124.0, 0.0),
                size: Vec2::splat(224.0),
            },
            Node {
                padding: UiRect::all(px(20)),
                ..default()
            },
            BackgroundColor(Color::WHITE),
            ChildOf(workspace),
        ))
        .with_children(|parent| {
            parent.spawn(text_editor("Hello, Box.", &typography, 1));
        })
        .id();
    commands.spawn((
        Square,
        InBox(workspace),
        CanvasItem {
            position: DVec2::new(-124.0, 0.0),
            size: Vec2::splat(224.0),
        },
        SendBoxEvent {
            box_entity: workspace,
            square: reply,
        },
        button(0),
        Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(PURPLE),
        ChildOf(workspace),
        children![(
            Text::new("Toggle square"),
            typography.text(24.0),
            TextColor(Color::WHITE)
        )],
    ));
}

#[derive(Resource)]
struct CapturePath(String);

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("provide a screenshot output path");
    interface_app()
        .insert_resource(WinitSettings::continuous())
        .insert_resource(CapturePath(path))
        .add_systems(Startup, setup)
        .add_observer(
            |event: On<SquareToggled>, mut boxes: Query<&mut TestEvents>| {
                boxes.get_mut(event.entity).unwrap().received_events += 1;
            },
        )
        .add_systems(Update, exercise)
        .run();
}

fn exercise(
    frame: Res<FrameCount>,
    path: Res<CapturePath>,
    triggers: Query<(Entity, &SendBoxEvent)>,
    squares: Query<&Visibility, With<Square>>,
    boxes: Query<&TestEvents>,
    mut editors: Query<(Entity, &mut EditableText)>,
    mut focus: ResMut<InputFocus>,
    mut keyboard: MessageWriter<KeyboardInput>,
    mut views: Query<&mut CanvasView>,
    mut items: Query<(&mut CanvasItem, &Node, &ComputedNode, &UiGlobalTransform)>,
    controls: Query<(Entity, &CanvasControl, &UiGlobalTransform)>,
    window: Single<Entity, With<PrimaryWindow>>,
    mut commands: Commands,
) {
    let Ok((trigger, effect)) = triggers.single() else {
        return;
    };
    match frame.0 {
        10 => {
            let (entity, mut editor) = editors.single_mut().unwrap();
            editor.queue_edit(TextEdit::SelectAll);
            focus.set(entity, FocusCause::Navigated);
        }
        12 => {
            keyboard.write(KeyboardInput {
                key_code: KeyCode::KeyH,
                logical_key: Key::Character("Hello, Bevy!".into()),
                state: ButtonState::Pressed,
                text: Some("Hello, Bevy!".into()),
                repeat: false,
                window: *window,
            });
        }
        16 => {
            assert_eq!(
                editors.single().unwrap().1.value().to_string(),
                "Hello, Bevy!"
            );
        }
        20 | 40 => commands.trigger(Activate { entity: trigger }),
        30 => {
            assert_eq!(*squares.get(effect.square).unwrap(), Visibility::Hidden);
            assert_eq!(boxes.get(effect.box_entity).unwrap().received_events, 1);
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(format!("{}.hidden.png", path.0)));
        }
        42 => {
            views.single_mut().unwrap().center = DVec2::splat(1e12);
        }
        46 => {
            for (mut item, node, computed, _) in &mut items {
                assert_eq!(node.display, Display::None);
                assert_eq!(computed.size(), Vec2::ZERO);
                item.position += DVec2::splat(1e12);
            }
        }
        52 => {
            for (mut item, node, computed, _) in &mut items {
                assert_eq!(node.display, Display::Flex);
                assert_eq!(computed.size() * computed.inverse_scale_factor(), item.size);
                item.position -= DVec2::splat(1e12);
            }
            views.single_mut().unwrap().center = DVec2::ZERO;
        }
        60 => {
            assert_eq!(
                editors.single().unwrap().1.value().to_string(),
                "Hello, Bevy!"
            );
            assert_eq!(*squares.get(effect.square).unwrap(), Visibility::Inherited);
            assert_eq!(boxes.get(effect.box_entity).unwrap().received_events, 2);
            views.single_mut().unwrap().center = DVec2::splat(1e12);
            let entity = controls
                .iter()
                .find(|(_, control, _)| control.action == CanvasAction::Recenter)
                .unwrap()
                .0;
            commands.trigger(Activate { entity });
        }
        64 => {
            assert_eq!(views.single().unwrap().center, DVec2::ZERO);
            views.single_mut().unwrap().center = DVec2::splat(1e12);
            let entity = controls
                .iter()
                .find(|(_, control, _)| control.action == CanvasAction::BringHere)
                .unwrap()
                .0;
            commands.trigger(Activate { entity });
        }
        68 => {
            for (item, node, _, _) in &items {
                assert!((item.position - DVec2::splat(1e12)).length() < 300.0);
                assert_eq!(node.display, Display::Flex);
            }
            let entity = controls
                .iter()
                .find(|(_, control, _)| control.action == CanvasAction::ZoomIn)
                .unwrap()
                .0;
            focus.set(entity, FocusCause::Navigated);
        }
        72 | 74 => {
            keyboard.write(KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: if frame.0 == 72 {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                },
                text: None,
                repeat: false,
                window: *window,
            });
        }
        78 => {
            assert_eq!(views.single().unwrap().zoom, 1.2);
            for (_, _, _, transform) in &items {
                assert!((transform.matrix2.x_axis.length() - 1.2).abs() < 0.001);
            }
            for (_, _, transform) in &controls {
                assert!((transform.matrix2.x_axis.length() - 1.0).abs() < 0.001);
            }
            assert_eq!(
                editors.single().unwrap().1.value().to_string(),
                "Hello, Bevy!"
            );
            commands
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path.0.clone()))
                .observe(finish);
        }
        1800 => panic!("Square screenshot did not complete"),
        _ => {}
    }
}

fn finish(
    event: On<ScreenshotCaptured>,
    editors: Query<
        (
            &ComputedNode,
            &UiGlobalTransform,
            &TextLayoutInfo,
            &TextScroll,
        ),
        With<EditableText>,
    >,
    mut exit: MessageWriter<AppExit>,
) {
    let (node, transform, layout, scroll) = editors.single().unwrap();
    let glyph = &layout.glyphs[0];
    let center = node.content_box().min - scroll.0 + glyph.position;
    let half = glyph.atlas_info.rect.size() * 0.5;
    let min = transform.transform_point2(center - half);
    let max = transform.transform_point2(center + half);
    let middle = (min.y + max.y) * 0.5;
    for (top, bottom) in [(min.y, middle), (middle, max.y)] {
        let mut ink = 0;
        for y in top.ceil() as u32..bottom.floor() as u32 {
            for x in min.x.ceil() as u32..max.x.floor() as u32 {
                let color = event.image.get_color_at(x, y).unwrap().to_srgba();
                if color.red < 0.4 && color.green < 0.4 && color.blue < 0.4 {
                    ink += 1;
                }
            }
        }
        assert!(ink > 2, "Zoom clipped the editable text glyph");
    }
    println!(
        "Canvas smoke passed: editing, hide/show, distant coordinates, culling, recenter, bring here, keyboard zoom, fixed toolbar and unclipped text pixels."
    );
    exit.write(AppExit::Success);
}
