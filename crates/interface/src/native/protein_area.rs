#[derive(bevy::prelude::SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct MeasureRecords;

mod assignees;
mod availability;
mod date_order;
pub(crate) mod filter;
pub(crate) mod grouping;
mod history;
mod local;
mod login;
mod model;
pub(crate) mod placement;
pub(crate) mod presentation;
mod property_actions;
pub(crate) mod record_layout;
mod rows;
mod sessions;
pub(crate) mod tests;
mod ui;

use crate::native::{
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
pub(crate) use ui::Command as ProteinAction;
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

pub(crate) fn feed_ready(world: &World, owner: Entity) -> bool {
    world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&owner))
        .is_some_and(|state| state.ready && state.subscription.is_some())
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
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub(crate) fn save_date(world: &mut World, editor: Entity, date: &str) -> Result<(), String> {
    rows::pick_date(world, editor, date)
}

pub(crate) fn attach_scalar(
    world: &mut World,
    editor: Entity,
    binding: RecordBinding,
    property: &str,
    data: &Value,
) {
    rows::attach_scalar(world, editor, binding, property, data);
}

#[derive(Component, Clone, Debug)]
pub struct RecordBinding {
    pub area: Entity,
    pub uid: String,
    pub source: Source,
}

pub(crate) fn room_frame(world: &World, entity: Entity) -> Option<Entity> {
    let owner = rows::room_frame(world, entity)?;
    if owner == entity {
        return None;
    }
    if world
        .get::<crate::native::topology::rooms::Room>(owner)
        .is_some()
        || world
            .get::<crate::native::topology::rooms::Member>(owner)
            .is_some()
    {
        Some(owner)
    } else {
        None
    }
}

pub(crate) fn room_fixed(world: &World, entity: Entity) -> bool {
    room_frame(world, entity).is_some() && rows::room_fixed(world, entity)
}

pub(crate) fn room_transform(
    world: &mut World,
    owner: Entity,
    before: &nucleus::canvas::Pose,
    after: &nucleus::canvas::Pose,
) {
    rows::room_transform(world, owner, before, after);
}

pub(crate) fn room_default(world: &mut World, entity: Entity) {
    rows::room_default(world, entity);
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

pub(crate) struct Remote {
    pub(crate) outgoing: tokio::sync::mpsc::Sender<ClientMessage>,
    pub(crate) incoming: tokio::sync::mpsc::Receiver<ServerMessage>,
    pub(crate) task: tokio::task::JoinHandle<()>,
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
    remote: Option<String>,
    ready: bool,
    refreshing: bool,
    single_record: Option<String>,
    login: bool,
    login_pending: bool,
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
    sessions: HashMap<String, sessions::Session>,
    outgoing: VecDeque<ClientMessage>,
    local_sender: Option<tokio::sync::mpsc::Sender<ClientMessage>>,
}

#[derive(Component)]
struct LaboratoryFeed;

pub(crate) fn laboratory_attach(world: &mut World, owner: Entity, data: Value) {
    laboratory_detach(world, owner);
    world.entity_mut(owner).insert(LaboratoryFeed);
    let query = serde_json::from_value(
        serde_json::json!({"source":"record","where":[{"uid_eq":data["uid"]}],
        "fields":["uid","head","threads"],"include":{"threads":{"messages_limit":50}},"limit":1}),
    )
    .unwrap();
    let subscription = id(world);
    world.resource_mut::<Runtime>().areas.insert(
        owner,
        State {
            applied: Some(Config::records()),
            ready: true,
            data: vec![data],
            subscription: Some(subscription.clone()),
            pending: VecDeque::from([ClientMessage::Subscribe {
                id: subscription,
                protein: query,
            }]),
            ..default()
        },
    );
}

pub(crate) fn authoring_draft(world: &World, entity: Entity) -> bool {
    rows::authoring_draft(world, entity)
}

pub(crate) fn uses_content_height(world: &World, entity: Entity) -> bool {
    world.get::<rows::Row>(entity).is_some()
}

pub(crate) fn row_area(world: &World, entity: Entity) -> Option<Entity> {
    world.get::<rows::Row>(entity).map(|row| row.area)
}

pub(crate) fn preferred_height(world: &World, entity: Entity) -> Option<f32> {
    world.get::<rows::Row>(entity)?;
    let axis = world
        .get::<crate::native::layout::LayoutBox>(entity)?
        .rules
        .axes[1];
    (axis.sizing == crate::native::layout::Sizing::Fit).then_some(axis.size)
}

pub(crate) fn laboratory_detach(world: &mut World, owner: Entity) {
    world.entity_mut(owner).remove::<LaboratoryFeed>();
    if let Some(previous) = world
        .get_resource_mut::<Runtime>()
        .and_then(|mut runtime| runtime.areas.remove(&owner))
        && let Some(id) = previous.subscription
        && let Some(bridge) = world.get_non_send::<CellBridge>()
    {
        let _ = bridge.outgoing.try_send(ClientMessage::Unsubscribe { id });
    }
}

fn laboratory_pump(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>,
) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<LaboratoryFeed>>()
        .iter(world)
        .collect();
    for owner in owners {
        for message in &messages {
            receive(world, owner, message.clone());
        }
        let Some(mut state) = world.resource_mut::<Runtime>().areas.remove(&owner) else {
            continue;
        };
        if state.ready {
            while let Some(message) = state.pending.pop_front() {
                let sender = world
                    .get_non_send::<CellBridge>()
                    .map(|bridge| bridge.outgoing.clone());
                match sender.map(|sender| sender.try_send(message.clone())) {
                    Some(Ok(())) => {}
                    Some(Err(tokio::sync::mpsc::error::TrySendError::Full(_))) => {
                        state.pending.push_front(message);
                        break;
                    }
                    _ => {
                        state.ready = false;
                        state.status = "Connection closed".into();
                        break;
                    }
                }
            }
        }
        let data = state.dirty.then(|| state.data.first().cloned()).flatten();
        state.dirty = false;
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        if let Some(data) = data {
            crate::native::thread_castle::refresh(world, owner, &data);
        }
    }
}

