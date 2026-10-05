use bevy::{a11y::AccessibilityNode, prelude::*, ui_widgets::Activate};

#[derive(Component)]
pub(super) struct ComposerTools {
    pub panel: Entity,
    pub toggle: Entity,
}

pub(super) fn create(world: &mut World, composer: Entity) -> Entity {
    let toggle = world
        .spawn((
            crate::sand::button(0),
            crate::sand::Square,
            crate::sand::Borderless,
            crate::icons::Tooltip("Message tools".into()),
            Node {
                width: px(32),
                height: px(32),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            ChildOf(composer),
        ))
        .id();
    crate::edit_mode::label(world, toggle, "+", 20.0);
    if let Some(mut node) = world.get_mut::<AccessibilityNode>(toggle) {
        node.set_label("Message tools");
        node.set_expanded(false);
    }
    let panel = world
        .spawn((
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                bottom: percent(100),
                left: px(0),
                width: percent(100),
                max_height: px(300),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(6)),
                ..default()
            },
            GlobalZIndex(20),
            crate::token_style::background(crate::tokens::Token::Surface),
            crate::token_style::border(crate::tokens::Token::Accent),
            ChildOf(composer),
        ))
        .id();
    crate::scroll_sand::attach(world, panel);
    world
        .entity_mut(composer)
        .insert(ComposerTools { panel, toggle });
    world.entity_mut(toggle).observe(
        move |_: On<Activate>,
              tools: Query<&ComposerTools>,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>| {
            let Ok(tools) = tools.get(composer) else {
                return;
            };
            let Ok(mut node) = nodes.get_mut(tools.panel) else {
                return;
            };
            let expanded = node.display == Display::None;
            node.display = if expanded {
                Display::Flex
            } else {
                Display::None
            };
            if let Ok(mut node) = accessibility.get_mut(tools.toggle) {
                node.set_expanded(expanded);
            }
        },
    );
    world.entity_mut(composer).observe(
        move |mut event: On<
            bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>,
        >,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>,
              mut focus: ResMut<bevy::input_focus::InputFocus>| {
            if event.input.key_code != KeyCode::Escape || !event.input.state.is_pressed() {
                return;
            }
            let Ok(mut node) = nodes.get_mut(panel) else {
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
    world.entity_mut(composer).observe(
        move |_: On<bevy::input_focus::FocusLost>,
              focus: Res<bevy::input_focus::InputFocus>,
              parents: Query<&ChildOf>,
              mut nodes: Query<&mut Node>,
              mut accessibility: Query<&mut AccessibilityNode>| {
            let mut cursor = focus.get();
            while let Some(entity) = cursor {
                if entity == toggle || entity == panel {
                    return;
                }
                cursor = parents.get(entity).ok().map(ChildOf::parent);
            }
            if let Ok(mut node) = nodes.get_mut(panel) {
                node.display = Display::None;
            }
            if let Ok(mut node) = accessibility.get_mut(toggle) {
                node.set_expanded(false);
            }
        },
    );
    panel
}
