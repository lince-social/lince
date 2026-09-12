use crate::{sand::Square, theme::Typography, token_style, tokens::Token};
use bevy::{
    a11y::AccessibilityNode,
    input_focus::tab_navigation::TabIndex,
    prelude::*,
    ui_widgets::{
        Slider, SliderOrientation, SliderRange, SliderStep, SliderThumb, SliderValue, TrackClick,
        ValueChange,
    },
};

#[derive(Component, Clone, Copy, Debug)]
pub struct SliderSand {
    pub start: f32,
    pub end: f32,
    pub step: f32,
    pub decimals: u8,
}

impl SliderSand {
    pub fn valid(self) -> bool {
        self.start.is_finite()
            && self.end.is_finite()
            && (self.end - self.start).is_finite()
            && self.end > self.start
            && self.step.is_finite()
            && self.step > 0.0
            && self.decimals <= 6
            && self.step >= 10.0_f32.powi(-i32::from(self.decimals))
    }

    pub fn snap(self, value: f32) -> Option<f32> {
        if !self.valid() || !value.is_finite() {
            return None;
        }
        let value = value.clamp(self.start, self.end);
        if value == self.end {
            return Some(self.end);
        }
        let steps = ((f64::from(value) - f64::from(self.start)) / f64::from(self.step)).round();
        let factor = 10.0_f64.powi(i32::from(self.decimals));
        let value =
            ((f64::from(self.start) + steps * f64::from(self.step)) * factor).round() / factor;
        Some((value as f32).clamp(self.start, self.end))
    }
}

#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct SliderChanged {
    pub entity: Entity,
    pub value: f32,
}

#[derive(Component)]
struct SliderParts {
    thumb: Entity,
    readout: Entity,
    suffix: String,
}

pub fn spawn(
    world: &mut World,
    parent: Entity,
    title: &str,
    config: SliderSand,
    value: f32,
    suffix: &str,
) -> Option<Entity> {
    let value = config.snap(value)?;
    let row = world
        .spawn((
            Node {
                width: percent(100),
                column_gap: px(12),
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let slider = world
        .spawn((
            config,
            Slider {
                track_click: TrackClick::Snap,
                orientation: SliderOrientation::Horizontal,
            },
            SliderRange::new(config.start, config.end),
            SliderStep(config.step),
            SliderValue(value),
            Square,
            TabIndex(0),
            Node {
                height: px(36),
                min_width: px(80),
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(row),
        ))
        .observe(change)
        .observe(
            |event: On<Pointer<Press>>, mut focus: ResMut<bevy::input_focus::InputFocus>| {
                if event.button == bevy::picking::pointer::PointerButton::Primary {
                    focus.set(event.entity, bevy::input_focus::FocusCause::Pressed);
                }
            },
        )
        .id();
    world
        .get_mut::<AccessibilityNode>(slider)
        .unwrap()
        .set_label(title);
    world.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: px(8),
            right: px(8),
            top: px(17),
            height: px(2),
            ..default()
        },
        token_style::background(Token::CanvasGrid),
        Pickable::IGNORE,
        ChildOf(slider),
    ));
    let travel = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(16),
                top: px(10),
                height: px(16),
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(slider),
        ))
        .id();
    let thumb = world
        .spawn((
            SliderThumb,
            Node {
                position_type: PositionType::Absolute,
                left: percent(100.0 * (value - config.start) / (config.end - config.start)),
                width: px(16),
                height: px(16),
                border_radius: BorderRadius::MAX,
                ..default()
            },
            token_style::background(Token::Ink),
            Pickable::IGNORE,
            ChildOf(travel),
        ))
        .id();
    let box_entity = world
        .spawn((
            Square,
            Node {
                min_width: px(68),
                height: px(36),
                padding: UiRect::horizontal(px(8)),
                border: UiRect::all(px(1)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                flex_shrink: 0.0,
                ..default()
            },
            token_style::background(Token::Surface),
            token_style::border(Token::CanvasGrid),
            ChildOf(row),
        ))
        .id();
    let typography = world.resource::<Typography>().text(16.0);
    let readout = world
        .spawn((
            Text::new(format!(
                "{:.*}{suffix}",
                usize::from(config.decimals),
                value
            )),
            typography,
            token_style::text(Token::Ink),
            ChildOf(box_entity),
        ))
        .id();
    world.entity_mut(slider).insert(SliderParts {
        thumb,
        readout,
        suffix: suffix.into(),
    });
    world
        .entity_mut(slider)
        .insert(crate::inspection::EventConnections(vec![
            crate::inspection::Connection {
                target: readout,
                name: "Slider changed number".into(),
            },
        ]));
    Some(slider)
}