pub struct ProteinAreaPlugin;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpdateProteinAreas;
impl Plugin for ProteinAreaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, laboratory_pump.after(UpdateProteinAreas));
        if !app.is_plugin_added::<crate::native::assertion_editor::AssertionEditorPlugin>() {
            app.add_plugins(crate::native::assertion_editor::AssertionEditorPlugin);
        }
        if !app.is_plugin_added::<crate::native::arrow_sand::ArrowSandPlugin>() {
            app.add_plugins(crate::native::arrow_sand::ArrowSandPlugin);
        }
        app.init_resource::<Runtime>()
            .add_message::<CellMessage>()
            .add_systems(
                PostUpdate,
                login::protect_passwords.before(bevy::text::EditableTextSystems),
            )
            .add_systems(
                PostUpdate,
                login::masks.after(crate::native::actions::ApplyActions),
            )
            .add_systems(
                Update,
                update
                    .in_set(UpdateProteinAreas)
                    .after(ReceiveCell)
                    .before(crate::native::physics::SimulateWorkspaces),
            )
            .add_systems(
                PostUpdate,
                (
                    ui::inputs,
                    crate::native::relation_castle::styles::inputs,
                    presentation::inputs,
                    history::update,
                    rows::commit_edits,
                    property_actions::commit_edits,
                    assignees::update,
                )
                    .chain()
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::native::actions::ApplyActions),
            )
            .add_systems(
                PostUpdate,
                (rows::save_indicators, property_actions::save_indicators)
                    .after(rows::commit_edits)
                    .after(property_actions::commit_edits)
                    .after(crate::native::actions::ApplyActions)
                    .after(crate::native::record_binding::SyncBindings)
                    .before(crate::native::sand::StyleSaveState),
            )
            .add_systems(
                PostUpdate,
                (rows::layout, crate::native::schedule_editor::maintain)
                    .chain()
                    .in_set(MeasureRecords)
                    .after(bevy::ui::UiSystems::PostLayout),
            );
    }
}

fn id(world: &mut World) -> String {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.next += 1;
    format!("protein-area-{}", runtime.next)
}

