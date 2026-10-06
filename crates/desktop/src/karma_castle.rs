use lince_interface::karma as model;
pub(crate) mod commands_ui;
mod history_ui;
mod persistence;
mod preview_ui;
mod schedules_ui;
pub(crate) mod tests;
mod ui;

use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use model::{Draft, Rule};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::{
    actions::Action,
    cell_bridge::{CellMessage, ReceiveCell},
    workspace::WorkspaceMember,
};
pub(crate) use persistence::{SavedKarmaCastle, snapshot};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct KarmaCastle {
    pub draft: Option<Draft>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub edits: Vec<Draft>,
    #[serde(default)]
    pub schedule: Option<model::schedules::Draft>,
    #[serde(default)]
    pub preview: preview_ui::Form,
}

pub(crate) use ui::Command as RuleAction;

pub(crate) fn saved_rule(
    world: &World,
    owner: Entity,
    slug: &str,
) -> Option<(String, i64, String)> {
    world
        .get::<View>(owner)?
        .rules
        .iter()
        .find(|rule| rule.slug == slug)
        .map(|rule| (rule.uid.clone(), rule.revision, rule.state.clone()))
}

pub(crate) fn preview(world: &mut World, owner: Entity) {
    preview_ui::run_action(world, owner);
}

pub(crate) fn previewed(world: &World, owner: Entity) -> bool {
    preview_ui::confirmed(world, owner)
}

pub(crate) fn preview_needs_refresh(world: &World, owner: Entity) -> bool {
    preview_ui::needs_refresh(world, owner)
}

pub(crate) fn ready(world: &World, owner: Entity) -> bool {
    world
        .get::<View>(owner)
        .is_some_and(|view| view.loaded.iter().all(|loaded| *loaded))
}

pub(crate) fn preview_form(inputs: Vec<engine::karma_preview::Input>) -> preview_ui::Form {
    preview_ui::Form::with_inputs(inputs)
}

pub(crate) fn history(world: &mut World, owner: Entity, uid: &str) {
    history_ui::inspect(world, owner, uid);
}

#[derive(Component)]
struct View {
    execution: Entity,
    execution_checked: Option<std::time::Instant>,
    controls: Entity,
    tools: Entity,
    selected: HashSet<String>,
    pausing: Vec<(String, i64, bool)>,
    pause_active: bool,
    editing: Option<(String, usize)>,
    deleting: Vec<String>,
    deleting_pending: Option<String>,
    saving: bool,
    form: Entity,
    list: Entity,
    status: Entity,
    rules: Vec<Rule>,
    records: Vec<Value>,
    frequencies: Vec<Value>,
    transfers: Vec<Value>,
    acting_person: Option<String>,
    record_lookup: HashMap<String, usize>,
    frequency_lookup: HashMap<String, usize>,
    pending: Option<String>,
    submitted: Option<Draft>,
    ready: bool,
    loaded: [bool; 4],
}

#[derive(Resource, Default)]
struct Requests {
    senders: HashMap<String, tokio::sync::mpsc::Sender<ClientMessage>>,
    executions: HashMap<String, Entity>,
    subscriptions: HashMap<String, (Entity, usize)>,
    readings: HashMap<String, Entity>,
}

#[derive(Component)]
struct FocusRule(String);

pub(crate) fn open_rule(world: &mut World, source: Entity, uid: &str) {
    let mut root = source;
    while world.get::<crate::workspace::Workspaces>(root).is_none() {
        let Some(parent) = world.get::<ChildOf>(root) else {
            return;
        };
        root = parent.parent();
    }
    let workspace = world
        .get::<WorkspaceMember>(source)
        .map_or(1, |member| member.0);
    let position = world
        .get::<crate::canvas::CanvasItem>(source)
        .map_or(DVec2::ZERO, |item| item.position + DVec2::new(40.0, 40.0));
    let owner = spawn(world, root, workspace, position, KarmaCastle::default());
    if let Some(source) = crate::practice_cells::source(world, source) {
        world
            .entity_mut(owner)
            .insert(crate::practice_cells::PracticeSource(source));
        crate::instinct::practice::track_custom(world, root, &[owner]);
    }
    world.entity_mut(owner).insert(FocusRule(uid.into()));
}

pub struct KarmaCastlePlugin;