fn change(
    event: On<ValueChange<f32>>,
    sliders: Query<(&SliderSand, &SliderValue, &SliderParts)>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
    mut commands: Commands,
) {
    let Ok((config, current, parts)) = sliders.get(event.source) else {
        return;
    };
    let Some(value) = config.snap(event.value) else {
        return;
    };
    if value == current.0 {
        return;
    }
    if let Ok(mut node) = nodes.get_mut(parts.thumb) {
        node.left = percent(100.0 * (value - config.start) / (config.end - config.start));
    }
    if let Ok(mut text) = texts.get_mut(parts.readout) {
        text.0 = format!("{:.*}{}", usize::from(config.decimals), value, parts.suffix);
    }
    commands.entity(event.source).insert(SliderValue(value));
    commands.trigger(SliderChanged {
        entity: event.source,
        value,
    });
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn ranges_snap_to_steps_and_support_decimal_precision() {
        let integer = SliderSand {
            start: 0.0,
            end: 100.0,
            step: 5.0,
            decimals: 0,
        };
        for (input, expected) in [
            (-5.0, 0.0),
            (2.0, 0.0),
            (3.0, 5.0),
            (98.0, 100.0),
            (110.0, 100.0),
        ] {
            assert_eq!(integer.snap(input), Some(expected));
        }
        let float = SliderSand {
            start: -1.0,
            end: 1.0,
            step: 0.01,
            decimals: 2,
        };
        assert_eq!(float.snap(0.126), Some(0.13));
        assert_eq!(float.snap(-0.126), Some(-0.13));
        assert_eq!(integer.snap(f32::NAN), None);
        for config in [
            SliderSand {
                step: 0.0,
                ..integer
            },
            SliderSand {
                end: 0.0,
                ..integer
            },
            SliderSand {
                start: f32::NEG_INFINITY,
                ..integer
            },
            SliderSand {
                decimals: 7,
                ..integer
            },
        ] {
            assert!(!config.valid());
        }
        assert_eq!(
            SliderSand {
                end: 99.0,
                ..integer
            }
            .snap(99.0),
            Some(99.0)
        );
    }

    #[cfg_attr(test, test)]
    fn numeric_events_update_the_thumb_readout_and_consumer() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        let root = world.spawn_empty().id();
        let thumb = world.spawn(Node::default()).id();
        let readout = world.spawn(Text::new("0%")).id();
        let slider = world
            .spawn((
                SliderSand {
                    start: 0.0,
                    end: 100.0,
                    step: 5.0,
                    decimals: 0,
                },
                SliderValue(0.0),
                SliderParts {
                    thumb,
                    readout,
                    suffix: "%".into(),
                },
            ))
            .observe(change)
            .observe(move |event: On<SliderChanged>, mut commands: Commands| {
                commands.entity(root).insert(SliderValue(event.value));
            })
            .id();
        world.trigger(ValueChange {
            source: slider,
            value: 48.0_f32,
            is_final: false,
        });
        world.flush();
        assert_eq!(world.get::<SliderValue>(slider).unwrap().0, 50.0);
        assert_eq!(world.get::<SliderValue>(root).unwrap().0, 50.0);
        assert_eq!(world.get::<Text>(readout).unwrap().0, "50%");
        assert_eq!(world.get::<Node>(thumb).unwrap().left, percent(50));
        world.trigger(ValueChange {
            source: slider,
            value: f32::NAN,
            is_final: true,
        });
        world.flush();
        assert_eq!(world.get::<SliderValue>(slider).unwrap().0, 50.0);
    }

    crate::laboratory_cases! {
        ranges_snap_to_steps_and_support_decimal_precision,
        numeric_events_update_the_thumb_readout_and_consumer,
    }
}