fn stop(world: &mut World, owner: Entity) {
    login::clear_passwords(world, owner);
    crate::native::record_presentation::remember(world, owner);
    if let Some((target, changes)) = world.get::<filter::Subscription>(owner).map(|s| (s.0, s.1)) {
        if world.get_entity(target).is_ok() {
            if changes {
                world.entity_mut(target).remove::<filter::ChangeMatches>();
            } else {
                world.entity_mut(target).remove::<filter::Matches>();
            }
        }
        if crate::native::area_mutation::armed(world, target) {
            crate::native::area_mutation::disarm(
                world,
                target,
                "Property changes inactive after the Protein filter changed or stopped.",
            );
        }
    }
    let state = world.resource_mut::<Runtime>().areas.remove(&owner);
    if let Some(mut state) = state {
        if let Some(id) = state.subscription.take() {
            if let Some(organ) = &state.remote {
                sessions::unsubscribe(world, organ, id);
            } else {
                world
                    .resource_mut::<Runtime>()
                    .outgoing
                    .push_back(ClientMessage::Unsubscribe { id });
            }
        }
        for entity in state
            .navigation
            .into_iter()
            .chain(state.row_entities.into_values())
            .chain(state.groups.into_values())
        {
            if let Ok(entity) = world.get_entity_mut(entity) {
                entity.despawn();
            }
        }
    }
}

pub(crate) fn connect_organ(world: &World, organ: &str) -> Result<Remote, String> {
    if let Some(remote) = crate::native::practice_cells::remote(world, organ) {
        return remote;
    }
    if organ.trim().is_empty() {
        return Err("Choose an Organ".into());
    }
    let runtime = world
        .get_resource::<crate::native::app::CellHandle>()
        .ok_or("No Cell connection")?
        .0
        .clone();
    let wake = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned()
        .ok_or("No Interface wake signal")?;
    let handle = tokio::runtime::Handle::try_current().map_err(|_| "No live runtime")?;
    let organ = organ.to_owned();
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
    Ok(Remote {
        outgoing,
        incoming,
        task,
    })
}

fn start(world: &mut World, owner: Entity, config: Config) {
    stop(world, owner);
    crate::native::full_record::window::restore(world, owner, &config);
    let single_record = availability::target(&config);
    let mut state = State {
        applied: Some(config.clone()),
        dirty: single_record.is_some(),
        single_record,
        ..default()
    };
    if config.enabled && config.creating() {
        state.status = "Creating window Record".into();
    } else if config.enabled {
        subscribe(world, owner, &config, &mut state);
    } else {
        state.status = "Stopped".into();
    }
    world.resource_mut::<Runtime>().areas.insert(owner, state);
}

fn subscribe(world: &mut World, owner: Entity, config: &Config, state: &mut State) {
    match query(world, owner, config) {
        Ok(query) => {
            let id = id(world);
            state.subscription = Some(id.clone());
            state.refreshing = state.single_record.is_some();
            state.ready = matches!(config.source, Source::Local);
            state.status = "Connecting".into();
            if let Source::Organ(organ) = &config.source {
                sessions::attach(world, organ, state);
            }
            state
                .pending
                .push_back(ClientMessage::Subscribe { id, protein: query });
        }
        Err(error) => {
            state.status = error;
        }
    }
}

fn refresh_relation_styles(
    world: &mut World,
    owner: Entity,
    previous: Option<&Config>,
    config: &Config,
) -> bool {
    let Some(previous) = previous.filter(|previous| previous.relations && config.relations) else {
        return false;
    };
    let mut comparable = previous.clone();
    comparable
        .relation_styles
        .clone_from(&config.relation_styles);
    if comparable != *config {
        return false;
    }
    let Ok(protein) = query(world, owner, config) else {
        return false;
    };
    let subscription = id(world);
    let state = world
        .resource_mut::<Runtime>()
        .into_inner()
        .areas
        .get_mut(&owner)
        .unwrap();
    state.applied = Some(config.clone());
    state.dirty = true;
    if let Some(id) = state.subscription.replace(subscription.clone()) {
        state.pending.push_back(ClientMessage::Unsubscribe { id });
    }
    state.pending.push_back(ClientMessage::Subscribe {
        id: subscription,
        protein,
    });
    true
}

fn status(world: &mut World, owner: Entity, message: impl Into<String>) {
    let message = message.into();
    let visible = world.get::<ChildOf>(owner).is_some_and(|parent| {
        world
            .get::<crate::native::edit_mode::EditMode>(parent.parent())
            .is_some_and(|mode| mode.enabled && mode.areas)
            && world
                .get::<crate::native::area_panel::AreaEditor>(parent.parent())
                .is_some_and(|editor| editor.selected == Some(owner))
    });
    if !visible {
        crate::native::notifications::report(world, "Protein Area", &message);
    }
    if let Some(state) = world.resource_mut::<Runtime>().areas.get_mut(&owner) {
        state.status = message;
    }
}

