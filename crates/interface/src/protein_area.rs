mod assignees;
mod date_order;
pub(crate) mod filter;
pub(crate) mod grouping;
mod history;
mod model;
pub(crate) mod placement;
mod property_actions;
mod record_layout;
mod rows;
pub(crate) mod tests;
mod ui;

use crate::{
    area::InfluenceArea,
    cell_bridge::{CellBridge, CellMessage, ReceiveCell},
    protein_castle::ProteinCastle,
};
use bevy::prelude::*;
use cell::{ClientMessage, ServerMessage};
pub use grouping::{GroupAxis, Grouping};
pub use model::{Binding, Config, OverflowMode, Source, SpawnPlacement};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
pub(crate) use ui::controls;

pub(crate) fn attach_field_history(world: &mut World, entity: Entity) {
    history::attach_text(world, entity);
}

pub(crate) fn sync_field_history(world: &mut World, entity: Entity, value: &str) {
    history::synced_text(world, entity, value);
}

pub(crate) fn preview_record(world: &mut World, parent: Entity, config: &Config, data: &Value) {
    rows::content(
        world,
        parent,
        config,
        data,
        Some(RecordBinding {
            area: parent,
            uid: String::new(),
            source: Source::Local,
        }),
    );
}

pub(crate) fn calendar_feed(world: &World, owner: Entity) -> Option<(&[Value], String)> {
    let state = world.get_resource::<Runtime>()?.areas.get(&owner)?;
    Some((
        &state.data,
        format!(
            "{:?}:{}:{}",
            state.subscription, state.revision, state.status
        ),
    ))
}

pub(crate) fn calendar_status(world: &World, owner: Entity) -> &str {
    world
        .get_resource::<Runtime>()
        .and_then(|r| r.areas.get(&owner))
        .map_or("Stopped", |s| s.status.as_str())
}

pub(crate) fn thread_load_error(world: &World, owner: Entity) -> Option<&str> {
    let state = world.get_resource::<Runtime>()?.areas.get(&owner)?;
    state.thread_error.as_deref()
}

pub(crate) fn load_thread_messages(
    world: &mut World,
    binding: &RecordBinding,
    thread: &str,
    limit: usize,
) -> Result<(), String> {
    let state = world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&binding.area))
        .ok_or("Thread connection is closed")?;
    let config = state
        .applied
        .as_ref()
        .ok_or("Thread connection is closed")?;
    if config.source != binding.source
        || !state
            .data
            .iter()
            .filter(|row| row["uid"].as_str() == Some(&binding.uid))
            .flat_map(|row| row["threads"].as_array().into_iter().flatten())
            .any(|row| row["uid"].as_str() == Some(thread))
    {
        return Err("Thread is not attached to this Record".into());
    }
    if state.pending.len() >= 64 {
        return Err("Wait for pending changes".into());
    }
    let id = state
        .subscription
        .clone()
        .ok_or("Thread connection is closed")?;
    let mut protein = query(world, binding.area, config)?;
    let mut limits = state.thread_limits.clone();
    limits.insert(thread.into(), limit);
    let include = protein
        .include
        .threads
        .as_mut()
        .ok_or("Threads are not included")?;
    include.message_limits = limits.clone();
    let state = world
        .resource_mut::<Runtime>()
        .into_inner()
        .areas
        .get_mut(&binding.area)
        .unwrap();
    state.thread_limits = limits;
    state.thread_requests += 1;
    state.thread_error = None;
    state
        .pending
        .push_back(ClientMessage::Subscribe { id, protein });
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub(crate) fn save_date(world: &mut World, editor: Entity, date: &str) -> Result<(), String> {
    rows::pick_date(world, editor, date)
}

#[derive(Component, Clone, Debug)]
pub struct RecordBinding {
    pub area: Entity,
    pub uid: String,
    pub source: Source,
}

#[derive(Component)]
pub struct RemoteRecord;

