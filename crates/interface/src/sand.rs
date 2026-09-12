use crate::theme::{PURPLE, Typography};
use bevy::{
    input_focus::{FocusCause, FocusGained, FocusLost, InputFocus, tab_navigation::TabIndex},
    prelude::*,
    text::{EditableText, TextCursorStyle},
    ui_widgets::Button as WidgetButton,
};
use std::time::Duration;

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
#[require(Node, BackgroundColor, Outline, crate::token_style::OutlineToken = crate::token_style::OutlineToken(crate::tokens::Token::Accent))]
pub struct Square;

#[derive(Component, Default, Reflect)]
#[reflect(Component)]
#[require(ImageNode)]
pub struct ImageSand;

#[derive(Component, Clone, Copy, Reflect)]
#[reflect(Component)]
pub struct InBox(#[entities] pub Entity);

pub const BUTTON_BORDER_WIDTH: f32 = 1.0;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StyleButtons;

pub struct SandPlugin;

impl Plugin for SandPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<crate::tokens::ThemeSettings>()
            .register_type::<Square>()
            .register_type::<ImageSand>()
            .register_type::<InBox>()
            .init_resource::<InputFocus>()
            .add_systems(
                PostUpdate,
                button_borders
                    .in_set(StyleButtons)
                    .after(crate::icons::SyncIcons)
                    .after(crate::token_style::ApplyTokenStyles)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_observer(
                |event: On<FocusGained>,
                 settings: Res<crate::tokens::ThemeSettings>,
                 mut squares: Query<&mut Outline, With<Square>>| {
                    if event.cause == FocusCause::Navigated
                        && event.entity == event.original_event_target()
                        && let Ok(mut outline) = squares.get_mut(event.entity)
                    {
                        *outline = Outline {
                            width: px(2),
                            offset: px(4),
                            color: settings
                                .resolve(crate::tokens::Token::Accent, None, &Default::default())
                                .0
                                .color(),
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

fn button_borders(
    settings: Res<crate::tokens::ThemeSettings>,
    mut buttons: Query<
        (
            Entity,
            &mut Node,
            Option<&BorderColor>,
            Option<&mut Outline>,
        ),
        Or<(With<WidgetButton>, With<crate::actions::ActionButton>)>,
    >,
    mut commands: Commands,
) {
    for (entity, mut node, color, outline) in &mut buttons {
        if let Some(mut outline) = outline
            && outline.width == px(1)
            && outline.offset == px(0)
        {
            outline.width = px(0);
        }
        let border = UiRect::all(px(settings
            .resolve(
                crate::tokens::Token::ControlBorder,
                None,
                &Default::default(),
            )
            .0
            .number()));
        if node.border != border {
            node.border = border;
        }
        if color.is_none() {
            commands
                .entity(entity)
                .insert(crate::token_style::border(crate::tokens::Token::Accent));
        }
    }
}

pub fn editable(value: &str) -> EditableText {
    EditableText {
        allow_newlines: true,
        visible_lines: Some(3.0),
        max_characters: Some(256),
        cursor_blink_period: Duration::MAX,
        ..EditableText::new(value)
    }
}

pub fn text_editor(value: &str, typography: &Typography, tab_index: i32) -> impl Bundle + use<> {
    (
        editable(value),
        Node {
            width: percent(100),
            ..default()
        },
        typography.text(22.0),
        crate::token_style::text(crate::tokens::Token::Ink),
        crate::token_style::CursorToken(crate::tokens::Token::Accent),
        TextCursorStyle {
            color: PURPLE,
            ..default()
        },
        TabIndex(tab_index),
    )
}

pub fn button(tab_index: i32) -> impl Bundle {
    (WidgetButton, TabIndex(tab_index))
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn every_action_button_uses_the_shared_border_without_resizing_its_content() {
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.add_plugins(SandPlugin);
        let button = app
            .world_mut()
            .spawn((
                button(0),
                Square,
                Node {
                    width: px(160),
                    height: px(48),
                    border: UiRect::all(px(5)),
                    ..default()
                },
            ))
            .id();
        let action = app
            .world_mut()
            .spawn((
                Node::default(),
                crate::actions::ActionButton::new(button, crate::actions![]),
            ))
            .id();
        app.update();
        for entity in [button, action] {
            assert_eq!(
                app.world().get::<Node>(entity).unwrap().border,
                UiRect::all(px(BUTTON_BORDER_WIDTH))
            );
        }
        assert_eq!(app.world().get::<Outline>(button).unwrap().width, px(0));
        app.world_mut().get_mut::<Outline>(button).unwrap().width = px(2);
        app.update();
        assert_eq!(app.world().get::<Outline>(button).unwrap().width, px(2));
        assert_eq!(app.world().get::<Node>(button).unwrap().width, px(160));
        assert_eq!(app.world().get::<Node>(button).unwrap().height, px(48));
    }

    crate::laboratory_cases! {
        every_action_button_uses_the_shared_border_without_resizing_its_content,
    }
}
