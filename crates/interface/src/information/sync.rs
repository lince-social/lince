use crate::{
    actions::Action,
    cell_bridge::{CellBridge, CellMessage},
    edit_mode::label,
};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
use nucleus::sync::{Destination, Direction, Instance, Outcome, Overview, Retention};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const REQUEST: &str = "interface-sync-status";

#[derive(Resource)]
struct SyncState {
    overview: Option<Overview>,
    error: Option<String>,
    revision: u64,
    next: Instant,
    waiting: Option<Instant>,
    before: Option<i64>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            overview: None,
            error: None,
            revision: 0,
            next: Instant::now(),
            waiting: None,
            before: None,
        }
    }
}

#[derive(Resource)]
struct SyncWake {
    open: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for SyncWake {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Component)]
struct SyncPanel {
    root: Entity,
    revision: Option<u64>,
}

pub(super) struct SyncPlugin;

impl Plugin for SyncPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SyncState>()
            .add_systems(Startup, connect)
            .add_systems(Update, (receive, poll, render).chain());
    }
}

fn connect(world: &mut World) {
    let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned() else {
        return;
    };
    let open = Arc::new(AtomicBool::new(false));
    let watching = open.clone();
    let task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;
            if watching.load(Ordering::Relaxed) {
                wake.ring();
            }
        }
    });
    world.insert_resource(SyncWake { open, task });
}

pub(super) fn panel(world: &mut World, root: Entity, parent: Entity) {
    world.spawn((
        SyncPanel {
            root,
            revision: None,
        },
        ChildOf(parent),
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            flex_shrink: 0.0,
            width: percent(100),
            ..default()
        },
    ));
    if let Some(mut state) = world.get_resource_mut::<SyncState>() {
        state.next = Instant::now();
    }
}

fn send(world: &mut World, request: ClientMessage) {
    let result = world
        .get_non_send::<CellBridge>()
        .ok_or_else(|| "Sync is not connected.".to_string())
        .and_then(|bridge| {
            bridge
                .outgoing
                .try_send(request)
                .map_err(|error| match error {
                    tokio::sync::mpsc::error::TrySendError::Full(_) => {
                        "Waiting for the Cell. Try again shortly.".into()
                    }
                    tokio::sync::mpsc::error::TrySendError::Closed(_) => {
                        "The Cell connection closed.".into()
                    }
                })
        });
    let mut state = world.resource_mut::<SyncState>();
    state.next = Instant::now() + Duration::from_secs(3);
    match result {
        Ok(()) => state.waiting = Some(Instant::now()),
        Err(error) => {
            state.waiting = None;
            state.error = Some(error);
            state.revision += 1;
        }
    }
}

