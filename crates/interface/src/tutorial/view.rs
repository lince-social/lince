use super::*;
use crate::description::button;

const TITLES: [&str; 5] = [
    "Protein Spawn",
    "Attraction",
    "Repulsion",
    "Property on entry",
    "Property on exit",
];
fn instructions(step: usize, prefix: &str) -> String {
    match step {
        0 => format!("Use your normal workspace. Two sample Records have been added to the local Organ.\n\n1. Click Edit mode > Areas of influence > Add square.\n2. In Protein, click + (Make this a Protein Area).\n3. Click its pencil to open the query. Under Filters, click + (Add a condition), choose Text contains and enter:\n{prefix}\nClick Run in the query Castle.\n4. Back in the area's Row template, keep Title and Description; use Add property (+) > Quantity.\n\nRun applies the query to the area. Both sample cards should appear. Scroll the edit panel to reach each section."),
        1 => format!("1. In Areas of influence, click Add circle. This is a separate force area.\n2. Under Filter, click Add property > Title. In Equals, enter:\n{prefix} 1\n3. Keep Attraction enabled. In Force, choose Attract and raise Strength (for example, 100). Choose Reach > Unlimited.\n4. Turn on Physics at the top of the panel. Move the area away from sample 1 if their centers overlap.\n\nWatch sample 1 move toward the area."),
        2 => "Select the same circle under Selection in Areas of influence. Under Force, click Repel. Leave Attraction enabled and Strength above zero.\n\nWatch sample 1 move away. The check uses the force actually applied to the card.".into(),
        3 => format!("1. Turn Physics off at the top of Areas of influence so the cards stay where you drag them.\n2. Click Add square for a separate property area. Move its outline into empty space, away from the cards, before setting changes. Under Filter > Add property, choose Title. Set Equals to:\n{prefix} 1\n3. Under Record changes, set On entry > Quantity to 1 and On exit > Quantity to 0. Keep Change properties enabled.\n4. Drag sample 1 inside the square. If it was already inside when you enabled changes, drag it outside first and wait for the area to finish saving. Use the card's edge to drag; its center must cross the outline.\n\nWait for the card's Quantity to show 1."),
        _ => "Drag sample 1 outside the property square until its center crosses the outline. Its On exit > Quantity value should still be 0.\n\nWait for the card's Quantity to return to 0. The Organ must confirm the change before you can finish.".into(),
    }
}

pub(super) fn render(world: &mut World, root: Entity) {
    let session = world.get::<Session>(root).unwrap();
    let (content, step, unlocked, complete, error, prefix) = (
        session.content,
        session.step,
        session.unlocked,
        session.completed,
        session.error.is_some(),
        session.sample_prefix.clone(),
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
    let instructions = if complete {
        "You used the edit panel to spawn Records, apply attraction and repulsion, and save property changes on entry and exit. Your areas remain in this workspace. You can keep editing them with the same controls.".into()
    } else {
        instructions(step, &prefix)
    };
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
    let text = crate::edit_mode::label(world, instructions_box, &instructions, 15.0);
    world.get_mut::<Node>(text).unwrap().width = percent(100);
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
