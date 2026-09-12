use crate::{
    actions::{Action, ActionButton, ActionsPlugin},
    icons::{Icon, IconButton},
    sand::{Square, button},
    theme::Typography,
    wake::WakeSignal,
};
use bevy::prelude::*;

#[derive(Resource)]
pub struct Notifications {
    pub log: cell::Diagnostics,
    subscription: Option<cell::DiagnosticSubscription>,
    revision: Option<u64>,
    count: usize,
}

impl Notifications {
    pub fn new(log: cell::Diagnostics) -> Self {
        Self {
            log,
            subscription: None,
            revision: None,
            count: 0,
        }
    }
}

impl Default for Notifications {
    fn default() -> Self {
        Self::new(cell::Diagnostics::global())
    }
}

#[derive(Component)]
pub(crate) struct NotificationPanel(Option<u64>);

#[derive(Component)]
pub(crate) struct NotificationCount;

#[derive(Clone, Copy)]
pub enum NotificationAction {
    Dismiss(u64),
    DismissAll,
}

impl Action for NotificationAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        vec![crate::inspection::Connection {
            target,
            name: match self {
                Self::Dismiss(_) => "Notification Clicked Dismiss",
                Self::DismissAll => "Notifications Clicked Dismiss All",
            }
            .into(),
        }]
    }

    fn apply(&self, world: &mut World, _: Entity) {
        let log = world.resource::<Notifications>().log.clone();
        match self {
            Self::Dismiss(id) => log.dismiss(*id),
            Self::DismissAll => {
                for notice in log.snapshot().1 {
                    log.dismiss(notice.id);
                }
            }
        }
    }
}

pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ActionsPlugin>() {
            app.add_plugins(ActionsPlugin);
        }
        app.init_resource::<Notifications>()
            .add_systems(Update, render);
    }
}

pub fn report(world: &World, source: &str, message: &str) {
    let log = world
        .get_resource::<Notifications>()
        .map(|notifications| notifications.log.clone())
        .unwrap_or_else(cell::Diagnostics::global);
    log.report(source, message);
}

pub(crate) fn panel(world: &mut World, parent: Entity) {
    world.spawn((
        NotificationPanel(None),
        ChildOf(parent),
        Node {
            width: percent(100),
            flex_direction: FlexDirection::Column,
            row_gap: px(10),
            flex_shrink: 0.0,
            ..default()
        },
    ));
}

fn recommendation(source: &str, message: &str) -> &'static str {
    let detail = format!("{source} {message}").to_lowercase();
    if detail.contains("no space") || detail.contains("disk full") {
        "Try this: free some disk space, then reopen Lince."
    } else if detail.contains("permission denied") || detail.contains("read-only") {
        "Try this: check that your account can write to the folder named above, then reopen Lince."
    } else if detail.contains("tray") {
        "You can keep using Lince. Enable a system tray in your desktop if you want its tray icon."
    } else if detail.contains("malformed")
        || detail.contains("invalid")
        || detail.contains("decode")
    {
        "Try this: keep a copy of the affected data and reopen Lince. If it happens again, share this message when reporting the problem."
    } else if detail.contains("connection to this cell") || detail.contains("interface::connection")
    {
        "Try this: reopen Lince to reconnect to this Cell. Keep a copy of any text whose save was not confirmed."
    } else if detail.contains("connection")
        || detail.contains("peer")
        || detail.contains("discovery")
        || detail.contains("sync")
    {
        "Try this: check your network and that the other Cell is running. If the connection stays unavailable, reopen Lince."
    } else if detail.contains("save") || detail.contains("write") || detail.contains("storage") {
        "Try this: check free disk space and access to the data folder. Keep Lince open and copy any unsaved text before restarting."
    } else {
        "Try this: repeat the action. If it keeps failing, keep a copy of your text and share this message when reporting the problem."
    }
}

