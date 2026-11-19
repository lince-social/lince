pub(crate) mod tests;
mod ui;

use crate::native::{
    actions::{Action, ActionsPlugin},
    wake::WakeSignal,
};
use bevy::prelude::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

pub(crate) use ui::close;

#[derive(Resource)]
pub struct Notifications {
    pub log: cell::Diagnostics,
    subscription: Option<cell::DiagnosticSubscription>,
    revision: u64,
    snapshot: Arc<NotificationSnapshot>,
    toasts: Arc<HashMap<u64, u64>>,
}

struct NotificationSnapshot {
    notices: Vec<cell::Notice>,
    label: String,
    badge: String,
}

impl NotificationSnapshot {
    fn new(notices: Vec<cell::Notice>) -> Self {
        nucleus::performance::add("notification-snapshots-prepared", 1);
        let count = notices.len();
        Self {
            notices,
            label: if count == 0 {
                "Notifications".into()
            } else {
                format!("Notifications ({count})")
            },
            badge: count.to_string(),
        }
    }
}

impl Notifications {
    pub fn new(log: cell::Diagnostics) -> Self {
        let (revision, notices) = log.snapshot();
        Self {
            log,
            subscription: None,
            revision,
            snapshot: Arc::new(NotificationSnapshot::new(notices)),
            toasts: Arc::default(),
        }
    }
}

impl Default for Notifications {
    fn default() -> Self {
        Self::new(cell::Diagnostics::global())
    }
}

#[derive(Component)]
struct NotificationPanel(Option<u64>);

#[derive(Component)]
struct NotificationCenter {
    button: Entity,
    panel: Option<Entity>,
    revision: u64,
}

#[derive(Component)]
struct ToastStack(Entity);

#[derive(Component)]
struct NotificationToast {
    id: u64,
    occurrences: u64,
}

#[derive(Component)]
struct NotificationCount;

#[derive(Component)]
struct NotificationBadge(Entity);

#[derive(Clone, Copy)]
pub enum NotificationAction {
    Open,
    Toggle,
    Close,
    CloseToast(u64),
    Delete(u64),
    DeleteAll,
}

impl Action for NotificationAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::native::inspection::Connection> {
        vec![crate::native::inspection::Connection {
            target,
            name: match self {
                Self::Open => "Notifications Clicked Open",
                Self::Toggle => "Notifications Clicked Toggle",
                Self::Close => "Notifications Clicked Close",
                Self::CloseToast(_) => "Notification Toast Clicked Close",
                Self::Delete(_) => "Notification Clicked Delete",
                Self::DeleteAll => "Notifications Clicked Delete All",
            }
            .into(),
        }]
    }

    fn apply(&self, world: &mut World, target: Entity) {
        match *self {
            Self::Open => {
                if world
                    .get::<NotificationCenter>(target)
                    .is_some_and(|center| center.panel.is_none())
                {
                    ui::toggle(world, target);
                }
            }
            Self::Toggle => ui::toggle(world, target),
            Self::Close => close(world, target),
            Self::CloseToast(id) => ui::remove_toast(world, id),
            Self::Delete(id) => {
                world.resource::<Notifications>().log.dismiss(id);
                ui::remove_toast(world, id);
            }
            Self::DeleteAll => {
                let log = world.resource::<Notifications>().log.clone();
                for notice in log.snapshot().1 {
                    log.dismiss(notice.id);
                    ui::remove_toast(world, notice.id);
                }
            }
        }
    }
}

pub struct NotificationsPlugin;

pub(crate) fn open_state(world: &World, root: Entity) -> Option<(bool, u64)> {
    world
        .get::<NotificationCenter>(root)
        .map(|center| (center.panel.is_some(), center.revision))
}

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ActionsPlugin>() {
            app.add_plugins(ActionsPlugin);
        }
        app.init_resource::<Notifications>()
            .add_systems(
                Update,
                (ui::setup, cell_connection, render)
                    .chain()
                    .after(crate::native::edit_mode::setup)
                    .after(crate::native::cell_bridge::ReceiveCell),
            )
            .add_systems(
                PostUpdate,
                ui::render_badges
                    .after(crate::native::icons::SyncIcons)
                    .after(crate::native::token_metrics::layout)
                    .before(bevy::ui::UiSystems::Prepare),
            )
            .add_systems(PostUpdate, ui::anchor.after(bevy::ui::UiSystems::Layout));
    }
}

