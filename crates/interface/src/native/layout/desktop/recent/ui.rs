use super::*;
use crate::{
    i18n::Token,
    native::{edit_mode::label, shortcuts::Shortcut},
};
use bevy::input_focus::tab_navigation::TabGroup;

pub(super) fn open(
    world: &mut World,
    root: Entity,
    entries: Vec<Entry>,
    selected: usize,
    hold: Modifiers,
) {
    focus::remember_input(world, root);
    let previous = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    let workspace = world.get::<Workspaces>(root).unwrap().active;
    let overlay = world
        .spawn((
            Name::new("desktop.recent"),
            crate::native::inspection::InspectionExcluded,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(95),
            TabGroup::modal(),
            ChildOf(root),
            crate::native::actions::KeyBindings(vec![crate::native::actions::KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::native::actions![Change::Cancel],
            )]),
        ))
        .id();
    let mut accessible = accesskit::Node::new(accesskit::Role::Dialog);
    accessible.set_label(Token::RecentWindows.text());
    world
        .entity_mut(overlay)
        .insert(bevy::a11y::AccessibilityNode::from(accessible));
    let panel = world
        .spawn((
            Node {
                width: px(520),
                max_width: percent(96),
                max_height: percent(70),
                min_height: px(0),
                padding: UiRect::all(px(16)),
                row_gap: px(10),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            crate::native::token_style::background(crate::native::tokens::Token::Surface),
            ChildOf(overlay),
        ))
        .id();
    label(world, panel, Token::RecentWindows.text(), 22.0);
    let bindings = world.resource::<Settings>();
    let keyboard = world.get_resource::<crate::host::input::KeyboardSnapshot>();
    let help = format!(
        "{} / {}\n{}",
        bindings.display_text(Shortcut::RecentNext, keyboard),
        bindings.display_text(Shortcut::RecentPrevious, keyboard),
        Token::RecentHelp.text()
    );
    let help = label(world, panel, &help, 13.0);
    world.get_mut::<Node>(help).unwrap().width = percent(100);
    world.entity_mut(help).insert(TextLayout::linebreak(
        bevy::text::LineBreak::WordOrCharacter,
    ));
    let list = world
        .spawn((
            Node {
                min_height: px(0),
                flex_shrink: 1.0,
                width: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            crate::native::edit_mode::FocusScroll,
            ChildOf(panel),
        ))
        .observe(|mut event: On<Pointer<Scroll>>| event.propagate(false))
        .id();
    let mut rows = BTreeMap::new();
    for entry in &entries {
        let (button, caption) = input_settings::button(
            world,
            list,
            overlay,
            &entry.title,
            Change::Choose(entry.target),
        );
        world.entity_mut(button).insert(Name::new(format!(
            "recent.target.{}",
            entry.target.entity.to_bits()
        )));
        rows.insert(entry.target, (button, caption));
    }
    let mut session = Session {
        root,
        workspace,
        output: catalogue::output(world, root),
        overlay,
        entries: entries.into_iter().map(|entry| entry.target).collect(),
        selected,
        rows,
        previous,
        hold,
    };
    crate::native::shortcuts::own_keyboard(world, overlay);
    refresh(world, &mut session, true);
    world.resource_mut::<Recent>().session = Some(session);
}

pub(super) fn refresh(world: &mut World, session: &mut Session, focus: bool) {
    let targets: BTreeSet<_> = session.entries.iter().copied().collect();
    session.rows.retain(|target, (button, _)| {
        if targets.contains(target) {
            return true;
        }
        if world.get_entity(*button).is_ok() {
            world.despawn(*button);
        }
        false
    });
    for (index, target) in session.entries.iter().enumerate() {
        let Some(entry) = catalogue::entry(world, session.root, *target) else {
            continue;
        };
        let Some((button, caption)) = session.rows.get(target).copied() else {
            continue;
        };
        let selected = index == session.selected;
        let text = format!(
            "{}{}\n{}",
            if selected { "▸ " } else { "" },
            entry.title,
            entry.context
        );
        if let Some(mut caption) = world.get_mut::<Text>(caption)
            && caption.0 != text
        {
            caption.0 = text.clone();
        }
        if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(button) {
            node.set_label(text);
            node.set_selected(selected);
        }
        world
            .entity_mut(button)
            .insert(crate::native::token_style::border(if selected {
                crate::native::tokens::Token::Accent
            } else {
                crate::native::tokens::Token::SandBorder
            }));
        if selected
            && focus
            && let Some(mut input) = world.get_resource_mut::<InputFocus>()
        {
            input.set(button, FocusCause::Navigated);
        }
    }
}