fn poll(world: &mut World) {
    let open = world.query::<&SyncPanel>().iter(world).next().is_some();
    if let Some(wake) = world.get_resource::<SyncWake>() {
        wake.open.store(open, Ordering::Relaxed);
    }
    if !open {
        return;
    }
    let state = world.resource::<SyncState>();
    if state.next > Instant::now()
        || state
            .waiting
            .is_some_and(|at| at.elapsed() < Duration::from_secs(10))
    {
        return;
    }
    let before = state.before;
    send(
        world,
        ClientMessage::SyncInspect {
            id: REQUEST.into(),
            before,
        },
    );
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let Some(messages) = world.get_resource::<Messages<CellMessage>>() else {
        return;
    };
    let messages: Vec<_> = cursor
        .read(messages)
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        let mut state = world.resource_mut::<SyncState>();
        match message {
            ServerMessage::SyncStatus { id, overview } if id == REQUEST => {
                state.waiting = None;
                if state.overview.as_ref() != Some(&overview) || state.error.is_some() {
                    state.overview = Some(overview);
                    state.error = None;
                    state.revision += 1;
                }
            }
            ServerMessage::Error { id, message, .. } if id == REQUEST => {
                state.waiting = None;
                state.error = Some(message);
                state.revision += 1;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy)]
enum SyncAction {
    Latest,
    Older,
    Retain(u64),
    Clear,
}

impl Action for SyncAction {
    fn apply(&self, world: &mut World, _: Entity) {
        if world.resource::<SyncState>().waiting.is_some() {
            return;
        }
        let request = match self {
            Self::Latest | Self::Older => {
                let before = if matches!(self, Self::Older) {
                    world
                        .resource::<SyncState>()
                        .overview
                        .as_ref()
                        .and_then(|overview| overview.history.last().map(|change| change.seq))
                } else {
                    None
                };
                world.resource_mut::<SyncState>().before = before;
                ClientMessage::SyncInspect {
                    id: REQUEST.into(),
                    before,
                }
            }
            Self::Retain(days) => {
                let max_entries = world
                    .resource::<SyncState>()
                    .overview
                    .as_ref()
                    .map_or(1000, |overview| overview.retention.max_entries);
                world.resource_mut::<SyncState>().before = None;
                ClientMessage::SyncHistoryPolicy {
                    id: REQUEST.into(),
                    retention: Retention {
                        seconds: days * 86400,
                        max_entries,
                    },
                }
            }
            Self::Clear => {
                world.resource_mut::<SyncState>().before = None;
                ClientMessage::SyncForgetHistory { id: REQUEST.into() }
            }
        };
        send(world, request);
    }
}

fn destination(instance: &Instance) -> String {
    match &instance.destination {
        Destination::Directory { path, .. } => format!("Files: {path}"),
        Destination::Organ { uid } => format!("Organ: {uid}"),
        Destination::Interface { .. } => "Interface".into(),
    }
}

fn outcome(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Applied => "Applied",
        Outcome::Delivered => "Delivered",
        Outcome::Refreshed => "Refreshed",
        Outcome::Pending => "Waiting",
        Outcome::Conflict => "Needs attention",
        Outcome::Failed => "Failed",
    }
}

fn direction(direction: Direction) -> &'static str {
    match direction {
        Direction::Incoming => "Incoming",
        Direction::Outgoing => "Outgoing",
        Direction::Both => "Both ways",
    }
}

