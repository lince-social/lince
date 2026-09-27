use super::*;
use crate::description::button;

const TITLES: [&str; 5] = [
    "Protein Spawn",
    "Attraction",
    "Repulsion",
    "Property on entry",
    "Property on exit",
];
pub(super) fn render(world: &mut World, root: Entity) {
    let session = world.get::<Session>(root).unwrap();
    let (content, step, unlocked, complete, error) = (
        session.content,
        session.step,
        session.unlocked,
        session.completed,
        session.error.is_some(),
    );
    if let Some(children) = world.get::<Children>(content) {
        let children: Vec<_> = children.iter().collect();
        for child in children {
            world.despawn(child);
        }
    }
    let title = world
        .spawn((
            Node {
                width: percent(100),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    crate::edit_mode::label(world, title, "Areas of Influence", 20.0);
    button(world, title, root, "Close", Command::Close);
    let tabs = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                column_gap: px(4),
                row_gap: px(4),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    for (index, title) in TITLES.iter().enumerate() {
        let tab = button(
            world,
            tabs,
            root,
            &(index + 1).to_string(),
            Command::Step(index),
        );
        world
            .entity_mut(tab)
            .insert(crate::icons::Tooltip((*title).into()));
        if index == step {
            world
                .entity_mut(tab)
                .insert(crate::token_style::background(crate::tokens::Token::Accent));
        }
        if let Some(mut accessibility) = world.get_mut::<bevy::a11y::AccessibilityNode>(tab) {
            accessibility.set_selected(index == step);
            accessibility.set_label(*title);
        }
        if index > unlocked {
            world.entity_mut(tab).insert(bevy::ui::InteractionDisabled);
        }
    }
    crate::description::heading(
        world,
        content,
        if complete {
            "You completed the tutorial"
        } else {
            TITLES[step]
        },
        18.0,
    );
    let mut accessibility = accesskit::Node::new(accesskit::Role::ScrollView);
    accessibility.set_label("Tutorial instructions");
    let instructions_box = world
        .spawn((
            Node {
                width: percent(100),
                min_height: px(64),
                max_height: Val::Vh(40.0),
                flex_direction: FlexDirection::Column,
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            bevy::input_focus::tab_navigation::TabIndex(0),
            bevy::a11y::AccessibilityNode::from(accessibility),
            ChildOf(content),
        ))
        .observe(scroll)
        .observe(keyboard)
        .id();
    world.entity_mut(root).insert(super::guide::Guide {
        container: instructions_box,
        instructions: Vec::new(),
        current: None,
    });
    let controls = world
        .spawn((
            Node {
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.0,
                column_gap: px(6),
                row_gap: px(6),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    if error {
        button(world, controls, root, "Retry connection", Command::Retry);
    }
    if !complete && matches!(step, 0 | 1 | 3) {
        button(
            world,
            controls,
            root,
            if step == 0 {
                "Copy lesson text"
            } else {
                "Copy sample 1 title"
            },
            Command::CopySample,
        );
    }
    let status = crate::edit_mode::label(world, content, "Checking…", 14.0);
    world.get_mut::<Node>(status).unwrap().width = percent(100);
    let footer = world
        .spawn((
            Node {
                justify_content: JustifyContent::SpaceBetween,
                flex_shrink: 0.0,
                width: percent(100),
                ..default()
            },
            ChildOf(content),
        ))
        .id();
    if step > 0 {
        button(world, footer, root, "Back", Command::Step(step - 1));
    }
    let next = button(
        world,
        footer,
        root,
        if complete {
            "Close tutorial"
        } else if step == 4 {
            "Finish"
        } else {
            "Next"
        },
        if complete {
            Command::Close
        } else {
            Command::Next
        },
    );
    if !complete {
        world.entity_mut(next).insert(bevy::ui::InteractionDisabled);
    }
    let mut session = world.get_mut::<Session>(root).unwrap();
    session.status = status;
    session.next = next;
}

fn scroll(
    mut event: On<Pointer<bevy::picking::events::Scroll>>,
    mut scrolls: Query<&mut ScrollPosition>,
) {
    if !event.y.is_finite() {
        return;
    }
    if let Ok(mut scroll) = scrolls.get_mut(event.entity) {
        let scale = if event.unit == bevy::input::mouse::MouseScrollUnit::Line {
            24.0
        } else {
            1.0
        };
        scroll.0.y = (scroll.0.y - event.y * scale).max(0.0);
        event.propagate(false);
    }
}

fn keyboard(
    mut event: On<bevy::input_focus::FocusedInput<bevy::input::keyboard::KeyboardInput>>,
    mut scrolls: Query<(&mut ScrollPosition, &ComputedNode)>,
) {
    if !event.input.state.is_pressed() {
        return;
    }
    let Ok((mut scroll, node)) = scrolls.get_mut(event.focused_entity) else {
        return;
    };
    let page = node.size().y * node.inverse_scale_factor();
    let max = (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
    let position = match event.input.key_code {
        KeyCode::ArrowDown => scroll.0.y + 24.0,
        KeyCode::ArrowUp => scroll.0.y - 24.0,
        KeyCode::PageDown => scroll.0.y + page,
        KeyCode::PageUp => scroll.0.y - page,
        KeyCode::Home => 0.0,
        KeyCode::End => max,
        _ => return,
    };
    scroll.0.y = position.clamp(0.0, max);
    event.propagate(false);
}

pub(super) fn checklist(
    world: &mut World,
    root: Entity,
    instructions: &[super::guide::Instruction],
    current: Option<usize>,
) {
    let container = world.get::<super::guide::Guide>(root).unwrap().container;
    if let Some(children) = world.get::<Children>(container) {
        for child in children.iter().collect::<Vec<_>>() {
            world.despawn(child);
        }
    }
    let mut current_row = None;
    for (index, instruction) in instructions.iter().enumerate() {
        let active = current == Some(index);
        let row = world
            .spawn((
                Node {
                    width: percent(100),
                    flex_shrink: 0.0,
                    padding: UiRect::all(px(6)),
                    border: UiRect::left(px(if active { 3 } else { 0 })),
                    ..default()
                },
                crate::token_style::border(crate::tokens::Token::Connections),
                ChildOf(container),
            ))
            .id();
        let value = format!(
            "{} {}",
            if instruction.done {
                "[x]"
            } else if active {
                "Now:"
            } else {
                "[ ]"
            },
            instruction.text
        );
        if active {
            crate::description::heading(world, row, &value, 15.0);
        } else {
            let text = crate::edit_mode::label(world, row, &value, 14.0);
            world.get_mut::<Node>(text).unwrap().width = percent(100);
        }
        if active {
            world.entity_mut(row).insert(crate::token_style::background(
                crate::tokens::Token::ConnectionFill,
            ));
            let mut accessibility = accesskit::Node::new(accesskit::Role::Status);
            accessibility.set_label(value.as_str());
            accessibility.set_live(accesskit::Live::Polite);
            world
                .entity_mut(row)
                .insert(bevy::a11y::AccessibilityNode::from(accessibility));
            current_row = Some(row);
        }
    }
    world.get_mut::<super::guide::Guide>(root).unwrap().current = current_row;
}
