use bevy::{
    core_pipeline::tonemapping::Tonemapping,
    input::mouse::{MouseScrollUnit, MouseWheel},
    input_focus::{
        FocusGained, FocusLost, InputFocus,
        tab_navigation::{TabGroup, TabIndex, TabNavigationPlugin},
    },
    prelude::*,
    render::RenderPlugin,
    text::{EditableText, TextCursorStyle},
    ui_widgets::{Activate, Button as WidgetButton},
    winit::{UpdateMode, WinitSettings},
};
use std::time::Duration;

const PAPER: Color = Color::srgb(248.0 / 255.0, 250.0 / 255.0, 252.0 / 255.0);
const INK: Color = Color::srgb(18.0 / 255.0, 18.0 / 255.0, 20.0 / 255.0);
const PURPLE: Color = Color::srgb(55.0 / 255.0, 48.0 / 255.0, 163.0 / 255.0);

pub const BEVY_LICENSE: &str = include_str!("../licenses/bevy-MIT.txt");
pub const LATO_LICENSE: &str = include_str!("../../../assets/fonts/Lato/OFL.txt");

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
#[require(Node, BackgroundColor, Outline)]
pub struct Square;

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct InBox(#[entities] pub Entity);

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct SendBoxEvent {
    #[entities]
    pub box_entity: Entity,
    #[entities]
    pub square: Entity,
}

#[derive(Component)]
pub struct BoxRoot {
    pub received_events: u64,
    pub status: Entity,
}

#[derive(EntityEvent, Debug)]
pub struct ToggleSquare {
    pub entity: Entity,
    pub source: Entity,
    pub square: Entity,
}

pub struct SandPlugin;

impl Plugin for SandPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Square>()
            .register_type::<InBox>()
            .register_type::<SendBoxEvent>()
            .init_resource::<InputFocus>()
            .add_observer(send_box_event)
            .add_observer(toggle_square)
            .add_observer(
                |event: On<FocusGained>, mut squares: Query<&mut Outline, With<Square>>| {
                    if let Ok(mut outline) = squares.get_mut(event.entity) {
                        *outline = Outline {
                            width: px(2),
                            offset: px(4),
                            color: PURPLE,
                        };
                    }
                },
            )
            .add_observer(
                |event: On<FocusLost>, mut squares: Query<&mut Outline, With<Square>>| {
                    if let Ok(mut outline) = squares.get_mut(event.entity) {
                        outline.width = px(0);
                    }
                },
            );
    }
}

fn send_box_event(
    activate: On<Activate>,
    effects: Query<&SendBoxEvent, With<Square>>,
    mut commands: Commands,
) {
    if let Ok(effect) = effects.get(activate.entity) {
        commands.trigger(ToggleSquare {
            entity: effect.box_entity,
            source: activate.entity,
            square: effect.square,
        });
    }
}

fn toggle_square(
    event: On<ToggleSquare>,
    mut boxes: Query<&mut BoxRoot>,
    sources: Query<(&InBox, &SendBoxEvent), With<Square>>,
    mut squares: Query<(&InBox, &mut Visibility), With<Square>>,
    mut text: Query<&mut Text>,
    parents: Query<&ChildOf>,
    mut focus: ResMut<InputFocus>,
) {
    let Ok((owner, effect)) = sources.get(event.source) else {
        return;
    };
    if owner.0 != event.entity || effect.box_entity != event.entity || effect.square != event.square
    {
        return;
    }
    let Ok(mut workspace) = boxes.get_mut(event.entity) else {
        return;
    };
    let Ok((owner, mut visibility)) = squares.get_mut(event.square) else {
        return;
    };
    if owner.0 != event.entity {
        return;
    }

    let showing = *visibility == Visibility::Hidden;
    *visibility = if showing {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if !showing {
        let mut focused = focus.get();
        while let Some(entity) = focused {
            if entity == event.square {
                focus.clear();
                break;
            }
            focused = parents.get(entity).ok().map(ChildOf::parent);
        }
    }
    workspace.received_events += 1;
    if let Ok(mut label) = text.get_mut(workspace.status) {
        label.0 = format!(
            "Box received ToggleSquare #{} · square {}",
            workspace.received_events,
            if showing { "shown" } else { "hidden" }
        );
    }
}

#[derive(Component, Default)]
struct TextPlacement(u8);

#[derive(Component)]
struct Credits;

pub fn idle_settings() -> WinitSettings {
    WinitSettings {
        focused_mode: UpdateMode::reactive_low_power(Duration::MAX),
        unfocused_mode: UpdateMode::reactive_low_power(Duration::MAX),
    }
}

pub fn run_native_interface() {
    square_app().run();
}

pub fn square_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Lince · Square".into(),
                    resolution: (800, 640).into(),
                    ..default()
                }),
                ..default()
            }),
    )
    .add_plugins((TabNavigationPlugin, SandPlugin))
    .insert_resource(ClearColor(PAPER))
    .insert_resource(idle_settings())
    .add_systems(Startup, setup)
    .add_systems(Update, (demo_keys, scroll_credits));
    app
}

fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let font = fonts.add(Font::from_bytes(
        include_bytes!("../../../assets/fonts/Lato/Lato-Regular.ttf").to_vec(),
    ));
    let typography = |size| TextFont {
        font: font.clone().into(),
        font_size: FontSize::Px(size),
        ..default()
    };

    commands.spawn((
        Camera3d::default(),
        Tonemapping::None,
        Projection::Orthographic(OrthographicProjection::default_3d()),
        Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let status = commands
        .spawn((
            Text::new("Box is waiting for an event."),
            typography(16.0),
            TextColor(INK),
        ))
        .id();
    let workspace = commands
        .spawn((
            Name::new("Box"),
            BoxRoot {
                received_events: 0,
                status,
            },
            Transform::default(),
            TabGroup::default(),
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::all(px(32)),
                row_gap: px(24),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
        ))
        .id();

    let response = commands
        .spawn((
            Name::new("Reply Square"),
            Square,
            InBox(workspace),
            TextPlacement::default(),
            Node {
                width: px(224),
                height: px(224),
                padding: UiRect::all(px(20)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexStart,
                ..default()
            },
            BackgroundColor(Color::WHITE),
            children![(
                Name::new("Square text — click to edit"),
                EditableText {
                    allow_newlines: true,
                    visible_lines: Some(3.0),
                    max_characters: Some(256),
                    cursor_blink_period: Duration::MAX,
                    ..EditableText::new("Hello, Box.\nYou can edit me.")
                },
                Node {
                    width: percent(100),
                    ..default()
                },
                typography(22.0),
                TextColor(INK),
                TextCursorStyle {
                    color: PURPLE,
                    ..default()
                },
                TabIndex(1),
            )],
        ))
        .id();

    let trigger = commands
        .spawn((
            Name::new("Toggle reply Square"),
            Square,
            InBox(workspace),
            SendBoxEvent {
                box_entity: workspace,
                square: response,
            },
            WidgetButton,
            TabIndex(0),
            Node {
                width: px(224),
                height: px(224),
                padding: UiRect::all(px(20)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(PURPLE),
            children![(
                Text::new("Toggle square"),
                typography(24.0),
                TextColor(Color::WHITE),
            )],
        ))
        .id();

    commands.entity(workspace).with_children(|parent| {
        parent.spawn((Text::new("Lince / Box"), typography(32.0), TextColor(INK)));
        parent
            .spawn(Node {
                column_gap: px(24),
                row_gap: px(24),
                flex_wrap: FlexWrap::Wrap,
                justify_content: JustifyContent::Center,
                ..default()
            })
            .add_children(&[trigger, response]);
    });
    commands.entity(workspace).add_child(status).with_children(|parent| {
        parent.spawn((
            Text::new("Click the purple Square, or Tab then Enter.\nEdit the other Square's text. F2 moves it: top / center / bottom."),
            typography(16.0),
            TextColor(INK),
        ));
        parent.spawn((
            Text::new("Bevy · Lato    F1: credits and licenses"),
            typography(14.0),
            TextColor(PURPLE),
        ));
    });

    commands.spawn((
        Name::new("Credits and licenses — F1 to close, wheel to scroll"),
        Credits,
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            left: px(16), right: px(16), top: px(16), bottom: px(16),
            padding: UiRect::all(px(24)),
            overflow: Overflow::scroll_y(),
            ..default()
        },
        GlobalZIndex(10),
        BackgroundColor(PAPER),
        children![(
            Text::new(format!(
                "Credits — F1 to close · scroll to read\n\nBevy contributors — https://bevy.org\n{BEVY_LICENSE}\n\nLato by Łukasz Dziedzic\n{LATO_LICENSE}"
            )),
            typography(16.0),
            TextColor(INK),
        )],
    ));
}

fn demo_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut squares: Query<(&mut TextPlacement, &mut Node), With<Square>>,
    mut credits: Query<&mut Node, (With<Credits>, Without<Square>)>,
    mut workspace: Query<&mut Node, (With<BoxRoot>, Without<Square>, Without<Credits>)>,
    mut focus: ResMut<InputFocus>,
) {
    if keys.just_pressed(KeyCode::F1) {
        for mut node in &mut credits {
            node.display = if node.display == Display::None {
                Display::Flex
            } else {
                Display::None
            };
            for mut root in &mut workspace {
                root.display = if node.display == Display::None {
                    Display::Flex
                } else {
                    Display::None
                };
            }
        }
        focus.clear();
    }
    if keys.just_pressed(KeyCode::F2) {
        for (mut placement, mut node) in &mut squares {
            placement.0 = (placement.0 + 1) % 3;
            node.justify_content = match placement.0 {
                0 => JustifyContent::FlexStart,
                1 => JustifyContent::Center,
                _ => JustifyContent::FlexEnd,
            };
        }
    }
}

fn scroll_credits(
    mut wheel: MessageReader<MouseWheel>,
    mut credits: Query<(&Node, &mut ScrollPosition), With<Credits>>,
) {
    for event in wheel.read() {
        for (node, mut position) in &mut credits {
            if node.display != Display::None {
                let multiplier = if event.unit == MouseScrollUnit::Line {
                    24.0
                } else {
                    1.0
                };
                position.0.y = (position.0.y - event.y * multiplier).max(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::{
        camera::RenderTarget,
        input::{
            ButtonState, InputPlugin,
            keyboard::{Key, KeyboardInput},
        },
        input_focus::{FocusCause, InputDispatchPlugin, InputFocusPlugin},
        picking::{
            backend::HitData,
            events::{Click, Pointer, Press},
            pointer::{Location, PointerButton, PointerId},
        },
        ui_widgets::ButtonPlugin,
        window::PrimaryWindow,
    };

    fn fixture() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(SandPlugin);
        let status = app.world_mut().spawn(Text::new("waiting")).id();
        let workspace = app
            .world_mut()
            .spawn(BoxRoot {
                received_events: 0,
                status,
            })
            .id();
        let reply = app
            .world_mut()
            .spawn((Square, InBox(workspace), Visibility::Inherited))
            .id();
        let trigger = app
            .world_mut()
            .spawn((
                Square,
                InBox(workspace),
                SendBoxEvent {
                    box_entity: workspace,
                    square: reply,
                },
            ))
            .id();
        (app, workspace, trigger, reply)
    }

    fn activate(app: &mut App, entity: Entity) {
        app.world_mut().trigger(Activate { entity });
        app.world_mut().flush();
    }

    #[test]
    fn activation_sends_one_box_event_and_toggles_without_replacing_the_entity() {
        let (mut app, workspace, trigger, reply) = fixture();
        let label = app
            .world_mut()
            .spawn((Text::new("my text"), ChildOf(reply)))
            .id();
        for (count, expected) in [(1, Visibility::Hidden), (2, Visibility::Inherited)] {
            activate(&mut app, trigger);
            assert_eq!(*app.world().get::<Visibility>(reply).unwrap(), expected);
            assert_eq!(
                app.world()
                    .get::<BoxRoot>(workspace)
                    .unwrap()
                    .received_events,
                count
            );
            assert_eq!(app.world().get::<Text>(label).unwrap().0, "my text");
        }
    }

    #[test]
    fn removed_effect_and_missing_target_are_inert() {
        let (mut app, workspace, trigger, reply) = fixture();
        let effect = *app.world().get::<SendBoxEvent>(trigger).unwrap();
        app.world_mut().entity_mut(trigger).remove::<SendBoxEvent>();
        activate(&mut app, trigger);
        app.world_mut().entity_mut(trigger).insert(effect);
        app.world_mut().despawn(reply);
        activate(&mut app, trigger);
        assert_eq!(
            app.world()
                .get::<BoxRoot>(workspace)
                .unwrap()
                .received_events,
            0
        );
    }

    #[test]
    fn event_cannot_toggle_a_square_owned_by_another_box() {
        let (mut app, workspace, trigger, reply) = fixture();
        let other = app.world_mut().spawn_empty().id();
        app.world_mut().entity_mut(reply).insert(InBox(other));
        activate(&mut app, trigger);
        assert_eq!(
            *app.world().get::<Visibility>(reply).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world()
                .get::<BoxRoot>(workspace)
                .unwrap()
                .received_events,
            0
        );
    }

    #[test]
    fn quiet_updates_do_not_touch_square_visibility_or_send_events() {
        let (mut app, workspace, _, reply) = fixture();
        app.world_mut().clear_trackers();
        for _ in 0..100 {
            app.update();
        }
        assert_eq!(
            app.world()
                .get::<BoxRoot>(workspace)
                .unwrap()
                .received_events,
            0
        );
        assert!(
            !app.world()
                .entity(reply)
                .get_ref::<Visibility>()
                .unwrap()
                .is_changed()
        );
        assert_eq!(
            idle_settings().focused_mode,
            UpdateMode::reactive_low_power(Duration::MAX)
        );
    }

    #[test]
    fn clicking_a_text_child_uses_bevys_button_and_emits_only_once() {
        let (mut app, workspace, trigger, reply) = fixture();
        app.add_plugins(ButtonPlugin);
        app.world_mut().entity_mut(trigger).insert(WidgetButton);
        let label = app
            .world_mut()
            .spawn((Text::new("click me"), ChildOf(trigger)))
            .id();
        let window = app.world_mut().spawn(Window::default()).id();
        let location = Location {
            target: RenderTarget::Window(bevy::window::WindowRef::Entity(window))
                .normalize(None)
                .unwrap(),
            position: Vec2::ZERO,
        };
        let hit = HitData::new(window, 0.0, None, None);
        app.world_mut().flush();
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            location.clone(),
            Press {
                button: PointerButton::Primary,
                hit: hit.clone(),
                count: 1,
            },
            label,
        ));
        app.world_mut().flush();
        assert!(app.world().get::<bevy::ui::Pressed>(trigger).is_some());
        app.world_mut().trigger(Pointer::new(
            PointerId::Mouse,
            location,
            Click {
                button: PointerButton::Primary,
                hit,
                duration: Duration::from_millis(50),
                count: 1,
            },
            label,
        ));
        app.world_mut().flush();
        assert_eq!(
            *app.world().get::<Visibility>(reply).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world()
                .get::<BoxRoot>(workspace)
                .unwrap()
                .received_events,
            1
        );
    }

    #[test]
    fn keyboard_activation_uses_focused_bevy_button_and_ignores_repeat() {
        let (mut app, workspace, trigger, reply) = fixture();
        app.add_plugins((
            InputPlugin,
            InputFocusPlugin,
            InputDispatchPlugin,
            ButtonPlugin,
        ));
        app.world_mut().entity_mut(trigger).insert(WidgetButton);
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(trigger, FocusCause::Navigated);
        for repeat in [false, true] {
            app.world_mut().write_message(KeyboardInput {
                key_code: KeyCode::Enter,
                logical_key: Key::Enter,
                state: ButtonState::Pressed,
                text: None,
                repeat,
                window,
            });
            app.update();
        }
        assert_eq!(
            *app.world().get::<Visibility>(reply).unwrap(),
            Visibility::Hidden
        );
        assert_eq!(
            app.world()
                .get::<BoxRoot>(workspace)
                .unwrap()
                .received_events,
            1
        );
    }

    #[test]
    fn hiding_an_editor_releases_focus_but_keeps_its_text() {
        let (mut app, _, trigger, reply) = fixture();
        let editor = app
            .world_mut()
            .spawn((EditableText::new("preserve me"), ChildOf(reply)))
            .id();
        app.world_mut()
            .resource_mut::<InputFocus>()
            .set(editor, FocusCause::Pressed);
        activate(&mut app, trigger);
        assert_eq!(app.world().resource::<InputFocus>().get(), None);
        assert_eq!(
            app.world()
                .get::<EditableText>(editor)
                .unwrap()
                .value()
                .to_string(),
            "preserve me"
        );
    }

    #[test]
    fn one_event_leaves_ten_thousand_unrelated_squares_unchanged() {
        let (mut app, workspace, trigger, _) = fixture();
        let unrelated = (0..10_000)
            .map(|_| {
                app.world_mut()
                    .spawn((Square, InBox(workspace), Visibility::Inherited))
                    .id()
            })
            .collect::<Vec<_>>();
        app.world_mut().clear_trackers();
        activate(&mut app, trigger);
        assert_eq!(
            app.world()
                .get::<BoxRoot>(workspace)
                .unwrap()
                .received_events,
            1
        );
        assert!(unrelated.into_iter().all(|entity| {
            !app.world()
                .entity(entity)
                .get_ref::<Visibility>()
                .unwrap()
                .is_changed()
        }));
    }
}