impl Plugin for KarmaCastlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>()
            .add_message::<CellMessage>()
            .add_observer(ui::hover_on)
            .add_observer(ui::hover_off)
            .add_systems(
                Update,
                (
                    receive.after(ReceiveCell),
                    maintain,
                    ui::tick,
                    ui::hover_actions,
                    schedules_ui::maintain,
                    preview_ui::maintain,
                )
                    .chain(),
            )
            .add_systems(PostUpdate, ui::keys.before(bevy::text::EditableTextSystems))
            .add_systems(
                PostUpdate,
                ui::fit_columns.after(bevy::ui::UiSystems::Layout),
            )
            .add_systems(
                PostUpdate,
                ui::inputs
                    .after(bevy::text::EditableTextSystems)
                    .before(crate::actions::ApplyActions),
            );
    }
}

pub fn spawn(
    world: &mut World,
    root: Entity,
    workspace: u64,
    position: DVec2,
    castle: KarmaCastle,
) -> Entity {
    world.init_resource::<Requests>();
    let owner = world
        .spawn((
            crate::castle::Castle,
            crate::sand::Square,
            crate::sand::InBox(root),
            ChildOf(root),
            WorkspaceMember(workspace),
            crate::sand_store::SandCredits(crate::credits::ATTRIBUTIONS),
            crate::canvas::CanvasItem {
                position,
                size: Vec2::new(980.0, 620.0),
            },
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12)),
                row_gap: px(8),
                overflow: Overflow::clip(),
                ..default()
            },
            crate::token_style::background(crate::tokens::Token::Surface),
            castle,
        ))
        .id();
    let header = ui::row(world, owner);
    crate::edit_mode::label(world, header, "Karma", 22.0);
    world.get_mut::<Node>(header).unwrap().align_items = AlignItems::Center;
    let controls = ui::row(world, header);
    world.get_mut::<Node>(controls).unwrap().width = Val::Auto;
    let tools = ui::row(world, header);
    {
        let mut node = world.get_mut::<Node>(tools).unwrap();
        node.width = Val::Auto;
        node.flex_grow = 1.0;
        node.justify_content = JustifyContent::End;
    }
    ui::search(world, header, owner);
    let execution = crate::edit_mode::label(world, owner, "Checking Karma execution…", 13.0);
    let scroll = world
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
                ..default()
            },
            ScrollPosition::default(),
            ChildOf(owner),
        ))
        .id();
    crate::scroll_sand::attach(world, scroll);
    world.get_mut::<Node>(scroll).unwrap().overflow = Overflow::scroll();
    ui::headings(world, scroll);
    let form = ui::stack(world, scroll);
    let list = ui::stack(world, scroll);
    let status = crate::edit_mode::label(world, owner, "", 12.0);
    world.get_mut::<Node>(status).unwrap().display = Display::None;
    world.entity_mut(owner).insert(View {
        execution,
        execution_checked: None,
        controls,
        tools,
        selected: HashSet::new(),
        pausing: Vec::new(),
        pause_active: false,
        editing: None,
        deleting: Vec::new(),
        deleting_pending: None,
        saving: false,
        form,
        list,
        status,
        rules: Vec::new(),
        records: Vec::new(),
        frequencies: Vec::new(),
        transfers: Vec::new(),
        acting_person: None,
        record_lookup: HashMap::new(),
        frequency_lookup: HashMap::new(),
        pending: None,
        submitted: None,
        ready: false,
        loaded: [false; 4],
    });
    ui::render_form(world, owner);
    schedules_ui::spawn(world, owner, scroll);
    preview_ui::spawn(world, owner, scroll);
    history_ui::spawn(world, owner, scroll);
    commands_ui::spawn(world, owner, scroll);
    ui::render_tools(world, owner);
    owner
}

pub(crate) fn store_entry(world: &mut World, root: Entity, parent: Entity) {
    crate::sand_store::castle_entry(
        world,
        root,
        parent,
        "Karma Castle",
        "Condition, threshold, consequence. Reuse fields and watch live readings.",
        ui::Command::Create,
        |world, root| spawn(world, root, 1, DVec2::ZERO, KarmaCastle::default()),
    );
}

fn status(world: &mut World, owner: Entity, text: impl Into<String>) {
    if let Some(view) = world.get::<View>(owner) {
        let entity = view.status;
        let text = text.into();
        world.get_mut::<Node>(entity).unwrap().display = if text.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        world.get_mut::<Text>(entity).unwrap().0 = text;
    }
}

