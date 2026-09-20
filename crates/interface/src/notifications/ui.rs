use super::{
    NotificationAction, NotificationBadge, NotificationCenter, NotificationCount,
    NotificationPanel, NotificationToast, Notifications, ToastStack,
};
use crate::{
    actions::{Action, ActionButton, KeyBinding, KeyBindings, Modifiers},
    edit_mode::{EditAction, EditMode},
    icons::{Icon, IconButton},
    sand::{Square, button},
    theme::Typography,
    tokens::Token,
};
use bevy::{
    a11y::AccessibilityNode,
    input_focus::{FocusCause, InputFocus, tab_navigation::TabGroup},
    prelude::*,
};

pub(super) fn setup(world: &mut World) {
    let roots: Vec<_> = world
        .query_filtered::<(Entity, &EditMode), Without<NotificationCenter>>()
        .iter(world)
        .map(|(root, mode)| (root, mode.toggle))
        .collect();
    for (root, edit) in roots {
        let toolbar = world.get::<ChildOf>(edit).unwrap().parent();
        let toggle = control(
            world,
            root,
            toolbar,
            NotificationAction::Toggle,
            Icon::Bell,
            "Notifications",
        );
        world.entity_mut(toggle).insert(NotificationCount);
        let index = world
            .get::<Children>(toolbar)
            .unwrap()
            .iter()
            .position(|child| child == edit)
            .unwrap();
        world.entity_mut(toolbar).insert_children(index, &[toggle]);
        world.entity_mut(root).insert(NotificationCenter {
            button: toggle,
            panel: None,
        });
    }
}

pub(super) fn render_badges(world: &mut World) {
    let count = world.resource::<Notifications>().notices.len();
    let buttons: Vec<_> = world
        .query_filtered::<Entity, With<NotificationCount>>()
        .iter(world)
        .collect();
    for button in buttons {
        let badge = world.get::<Children>(button).and_then(|children| {
            children
                .iter()
                .find(|child| world.get::<NotificationBadge>(*child).is_some())
        });
        if count == 0 {
            if let Some(badge) = badge {
                world.despawn(badge);
            }
            continue;
        }
        let extent = match world.get::<Node>(button).unwrap().width {
            Val::Px(width) => width,
            _ => 40.0,
        };
        let font_size = extent * 0.275;
        let badge = badge.unwrap_or_else(|| {
            let badge = world
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(0),
                        right: px(0),
                        width: percent(50),
                        height: percent(50),
                        border_radius: BorderRadius::MAX,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    crate::token_style::background(Token::Ink),
                    ZIndex(1),
                    Pickable::IGNORE,
                    ChildOf(button),
                ))
                .id();
            let font = world.resource::<Typography>().text(font_size);
            let label = world
                .spawn((
                    Text::new(count.to_string()),
                    font,
                    TextLayout::no_wrap(),
                    crate::token_style::text(Token::Surface),
                    Pickable::IGNORE,
                    ChildOf(badge),
                ))
                .id();
            world.entity_mut(badge).insert(NotificationBadge(label));
            badge
        });
        let label = world.get::<NotificationBadge>(badge).unwrap().0;
        let value = count.to_string();
        if world.get::<Text>(label).unwrap().0 != value {
            world.get_mut::<Text>(label).unwrap().0 = value;
        }
        if world.get::<TextFont>(label).unwrap().font_size != FontSize::Px(font_size) {
            world.get_mut::<TextFont>(label).unwrap().font_size = FontSize::Px(font_size);
        }
    }
}

fn root(world: &World, mut entity: Entity) -> Option<Entity> {
    loop {
        if world.get::<NotificationCenter>(entity).is_some() {
            return Some(entity);
        }
        entity = world.get::<ChildOf>(entity)?.parent();
    }
}

pub(crate) fn close(world: &mut World, target: Entity) {
    let Some(root) = root(world, target) else {
        return;
    };
    let mut center = world.get_mut::<NotificationCenter>(root).unwrap();
    let Some(panel) = center.panel.take() else {
        return;
    };
    let button = center.button;
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(button, FocusCause::Navigated);
    }
    world.despawn(panel);
}

