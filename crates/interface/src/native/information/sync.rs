use crate::native::{
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
    presented: bool,
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
            presented: false,
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
    rows: crate::native::retained::Children<Row>,
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
    let Some(wake) = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned()
    else {
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

pub(crate) fn panel(world: &mut World, root: Entity, parent: Entity) {
    world.spawn((
        SyncPanel {
            root,
            revision: None,
            rows: Default::default(),
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
    let open = world
        .query_filtered::<Entity, With<SyncPanel>>()
        .iter(world)
        .any(|entity| crate::native::output_views::presents_entity(world, entity));
    if let Some(wake) = world.get_resource::<SyncWake>() {
        wake.open.store(open, Ordering::Relaxed);
    }
    {
        let mut state = world.resource_mut::<SyncState>();
        if state.presented != open {
            if open {
                state.next = Instant::now();
            }
            state.presented = open;
        }
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
    for message in messages.iter().map(|message| message.as_ref()) {
        let mut state = world.resource_mut::<SyncState>();
        match message {
            ServerMessage::SyncStatus { id, overview } if id == REQUEST => {
                state.waiting = None;
                if state.overview.as_ref() != Some(overview) || state.error.is_some() {
                    state.overview = Some(overview.clone());
                    state.error = None;
                    state.revision += 1;
                }
            }
            ServerMessage::Error { id, message, .. } if id == REQUEST => {
                state.waiting = None;
                state.error = Some(message.clone());
                state.revision += 1;
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncAction {
    Latest,
    Older,
    Retain(u64),
    RecordHistory,
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
                        enabled: world
                            .resource::<SyncState>()
                            .overview
                            .as_ref()
                            .is_some_and(|overview| overview.retention.enabled),
                        seconds: days * 86400,
                        max_entries,
                    },
                }
            }
            Self::RecordHistory => {
                let Some(overview) = world.resource::<SyncState>().overview.as_ref() else {
                    return;
                };
                let mut retention = overview.retention;
                retention.enabled = !retention.enabled;
                world.resource_mut::<SyncState>().before = None;
                ClientMessage::SyncHistoryPolicy {
                    id: REQUEST.into(),
                    retention,
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
        Destination::Plugin { provider, .. } => format!("Plugin: {provider}"),
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

#[derive(Clone, PartialEq, Eq, Hash)]
enum Row {
    Heading,
    Error,
    Loading,
    Summary,
    Limits,
    HistoryToggle,
    HistoryError,
    Action(usize),
    Queue(usize),
    Pending(String),
    More,
    Empty,
    History(i64),
    Subjects(i64),
}

#[derive(Clone)]
enum RowContent {
    Label(String, f32),
    Button(String, SyncAction),
}

fn rows(overview: Option<&Overview>, error: Option<&str>, now: u64) -> Vec<(Row, RowContent)> {
    let text = |value: &str, size| RowContent::Label(value.into(), size);
    let mut rows = vec![(Row::Heading, text("Sync activity", 22.0))];
    if let Some(error) = error {
        rows.push((Row::Error, text(error, 14.0)));
    }
    let Some(overview) = overview else {
        rows.push((Row::Loading, text("Loading sync activity…", 14.0)));
        return rows;
    };
    rows.push((
        Row::Summary,
        RowContent::Label(
            format!(
                "{} running · {} waiting to send · {} waiting to apply · {} need attention",
                overview.active.len(),
                overview.outgoing,
                overview.incoming,
                overview.held,
            ),
            15.0,
        ),
    ));
    rows.push((
        Row::Limits,
        RowContent::Label(
            format!(
                "History limits: {} days, up to {} entries. Pending work is kept separately.",
                overview.retention.seconds / 86400,
                overview.retention.max_entries,
            ),
            14.0,
        ),
    ));
    rows.push((
        Row::HistoryToggle,
        RowContent::Button(
            if overview.retention.enabled {
                "Save sync history: on"
            } else {
                "Save sync history: off"
            }
            .into(),
            SyncAction::RecordHistory,
        ),
    ));
    if let Some(error) = &overview.history_error {
        rows.push((
            Row::HistoryError,
            RowContent::Label(format!("History could not be saved: {error}"), 14.0),
        ));
    }
    for (index, (name, action)) in [
        ("Latest activity", SyncAction::Latest),
        ("Older activity", SyncAction::Older),
        ("Keep 1 day", SyncAction::Retain(1)),
        ("Keep 7 days", SyncAction::Retain(7)),
        ("Keep 30 days", SyncAction::Retain(30)),
        ("Clear recent history", SyncAction::Clear),
    ]
    .into_iter()
    .enumerate()
    {
        rows.push((Row::Action(index), RowContent::Button(name.into(), action)));
    }
    for (index, queue) in overview.queues.iter().enumerate() {
        rows.push((
            Row::Queue(index),
            RowContent::Label(
                format!(
                    "{} · {} incoming · {} outgoing",
                    destination(&queue.instance),
                    queue.incoming,
                    queue.outgoing,
                ),
                14.0,
            ),
        ));
    }
    for pending in &overview.pending {
        rows.push((
            Row::Pending(pending.id.clone()),
            RowContent::Label(
                format!(
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
                        .unwrap_or_default(),
                ),
                14.0,
            ),
        ));
    }
    let buffered: u64 = overview.queues.iter().map(|queue| queue.outgoing).sum();
    if (overview.pending.len() as u64) < overview.outgoing.saturating_sub(buffered) + overview.held
    {
        rows.push((
            Row::More,
            text("Only the first pending items are shown.", 14.0),
        ));
    }
    if overview.history.is_empty() {
        rows.push((
            Row::Empty,
            text("No recent sync activity on this page.", 14.0),
        ));
    }
    for change in &overview.history {
        let age = now.saturating_sub(change.at.max(0) as u64);
        rows.push((
            Row::History(change.seq),
            RowContent::Label(
                format!(
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
                        .unwrap_or_default(),
                ),
                14.0,
            ),
        ));
        if !change.summary.subjects.is_empty() {
            rows.push((
                Row::Subjects(change.seq),
                RowContent::Label(change.summary.subjects.join(", "), 12.0),
            ));
        }
    }
    rows
}

fn update_row(world: &mut World, entity: Entity, content: &RowContent) {
    match content {
        RowContent::Label(value, _) => {
            if let Some(mut text) = world.get_mut::<Text>(entity)
                && text.0 != *value
            {
                text.0.clone_from(value);
            }
        }
        RowContent::Button(value, _) => {
            let label = world.get::<Children>(entity).and_then(|children| {
                children
                    .iter()
                    .find(|entity| world.get::<Text>(*entity).is_some())
            });
            if let Some(label) = label
                && world
                    .get::<Text>(label)
                    .is_some_and(|text| text.0 != *value)
            {
                world.get_mut::<Text>(label).unwrap().0.clone_from(value);
                if let Some(mut node) = world.get_mut::<bevy::a11y::AccessibilityNode>(entity) {
                    node.set_label(value.as_str());
                }
            }
        }
    }
}

fn render(world: &mut World) {
    let revision = world.resource::<SyncState>().revision;
    let panels: Vec<_> = world
        .query::<(Entity, &SyncPanel)>()
        .iter(world)
        .filter(|(entity, panel)| {
            panel.revision != Some(revision)
                && crate::native::output_views::presents_entity(world, *entity)
        })
        .map(|(entity, panel)| (entity, panel.root))
        .collect();
    if panels.is_empty() {
        return;
    }
    let state = world.resource::<SyncState>();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let rows = rows(state.overview.as_ref(), state.error.as_deref(), now);
    for (panel, root) in panels {
        let mut retained = std::mem::take(&mut world.get_mut::<SyncPanel>(panel).unwrap().rows);
        retained.reconcile(
            world,
            panel,
            rows.iter().cloned(),
            |world, parent, content| match content {
                RowContent::Label(value, size) => label(world, parent, value, *size),
                RowContent::Button(value, action) => super::action_button(
                    world,
                    parent,
                    root,
                    value,
                    crate::native::actions![*action],
                ),
            },
            update_row,
        );
        let mut state = world.get_mut::<SyncPanel>(panel).unwrap();
        state.rows = retained;
        state.revision = Some(revision);
    }
}

pub(crate) mod tests {
    use super::*;

    pub(crate) fn present(world: &mut World, root: Entity, enabled: bool) {
        world.init_resource::<SyncState>();
        if world.query::<&SyncPanel>().iter(world).next().is_none() {
            panel(world, root, root);
        }
        let mut state = world.resource_mut::<SyncState>();
        state.overview = Some(Overview {
            active: Vec::new(),
            queues: Vec::new(),
            history: Vec::new(),
            pending: Vec::new(),
            outgoing: 3,
            incoming: 0,
            held: 1,
            retention: Retention {
                enabled,
                ..Default::default()
            },
            history_error: None,
        });
        state.error = None;
        state.revision += 1;
        drop(state);
        render(world);
    }

    pub(crate) fn connection_error(world: &World) -> Option<&str> {
        world.resource::<SyncState>().error.as_deref()
    }

    #[cfg_attr(test, test)]
    fn sync_panel_shows_history_limits_pending_work_and_failures() {
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
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
        assert!(labels.iter().any(|label| label == "Save sync history: off"));
        world.init_resource::<Messages<CellMessage>>();
        world.write_message(CellMessage::new(ServerMessage::Error {
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

    crate::native::laboratory_cases! {
        sync_panel_shows_history_limits_pending_work_and_failures,
        sync_history_retains_unchanged_rows_and_controls_and_defers_hidden_panels,
    }

    #[cfg_attr(test, test)]
    fn sync_history_retains_unchanged_rows_and_controls_and_defers_hidden_panels() {
        use crate::native::workspace::{WorkspaceMember, Workspaces};
        use bevy::input_focus::{FocusCause, InputFocus};
        let mut world = World::new();
        crate::native::laboratory::isolate(&mut world);
        world.init_resource::<Assets<Font>>();
        world.init_resource::<crate::native::theme::Typography>();
        world.init_resource::<SyncState>();
        world.init_resource::<InputFocus>();
        let root = world.spawn(Workspaces::default()).id();
        let owner = world
            .spawn((Node::default(), WorkspaceMember(2), ChildOf(root)))
            .id();
        panel(&mut world, root, owner);
        let panel = world
            .query_filtered::<Entity, With<SyncPanel>>()
            .single(&world)
            .unwrap();
        let overview = Overview {
            active: Vec::new(),
            queues: Vec::new(),
            pending: Vec::new(),
            history: (0..160)
                .map(|seq| nucleus::sync::Change {
                    seq,
                    at: 0,
                    activity: nucleus::sync::Activity {
                        instance: Instance::interface("fixture", "history"),
                        direction: Direction::Incoming,
                        update: nucleus::sync::Update::Full,
                    },
                    summary: nucleus::sync::Summary {
                        outcome: Outcome::Applied,
                        count: 1,
                        subjects: vec![format!("{seq}: {}", "Stable subject ".repeat(50))],
                        message: None,
                    },
                })
                .collect(),
            outgoing: 0,
            incoming: 0,
            held: 0,
            retention: Retention::default(),
            history_error: None,
        };
        world.resource_mut::<SyncState>().overview = Some(overview);
        render(&mut world);
        poll(&mut world);
        assert!(world.get::<Children>(panel).is_none());
        assert!(world.resource::<SyncState>().error.is_none());
        assert!(world.resource::<SyncState>().waiting.is_none());
        world.get_mut::<Workspaces>(root).unwrap().active = 2;
        render(&mut world);
        let old: Vec<_> = world.get::<Children>(panel).unwrap().iter().collect();
        let text = old
            .iter()
            .copied()
            .find(|entity| {
                world
                    .get::<Text>(*entity)
                    .is_some_and(|text| text.0.starts_with("100: Stable subject"))
            })
            .unwrap();
        let stale = old
            .iter()
            .copied()
            .find(|entity| {
                world
                    .get::<Text>(*entity)
                    .is_some_and(|text| text.0.starts_with("0: Stable subject"))
            })
            .unwrap();
        let button = old
            .iter()
            .copied()
            .find(|entity| {
                world.get::<Children>(*entity).is_some_and(|children| {
                    children.iter().any(|child| {
                        world
                            .get::<Text>(child)
                            .is_some_and(|text| text.0 == "Latest activity")
                    })
                })
            })
            .unwrap();
        let tick = world.entity(text).get_ref::<Text>().unwrap().last_changed();
        world
            .resource_mut::<InputFocus>()
            .set(button, FocusCause::Navigated);
        world.increment_change_tick();
        {
            let mut state = world.resource_mut::<SyncState>();
            let overview = state.overview.as_mut().unwrap();
            overview.history.remove(0);
            overview.history.reverse();
            overview.outgoing = 3;
            overview.retention.enabled = true;
            state.revision += 1;
        }
        render(&mut world);
        assert_eq!(world.resource::<InputFocus>().get(), Some(button));
        assert_eq!(
            world.entity(text).get_ref::<Text>().unwrap().last_changed(),
            tick
        );
        assert!(world.get_entity(stale).is_err());
        let changed: Vec<_> = world.get::<Children>(panel).unwrap().iter().collect();
        assert!(changed.iter().all(|entity| {
            old.contains(entity)
                || world
                    .get::<Text>(*entity)
                    .is_some_and(|text| text.0 == "Only the first pending items are shown.")
        }));
        let subjects: Vec<_> = changed
            .iter()
            .filter_map(|entity| world.get::<Text>(*entity))
            .filter(|text| text.0.contains(": Stable subject"))
            .map(|text| text.0.split(':').next().unwrap().parse::<i64>().unwrap())
            .collect();
        assert_eq!(subjects, (1..160).rev().collect::<Vec<_>>());
        assert!(
            world
                .query::<&Text>()
                .iter(&world)
                .any(|text| text.0 == "Save sync history: on")
        );
        world.get_mut::<Node>(owner).unwrap().display = Display::None;
        let seen = world.get::<SyncPanel>(panel).unwrap().revision;
        world
            .resource_mut::<SyncState>()
            .overview
            .as_mut()
            .unwrap()
            .history
            .clear();
        world.resource_mut::<SyncState>().revision += 1;
        render(&mut world);
        assert_eq!(world.get::<SyncPanel>(panel).unwrap().revision, seen);
        assert!(world.get_entity(text).is_ok());
        world.get_mut::<Node>(owner).unwrap().display = Display::Flex;
        render(&mut world);
        assert!(world.get_entity(text).is_err());
        assert_eq!(world.resource::<InputFocus>().get(), Some(button));
        assert!(
            world
                .query::<&Text>()
                .iter(&world)
                .any(|text| text.0 == "No recent sync activity on this page.")
        );
    }
}
