pub(crate) mod tests;
mod ui;

use crate::{
    actions::{Action, ActionsPlugin},
    wake::WakeSignal,
};
use bevy::prelude::*;
use std::collections::HashSet;

pub(crate) use ui::close;

#[derive(Resource)]
pub struct Notifications {
    pub log: cell::Diagnostics,
    subscription: Option<cell::DiagnosticSubscription>,
    revision: u64,
    notices: Vec<cell::Notice>,
    toasts: HashSet<u64>,
}

impl Notifications {
    pub fn new(log: cell::Diagnostics) -> Self {
        let (revision, notices) = log.snapshot();
        Self {
            log,
            subscription: None,
            revision,
            notices,
            toasts: HashSet::new(),
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
    Toggle,
    Close,
    CloseToast(u64),
    Delete(u64),
    DeleteAll,
}

impl Action for NotificationAction {
    fn connections(&self, _: &World, target: Entity) -> Vec<crate::inspection::Connection> {
        vec![crate::inspection::Connection {
            target,
            name: match self {
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

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ActionsPlugin>() {
            app.add_plugins(ActionsPlugin);
        }
        app.init_resource::<Notifications>()
            .add_systems(
                Update,
                (ui::setup, render).chain().after(crate::edit_mode::setup),
            )
            .add_systems(
                PostUpdate,
                ui::render_badges
                    .after(crate::icons::SyncIcons)
                    .after(crate::token_metrics::layout)
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
    if world.resource::<Notifications>().revision != log.revision() {
        let (revision, notices) = log.snapshot();
        let mut state = world.resource_mut::<Notifications>();
        for notice in &notices {
            if !state.notices.iter().any(|previous| {
                previous.id == notice.id && previous.occurrences == notice.occurrences
            }) {
                state.toasts.insert(notice.id);
            }
        }
        state
            .toasts
            .retain(|id| notices.iter().any(|notice| notice.id == *id));
        state.revision = revision;
        state.notices = notices;
    }
    let count = world.resource::<Notifications>().notices.len();
    let value = if count == 0 {
        "Notifications".into()
    } else {
        format!("Notifications ({count})")
    };
    for mut label in world
        .query_filtered::<&mut crate::icons::IconButton, With<NotificationCount>>()
        .iter_mut(world)
    {
        if label.label != value {
            label.label.clone_from(&value);
        }
    }
    ui::render_panels(world);
    ui::render_toasts(world);
}