pub fn report(world: &World, source: &str, message: &str) {
    let log = world
        .get_resource::<Notifications>()
        .map(|notifications| notifications.log.clone())
        .unwrap_or_else(cell::Diagnostics::global);
    log.report(source, message);
}

pub(crate) fn cell_connection(
    messages: Option<Res<Messages<crate::native::cell_bridge::CellMessage>>>,
    notifications: Option<Res<Notifications>>,
    mut cursor: Local<bevy::ecs::message::MessageCursor<crate::native::cell_bridge::CellMessage>>,
) {
    let Some(messages) = messages else {
        return;
    };
    for event in cursor.read(&messages) {
        if let cell::ServerMessage::Error { id, message, .. } = event.0.as_ref()
            && id == crate::native::cell_bridge::CONNECTION
        {
            if let Some(notifications) = &notifications {
                notifications.log.report("interface::connection", message);
            } else {
                cell::Diagnostics::global().report("interface::connection", message);
            }
        }
    }
}

fn recommendation(source: &str, message: &str) -> &'static str {
    if source == "lince::owner-backup" && message.starts_with("Encrypted owner backup saved.") {
        return "Keep the backup file and its passphrase. Restoring this file in Lince is still being implemented.";
    }
    let detail = format!("{source} {message}").to_lowercase();
    if detail.contains("no space") || detail.contains("disk full") {
        "Try this: free some disk space, then reopen Lince."
    } else if message.starts_with("Workspace restored with compatible items.")
        && message.contains("Saving is enabled.")
    {
        "You can keep working. The skipped items remain in the saved original."
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
        "Try this: open the Configuration Sand and choose Refresh to reconnect to this Cell. Keep any text whose save was not confirmed until you check its Record."
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

#[derive(Default)]
struct SnapshotPreparation {
    previous: HashSet<(u64, u64)>,
    present: HashSet<u64>,
}

fn render(
    world: &mut World,
    labels: &mut QueryState<&'static mut crate::native::icons::IconButton, With<NotificationCount>>,
    mut preparation: Local<SnapshotPreparation>,
) {
    let _presentation = nucleus::performance::Span::new("notification-presentation");
    if world.resource::<Notifications>().subscription.is_none()
        && let Some(wake) = world.get_resource::<WakeSignal>().cloned()
    {
        let subscription = world
            .resource::<Notifications>()
            .log
            .subscribe(move || wake.ring());
        world.resource_mut::<Notifications>().subscription = Some(subscription);
    }
    let changed = {
        let state = world.resource::<Notifications>();
        (state.revision != state.log.revision()).then(|| state.log.clone())
    };
    if let Some(log) = changed {
        let _preparation = nucleus::performance::Span::new("notification-snapshot-preparation");
        let (revision, notices) = log.snapshot();
        let mut state = world.resource_mut::<Notifications>();
        preparation.previous.clear();
        preparation.previous.extend(
            state
                .snapshot
                .notices
                .iter()
                .map(|notice| (notice.id, notice.occurrences)),
        );
        preparation.present.clear();
        for notice in &notices {
            preparation.present.insert(notice.id);
            if !preparation
                .previous
                .contains(&(notice.id, notice.occurrences))
            {
                Arc::make_mut(&mut state.toasts).insert(notice.id, notice.occurrences);
            }
        }
        Arc::make_mut(&mut state.toasts).retain(|id, _| preparation.present.contains(id));
        nucleus::performance::add(
            "notification-snapshot-entries-indexed",
            (state.snapshot.notices.len() + notices.len()) as u64,
        );
        state.revision = revision;
        state.snapshot = Arc::new(NotificationSnapshot::new(notices));
        preparation.previous.clear();
        preparation.present.clear();
    }
    if world
        .resource::<Notifications>()
        .snapshot
        .notices
        .is_empty()
    {
        preparation.previous.shrink_to_fit();
        preparation.present.shrink_to_fit();
    }
    if labels.iter(world).next().is_some() {
        let snapshot = Arc::clone(&world.resource::<Notifications>().snapshot);
        for mut label in labels.iter_mut(world) {
            if label.label != snapshot.label {
                label.label.clone_from(&snapshot.label);
            }
        }
    }
    world
        .run_system_cached(ui::render_panels)
        .expect("present notification panels");
    world
        .run_system_cached(ui::render_toasts)
        .expect("present notification toasts");
}
