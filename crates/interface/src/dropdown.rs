use crate::{
    actions::{ActionButton, ActionSequence},
    castle::Castle,
    edit_mode::label,
    token_style,
    tokens::Token,
};
use bevy::{a11y::AccessibilityNode, prelude::*, ui_widgets::Activate};

#[derive(Component)]
pub struct Dropdown {
    pub menu: Entity,
}

pub fn spawn(
    world: &mut World,
    parent: Entity,
    target: Entity,
    name: &str,
    selected: &str,
    choices: Vec<(String, ActionSequence)>,
) -> Entity {
    let group = world
        .spawn((
            Castle,
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(parent),
        ))
        .id();
    let toggle = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            token_style::background(Token::Surface),
            token_style::border(Token::Accent),
            Node {
                padding: UiRect::axes(px(10), px(7)),
                border: UiRect::all(px(1)),
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            },
            ChildOf(group),
        ))
        .id();
    world
        .get_mut::<AccessibilityNode>(toggle)
        .unwrap()
        .set_label(name);
    world
        .get_mut::<AccessibilityNode>(toggle)
        .unwrap()
        .set_expanded(false);
    label(world, toggle, selected, 14.0);
    label(world, toggle, "v", 14.0);
    let menu = world
        .spawn((
            Castle,
            Node {
                display: Display::None,
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            ChildOf(group),
        ))
        .id();
    world.entity_mut(toggle).insert(Dropdown { menu }).observe(
        |event: On<Activate>,
         dropdowns: Query<&Dropdown>,
         mut nodes: Query<&mut Node>,
         mut accessibility: Query<&mut AccessibilityNode>| {
            let Ok(dropdown) = dropdowns.get(event.entity) else {
                return;
            };
            let Ok(mut node) = nodes.get_mut(dropdown.menu) else {
                return;
            };
            let expanded = node.display == Display::None;
            node.display = if expanded {
                Display::Flex
            } else {
                Display::None
            };
            if let Ok(mut node) = accessibility.get_mut(event.entity) {
                node.set_expanded(expanded);
            }
        },
    );
    world.entity_mut(group).observe(
        move |mut event: On<
            bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>,
        >,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>,
              mut focus: ResMut<bevy::input_focus::InputFocus>| {
            if event.input.key_code != KeyCode::Escape || !event.input.state.is_pressed() {
                return;
            }
            let Ok(mut node) = nodes.get_mut(menu) else {
                return;
            };
            if node.display == Display::None {
                return;
            }
            node.display = Display::None;
            if let Ok(mut node) = accessibility.get_mut(toggle) {
                node.set_expanded(false);
            }
            focus.set(toggle, bevy::input_focus::FocusCause::Navigated);
            event.propagate(false);
        },
    );
    for (title, actions) in choices {
        let button = world
            .spawn((
                crate::sand::button(0),
                crate::sand::Square,
                ActionButton::new(target, actions),
                token_style::background(Token::Surface),
                token_style::border(Token::Accent),
                Node {
                    padding: UiRect::axes(px(10), px(7)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                ChildOf(menu),
            ))
            .id();
        world
            .get_mut::<AccessibilityNode>(button)
            .unwrap()
            .set_label(title.as_str());
        label(world, button, &title, 14.0);
    }
    world.entity_mut(group).observe(
        move |_: On<bevy::input_focus::FocusLost>,
              focus: Res<bevy::input_focus::InputFocus>,
              parents: Query<&ChildOf>,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>| {
            let mut cursor = focus.get();
            while let Some(entity) = cursor {
                if entity == group {
                    return;
                }
                cursor = parents.get(entity).ok().map(ChildOf::parent);
            }
            if let Ok(mut node) = nodes.get_mut(menu)
                && node.display != Display::None
            {
                node.display = Display::None;
            }
            if let Ok(mut node) = accessibility.get_mut(toggle) {
                node.set_expanded(false);
            }
        },
    );
    toggle
}
