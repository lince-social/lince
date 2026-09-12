use bevy::{
    a11y::AccessibilityNode,
    diagnostic::FrameCount,
    input::{ButtonState, mouse::MouseButtonInput},
    math::DVec2,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    text::EditableText,
    window::{CursorMoved, PrimaryWindow, WindowEvent},
    winit::WinitSettings,
};
use lince_interface::{
    actions::{Action, dispatch},
    app::interface_app,
    container::BoxRoot,
    customization::{CustomizationAction, Scope},
    edit_mode::{EditAction, EditControl, EditMode},
    sand_store::{SandKind, spawn_sand},
    tokens::{ColorScheme, ThemeSettings, Token, TokenValue},
};

#[derive(Resource)]
struct Capture {
    path: String,
    saved: bool,
    sand: Option<Entity>,
}

fn exercise(world: &mut World) {
    let frame = world.resource::<FrameCount>().0;
    let root = world
        .query_filtered::<Entity, With<BoxRoot>>()
        .single(world)
        .unwrap();
    match frame {
        6 => {
            let sand = spawn_sand(
                world,
                root,
                1,
                SandKind::Square,
                "",
                DVec2::new(-290.0, 0.0),
            );
            world.resource_mut::<Capture>().sand = Some(sand);
            dispatch(world, root, lince_interface::actions![EditAction::Open]);
        }
        12 | 14 | 16 => {
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            if frame == 12 {
                let control = world
                    .query::<(Entity, &EditControl)>()
                    .iter(world)
                    .find(|(_, control)| control.action == EditAction::Customization)
                    .unwrap()
                    .0;
                let position = world.get::<UiGlobalTransform>(control).unwrap().translation
                    * world
                        .get::<ComputedNode>(control)
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
                    state: if frame == 14 {
                        ButtonState::Pressed
                    } else {
                        ButtonState::Released
                    },
                }));
            }
        }
        22 => {
            let window = world
                .query_filtered::<Entity, With<PrimaryWindow>>()
                .single(world)
                .unwrap();
            world.write_message(WindowEvent::CursorMoved(CursorMoved {
                window,
                position: Vec2::splat(5.0),
                delta: None,
            }));
            assert!(
                world
                    .query::<&Text>()
                    .iter(world)
                    .any(|text| text.0 == "Customization")
            );
            CustomizationAction::Scheme(ColorScheme::Light).apply(world, root);
            let fields: Vec<_> = world
                .query::<(Entity, &AccessibilityNode)>()
                .iter(world)
                .filter(|(entity, _)| world.get::<EditableText>(*entity).is_some())
                .filter_map(|(entity, node)| node.label().map(|label| (entity, label.to_string())))
                .collect();
            for (entity, label) in fields {
                let value = match label.as_str() {
                    "Sand background" => "#D9C6FF",
                    "Sand roundness" => "18",
                    "Sand border thickness" => "2",
                    _ => continue,
                };
                world
                    .get_mut::<EditableText>(entity)
                    .unwrap()
                    .editor
                    .set_text(value);
            }
        }
        30 => {
            let sand = world.resource::<Capture>().sand.unwrap();
            assert_eq!(
                world.get::<BackgroundColor>(sand).unwrap().0,
                Color::srgb_u8(217, 198, 255)
            );
            assert_eq!(
                world.resource::<ThemeSettings>().global.0[&Token::Roundness],
                TokenValue::Number(18.0)
            );
            CustomizationAction::Scope(Scope::Sand(sand)).apply(world, root);
        }
        38 => {
            let panel = world.get::<EditMode>(root).unwrap().panel;
            assert!(world.get::<ComputedNode>(panel).unwrap().size().x > 600.0);
            let path = world.resource::<Capture>().path.clone();
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path))
                .observe(|_: On<ScreenshotCaptured>, mut capture: ResMut<Capture>| {
                    capture.saved = true;
                });
        }
        44 => {
            CustomizationAction::Scope(Scope::All).apply(world, root);
            for (token, value) in [
                (Token::Spacing, 12.0),
                (Token::Padding, 10.0),
                (Token::FontSize, 18.0),
                (Token::IconSize, 26.0),
                (Token::CustomizationWidth, 680.0),
            ] {
                world
                    .resource_mut::<ThemeSettings>()
                    .global
                    .set(token, TokenValue::Number(value));
            }
        }
        50 => {
            let panel = world.get::<EditMode>(root).unwrap().panel;
            let node = world.get::<Node>(panel).unwrap();
            assert_eq!(node.width, px(680));
            assert_eq!(node.row_gap, px(15));
            assert_eq!(node.padding, UiRect::all(px(20)));
            let dropdown = world
                .query::<(Entity, &AccessibilityNode)>()
                .iter(world)
                .find(|(_, node)| node.label() == Some("Colorscheme"))
                .unwrap()
                .0;
            world.trigger(bevy::ui_widgets::Activate { entity: dropdown });
        }
        54 => {
            let path = format!("{}.dropdown.png", world.resource::<Capture>().path);
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
            let options = world
                .query::<(&lince_interface::dropdown::Dropdown, &AccessibilityNode)>()
                .iter(world)
                .find(|(_, node)| node.label() == Some("Colorscheme"))
                .unwrap()
                .0
                .menu;
            assert_eq!(world.get::<Node>(options).unwrap().display, Display::Flex);
            assert_eq!(
                world.get::<Children>(options).unwrap().len(),
                ColorScheme::ALL.len()
            );
        }
        64 => {
            assert!(world.resource::<Capture>().saved);
            println!(
                "Customization smoke passed: theme and scope controls, retained overrides, live spacing, padding, text, icon and panel sizing, and theme dropdown."
            );
            world.write_message(AppExit::Success);
        }
        1800 => panic!("Customization smoke timed out"),
        _ => {}
    }
}

fn main() {
    interface_app()
        .insert_resource(Capture {
            path: std::env::args().nth(1).expect("provide screenshot path"),
            saved: false,
            sand: None,
        })
        .insert_resource(WinitSettings::continuous())
        .add_systems(Startup, |mut commands: Commands| {
            commands.spawn(BoxRoot);
        })
        .add_systems(Update, exercise)
        .run();
}