fn render(world: &mut World) {
    let state = world.resource::<SyncState>();
    let revision = state.revision;
    let overview = state.overview.clone();
    let error = state.error.clone();
    let panels: Vec<_> = world
        .query::<(Entity, &SyncPanel)>()
        .iter(world)
        .filter(|(_, panel)| panel.revision != Some(revision))
        .map(|(entity, panel)| (entity, panel.root))
        .collect();
    for (panel, root) in panels {
        world.entity_mut(panel).despawn_children();
        world.get_mut::<SyncPanel>(panel).unwrap().revision = Some(revision);
        label(world, panel, "Sync", 22.0);
        if let Some(error) = &error {
            label(world, panel, error, 14.0);
        }
        let Some(overview) = &overview else {
            label(world, panel, "Loading sync activity…", 14.0);
            continue;
        };
        label(
            world,
            panel,
            &format!(
                "{} running · {} waiting to send · {} waiting to apply · {} need attention",
                overview.active.len(),
                overview.outgoing,
                overview.incoming,
                overview.held
            ),
            15.0,
        );
        label(
            world,
            panel,
            &format!(
                "History is kept for {} days, up to {} entries. Pending work is kept separately.",
                overview.retention.seconds / 86400,
                overview.retention.max_entries
            ),
            14.0,
        );
        if let Some(error) = &overview.history_error {
            label(
                world,
                panel,
                &format!("History could not be saved: {error}"),
                14.0,
            );
        }
        for (name, action) in [
            ("Latest activity", SyncAction::Latest),
            ("Older activity", SyncAction::Older),
            ("Keep 1 day", SyncAction::Retain(1)),
            ("Keep 7 days", SyncAction::Retain(7)),
            ("Keep 30 days", SyncAction::Retain(30)),
            ("Clear recent history", SyncAction::Clear),
        ] {
            super::action_button(world, panel, root, name, crate::actions![action]);
        }
        for queue in &overview.queues {
            label(
                world,
                panel,
                &format!(
                    "{} · {} incoming · {} outgoing",
                    destination(&queue.instance),
                    queue.incoming,
                    queue.outgoing
                ),
                14.0,
            );
        }
        for pending in &overview.pending {
            label(
                world,
                panel,
                &format!(
                    "{} · {} · {}{}{}",
                    direction(pending.direction),
                    destination(&pending.instance),
                    outcome(pending.outcome),
                    pending
                        .subject
                        .as_ref()
                        .map(|subject| format!(" · {subject}"))
                        .unwrap_or_default(),
                    pending
                        .message
                        .as_ref()
                        .map(|message| format!(" · {message}"))
                        .unwrap_or_default()
                ),
                14.0,
            );
        }
        let buffered: u64 = overview.queues.iter().map(|queue| queue.outgoing).sum();
        if (overview.pending.len() as u64)
            < overview.outgoing.saturating_sub(buffered) + overview.held
        {
            label(
                world,
                panel,
                "Only the first pending items are shown.",
                14.0,
            );
        }
        if overview.history.is_empty() {
            label(world, panel, "No recent sync activity on this page.", 14.0);
        }
        for change in &overview.history {
            let age = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(change.at.max(0) as u64);
            label(
                world,
                panel,
                &format!(
                    "{} · {} · {} · {} items · {}m ago{}",
                    direction(change.activity.direction),
                    destination(&change.activity.instance),
                    outcome(change.summary.outcome),
                    change.summary.count,
                    age / 60,
                    change
                        .summary
                        .message
                        .as_ref()
                        .map(|message| format!(" · {message}"))
                        .unwrap_or_default()
                ),
                14.0,
            );
            if !change.summary.subjects.is_empty() {
                label(world, panel, &change.summary.subjects.join(", "), 12.0);
            }
        }
    }
}

pub(crate) mod tests {
    use super::*;

    #[cfg_attr(test, test)]
    fn sync_panel_shows_history_limits_pending_work_and_failures() {
        let mut world = World::new();
        crate::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::theme::Typography>();
        world.init_resource::<SyncState>();
        let root = world.spawn_empty().id();
        panel(&mut world, root, root);
        world.resource_mut::<SyncState>().overview = Some(Overview {
            active: Vec::new(),
            queues: Vec::new(),
            history: Vec::new(),
            pending: Vec::new(),
            outgoing: 3,
            incoming: 0,
            held: 1,
            retention: Retention::default(),
            history_error: Some("Disk full".into()),
        });
        render(&mut world);
        let labels: Vec<_> = world
            .query::<&Text>()
            .iter(&world)
            .map(|text| text.0.clone())
            .collect();
        assert!(
            labels
                .iter()
                .any(|label| label.contains("3 waiting to send")
                    && label.contains("1 need attention"))
        );
        assert!(
            labels
                .iter()
                .any(|label| label.contains("7 days") && label.contains("1000 entries"))
        );
        assert!(labels.iter().any(|label| label.contains("Disk full")));
        assert!(labels.iter().any(|label| label == "Clear recent history"));
        world.init_resource::<Messages<CellMessage>>();
        world.write_message(CellMessage(ServerMessage::Error {
            id: REQUEST.into(),
            message: "Disconnected".into(),
            code: None,
        }));
        let mut schedule = Schedule::default();
        schedule.add_systems((receive, render).chain());
        schedule.run(&mut world);
        assert!(
            world
                .query::<&Text>()
                .iter(&world)
                .any(|text| text.0 == "Disconnected")
        );
        assert_eq!(
            world
                .resource::<SyncState>()
                .overview
                .as_ref()
                .unwrap()
                .outgoing,
            3
        );
    }

    crate::laboratory_cases! {
        sync_panel_shows_history_limits_pending_work_and_failures,
    }
}