pub(super) fn toggle(world: &mut World, target: Entity) {
    let Some(root) = root(world, target) else {
        return;
    };
    if world
        .get::<NotificationCenter>(root)
        .unwrap()
        .panel
        .is_some()
    {
        close(world, root);
        return;
    }
    EditAction::Close.apply(world, root);
    let drawer = world
        .spawn((
            crate::inspection::InspectionExcluded,
            Square,
            crate::token_style::background(Token::Surface),
            crate::token_style::border(Token::Accent),
            crate::token_metrics::WidthToken(Token::PanelWidth, false),
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(12),
                bottom: px(64),
                width: px(376),
                max_width: percent(94),
                padding: UiRect::all(px(16)),
                border: UiRect::all(px(1)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
            TabGroup::modal(),
            GlobalZIndex(23),
            KeyBindings(vec![KeyBinding::new(
                KeyCode::Escape,
                Modifiers::NONE,
                crate::actions![NotificationAction::Close],
            )]),
            ChildOf(root),
        ))
        .id();
    let heading = world
        .spawn((
            Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                flex_shrink: 0.0,
                ..default()
            },
            ChildOf(drawer),
        ))
        .id();
    label(world, heading, "Notifications", 22.0);
    let close = control(
        world,
        root,
        heading,
        NotificationAction::Close,
        Icon::Close,
        "Close notifications",
    );
    let list = world
        .spawn((
            NotificationPanel(None),
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                min_height: px(0),
                flex_grow: 1.0,
                ..default()
            },
            ChildOf(drawer),
        ))
        .id();
    crate::scroll_sand::attach(world, list);
    world.get_mut::<NotificationCenter>(root).unwrap().panel = Some(drawer);
    if let Some(mut focus) = world.get_resource_mut::<InputFocus>() {
        focus.set(close, FocusCause::Navigated);
    }
}

pub(super) fn anchor(
    centers: Query<(Entity, &NotificationCenter)>,
    geometry: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut nodes: Query<&mut Node>,
) {
    for (root, center) in &centers {
        let Some(panel) = center.panel else { continue };
        let (Ok((viewport, transform)), Ok((button, button_transform))) =
            (geometry.get(root), geometry.get(center.button))
        else {
            continue;
        };
        if viewport.size().min_element() <= 0.0 || button.size().min_element() <= 0.0 {
            continue;
        }
        let position = button_transform.translation - transform.translation + viewport.size() * 0.5;
        let bottom = px((viewport.size().y - position.y + button.size().y * 0.5)
            * viewport.inverse_scale_factor()
            + 8.0);
        if let Ok(mut node) = nodes.get_mut(panel)
            && node.bottom != bottom
        {
            node.bottom = bottom;
        }
    }
}

pub(super) fn remove_toast(world: &mut World, id: u64) {
    world.resource_mut::<Notifications>().toasts.remove(&id);
    let entities: Vec<_> = world
        .query::<(Entity, &NotificationToast)>()
        .iter(world)
        .filter(|(_, toast)| toast.id == id)
        .map(|(entity, _)| entity)
        .collect();
    for entity in entities {
        preserve_focus(world, entity);
        world.despawn(entity);
    }
    remove_empty_stacks(world);
}

fn preserve_focus(world: &mut World, subtree: Entity) {
    let mut focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    while let Some(entity) = focus {
        if entity == subtree {
            if let Some(root) = root(world, subtree) {
                let center = world.get::<NotificationCenter>(root).unwrap();
                let target = center.panel.unwrap_or(center.button);
                world
                    .resource_mut::<InputFocus>()
                    .set(target, FocusCause::Navigated);
            }
            return;
        }
        focus = world.get::<ChildOf>(entity).map(ChildOf::parent);
    }
}

fn remove_empty_stacks(world: &mut World) {
    let stacks: Vec<_> = world
        .query_filtered::<(Entity, &Children), With<ToastStack>>()
        .iter(world)
        .filter(|(_, children)| {
            !children
                .iter()
                .any(|child| world.get::<NotificationToast>(child).is_some())
        })
        .map(|(entity, _)| entity)
        .collect();
    for stack in stacks {
        world.despawn(stack);
    }
}

pub(super) fn render_toasts(world: &mut World) {
    let state = world.resource::<Notifications>();
    let notices: Vec<_> = state
        .notices
        .iter()
        .filter(|notice| state.toasts.contains(&notice.id))
        .cloned()
        .collect();
    let stale: Vec<_> = world
        .query::<(Entity, &NotificationToast)>()
        .iter(world)
        .filter(|(_, toast)| {
            !notices
                .iter()
                .any(|notice| notice.id == toast.id && notice.occurrences == toast.occurrences)
        })
        .map(|(entity, _)| entity)
        .collect();
    for entity in stale {
        preserve_focus(world, entity);
        world.despawn(entity);
    }
    remove_empty_stacks(world);
    if notices.is_empty() {
        return;
    }
    let roots: Vec<_> = world
        .query_filtered::<Entity, With<NotificationCenter>>()
        .iter(world)
        .collect();
    for root in roots {
        let stack = world
            .query::<(Entity, &ToastStack)>()
            .iter(world)
            .find(|(_, stack)| stack.0 == root)
            .map(|(entity, _)| entity);
        let stack = stack.unwrap_or_else(|| {
            let stack = world
                .spawn((
                    ToastStack(root),
                    crate::inspection::InspectionExcluded,
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(16),
                        bottom: px(64),
                        width: px(376),
                        max_width: percent(94),
                        max_height: percent(60),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    },
                    GlobalZIndex(22),
                    ChildOf(root),
                ))
                .id();
            crate::scroll_sand::attach(world, stack);
            stack
        });
        for notice in &notices {
            let exists = world.get::<Children>(stack).is_some_and(|children| {
                children.iter().any(|child| {
                    world
                        .get::<NotificationToast>(child)
                        .is_some_and(|toast| toast.id == notice.id)
                })
            });
            if exists {
                continue;
            }
            let row = card(world, root, stack, notice, true);
            world.entity_mut(row).insert(NotificationToast {
                id: notice.id,
                occurrences: notice.occurrences,
            });
        }
    }
}

