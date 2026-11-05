use crate::native::{
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

#[derive(Component)]
pub(crate) struct DropdownMenu;

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
                width: percent(100),
                min_width: px(0),
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
            crate::native::sand::button(0),
            crate::native::sand::Square,
            token_style::background(Token::Surface),
            token_style::border(Token::Accent),
            Node {
                width: percent(100),
                min_width: px(0),
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
    let caption = label(world, toggle, selected, 14.0);
    world.entity_mut(caption).insert((
        Node {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            min_width: px(0),
            ..default()
        },
        TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
    ));
    label(world, toggle, "v", 14.0);
    let menu = world
        .spawn((
            Castle,
            DropdownMenu,
            Node {
                display: Display::None,
                width: percent(100),
                min_width: px(0),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::left(px(8)),
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
                crate::native::sand::button(0),
                crate::native::sand::Square,
                ActionButton::new(target, actions),
                token_style::background(Token::Surface),
                token_style::border(Token::Accent),
                Node {
                    width: percent(100),
                    min_width: px(0),
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
        let caption = label(world, button, &title, 14.0);
        world.entity_mut(caption).insert((
            Node {
                min_width: px(0),
                flex_shrink: 1.0,
                ..default()
            },
            TextLayout::linebreak(bevy::text::LineBreak::WordOrCharacter),
        ));
        world
            .entity_mut(button)
            .observe(move |_: On<Activate>, mut commands: Commands| {
                commands.queue(move |world: &mut World| {
                    if let Some(mut node) = world.get_mut::<Node>(menu) {
                        node.display = Display::None;
                    }
                    if let Some(mut node) = world.get_mut::<AccessibilityNode>(toggle) {
                        node.set_expanded(false);
                    }
                    if world.get_entity(toggle).is_ok() {
                        world
                            .resource_mut::<bevy::input_focus::InputFocus>()
                            .set(toggle, bevy::input_focus::FocusCause::Navigated);
                    }
                    crate::native::customization::synchronize_keyboard(world);
                });
            });
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input_focus::{FocusCause, InputFocus, tab_navigation::TabIndex};

    #[test]
    fn hidden_choices_leave_tab_order_and_selection_closes_without_replacing_controls() {
        let mut world = World::new();
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<InputFocus>();
        let root = world.spawn(Node::default()).id();
        let toggle = spawn(
            &mut world,
            root,
            root,
            "Selected Area",
            "First",
            vec![
                ("First".into(), ActionSequence::default()),
                ("Second".into(), ActionSequence::default()),
            ],
        );
        let menu = world.get::<Dropdown>(toggle).unwrap().menu;
        let choices = world
            .get::<Children>(menu)
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        crate::native::customization::synchronize_keyboard(&mut world);
        assert!(
            choices
                .iter()
                .all(|entity| world.get::<TabIndex>(*entity).unwrap().0 == -1)
        );
        world.trigger(Activate { entity: toggle });
        world.flush();
        crate::native::customization::synchronize_keyboard(&mut world);
        assert!(
            choices
                .iter()
                .all(|entity| world.get::<TabIndex>(*entity).unwrap().0 == 0)
        );
        world
            .resource_mut::<InputFocus>()
            .set(choices[1], FocusCause::Navigated);
        world.trigger(Activate { entity: choices[1] });
        world.flush();
        assert_eq!(world.get::<Node>(menu).unwrap().display, Display::None);
        assert_eq!(world.resource::<InputFocus>().get(), Some(toggle));
        assert!(
            choices
                .iter()
                .all(|entity| world.get::<TabIndex>(*entity).unwrap().0 == -1)
        );
        assert!(world.get::<Dropdown>(toggle).is_some());
    }
}
