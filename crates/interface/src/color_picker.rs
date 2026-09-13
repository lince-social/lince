use crate::{
    castle::Castle,
    edit_mode::label,
    slider::{SliderChanged, SliderSand},
    tokens::{Token, TokenValue},
};
use bevy::{
    a11y::AccessibilityNode,
    input_focus::{FocusCause, FocusedInput, InputFocus},
    prelude::*,
    text::EditableText,
    ui_widgets::Activate,
};

#[derive(Component, Clone)]
pub struct ColorPicker {
    pub editor: Entity,
    pub preview: Entity,
    pub toggle: Entity,
    pub controls: Entity,
    pub channels: Vec<Entity>,
    rgba: [u8; 4],
    alpha: bool,
    observed: String,
    error: Entity,
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct ColorChanged {
    pub entity: Entity,
    pub rgba: [u8; 4],
}

pub struct ColorPickerPlugin;

impl Plugin for ColorPickerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            PostUpdate,
            sync.after(bevy::text::EditableTextSystems)
                .after(crate::customization::autosave)
                .before(crate::actions::ApplyActions),
        );
    }
}

pub fn spawn(
    world: &mut World,
    parent: Entity,
    title: &str,
    mut rgba: [u8; 4],
    alpha: bool,
) -> Entity {
    if !alpha {
        rgba[3] = 255;
    }
    let group = world
        .spawn((
            Castle,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                min_width: px(260),
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let header = world
        .spawn((
            Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            },
            ChildOf(group),
        ))
        .id();
    let toggle = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            Node {
                width: px(48),
                height: px(32),
                flex_shrink: 0.0,
                border: UiRect::all(px(1)),
                ..default()
            },
            crate::token_style::border(Token::Accent),
            ChildOf(header),
        ))
        .id();
    {
        let mut node = world.get_mut::<AccessibilityNode>(toggle).unwrap();
        node.set_label(format!("Choose {title} color"));
        node.set_expanded(false);
    }
    for index in 0..8 {
        world.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent((index % 4) as f32 * 25.0),
                top: percent((index / 4) as f32 * 50.0),
                width: percent(25),
                height: percent(50),
                ..default()
            },
            BackgroundColor(if (index + index / 4) % 2 == 0 {
                Color::srgb_u8(220, 220, 220)
            } else {
                Color::srgb_u8(100, 100, 100)
            }),
            Pickable::IGNORE,
            ChildOf(toggle),
        ));
    }
    let preview = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            BackgroundColor(TokenValue::Color(rgba).color()),
            Pickable::IGNORE,
            ChildOf(toggle),
        ))
        .id();
    let observed = TokenValue::Color(rgba).display();
    let bundle =
        crate::sand::text_editor(&observed, world.resource::<crate::theme::Typography>(), 0);
    let editor = world
        .spawn((bundle, AccessibilityNode::default(), ChildOf(header)))
        .id();
    world.get_mut::<Node>(editor).unwrap().width = px(160);
    {
        let mut text = world.get_mut::<EditableText>(editor).unwrap();
        text.allow_newlines = false;
        text.visible_lines = Some(1.0);
        text.max_characters = Some(24);
    }
    world
        .get_mut::<AccessibilityNode>(editor)
        .unwrap()
        .set_label(format!("{title} color, hex value"));
    let error = label(world, group, "", 12.0);
    world.get_mut::<Node>(error).unwrap().display = Display::None;
    let controls = world
        .spawn((
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::all(px(8)),
                ..default()
            },
            crate::token_style::background(Token::Surface),
            ChildOf(group),
        ))
        .id();
    let mut channels = Vec::new();
    for (index, name) in ["Red", "Green", "Blue", "Opacity"]
        .into_iter()
        .take(if alpha { 4 } else { 3 })
        .enumerate()
    {
        label(world, controls, name, 12.0);
        let channel = crate::slider::spawn(
            world,
            controls,
            &format!("{title} {name}"),
            SliderSand {
                start: 0.0,
                end: if index == 3 { 100.0 } else { 255.0 },
                step: 1.0,
                decimals: 0,
            },
            channel_value(rgba, index),
            if index == 3 { "%" } else { "" },
        )
        .unwrap();
        world.entity_mut(channel).insert(gradient(rgba, index));
        world.entity_mut(channel).observe(
            move |event: On<SliderChanged>, mut commands: Commands| {
                let value = event.value;
                commands.queue(move |world: &mut World| {
                    let Some(picker) = world.get::<ColorPicker>(group).cloned() else {
                        return;
                    };
                    let Some(text) = world.get::<EditableText>(picker.editor) else {
                        return;
                    };
                    if text.is_composing() || text.pending_paste.is_some() {
                        crate::slider::set_value(world, channel, channel_value(picker.rgba, index));
                        return;
                    }
                    let mut rgba =
                        parse(&text.value().to_string(), picker.alpha).unwrap_or(picker.rgba);
                    rgba[index] = if index == 3 {
                        (value * 255.0 / 100.0).round() as u8
                    } else {
                        value as u8
                    };
                    world
                        .get_mut::<EditableText>(picker.editor)
                        .unwrap()
                        .editor
                        .set_text(&TokenValue::Color(rgba).display());
                    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                        wake.ring();
                    }
                });
            },
        );
        channels.push(channel);
    }
    world.entity_mut(toggle).observe(
        move |_: On<Activate>,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>| {
            let Ok(mut node) = nodes.get_mut(controls) else {
                return;
            };
            let expanded = node.display == Display::None;
            node.display = if expanded {
                Display::Flex
            } else {
                Display::None
            };
            if let Ok(mut node) = accessibility.get_mut(toggle) {
                node.set_expanded(expanded);
            }
        },
    );
    world.entity_mut(group).observe(
        move |mut event: On<FocusedInput<bevy::input::keyboard::KeyboardInput>>,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>,
              mut focus: ResMut<InputFocus>| {
            if event.input.key_code != KeyCode::Escape || !event.input.state.is_pressed() {
                return;
            }
            let Ok(mut node) = nodes.get_mut(controls) else {
                return;
            };
            if node.display == Display::None {
                return;
            }
            node.display = Display::None;
            if let Ok(mut node) = accessibility.get_mut(toggle) {
                node.set_expanded(false);
            }
            focus.set(toggle, FocusCause::Navigated);
            event.propagate(false);
        },
    );
    world.entity_mut(group).insert(ColorPicker {
        editor,
        preview,
        toggle,
        controls,
        channels,
        rgba,
        alpha,
        observed,
        error,
    });
    group
}