fn retry_wake(world: &World) {
    if let Some(wake) = world
        .get_resource::<crate::native::wake::WakeSignal>()
        .cloned()
    {
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            wake.ring();
        });
    }
}

pub(crate) fn auxiliary_sender(
    world: &World,
    source: &Source,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    if let Source::Organ(source) = source
        && crate::native::laboratory::reserved_source(source)
    {
        return None;
    }
    match source {
        Source::Local => world
            .get_non_send::<CellBridge>()
            .map(|bridge| bridge.outgoing.clone()),
        Source::Organ(organ) => sessions::sender(world, organ),
    }
}

pub(crate) fn ensure_auxiliary(world: &mut World, source: &Source) {
    if let Source::Organ(source) = source
        && crate::native::laboratory::reserved_source(source)
    {
        return;
    }
    if let Source::Organ(organ) = source {
        sessions::ensure_auxiliary(world, organ);
    }
}

pub(crate) fn release_practice_source(world: &mut World, organ: &str) {
    if !crate::native::practice_cells::owns_source(world, &Source::Organ(organ.into())) {
        return;
    }
    let owners: Vec<_> = world
        .get_resource::<Runtime>()
        .map(|runtime| {
            runtime
                .areas
                .iter()
                .filter(|(_, state)| state.remote.as_deref() == Some(organ))
                .map(|(owner, _)| *owner)
                .collect()
        })
        .unwrap_or_default();
    for owner in owners {
        stop(world, owner);
    }
    if let Some(mut runtime) = world.get_resource_mut::<Runtime>() {
        runtime.sessions.remove(organ);
    }
}

pub(crate) fn logout_organ(world: &mut World, organ: &str) {
    sessions::logout(world, organ);
}
pub(crate) fn reconnect_auxiliary(world: &mut World, organ: &str) {
    sessions::ensure_auxiliary(world, organ);
    sessions::reconnect(world, organ);
}

pub(crate) fn auxiliary_login(
    world: &mut World,
    organ: &str,
    username: String,
    password: String,
) -> Result<(), String> {
    sessions::login(world, organ, username, password)
}

pub(crate) fn row_entities(world: &World, owner: Entity) -> Vec<Entity> {
    world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&owner))
        .map(|state| state.row_entities.values().copied().collect())
        .unwrap_or_default()
}

pub(crate) fn selection_pending(world: &mut World, owner: Entity) -> bool {
    world
        .get_resource::<Runtime>()
        .and_then(|runtime| runtime.areas.get(&owner))
        .is_some_and(|state| !state.actions.is_empty())
        || property_actions::selection_pending(world, owner)
}

pub(crate) fn auxiliary_subscribed(world: &mut World, source: &Source, message: &ClientMessage) {
    if let Source::Organ(organ) = source {
        sessions::subscribed(world, organ, message);
    }
}

pub(crate) fn auxiliary_unsubscribe(world: &mut World, organ: &str, id: String) {
    sessions::unsubscribe(world, organ, id);
}

fn observe(world: &mut World, source: &Source, message: &ServerMessage) {
    if crate::native::practice_cells::owns_source(world, source)
        && matches!(
            message,
            ServerMessage::ActionOk { .. }
                | ServerMessage::Error { .. }
                | ServerMessage::Snapshot { .. }
                | ServerMessage::Update { .. }
                | ServerMessage::Fiote { .. }
                | ServerMessage::Speech { .. }
        )
    {
        if let (
            Source::Organ(source),
            ServerMessage::ActionOk {
                created: Some(uid), ..
            },
        ) = (source, message)
            && nucleus::valid_uid(uid, "r")
        {
            world
                .resource_mut::<crate::native::practice_cells::PracticeCells>()
                .records
                .entry(source.clone())
                .or_default()
                .insert(uid.clone());
        }
        world.write_message(CellMessage(message.clone()));
    }
    if matches!(source, Source::Organ(_)) {
        crate::native::workspace_sync::receive(world, source, message);
    }
    if matches!(source, Source::Organ(_)) {
        crate::native::time_castle::receive(world, source, message);
    }
    if matches!(source, Source::Organ(_)) {
        crate::native::record_extensions::receive(world, source, message);
        crate::native::shader_effects::receive(world, source, message);
    }
    if matches!(source, Source::Organ(_)) {
        crate::native::description::receive_live(world, source, message);
    }
    #[cfg(feature = "native-media")]
    crate::native::communication::calls::receive(world, message);
    crate::native::work_timer::receive(world, message);
    crate::native::thread_castle::transcript::receive_message(world, message);
    crate::native::thread_castle::mentions::receive_message(world, message);
    if matches!(source, Source::Organ(_)) {
        crate::native::assertion_editor::receive(world, message);
        crate::native::record_binding::receive(world, source.clone(), message.clone());
    }
}