fn send(world: &World, owner: Entity, message: ClientMessage) -> Result<(), String> {
    if crate::laboratory::active(world) {
        return Err("Saving is unavailable in the Laboratory".into());
    }
    crate::practice_cells::send(world, owner, message)
}

fn save(world: &mut World, owner: Entity) {
    if world
        .get::<View>(owner)
        .is_none_or(|view| view.pending.is_some())
    {
        return;
    }
    ui::capture(world, owner);
    let castle = world.get::<KarmaCastle>(owner).unwrap();
    let Some(draft) = castle
        .draft
        .clone()
        .or_else(|| castle.edits.first().cloned())
    else {
        world.get_mut::<View>(owner).unwrap().saving = false;
        return;
    };
    if (!draft.slug.trim().is_empty() && !nucleus::valid_slug(draft.slug.trim())) || !draft.valid()
    {
        world.get_mut::<View>(owner).unwrap().saving = false;
        status(
            world,
            owner,
            "Use an optional lowercase slug with letters, numbers, hyphens or dots",
        );
        return;
    }
    let request_id = nucleus::new_uid("karma-edit");
    let action = engine::actions::Action::SaveKarmaRule {
        identity: Some(nucleus::karma::rule_field::RuleIdentity {
            name: draft.name.clone(),
            slug: draft.slug.clone(),
        }),
        rule: draft.rule.clone(),
        expected_revision: draft.revision,
        fields: draft.fields.clone().map(|field| field.input()),
        request_id: request_id.clone(),
    };
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: request_id.clone(),
            action,
        },
    ) {
        Ok(()) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending = Some(request_id);
            view.submitted = Some(draft);
            view.saving = true;
            status(world, owner, "Saving…");
            ui::render_form(world, owner);
            ui::render_list(world, owner);
        }
        Err(error) => {
            world.get_mut::<View>(owner).unwrap().saving = false;
            status(world, owner, error);
        }
    }
}

fn delete_next(world: &mut World, owner: Entity) {
    let Some(uid) = world.get::<View>(owner).unwrap().deleting.first().cloned() else {
        return;
    };
    let id = nucleus::new_uid("karma-delete");
    match send(
        world,
        owner,
        ClientMessage::Act {
            id: id.clone(),
            action: engine::actions::Action::DeleteRecurrence {
                recurrence: uid.clone(),
            },
        },
    ) {
        Ok(()) => {
            let mut view = world.get_mut::<View>(owner).unwrap();
            view.pending = Some(id);
            view.deleting_pending = Some(uid);
            ui::render_controls(world, owner);
        }
        Err(error) => status(world, owner, error),
    }
}

fn query(index: usize) -> protein::Protein {
    let source = ["karma_rule", "record", "frequency", "transfer"][index];
    let mut query: protein::Protein =
        serde_json::from_value(serde_json::json!({"source": source})).unwrap();
    if index == 1 {
        query.include.numeric_extensions = true;
        query.fields = Some(
            ["uid", "slug", "head", "quantity", "numeric_extensions"]
                .map(str::to_owned)
                .into(),
        );
    }
    if index == 3 {
        query.fields = Some(
            ["uid", "slug", "head", "parties", "promises", "karma_state"]
                .map(str::to_owned)
                .into(),
        );
    }
    query
}

fn lookup(rows: &[Value]) -> HashMap<String, usize> {
    rows.iter()
        .enumerate()
        .flat_map(|(index, row)| {
            ["uid", "slug"]
                .into_iter()
                .filter_map(move |key| row[key].as_str().map(|value| (value.to_owned(), index)))
        })
        .collect()
}