#[derive(EntityEvent, Clone, Debug)]
pub struct RecordClicked {
    pub entity: Entity,
    pub sand: Entity,
    pub uid: String,
    pub source: Source,
}

#[derive(Component)]
pub(crate) struct QueryEditor(pub Entity);

struct Remote {
    outgoing: tokio::sync::mpsc::Sender<ClientMessage>,
    incoming: tokio::sync::mpsc::Receiver<ServerMessage>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Remote {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[derive(Default)]
struct State {
    thread_limits: std::collections::BTreeMap<String, usize>,
    thread_requests: usize,
    thread_error: Option<String>,
    revision: u64,
    applied: Option<Config>,
    subscription: Option<String>,
    remote: Option<Remote>,
    ready: bool,
    login: bool,
    retry_at: Option<std::time::Instant>,
    status: String,
    data: Vec<Value>,
    ordered_day: Option<chrono::NaiveDate>,
    order: std::sync::Arc<Vec<String>>,
    row_entities: HashMap<String, Entity>,
    groups: HashMap<(bool, String), Entity>,
    page: usize,
    dirty: bool,
    template_dirty: bool,
    navigation: Option<Entity>,
    actions: HashMap<String, Entity>,
    pending: VecDeque<ClientMessage>,
}

#[derive(Resource, Default)]
struct Runtime {
    next: u64,
    areas: HashMap<Entity, State>,
    outgoing: VecDeque<ClientMessage>,
}

pub struct ProteinAreaPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpdateProteinAreas;
impl Plugin for ProteinAreaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Runtime>()
            .add_message::<CellMessage>()
            .add_systems(
                Update,
                update
                    .in_set(UpdateProteinAreas)
                    .after(ReceiveCell)
                    .before(crate::physics::SimulateWorkspaces),
            )
            .add_systems(
                PostUpdate,
                (
                    ui::inputs,
                    history::update,
                    rows::commit_edits,
                    property_actions::commit_edits,
                    assignees::update,
                )
                    .chain()
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            )
            .add_systems(
                PostUpdate,
                rows::layout.after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

fn id(world: &mut World) -> String {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.next += 1;
    format!("protein-area-{}", runtime.next)
}

fn stop(world: &mut World, owner: Entity) {
    if let Some((target, changes)) = world.get::<filter::Subscription>(owner).map(|s| (s.0, s.1)) {
        if world.get_entity(target).is_ok() {
            if changes {
                world.entity_mut(target).remove::<filter::ChangeMatches>();
            } else {
                world.entity_mut(target).remove::<filter::Matches>();
            }
        }
        if crate::area_mutation::armed(world, target) {
            crate::area_mutation::disarm(
                world,
                target,
                "Property changes inactive after the Protein filter changed or stopped.",
            );
        }
    }
    let state = world.resource_mut::<Runtime>().areas.remove(&owner);
    if let Some(mut state) = state {
        if let Some(entity) = state.navigation.take() {
            let _ = world.despawn(entity);
        }
        if state.remote.is_none() {
            if let Some(id) = state.subscription.take() {
                world
                    .resource_mut::<Runtime>()
                    .outgoing
                    .push_back(ClientMessage::Unsubscribe { id });
            }
        }
        for entity in state.row_entities.into_values() {
            let _ = world.despawn(entity);
        }
        for entity in state.groups.into_values() {
            let _ = world.despawn(entity);
        }
    }
}

fn connect(world: &mut World, owner: Entity, config: &Config) -> Result<Option<Remote>, String> {
    let Source::Organ(organ) = &config.source else {
        return Ok(None);
    };
    if organ.trim().is_empty() {
        return Err("Choose an Organ".into());
    }
    let runtime = world
        .get_resource::<crate::app::CellHandle>()
        .ok_or("No Cell connection")?
        .0
        .clone();
    let wake = world
        .get_resource::<crate::wake::WakeSignal>()
        .cloned()
        .ok_or("No Interface wake signal")?;
    let handle = tokio::runtime::Handle::try_current().map_err(|_| "No live runtime")?;
    let organ = organ.clone();
    let (outgoing, requests) = tokio::sync::mpsc::channel(32);
    let (responses, incoming) = tokio::sync::mpsc::channel(32);
    let task = handle.spawn(async move {
        let run = async {
            let wire = runtime
                .wire
                .read()
                .await
                .clone()
                .ok_or("Networking is disabled")?;
            let connection =
                tokio::time::timeout(std::time::Duration::from_secs(30), wire.open_live(&organ))
                    .await
                    .map_err(|_| "Organ connection timed out")?
                    .map_err(|error| error.to_string())?;
            let signal = wake.clone();
            cell::live_client::drive(connection, requests, responses.clone(), move || {
                signal.ring()
            })
            .await
        }
        .await;
        let message = run.err().unwrap_or_else(|| "Connection closed".into());
        let _ = responses
            .send(ServerMessage::Error {
                id: "connection".into(),
                message,
                code: None,
            })
            .await;
        wake.ring();
    });
    let _ = owner;
    Ok(Some(Remote {
        outgoing,
        incoming,
        task,
    }))
}

fn start(world: &mut World, owner: Entity, config: Config) {
    stop(world, owner);
    let mut state = State {
        applied: Some(config.clone()),
        ..default()
    };
    if config.enabled {
        match query(world, owner, &config)
            .and_then(|query| connect(world, owner, &config).map(|remote| (query, remote)))
        {
            Ok((query, remote)) => {
                let id = id(world);
                state.subscription = Some(id.clone());
                state.ready = remote.is_none();
                state.remote = remote;
                state.status = "Connecting".into();
                state
                    .pending
                    .push_back(ClientMessage::Subscribe { id, protein: query });
            }
            Err(error) => {
                state.status = error;
                if matches!(config.source, Source::Organ(_)) {
                    state.retry_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
                    retry_wake(world);
                }
            }
        }
    } else {
        state.status = "Stopped".into();
    }
    world.resource_mut::<Runtime>().areas.insert(owner, state);
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    let message = message.into();
    let visible = world.get::<ChildOf>(owner).is_some_and(|parent| {
        world
            .get::<crate::edit_mode::EditMode>(parent.parent())
            .is_some_and(|mode| mode.enabled && mode.areas)
            && world
                .get::<crate::area_panel::AreaEditor>(parent.parent())
                .is_some_and(|editor| editor.selected == Some(owner))
    });
    if !visible {
        crate::notifications::report(world, "Protein Area", &message);
    }
    if let Some(state) = world.resource_mut::<Runtime>().areas.get_mut(&owner) {
        state.status = message;
    }
}

fn retry_wake(world: &World) {
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>().cloned() {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            wake.ring();
        });
    }
}

fn receive(world: &mut World, owner: Entity, message: ServerMessage) {
    let snapshot = matches!(&message, ServerMessage::Snapshot { .. });
    crate::work_timer::receive(world, &message);
    if let Some(Source::Organ(organ)) = world
        .resource::<Runtime>()
        .areas
        .get(&owner)
        .and_then(|state| state.applied.as_ref())
        .map(|config| config.source.clone())
    {
        crate::record_binding::receive(world, Source::Organ(organ), message.clone());
    }
    let mut runtime = world.resource_mut::<Runtime>();
    let Some(state) = runtime.areas.get_mut(&owner) else {
        return;
    };
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if state.subscription.as_ref() == Some(&id) =>
        {
            if snapshot {
                state.thread_requests = state.thread_requests.saturating_sub(1);
                state.thread_error = None;
            }
            state.revision = state.revision.wrapping_add(1);
            if rows.len() > 100_000
                || rows
                    .iter()
                    .any(|row| !row.is_object() || row["uid"].as_str().is_none())
            {
                state.status = "Invalid Record rows".into();
                state.ready = false;
                state.data.clear();
                state.dirty = true;
            } else {
                state.ready = true;
                state.order = std::sync::Arc::new(
                    rows.iter()
                        .filter_map(|row| row["uid"].as_str().map(str::to_string))
                        .collect(),
                );
                state.data = rows;
                date_order::sort(state);
                state.dirty = true;
                state.status = "Live".into();
            }
        }
        ServerMessage::SessionAuthenticated { .. } => {
            state.ready = true;
            state.login = false;
            state.status = "Loading".into();
        }
        ServerMessage::LiveHello {
            login_required: true,
        } => {
            state.login = true;
            state.status = "Login required".into();
        }
        ServerMessage::ActionOk {
            id,
            warnings,
            created,
            data,
            ..
        } => {
            if let Some(editor) = state.actions.remove(&id) {
                let message = if warnings.is_empty() {
                    "Saved".into()
                } else {
                    warnings.join("; ")
                };
                state.status = message;
                drop(runtime);
                history::finished(
                    world,
                    &id,
                    created,
                    data.as_ref()
                        .and_then(|data| data["changed"].as_bool())
                        .unwrap_or(true),
                    true,
                );
                rows::action_finished(world, editor, None);
            }
        }
        ServerMessage::Error { id, message, .. } => {
            if state.subscription.as_ref() == Some(&id) && state.thread_requests > 0 {
                state.thread_requests -= 1;
                state.thread_error = Some(message);
                return;
            }
            if id != "connection"
                && id != crate::cell_bridge::CONNECTION
                && state.subscription.as_ref() != Some(&id)
                && !state.actions.contains_key(&id)
            {
                return;
            }
            state.status = message.clone();
            if id == "connection" {
                state.retry_at =
                    Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
            }
            let alert = message.clone();
            if id == "connection"
                || id == crate::cell_bridge::CONNECTION
                || state.subscription.as_ref() == Some(&id)
            {
                state.ready = false;
                state.data.clear();
                state.dirty = true;
                state.pending.clear();
            }
            if let Some(editor) = state.actions.remove(&id) {
                drop(runtime);
                history::finished(world, &id, None, false, false);
                rows::action_finished(world, editor, Some(message));
            }
            crate::notifications::report(world, "Protein Area", &alert);
            if id == "connection" {
                retry_wake(world);
            }
        }
        _ => {}
    }
    mirror_editor(world, owner);
}