pub(super) fn render_panels(world: &mut World) {
    let revision = world.resource::<Notifications>().revision;
    let panels: Vec<_> = world
        .query::<(Entity, &NotificationPanel)>()
        .iter(world)
        .filter(|(_, panel)| panel.0 != Some(revision))
        .map(|(entity, _)| entity)
        .collect();
    if panels.is_empty() {
        return;
    }
    let notices = world.resource::<Notifications>().notices.clone();
    for panel in panels {
        let root = root(world, panel).unwrap();
        preserve_focus(world, panel);
        world.entity_mut(panel).despawn_children();
        world.get_mut::<NotificationPanel>(panel).unwrap().0 = Some(revision);
        world.spawn((
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            ChildOf(panel),
        ));
        if notices.is_empty() {
            label(world, panel, "No notifications.", 15.0);
            continue;
        }
        control(
            world,
            root,
            panel,
            NotificationAction::DeleteAll,
            Icon::Delete,
            "Delete all notifications",
        );
        for notice in notices.iter().rev() {
            card(world, root, panel, notice, false);
        }
    }
}

fn card(
    world: &mut World,
    root: Entity,
    parent: Entity,
    notice: &cell::Notice,
    toast: bool,
) -> Entity {
    let row = world
        .spawn((
            ChildOf(parent),
            Square,
            crate::token_style::background(Token::Surface),
            crate::token_style::border(Token::Accent),
            Node {
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                width: percent(100),
                column_gap: px(8),
                flex_shrink: 0.0,
                ..default()
            },
        ))
        .id();
    if toast {
        let mut accessibility =
            AccessibilityNode::from(accesskit::Node::new(accesskit::Role::Status));
        accessibility.set_label(notice.message.as_str());
        accessibility.set_live(accesskit::Live::Polite);
        world.entity_mut(row).insert(accessibility);
    }
    let content = world
        .spawn((
            ChildOf(row),
            Node {
                flex_direction: FlexDirection::Column,
                flex_grow: 1.0,
                flex_basis: px(0),
                min_width: px(0),
                row_gap: px(4),
                ..default()
            },
        ))
        .id();
    label(world, content, &notice.message, 15.0);
    if notice.source == "cell::update_available" {
        crate::information::open_button(world, content, root);
    } else if !toast {
        label(
            world,
            content,
            super::recommendation(&notice.source, &notice.message),
            14.0,
        );
    }
    if notice.occurrences > 1 {
        label(
            world,
            content,
            &format!("Seen {} times", notice.occurrences),
            11.0,
        );
    }
    let (action, icon, label) = if toast {
        (
            NotificationAction::CloseToast(notice.id),
            Icon::Close,
            "Close notification toast",
        )
    } else {
        (
            NotificationAction::Delete(notice.id),
            Icon::Delete,
            "Delete notification",
        )
    };
    control(world, root, row, action, icon, label);
    row
}

fn label(world: &mut World, parent: Entity, value: &str, size: f32) {
    let font = world.resource::<Typography>().text(size);
    world.spawn((
        Text::new(value),
        font,
        crate::token_style::text(Token::Ink),
        ChildOf(parent),
    ));
}

fn control(
    world: &mut World,
    root: Entity,
    parent: Entity,
    action: NotificationAction,
    icon: Icon,
    label: &str,
) -> Entity {
    world
        .spawn((
            button(0),
            IconButton::new(icon, label),
            ActionButton::new(root, crate::actions![action]),
            ChildOf(parent),
            Node {
                padding: UiRect::all(px(7)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                align_self: AlignSelf::Start,
                ..default()
            },
            crate::token_style::background(Token::Surface),
            crate::token_style::border(Token::Accent),
        ))
        .id()
}