fn maintain(world: &mut World) {
    let stale: Vec<_> = world
        .resource::<Requests>()
        .subscriptions
        .iter()
        .filter(|(_, (owner, _))| world.get::<KarmaCastle>(*owner).is_none())
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        let sender = world.resource::<Requests>().senders.get(&id).cloned();
        if sender.is_none_or(|sender| {
            sender.is_closed()
                || sender
                    .try_send(ClientMessage::Unsubscribe { id: id.clone() })
                    .is_ok()
        }) {
            world.resource_mut::<Requests>().subscriptions.remove(&id);
            world.resource_mut::<Requests>().senders.remove(&id);
        }
    }
    let owners: Vec<_> = world
        .query_filtered::<Entity, With<KarmaCastle>>()
        .iter(world)
        .collect();
    world
        .resource_mut::<Requests>()
        .executions
        .retain(|_, owner| owners.contains(owner));
    for owner in owners {
        let due = world
            .get::<View>(owner)
            .unwrap()
            .execution_checked
            .is_none_or(|checked| checked.elapsed() >= std::time::Duration::from_secs(5));
        if due
            && !world
                .resource::<Requests>()
                .executions
                .values()
                .any(|value| *value == owner)
        {
            let id = nucleus::new_uid("karma-execution");
            if send(
                world,
                owner,
                ClientMessage::Act {
                    id: id.clone(),
                    action: engine::actions::Action::RosterStatus,
                },
            )
            .is_ok()
            {
                world
                    .resource_mut::<Requests>()
                    .executions
                    .insert(id, owner);
                world.get_mut::<View>(owner).unwrap().execution_checked =
                    Some(std::time::Instant::now());
            }
        }
        for index in 0..4 {
            let id = format!("karma-castle-{}-{index}", owner.to_bits());
            if world.resource::<Requests>().subscriptions.contains_key(&id) {
                continue;
            }
            match send(
                world,
                owner,
                ClientMessage::Subscribe {
                    id: id.clone(),
                    protein: query(index),
                },
            ) {
                Ok(()) => {
                    if let Some(sender) = crate::practice_cells::sender(world, owner) {
                        world
                            .resource_mut::<Requests>()
                            .senders
                            .insert(id.clone(), sender);
                    }
                    world
                        .resource_mut::<Requests>()
                        .subscriptions
                        .insert(id, (owner, index));
                }
                Err(error) => status(world, owner, error),
            }
        }
    }
}