fn render(world: &mut World) {
    if world.resource::<Notifications>().subscription.is_none()
        && let Some(wake) = world.get_resource::<WakeSignal>().cloned()
    {
        let subscription = world
            .resource::<Notifications>()
            .log
            .subscribe(move || wake.ring());
        world.resource_mut::<Notifications>().subscription = Some(subscription);
    }
    let log = world.resource::<Notifications>().log.clone();
    let revision = log.revision();
    if world.resource::<Notifications>().revision != Some(revision) {
        let count = log.snapshot().1.len();
        let mut state = world.resource_mut::<Notifications>();
        state.revision = Some(revision);
        state.count = count;
    }
    let count = world.resource::<Notifications>().count;
    let mut labels =
        world.query_filtered::<&mut crate::icons::IconButton, With<NotificationCount>>();
    for mut label in labels.iter_mut(world) {
        let value = if count == 0 {
            "Notifications".into()
        } else {
            format!("Notifications ({count})")
        };
        if label.label != value {
            label.label = value;
        }
    }
    let panels: Vec<_> = world
        .query::<(Entity, &NotificationPanel)>()
        .iter(world)
        .filter(|(_, panel)| panel.0 != Some(revision))
        .map(|(entity, _)| entity)
        .collect();
    if panels.is_empty() {
        return;
    }
    let (_, notices) = log.snapshot();
    for panel in panels {
        world.entity_mut(panel).despawn_children();
        world.get_mut::<NotificationPanel>(panel).unwrap().0 = Some(revision);
        if notices.is_empty() {
            label(world, panel, "No notifications.", 15.0);
            continue;
        }
        let row = world
            .spawn((
                ChildOf(panel),
                Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .id();
        label(world, row, "Notifications", 18.0);
        dismiss(
            world,
            panel,
            row,
            NotificationAction::DismissAll,
            "Dismiss all notifications",
        );
        for notice in notices.iter().rev() {
            let row = world
                .spawn((
                    ChildOf(panel),
                    Square,
                    crate::token_style::background(crate::tokens::Token::Surface),
                    crate::token_style::border(crate::tokens::Token::Accent),
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
                crate::information::open_button(world, content, panel);
            } else {
                label(
                    world,
                    content,
                    recommendation(&notice.source, &notice.message),
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
            dismiss(
                world,
                panel,
                row,
                NotificationAction::Dismiss(notice.id),
                "Dismiss notification",
            );
        }
    }
    if let Some(wake) = world.get_resource::<WakeSignal>() {
        wake.ring();
    }
}

fn label(world: &mut World, parent: Entity, value: &str, size: f32) {
    let font = world.resource::<Typography>().text(size);
    world.spawn((
        Text::new(value),
        font,
        crate::token_style::text(crate::tokens::Token::Ink),
        ChildOf(parent),
    ));
}

fn dismiss(
    world: &mut World,
    panel: Entity,
    parent: Entity,
    action: NotificationAction,
    label: &str,
) {
    world.spawn((
        button(0),
        IconButton::new(Icon::Close, label),
        ActionButton::new(panel, crate::actions![action]),
        ChildOf(parent),
        Node {
            padding: UiRect::all(px(5)),
            flex_shrink: 0.0,
            align_self: AlignSelf::Start,
            ..default()
        },
    ));
}

pub(crate) mod tests {
    use super::*;
    use bevy::ui_widgets::Activate;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[cfg_attr(test, test)]
    fn recommendations_distinguish_local_connections_storage_and_desktop_problems() {
        assert!(recommendation("interface::connection", "Connection closed").contains("this Cell"));
        assert!(recommendation("cell", "No space left on device").contains("disk space"));
        assert!(recommendation("cell", "Permission denied").contains("your account"));
        assert!(
            recommendation("lince_interface::tray", "Unavailable").contains("keep using Lince")
        );
    }

    #[cfg_attr(test, test)]
    fn notifications_wake_render_once_and_dismiss_through_actions() {
        let log = cell::Diagnostics::default();
        let wakes = Arc::new(AtomicUsize::new(0));
        let count = wakes.clone();
        let mut app = App::new();
        crate::laboratory::isolate(app.world_mut());
        app.init_resource::<Assets<Font>>()
            .init_resource::<Typography>()
            .insert_resource(Notifications::new(log.clone()))
            .insert_resource(WakeSignal::new(move || {
                count.fetch_add(1, Ordering::Relaxed);
            }))
            .add_plugins(NotificationsPlugin);
        let root = app.world_mut().spawn(crate::container::BoxRoot).id();
        panel(app.world_mut(), root);
        app.update();
        let before = wakes.load(Ordering::Relaxed);
        log.report("cell", "Could not save the document");
        assert!(wakes.load(Ordering::Relaxed) > before);
        log.report("cell", "Could not save the document");
        app.update();
        let panel = app
            .world_mut()
            .query_filtered::<Entity, With<NotificationPanel>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::Flex
        );
        assert!(
            app.world_mut()
                .query::<&Text>()
                .iter(app.world())
                .any(|text| text.0 == "Seen 2 times")
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, (With<Square>, Without<ActionButton>)>()
                .iter(app.world())
                .count(),
            1
        );
        let entities = app.world().entities().len();
        app.update();
        assert_eq!(app.world().entities().len(), entities);
        assert!(
            !app.world()
                .entity(panel)
                .get_ref::<Node>()
                .unwrap()
                .is_changed()
        );
        let close = app
            .world_mut()
            .query::<(Entity, &IconButton)>()
            .iter(app.world())
            .find(|(_, icon)| icon.label == "Dismiss notification")
            .unwrap()
            .0;
        app.world_mut().trigger(Activate { entity: close });
        app.update();
        app.update();
        assert!(log.snapshot().1.is_empty());
        assert_eq!(
            app.world().get::<Node>(panel).unwrap().display,
            Display::Flex
        );
    }

    crate::laboratory_cases! {
        recommendations_distinguish_local_connections_storage_and_desktop_problems,
        notifications_wake_render_once_and_dismiss_through_actions,
    }
}