fn update(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    if crate::laboratory::active(world) {
        return;
    }
    filter::maintain(world);
    let editors: Vec<_> = world
        .query::<(&QueryEditor, &ProteinCastle)>()
        .iter(world)
        .map(|(link, castle)| (link.0, castle.draft.clone()))
        .collect();
    for (owner, draft) in editors {
        if let Some(mut config) = configuration(world, owner) {
            if config.draft != draft {
                config.draft = draft;
                config.enabled = false;
                set_configuration(world, owner, Some(config));
            }
        }
    }
    let mut configs: Vec<_> = world
        .query::<(Entity, &InfluenceArea)>()
        .iter(world)
        .filter_map(|(entity, area)| area.protein.clone().map(|config| (entity, config)))
        .collect();
    configs.extend(
        world
            .query::<(Entity, &filter::Subscription)>()
            .iter(world)
            .filter_map(|(entity, _)| configuration(world, entity).map(|config| (entity, config))),
    );
    for (entity, config) in &mut configs {
        let owner = world
            .get::<filter::Subscription>(*entity)
            .map_or(*entity, |s| s.0);
        if world
            .get::<InfluenceArea>(owner)
            .is_some_and(|area| !area.enabled)
        {
            config.enabled = false;
        }
    }
    let gone: Vec<_> = world
        .resource::<Runtime>()
        .areas
        .keys()
        .filter(|entity| !configs.iter().any(|(owner, _)| owner == *entity))
        .copied()
        .collect();
    for entity in gone {
        stop(world, entity);
        let editors: Vec<_> = world
            .query::<(Entity, &QueryEditor)>()
            .iter(world)
            .filter(|(_, editor)| editor.0 == entity)
            .map(|(editor, _)| editor)
            .collect();
        for editor in editors {
            world.despawn(editor);
        }
    }
    for (owner, config) in configs {
        let retry = world
            .resource::<Runtime>()
            .areas
            .get(&owner)
            .and_then(|state| state.retry_at)
            .is_some_and(|at| std::time::Instant::now() >= at);
        if retry && config.enabled {
            start(world, owner, config);
            continue;
        }
        let previous = world
            .resource::<Runtime>()
            .areas
            .get(&owner)
            .and_then(|state| state.applied.clone());
        if previous.as_ref() != Some(&config) {
            let query_same = previous.as_ref().is_some_and(|old| {
                old.enabled == config.enabled
                    && old.source == config.source
                    && query(world, owner, old)
                        .ok()
                        .and_then(|q| serde_json::to_value(q).ok())
                        == query(world, owner, &config)
                            .ok()
                            .and_then(|q| serde_json::to_value(q).ok())
            });
            if query_same {
                let state = world
                    .resource_mut::<Runtime>()
                    .into_inner()
                    .areas
                    .get_mut(&owner)
                    .unwrap();
                state.applied = Some(config);
                state.dirty = true;
                state.template_dirty = true;
            } else {
                start(world, owner, config);
            }
        }
    }
    let local: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    let owners: Vec<_> = world.resource::<Runtime>().areas.keys().copied().collect();
    for owner in owners {
        let mut messages = Vec::new();
        {
            let state = world
                .resource_mut::<Runtime>()
                .into_inner()
                .areas
                .get_mut(&owner)
                .unwrap();
            if let Some(remote) = state.remote.as_mut() {
                while let Ok(message) = remote.incoming.try_recv() {
                    messages.push(message);
                }
            } else {
                messages.extend(local.iter().cloned());
            }
        }
        for message in messages {
            receive(world, owner, message);
        }
        let mut state = world
            .resource_mut::<Runtime>()
            .areas
            .remove(&owner)
            .unwrap();
        if state.ready {
            while let Some(message) = state.pending.pop_front() {
                let sender = state
                    .remote
                    .as_ref()
                    .map(|remote| &remote.outgoing)
                    .or_else(|| {
                        world
                            .get_non_send::<CellBridge>()
                            .map(|bridge| &bridge.outgoing)
                    });
                let result = sender.map(|sender| sender.try_send(message.clone()));
                match result {
                    Some(Ok(())) => {}
                    Some(Err(tokio::sync::mpsc::error::TrySendError::Full(_))) => {
                        state.pending.push_front(message);
                        if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                            wake.ring();
                        }
                        break;
                    }
                    _ => {
                        state.ready = false;
                        state.status = "Connection closed".into();
                        state.data.clear();
                        state.dirty = true;
                        break;
                    }
                }
            }
        }
        date_order::sort(&mut state);
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        rows::reconcile(world, owner);
    }
    filter::publish(world);
    date_order::wake(world);
    while let Some(message) = world.resource_mut::<Runtime>().outgoing.pop_front() {
        let Some(bridge) = world.get_non_send::<CellBridge>() else {
            break;
        };
        if let Err(tokio::sync::mpsc::error::TrySendError::Full(message)) =
            bridge.outgoing.try_send(message)
        {
            world.resource_mut::<Runtime>().outgoing.push_front(message);
            break;
        }
    }
    ui::statuses(world);
}