fn parse(text: &str, alpha: bool) -> Option<[u8; 4]> {
    match if alpha {
        Token::SandBackground
    } else {
        Token::CanvasBackground
    }
    .parse(text)?
    {
        TokenValue::Color(rgba) => Some(rgba),
        TokenValue::Number(_) => None,
    }
}

fn gradient(mut rgba: [u8; 4], index: usize) -> BackgroundGradient {
    if index < 3 {
        rgba[3] = 255;
    }
    rgba[index] = 0;
    let start = TokenValue::Color(rgba).color();
    rgba[index] = 255;
    BackgroundGradient::from(LinearGradient::new(
        LinearGradient::TO_RIGHT,
        vec![
            ColorStop::auto(start),
            ColorStop::auto(TokenValue::Color(rgba).color()),
        ],
    ))
}

fn channel_value(rgba: [u8; 4], index: usize) -> f32 {
    if index == 3 {
        f32::from(rgba[index]) * 100.0 / 255.0
    } else {
        f32::from(rgba[index])
    }
}

fn sync(world: &mut World) {
    let changed: Vec<_> = world
        .query::<(Entity, &ColorPicker)>()
        .iter(world)
        .filter_map(|(entity, picker)| {
            let text = world.get::<EditableText>(picker.editor)?;
            if text.is_composing() || text.pending_paste.is_some() {
                return None;
            }
            let text = text.value().to_string();
            (text != picker.observed).then(|| (entity, picker.clone(), text))
        })
        .collect();
    for (entity, picker, text) in changed {
        world.get_mut::<ColorPicker>(entity).unwrap().observed = text.clone();
        let Some(rgba) = parse(&text, picker.alpha) else {
            world.get_mut::<Node>(picker.error).unwrap().display = Display::Flex;
            world.get_mut::<Text>(picker.error).unwrap().0 = if picker.alpha {
                "Use #RGB, #RRGGBB or #RRGGBBAA."
            } else {
                "Use an opaque color: #RGB or #RRGGBB."
            }
            .into();
            continue;
        };
        world.get_mut::<Node>(picker.error).unwrap().display = Display::None;
        world.get_mut::<ColorPicker>(entity).unwrap().rgba = rgba;
        world.get_mut::<BackgroundColor>(picker.preview).unwrap().0 =
            TokenValue::Color(rgba).color();
        for (index, channel) in picker.channels.iter().enumerate() {
            crate::slider::set_value(world, *channel, channel_value(rgba, index));
            world.entity_mut(*channel).insert(gradient(rgba, index));
        }
        if rgba != picker.rgba {
            world.trigger(ColorChanged { entity, rgba });
        }
    }
}