fn receive(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let messages: Vec<_> = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    for message in messages {
        let execution_id = match &message {
            ServerMessage::ActionOk { id, .. } | ServerMessage::Error { id, .. } => Some(id),
            _ => None,
        };
        if let Some(owner) =
            execution_id.and_then(|id| world.resource_mut::<Requests>().executions.remove(id))
        {
            if let Some(view) = world.get::<View>(owner) {
                let label = view.execution;
                let text = match &message {
                    ServerMessage::ActionOk {
                        data: Some(data), ..
                    } if data["karma"]["executing"] == true => {
                        "Karma is running on this Cell.".to_owned()
                    }
                    ServerMessage::ActionOk {
                        data: Some(data), ..
                    } => format!(
                        "Karma is not running: {}. Check My devices and Karma authority.",
                        data["karma"]["reason"]
                            .as_str()
                            .unwrap_or("Execution is unavailable")
                    ),
                    ServerMessage::Error { message, .. } => {
                        format!("Could not check Karma execution: {message}")
                    }
                    _ => "Could not check Karma execution.".to_owned(),
                };
                world.get_mut::<Text>(label).unwrap().0 = text;
            }
            continue;
        }
        if schedules_ui::receive(world, &message) {
            continue;
        }
        if preview_ui::receive(world, &message) {
            continue;
        }
        if commands_ui::receive(world, &message) {
            continue;
        }
        if history_ui::receive(world, &message) {
            continue;
        }
        match message {
            ServerMessage::Snapshot { id, rows } | ServerMessage::Update { id, rows } => {
                let Some((owner, index)) =
                    world.resource::<Requests>().subscriptions.get(&id).copied()
                else {
                    continue;
                };
                let Some(mut view) = world.get_mut::<View>(owner) else {
                    continue;
                };
                view.loaded[index] = true;
                match index {
                    0 => {
                        match rows
                            .into_iter()
                            .map(serde_json::from_value)
                            .collect::<Result<Vec<Rule>, _>>()
                        {
                            Ok(rules) => {
                                if view.ready && view.rules == rules {
                                    continue;
                                }
                                view.ready = true;
                                view.rules = rules;
                                ui::capture(world, owner);
                                ui::render_list(world, owner);
                            }
                            Err(error) => {
                                status(world, owner, format!("Could not read rules: {error}"));
                                continue;
                            }
                        }
                        if world.get::<View>(owner).unwrap().pending.is_none() {
                            status(world, owner, "");
                        }
                    }
                    1 => {
                        if view.records == rows {
                            continue;
                        }
                        view.record_lookup = lookup(&rows);
                        view.records = rows;
                    }
                    2 => {
                        if view.frequencies == rows {
                            continue;
                        }
                        view.frequency_lookup = lookup(&rows);
                        view.frequencies = rows;
                    }
                    _ => {
                        let acting = rows
                            .iter()
                            .find(|row| row["kind"] == "transfer_context")
                            .and_then(|row| row["acting_person"].as_str())
                            .map(str::to_owned);
                        let transfers: Vec<_> = rows
                            .into_iter()
                            .filter(|row| row["kind"] != "transfer_context")
                            .collect();
                        if view.transfers == transfers && view.acting_person == acting {
                            continue;
                        }
                        view.transfers = transfers;
                        view.acting_person = acting;
                    }
                }
                schedules_ui::dirty(world, owner);
                preview_ui::dirty(world, owner);
                ui::refresh_links(world, owner);
                if index == 0
                    && let Some(uid) = world.get::<FocusRule>(owner).map(|focus| focus.0.clone())
                    && world
                        .get::<View>(owner)
                        .unwrap()
                        .rules
                        .iter()
                        .any(|rule| rule.uid == uid)
                {
                    world.entity_mut(owner).remove::<FocusRule>();
                    ui::Command::EditCell(uid, 0).apply(world, owner);
                }
            }
            ServerMessage::ActionOk { id, data, .. } => {
                let reading = world.resource_mut::<Requests>().readings.remove(&id);
                if let Some(entity) = reading {
                    ui::reading_reply(
                        world,
                        entity,
                        data.as_ref()
                            .and_then(|data| data["value"].as_str())
                            .unwrap_or("No reading")
                            .into(),
                    );
                    continue;
                }
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(_, view)| view.pending.as_ref() == Some(&id))
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    ui::capture(world, owner);
                    world.get_mut::<View>(owner).unwrap().pending = None;
                    let submitted = world.get_mut::<View>(owner).unwrap().submitted.take();
                    if let Some(submitted) = submitted {
                        let mut castle = world.get_mut::<KarmaCastle>(owner).unwrap();
                        if castle.draft.as_ref() == Some(&submitted) {
                            castle.draft = None;
                        }
                        castle.edits.retain(|draft| draft != &submitted);
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        if view
                            .editing
                            .as_ref()
                            .is_some_and(|(uid, _)| submitted.rule.as_ref() == Some(uid))
                        {
                            view.editing = None;
                        }
                    }
                    let deleted = world
                        .get_mut::<View>(owner)
                        .unwrap()
                        .deleting_pending
                        .take();
                    if let Some(deleted) = deleted {
                        world
                            .get_mut::<KarmaCastle>(owner)
                            .unwrap()
                            .edits
                            .retain(|draft| draft.rule.as_ref() != Some(&deleted));
                        let mut view = world.get_mut::<View>(owner).unwrap();
                        if view
                            .editing
                            .as_ref()
                            .is_some_and(|(uid, _)| uid == &deleted)
                        {
                            view.editing = None;
                        }
                        view.deleting.retain(|uid| uid != &deleted);
                        view.rules.retain(|rule| rule.uid != deleted);
                        delete_next(world, owner);
                    }
                    ui::render_form(world, owner);
                    ui::render_list(world, owner);
                    if world.get::<View>(owner).unwrap().saving {
                        save(world, owner);
                    }
                    if world.get::<View>(owner).unwrap().pause_active {
                        ui::Command::ConfirmPause.apply(world, owner);
                    }
                    if world.get::<View>(owner).unwrap().pending.is_none() {
                        status(world, owner, "");
                    }
                }
            }
            ServerMessage::Error { id, message, .. } => {
                let reading = world.resource_mut::<Requests>().readings.remove(&id);
                if let Some(entity) = reading {
                    ui::reading_reply(world, entity, message);
                    continue;
                }
                let owners: Vec<_> = world
                    .query::<(Entity, &View)>()
                    .iter(world)
                    .filter(|(owner, view)| {
                        view.pending.as_ref() == Some(&id)
                            || world
                                .resource::<Requests>()
                                .subscriptions
                                .get(&id)
                                .is_some_and(|(entity, _)| entity == owner)
                            || id == crate::cell_bridge::CONNECTION
                    })
                    .map(|(owner, _)| owner)
                    .collect();
                for owner in owners {
                    let mut view = world.get_mut::<View>(owner).unwrap();
                    view.pending = None;
                    view.submitted = None;
                    view.saving = false;
                    view.deleting_pending = None;
                    view.pause_active = false;
                    view.pausing.clear();
                    ui::render_form(world, owner);
                    ui::render_list(world, owner);
                    status(world, owner, &message);
                }
            }
            _ => {}
        }
    }
}
