use super::{
    NotificationAction, NotificationBadge, NotificationCenter, NotificationCount,
    NotificationPanel, NotificationToast, Notifications, ToastStack,
};
use crate::i18n::Token as Tr;
use crate::native::{
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
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(super) fn setup(
    world: &mut World,
    roots: &mut QueryState<(Entity, &'static EditMode), Without<NotificationCenter>>,
    mut pending: Local<Vec<(Entity, Entity)>>,
) {
    pending.clear();
    pending.extend(roots.iter(world).map(|(root, mode)| (root, mode.toggle)));
    if pending.is_empty() {
        *pending = Vec::new();
        return;
    }
    for (root, edit) in pending.drain(..) {
        let toolbar = world.get::<ChildOf>(edit).unwrap().parent();
        let toggle = control(
            world,
            root,
            toolbar,
            NotificationAction::Toggle,
            Icon::Bell,
            Tr::Notifications.text(),
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
            revision: 0,
        });
    }
}

pub(super) fn render_badges(
    world: &mut World,
    buttons: &mut QueryState<Entity, With<NotificationCount>>,
    mut pending: Local<Vec<Entity>>,
) {
    let _presentation = nucleus::performance::Span::new("notification-badge-presentation");
    pending.clear();
    pending.extend(buttons.iter(world));
    if pending.is_empty() {
        *pending = Vec::new();
        return;
    }
    let snapshot = Arc::clone(&world.resource::<Notifications>().snapshot);
    let count = snapshot.notices.len();
    for button in pending.drain(..) {
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
                    crate::native::token_style::background(Token::Ink),
                    ZIndex(1),
                    Pickable::IGNORE,
                    ChildOf(button),
                ))
                .id();
            let font = world.resource::<Typography>().text(font_size);
            let label = world
                .spawn((
                    Text::new(snapshot.badge.clone()),
                    font,
                    TextLayout::no_wrap(),
                    crate::native::token_style::text(Token::Surface),
                    Pickable::IGNORE,
                    ChildOf(badge),
                ))
                .id();
            world.entity_mut(badge).insert(NotificationBadge(label));
            badge
        });
        let label = world.get::<NotificationBadge>(badge).unwrap().0;
        if world.get::<Text>(label).unwrap().0 != snapshot.badge {
            world
                .get_mut::<Text>(label)
                .unwrap()
                .0
                .clone_from(&snapshot.badge);
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
    center.revision = center.revision.wrapping_add(1);
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
            crate::native::inspection::InspectionExcluded,
            Square,
            crate::native::token_style::background(Token::Surface),
            crate::native::token_style::border(Token::Accent),
            crate::native::token_metrics::WidthToken(Token::PanelWidth, false),
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
                crate::native::actions![NotificationAction::Close],
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
    label(world, heading, Tr::Notifications.text(), 22.0);
    let close = control(
        world,
        root,
        heading,
        NotificationAction::Close,
        Icon::Close,
        Tr::CloseNotifications.text(),
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
    crate::native::scroll_sand::attach(world, list);
    let mut center = world.get_mut::<NotificationCenter>(root).unwrap();
    center.panel = Some(drawer);
    center.revision = center.revision.wrapping_add(1);
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
    Arc::make_mut(&mut world.resource_mut::<Notifications>().toasts).remove(&id);
    world
        .run_system_cached_with(remove_toast_items, id)
        .expect("dismiss notification toast items");
}

fn remove_toast_items(
    In(id): In<u64>,
    world: &mut World,
    toasts: &mut QueryState<(Entity, &'static NotificationToast)>,
    mut pending: Local<Vec<Entity>>,
) {
    pending.clear();
    pending.extend(
        toasts
            .iter(world)
            .filter(|(_, toast)| toast.id == id)
            .map(|(entity, _)| entity),
    );
    for entity in pending.drain(..) {
        preserve_focus(world, entity);
        world.despawn(entity);
    }
    remove_empty_stacks(world);
    if toasts.iter(world).next().is_none() {
        *pending = Vec::new();
    }
}

fn preserve_focus(world: &mut World, subtree: Entity) {
    let mut focus = world.get_resource::<InputFocus>().and_then(InputFocus::get);
    while let Some(entity) = focus {
        if entity == subtree {
            if let Some(root) = root(world, subtree) {
                let center = world.get::<NotificationCenter>(root).unwrap();
                let target = center.panel.unwrap_or(root);
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
    world
        .run_system_cached(prune_empty_stacks)
        .expect("retire empty notification stacks");
}

fn prune_empty_stacks(
    world: &mut World,
    stacks: &mut QueryState<(Entity, &'static Children), With<ToastStack>>,
    mut pending: Local<Vec<Entity>>,
) {
    pending.clear();
    pending.extend(
        stacks
            .iter(world)
            .filter(|(_, children)| {
                !children
                    .iter()
                    .any(|child| world.get::<NotificationToast>(child).is_some())
            })
            .map(|(entity, _)| entity),
    );
    for stack in pending.drain(..) {
        world.despawn(stack);
    }
    if stacks.iter(world).next().is_none() {
        *pending = Vec::new();
    }
}

#[derive(Default)]
pub(super) struct ToastPreparation {
    stale: Vec<Entity>,
    roots: Vec<Entity>,
    stacks: HashMap<Entity, Entity>,
    existing: HashSet<u64>,
}

impl ToastPreparation {
    fn retire(&mut self) {
        self.stale = Vec::new();
        self.roots = Vec::new();
        self.stacks.clear();
        self.stacks.shrink_to_fit();
        self.existing.clear();
        self.existing.shrink_to_fit();
    }
}

pub(super) fn render_toasts(
    world: &mut World,
    toast_query: &mut QueryState<(Entity, &'static NotificationToast)>,
    roots_query: &mut QueryState<Entity, With<NotificationCenter>>,
    stacks_query: &mut QueryState<(Entity, &'static ToastStack)>,
    mut preparation: Local<ToastPreparation>,
) {
    let _presentation = nucleus::performance::Span::new("notification-toast-presentation");
    let state = world.resource::<Notifications>();
    let snapshot = Arc::clone(&state.snapshot);
    let toasts = Arc::clone(&state.toasts);
    let ToastPreparation {
        stale,
        roots,
        stacks,
        existing,
    } = &mut *preparation;
    stale.clear();
    stale.extend(
        toast_query
            .iter(world)
            .filter(|(_, toast)| toasts.get(&toast.id) != Some(&toast.occurrences))
            .map(|(entity, _)| entity),
    );
    for entity in stale.drain(..) {
        preserve_focus(world, entity);
        world.despawn(entity);
    }
    remove_empty_stacks(world);
    if toasts.is_empty() {
        preparation.retire();
        return;
    }
    roots.clear();
    roots.extend(roots_query.iter(world));
    if roots.is_empty() {
        preparation.retire();
        return;
    }
    let indexed = roots.len() > 1;
    stacks.clear();
    if indexed {
        for (entity, stack) in stacks_query.iter(world) {
            stacks.entry(stack.0).or_insert(entity);
        }
    }
    nucleus::performance::add("notification-toast-roots-prepared", roots.len() as u64);
    for root in roots.drain(..) {
        let stack = if indexed {
            stacks.get(&root).copied()
        } else {
            stacks_query
                .iter(world)
                .find(|(_, stack)| stack.0 == root)
                .map(|(entity, _)| entity)
        };
        let stack = stack.unwrap_or_else(|| {
            let stack = world
                .spawn((
                    ToastStack(root),
                    crate::native::inspection::InspectionExcluded,
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
                    GlobalZIndex(20),
                    ChildOf(root),
                ))
                .id();
            crate::native::scroll_sand::attach(world, stack);
            stack
        });
        existing.clear();
        existing.extend(
            world
                .get::<Children>(stack)
                .into_iter()
                .flat_map(|children| children.iter())
                .filter_map(|child| world.get::<NotificationToast>(child).map(|toast| toast.id)),
        );
        for notice in snapshot
            .notices
            .iter()
            .filter(|notice| toasts.contains_key(&notice.id))
        {
            if !existing.insert(notice.id) {
                continue;
            }
            let row = card(world, root, stack, notice, true);
            world.entity_mut(row).insert(NotificationToast {
                id: notice.id,
                occurrences: notice.occurrences,
            });
        }
    }
    stacks.clear();
    existing.clear();
}

pub(super) fn render_panels(
    world: &mut World,
    panels: &mut QueryState<(Entity, &'static NotificationPanel)>,
    mut pending: Local<Vec<Entity>>,
) {
    let _presentation = nucleus::performance::Span::new("notification-panel-presentation");
    let revision = world.resource::<Notifications>().revision;
    pending.clear();
    pending.extend(
        panels
            .iter(world)
            .filter(|(_, panel)| panel.0 != Some(revision))
            .map(|(entity, _)| entity),
    );
    if pending.is_empty() {
        if panels.iter(world).next().is_none() {
            *pending = Vec::new();
        }
        return;
    }
    let snapshot = Arc::clone(&world.resource::<Notifications>().snapshot);
    for panel in pending.drain(..) {
        let root = root(world, panel).unwrap();
        preserve_focus(world, panel);
        world.entity_mut(panel).despawn_children();
        world.get_mut::<NotificationPanel>(panel).unwrap().0 = Some(revision);
        world.spawn((
            crate::native::sand_store::SandCredits(crate::native::credits::ATTRIBUTIONS),
            ChildOf(panel),
        ));
        if snapshot.notices.is_empty() {
            label(world, panel, Tr::NoNotificationsShort.text(), 15.0);
            continue;
        }
        control(
            world,
            root,
            panel,
            NotificationAction::DeleteAll,
            Icon::Delete,
            Tr::DeleteNotifications.text(),
        );
        for notice in snapshot.notices.iter().rev() {
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
    nucleus::performance::add("notification-cards-created", 1);
    let row = world
        .spawn((
            ChildOf(parent),
            Square,
            crate::native::token_style::background(Token::Surface),
            crate::native::token_style::border(Token::Accent),
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
        crate::native::information::open_button(world, content, root);
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
            Tr::CloseToast.text(),
        )
    } else {
        (
            NotificationAction::Delete(notice.id),
            Icon::Delete,
            Tr::DeleteNotification.text(),
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
        crate::native::token_style::text(Token::Ink),
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
            ActionButton::new(root, crate::native::actions![action]),
            ChildOf(parent),
            Node {
                padding: UiRect::all(px(7)),
                border: UiRect::all(px(1)),
                flex_shrink: 0.0,
                align_self: AlignSelf::Start,
                ..default()
            },
            crate::native::token_style::background(Token::Surface),
            crate::native::token_style::border(Token::Accent),
        ))
        .id()
}