pub(crate) mod tests {
    use super::*;
    use bevy::ui_widgets::{SliderValue, ValueChange};

    #[derive(Resource, Default)]
    struct Changes(Vec<[u8; 4]>);

    fn fixture(alpha: bool) -> (App, Entity) {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<crate::theme::Typography>()
            .init_resource::<InputFocus>()
            .init_resource::<Changes>()
            .add_plugins(ColorPickerPlugin);
        let parent = app.world_mut().spawn(Node::default()).id();
        let picker = spawn(app.world_mut(), parent, "Test", [18, 52, 86, 128], alpha);
        app.world_mut().entity_mut(picker).observe(
            |event: On<ColorChanged>, mut changes: ResMut<Changes>| {
                changes.0.push(event.rgba);
            },
        );
        (app, picker)
    }

    #[cfg_attr(test, test)]
    fn sliders_and_hex_share_color_without_duplicate_events() {
        let (mut app, entity) = fixture(true);
        let picker = app.world().get::<ColorPicker>(entity).unwrap().clone();
        assert!(app.world().get::<Castle>(entity).is_some());
        assert_eq!(picker.channels.len(), 4);
        app.world_mut().trigger(Activate {
            entity: picker.toggle,
        });
        app.update();
        assert_eq!(
            app.world().get::<Node>(picker.controls).unwrap().display,
            Display::Flex
        );
        for (index, value) in [(0, 200.0_f32), (1, 100.0), (3, 0.0)] {
            app.world_mut().trigger(ValueChange {
                source: picker.channels[index],
                value,
                is_final: false,
            });
        }
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(picker.editor)
                .unwrap()
                .value()
                .to_string(),
            "#C8645600"
        );
        assert_eq!(
            app.world()
                .get::<BackgroundColor>(picker.preview)
                .unwrap()
                .0,
            Color::srgba_u8(200, 100, 86, 0)
        );
        assert_eq!(app.world().resource::<Changes>().0, [[200, 100, 86, 0]]);
        app.world_mut()
            .get_mut::<EditableText>(picker.editor)
            .unwrap()
            .editor
            .set_text("#aBc");
        app.update();
        for (channel, value) in picker.channels.iter().zip([170.0, 187.0, 204.0, 100.0]) {
            assert_eq!(app.world().get::<SliderValue>(*channel).unwrap().0, value);
        }
        app.update();
        assert_eq!(app.world().resource::<Changes>().0.len(), 2);
        app.world_mut().trigger(Activate {
            entity: picker.toggle,
        });
        app.update();
        assert_eq!(
            app.world().get::<Node>(picker.controls).unwrap().display,
            Display::None
        );
    }

    #[cfg_attr(test, test)]
    fn invalid_hex_keeps_last_color_and_opaque_pickers_reject_alpha() {
        let (mut app, entity) = fixture(false);
        let picker = app.world().get::<ColorPicker>(entity).unwrap().clone();
        assert_eq!(picker.channels.len(), 3);
        for value in ["", "#12", "#12ggff", "💜ab", "ééé", "#12345600"] {
            app.world_mut()
                .get_mut::<EditableText>(picker.editor)
                .unwrap()
                .editor
                .set_text(value);
            app.update();
            assert_eq!(
                app.world().get::<ColorPicker>(entity).unwrap().rgba,
                [18, 52, 86, 255]
            );
            assert_eq!(
                app.world().get::<Node>(picker.error).unwrap().display,
                Display::Flex
            );
        }
        assert!(app.world().resource::<Changes>().0.is_empty());
        app.world_mut().trigger(ValueChange {
            source: picker.channels[2],
            value: 255.0_f32,
            is_final: true,
        });
        app.update();
        assert_eq!(
            app.world()
                .get::<EditableText>(picker.editor)
                .unwrap()
                .value()
                .to_string(),
            "#1234FF"
        );
        assert_eq!(
            app.world().get::<Node>(picker.error).unwrap().display,
            Display::None
        );
        app.world_mut().trigger(ValueChange {
            source: picker.channels[0],
            value: f32::NAN,
            is_final: true,
        });
        app.update();
        assert_eq!(app.world().resource::<Changes>().0, [[18, 52, 255, 255]]);
    }

    crate::laboratory_cases! {
        sliders_and_hex_share_color_without_duplicate_events,
        invalid_hex_keeps_last_color_and_opaque_pickers_reject_alpha,
    }
}