pub fn execute(
    world: &mut World,
    binding: &RecordBinding,
    editor: Entity,
    action: engine::actions::Action,
) -> Result<(), String> {
    if crate::laboratory::suspended(world, editor) {
        return Err("Workspace is suspended".into());
    }
    let runtime = world.resource::<Runtime>();
    let state = runtime
        .areas
        .get(&binding.area)
        .ok_or("Protein Area is closed")?;
    if !state.ready
        || !state
            .applied
            .as_ref()
            .is_some_and(|config| config.source == binding.source)
        || !state
            .data
            .iter()
            .any(|row| row["uid"].as_str() == Some(&binding.uid))
    {
        return Err("Record is no longer available in this Area".into());
    }
    if state.pending.len() >= 64 {
        return Err("Wait for pending changes".into());
    }
    let target = match &action {
        engine::actions::Action::DeleteRecord { target } => {
            let attached = state
                .data
                .iter()
                .filter(|row| row["uid"].as_str() == Some(&binding.uid))
                .flat_map(|row| row["threads"].as_array().into_iter().flatten())
                .flat_map(|thread| thread["messages"].as_array().into_iter().flatten())
                .any(|message| message["uid"].as_str() == Some(target));
            if target != &binding.uid && !attached {
                return Err("Message is not attached to this Record".into());
            }
            &binding.uid
        }
        engine::actions::Action::CreateThread { target, .. } => target,
        engine::actions::Action::CreateMessage { thread, .. } => {
            let row = state
                .data
                .iter()
                .find(|row| row["uid"].as_str() == Some(&binding.uid))
                .unwrap();
            if !row["threads"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|item| item["uid"].as_str() == Some(thread))
            {
                return Err("Thread is not attached to this Record".into());
            }
            &binding.uid
        }
        engine::actions::Action::ChangeRecord { request } => &request.record_uid,
        engine::actions::Action::AssertRecord { subject, .. } => subject,
        engine::actions::Action::RetractAssertion { assertion } => {
            let row = state
                .data
                .iter()
                .find(|row| row["uid"].as_str() == Some(&binding.uid))
                .unwrap();
            let attached = row["assertions"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|value| value["uid"].as_str() == Some(assertion))
                || row["assignees"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|value| value["assertion"].as_str() == Some(assertion));
            if !attached {
                return Err("Assertion is not attached to this Record".into());
            }
            &binding.uid
        }
        engine::actions::Action::EditRecordText { target, .. }
        | engine::actions::Action::SetSlug { target, .. }
        | engine::actions::Action::SetQuantityExact { target, .. }
        | engine::actions::Action::SetExtension { target, .. } => target,
        _ => return Err("Unsupported bound Record Action".into()),
    };
    if target != &binding.uid {
        return Err("Action target differs from the bound Record".into());
    }
    let id = match &action {
        engine::actions::Action::ChangeRecord { request } => request.id.clone(),
        _ => id(world),
    };
    let durable = if let engine::actions::Action::ChangeRecord { request } = &action {
        if crate::record_binding::enabled(world) {
            crate::record_binding::submit(world, binding, request.clone())?;
            true
        } else {
            false
        }
    } else {
        false
    };
    history::capture(world, binding, editor, &action);
    let state = world
        .resource_mut::<Runtime>()
        .into_inner()
        .areas
        .get_mut(&binding.area)
        .unwrap();
    state.actions.insert(id.clone(), editor);
    if !durable {
        state.pending.push_back(ClientMessage::Act { id, action });
    }
    state.status = "Saving".into();
    if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub(crate) fn editor_sender(
    world: &World,
    binding: &RecordBinding,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    match &binding.source {
        Source::Local => world
            .get_non_send::<CellBridge>()
            .map(|bridge| bridge.outgoing.clone()),
        Source::Organ(_) => {
            let areas = &world.get_resource::<Runtime>()?.areas;
            areas
                .get(&binding.area)
                .into_iter()
                .chain(areas.values())
                .filter(|state| {
                    state.ready
                        && state
                            .applied
                            .as_ref()
                            .is_some_and(|config| config.source == binding.source)
                })
                .filter_map(|state| state.remote.as_ref())
                .find(|remote| !remote.outgoing.is_closed())
                .map(|remote| remote.outgoing.clone())
        }
    }
}

pub(crate) fn editor_changed(world: &mut World, editor: Entity) {
    let Some(owner) = world.get::<QueryEditor>(editor).map(|link| link.0) else {
        return;
    };
    let Some(draft) = world
        .get::<ProteinCastle>(editor)
        .map(|castle| castle.draft.clone())
    else {
        return;
    };
    if let Some(mut config) = configuration(world, owner) {
        config.draft = draft;
        config.enabled = false;
        set_configuration(world, owner, Some(config));
    }
}

pub(crate) fn run_editor(world: &mut World, editor: Entity) -> bool {
    let Some(owner) = world.get::<QueryEditor>(editor).map(|link| link.0) else {
        return false;
    };
    editor_changed(world, editor);
    if let Some(mut config) = configuration(world, owner) {
        config.enabled = true;
        set_configuration(world, owner, Some(config));
    }
    crate::protein_castle::status(world, editor, "Loading Area data");
    stop(world, owner);
    true
}

pub(crate) fn stop_editor(world: &mut World, editor: Entity) {
    let Some(owner) = world.get::<QueryEditor>(editor).map(|link| link.0) else {
        return;
    };
    if let Some(mut config) = configuration(world, owner) {
        config.enabled = false;
        set_configuration(world, owner, Some(config));
    }
}

fn mirror_editor(world: &mut World, owner: Entity) {
    let editors: Vec<_> = world
        .query::<(Entity, &QueryEditor)>()
        .iter(world)
        .filter(|(_, link)| link.0 == owner)
        .map(|(entity, _)| entity)
        .collect();
    for editor in editors {
        let Some(state) = world.resource::<Runtime>().areas.get(&owner) else {
            continue;
        };
        let rows = state.data.clone();
        let ready = state.ready;
        let status = state.status.clone();
        let query = state
            .applied
            .as_ref()
            .and_then(|config| query(world, owner, config).ok());
        if let Some(mut results) = world.get_mut::<crate::protein_castle::ProteinResults>(editor) {
            results.columns = rows
                .iter()
                .filter_map(Value::as_object)
                .flat_map(|row| row.keys().cloned())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            results.rows = rows;
            results.current = ready;
            results.query = query;
            results.revision += 1;
        }
        crate::protein_castle::status(world, editor, status);
    }
}

fn configuration(world: &World, entity: Entity) -> Option<Config> {
    if let Some(filter) = world.get::<filter::Subscription>(entity) {
        let area = world.get::<InfluenceArea>(filter.0)?;
        if filter.1 {
            area.change_filter.clone()
        } else {
            area.filter.clone()
        }
    } else {
        world.get::<InfluenceArea>(entity)?.protein.clone()
    }
}

fn set_configuration(world: &mut World, entity: Entity, config: Option<Config>) {
    let filter = world
        .get::<filter::Subscription>(entity)
        .map(|filter| (filter.0, filter.1));
    if let Some(mut area) = world.get_mut::<InfluenceArea>(filter.map_or(entity, |f| f.0)) {
        if let Some((_, changes)) = filter {
            if changes {
                area.change_filter = config;
            } else {
                area.filter = config;
            }
        } else {
            area.protein = config;
        }
    }
}

fn query(world: &World, entity: Entity, config: &Config) -> Result<protein::Protein, String> {
    if world.get::<filter::Subscription>(entity).is_some() {
        filter::query(config)
    } else {
        config.query()
    }
}

pub(crate) fn ordered_records(
    world: &mut World,
    owner: Entity,
) -> Option<(Source, std::sync::Arc<Vec<String>>)> {
    let area = world.get::<InfluenceArea>(owner)?;
    let (config, filter) = if let Some(config) = &area.filter {
        (config.clone(), true)
    } else {
        (area.protein.as_ref()?.clone(), false)
    };
    let entity = if filter {
        world
            .query::<(Entity, &filter::Subscription)>()
            .iter(world)
            .find(|(_, s)| s.0 == owner && !s.1)
            .map(|(e, _)| e)
    } else {
        Some(owner)
    };
    let ids = entity
        .and_then(|e| world.get_resource::<Runtime>()?.areas.get(&e))
        .filter(|state| config.enabled && state.ready && state.applied.as_ref() == Some(&config))
        .map(|state| state.order.clone())
        .unwrap_or_default();
    Some((config.source, ids))
}