fn receive(world: &mut World, owner: Entity, message: ServerMessage) {
    let snapshot = matches!(&message, ServerMessage::Snapshot { .. });
    let mut runtime = world.resource_mut::<Runtime>();
    let Some(state) = runtime.areas.get_mut(&owner) else {
        return;
    };
    match message {
        ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows }
            if state.subscription.as_ref() == Some(&id) && !state.login =>
        {
            state.refreshing = false;
            if snapshot {
                state.thread_requests = state.thread_requests.saturating_sub(1);
                state.thread_error = None;
            }
            state.revision = state.revision.wrapping_add(1);
            if rows.len() > 100_000
                || rows
                    .iter()
                    .any(|row| !row.is_object() || row["uid"].as_str().is_none())
                || state
                    .applied
                    .as_ref()
                    .and_then(availability::target)
                    .is_some_and(|uid| {
                        rows.iter()
                            .any(|row| row["uid"].as_str() != Some(uid.as_str()))
                    })
            {
                state.status = "Invalid Record rows".into();
                state.ready = false;
                state.data.clear();
                state.order = Default::default();
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
            state.ready = state.ready || state.remote.is_some() || state.data.is_empty();
            state.login = false;
            state.login_pending = false;
            state.refreshing = state.data.is_empty() && state.subscription.is_some();
            state.status = "Loading".into();
        }
        ServerMessage::LiveHello {
            login_required: true,
        } => {
            state.ready = false;
            state.login = true;
            state.login_pending = false;
            state.data.clear();
            state.refreshing = false;
            state.order = Default::default();
            state.dirty = true;
            state
                .pending
                .retain(|message| matches!(message, ClientMessage::Subscribe { .. }));
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
                crate::native::thread_castle::created(world, editor, created.as_deref());
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
        ServerMessage::Error { id, message, code } => {
            if state.subscription.as_ref() == Some(&id) && state.thread_requests > 0 {
                state.thread_requests -= 1;
                state.thread_error = Some(message);
                return;
            }
            if id != "connection"
                && id != crate::native::cell_bridge::CONNECTION
                && state.subscription.as_ref() != Some(&id)
                && !state.actions.contains_key(&id)
            {
                return;
            }
            state.status = message.clone();
            state.refreshing = false;
            if id == crate::native::cell_bridge::CONNECTION
                && code.as_deref() == Some("connection_closed")
                && local::owns(state)
            {
                let actions = local::interrupt(state);
                drop(runtime);
                local::failed(world, actions, &message);
                mirror_editor(world, owner);
                return;
            }
            if id == "connection" {
                state.login = false;
                state.login_pending = false;
            }
            let alert = message.clone();
            if id == "connection"
                || id == crate::native::cell_bridge::CONNECTION
                || state.subscription.as_ref() == Some(&id)
            {
                state.ready = false;
                state.data.clear();
                state.order = Default::default();
                state.dirty = true;
                state.pending.clear();
            }
            if let Some(editor) = state.actions.remove(&id) {
                drop(runtime);
                history::finished(world, &id, None, false, false);
                rows::action_finished(world, editor, Some(message));
            }
            if id != "connection" {
                crate::native::notifications::report(world, "Protein Area", &alert);
            }
        }
        _ => {}
    }
    mirror_editor(world, owner);
}

fn update(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    if !crate::native::laboratory::normal(world) {
        return;
    }
    filter::maintain(world);
    let editors: Vec<_> = world
        .query::<(&QueryEditor, &ProteinCastle)>()
        .iter(world)
        .map(|(link, castle)| (link.0, castle.draft.clone()))
        .collect();
    for (owner, draft) in editors {
        if crate::native::laboratory::blocked(world, owner) {
            continue;
        }
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
    configs.retain(|(owner, _)| !crate::native::laboratory::blocked(world, *owner));
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
        .filter(|entity| {
            world.get::<LaboratoryFeed>(**entity).is_none()
                && !configs.iter().any(|(owner, _)| owner == *entity)
        })
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
        crate::native::full_record::window::restore(world, owner, &config);
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
            if query_same && previous.as_ref().is_some_and(Config::creating) && !config.creating() {
                let mut state = world
                    .resource_mut::<Runtime>()
                    .areas
                    .remove(&owner)
                    .unwrap();
                state.applied = Some(config.clone());
                state.dirty = true;
                subscribe(world, owner, &config, &mut state);
                world.resource_mut::<Runtime>().areas.insert(owner, state);
            } else if query_same
                && previous.as_ref().is_some_and(Config::creating) != config.creating()
            {
                start(world, owner, config);
            } else if query_same {
                let state = world
                    .resource_mut::<Runtime>()
                    .into_inner()
                    .areas
                    .get_mut(&owner)
                    .unwrap();
                state.template_dirty = previous
                    .as_ref()
                    .is_none_or(|old| !old.same_template(&config));
                state.applied = Some(config);
                state.dirty = true;
            } else if !refresh_relation_styles(world, owner, previous.as_ref(), &config) {
                start(world, owner, config);
            }
        }
    }
    let local: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in &local {
        observe(world, &Source::Local, message);
    }
    sessions::update(world);
    let owners: Vec<_> = world
        .resource::<Runtime>()
        .areas
        .keys()
        .copied()
        .filter(|owner| world.get::<LaboratoryFeed>(*owner).is_none())
        .collect();
    for owner in &owners {
        let owner = *owner;
        if world.resource::<Runtime>().areas[&owner]
            .applied
            .as_ref()
            .is_some_and(|config| matches!(config.source, Source::Local))
        {
            for message in &local {
                receive(world, owner, message.clone());
            }
        }
    }
    local::synchronize(world);
    for owner in &owners {
        let owner = *owner;
        let mut state = world
            .resource_mut::<Runtime>()
            .areas
            .remove(&owner)
            .unwrap();
        let mut interrupted = Vec::new();
        while local::sendable(&state) {
            if let Some(message) = state.pending.pop_front() {
                let sender = match state.remote.as_deref() {
                    Some(organ) => sessions::sender(world, organ),
                    None => world
                        .get_non_send::<CellBridge>()
                        .map(|bridge| bridge.outgoing.clone()),
                };
                let result = sender.map(|sender| sender.try_send(message.clone()));
                match result {
                    Some(Ok(())) => {
                        if let Some(organ) = &state.remote {
                            sessions::subscribed(world, organ, &message);
                        }
                    }
                    Some(Err(tokio::sync::mpsc::error::TrySendError::Full(_))) => {
                        state.pending.push_front(message);
                        if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>()
                        {
                            wake.ring();
                        }
                        break;
                    }
                    _ => {
                        state.ready = false;
                        state.login = false;
                        state.login_pending = false;
                        state.status = "Connection closed".into();
                        if !local::owns(&state) {
                            state.data.clear();
                            state.order = Default::default();
                            state.pending.clear();
                            state.dirty = true;
                            if let Some(organ) = &state.remote {
                                sessions::disconnected(world, organ);
                            }
                        } else {
                            interrupted = local::interrupt(&mut state);
                        }
                        break;
                    }
                }
            } else {
                break;
            }
        }
        date_order::sort(&mut state);
        world.resource_mut::<Runtime>().areas.insert(owner, state);
        local::failed(world, interrupted, "Connection closed");
    }
    for owner in owners {
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
    login::sync(world);
    ui::statuses(world);
}

pub fn execute(
    world: &mut World,
    binding: &RecordBinding,
    editor: Entity,
    action: engine::actions::Action,
) -> Result<(), String> {
    let result = execute_action(world, binding, editor, action);
    crate::native::save_feedback::set_failed(world, editor, result.is_err());
    result
}

fn execute_action(
    world: &mut World,
    binding: &RecordBinding,
    editor: Entity,
    action: engine::actions::Action,
) -> Result<(), String> {
    if crate::native::laboratory::blocked(world, editor) {
        return Err("Workspace is suspended".into());
    }
    let runtime = world.resource::<Runtime>();
    let state = runtime
        .areas
        .get(&binding.area)
        .ok_or("Protein Area is closed")?;
    if !state.ready && !state.data.is_empty() {
        return Err(
            "The Cell connection is unavailable. Your draft is kept; refresh before saving.".into(),
        );
    }
    if !state.ready
        || !state
            .applied
            .as_ref()
            .is_some_and(|config| config.source == binding.source && !config.creating())
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
        engine::actions::Action::DeleteRecord { target }
        | engine::actions::Action::SetExtension {
            target,
            namespace: _,
            ..
        } if matches!(&action, engine::actions::Action::DeleteRecord { .. })
            || matches!(&action, engine::actions::Action::SetExtension { namespace, .. } if namespace == "lince.message-content") =>
        {
            let attached = state
                .data
                .iter()
                .filter(|row| row["uid"].as_str() == Some(&binding.uid))
                .flat_map(|row| row["threads"].as_array().into_iter().flatten())
                .any(|thread| {
                    thread["uid"].as_str() == Some(target)
                        || thread["messages"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .any(|message| {
                                message["uid"].as_str() == Some(target)
                                    || message["tool_call"]["thread"].as_str() == Some(target)
                            })
                });
            if target != &binding.uid && !attached {
                return Err("Thread or message is not attached to this Record".into());
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
        if crate::native::record_binding::enabled(world)
            && !matches!(
                request.mutation,
                engine::record_change::Mutation::NumberAssertion { .. }
            )
        {
            crate::native::record_binding::submit(world, binding, request.clone())?;
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
    if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
        wake.ring();
    }
    Ok(())
}

pub(crate) fn editor_sender(
    world: &World,
    binding: &RecordBinding,
) -> Option<tokio::sync::mpsc::Sender<ClientMessage>> {
    if binding.area != Entity::PLACEHOLDER
        && crate::native::laboratory::blocked(world, binding.area)
    {
        return None;
    }
    match &binding.source {
        Source::Local => {
            if world
                .get_resource::<Runtime>()
                .and_then(|runtime| runtime.areas.get(&binding.area))
                .is_some_and(|state| {
                    !state.ready
                        || !state.applied.as_ref().is_some_and(|config| {
                            config.source == binding.source && !config.creating()
                        })
                        || !state
                            .data
                            .iter()
                            .any(|row| row["uid"].as_str() == Some(&binding.uid))
                })
            {
                return None;
            }
            world
                .get_non_send::<CellBridge>()
                .filter(|bridge| !bridge.outgoing.is_closed() && !bridge.incoming.is_closed())
                .map(|bridge| bridge.outgoing.clone())
        }
        Source::Organ(organ) => {
            let areas = &world.get_resource::<Runtime>()?.areas;
            let active = areas
                .get(&binding.area)
                .into_iter()
                .chain(areas.values())
                .any(|state| {
                    state.ready
                        && state
                            .applied
                            .as_ref()
                            .is_some_and(|config| config.source == binding.source)
                });
            active.then(|| sessions::sender(world, organ)).flatten()
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
    crate::native::protein_castle::status(world, editor, "Loading Area data");
    stop(world, owner);
    true
}

pub(crate) fn query_editor_source(world: &World, editor: Entity) -> Option<Source> {
    let owner = world.get::<QueryEditor>(editor)?.0;
    configuration(world, owner).map(|config| config.source)
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
        if let Some(mut results) =
            world.get_mut::<crate::native::protein_castle::ProteinResults>(editor)
        {
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
        crate::native::protein_castle::status(world, editor, status);
    }
}

pub(crate) fn configuration(world: &World, entity: Entity) -> Option<Config> {
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

pub(crate) fn set_configuration(world: &mut World, entity: Entity, config: Option<Config>) {
    let filter = world
        .get::<filter::Subscription>(entity)
        .map(|filter| (filter.0, filter.1));
    let owner = filter.map_or(entity, |filter| filter.0);
    if let Some(source) = world.get::<crate::native::practice_cells::PracticeSource>(owner)
        && config
            .as_ref()
            .is_some_and(|config| config.source != Source::Organ(source.0.clone()))
    {
        crate::native::notifications::report(
            world,
            "Instinct",
            "This view uses the isolated practice Cell. Close practice before choosing personal data.",
        );
        return;
    }
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
        let mut query = config.query()?;
        if world
            .get::<crate::native::assertion_castle::AssertionFeed>(entity)
            .is_some()
        {
            let fields = query.fields.get_or_insert_default();
            for field in ["head", "assertions"] {
                if !fields.iter().any(|existing| existing == field) {
                    fields.push(field.into());
                }
            }
        }
        if world
            .get::<crate::native::shader_castle::ShaderFeed>(entity)
            .is_some()
        {
            query.limit = Some(1);
        }
        Ok(query)
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
