use crate::{
    area::{InfluenceArea, RecordProperties},
    canvas::CanvasItem,
    cell_bridge::{CellBridge, CellMessage},
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec2, prelude::*};
use cell::{ClientMessage, ServerMessage};
use engine::{
    actions::Action,
    area_transition::{RecordChanges, TransitionPreview},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[path = "area_mutation_tests.rs"]
pub(crate) mod tests;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaChanges {
    pub enter: RecordChanges,
    pub leave: RecordChanges,
}

impl AreaChanges {
    pub fn validate(&self) -> bool {
        self.enter.validate() && self.leave.validate()
    }

    pub fn is_empty(&self) -> bool {
        self.enter.is_empty() && self.leave.is_empty()
    }
}

#[derive(Component, Default)]
pub struct MutationStatus(pub String);

#[derive(Component)]
pub(crate) struct StatusLabel(pub Entity);

#[derive(Component)]
pub(crate) struct DisarmControl(pub Entity);

#[derive(Clone, Copy)]
struct Visit {
    inside: bool,
    eligible: bool,
}

struct Grant {
    root: Entity,
    workspace: u64,
    area: InfluenceArea,
    visits: HashMap<String, Visit>,
    remaining: usize,
}

struct Pending {
    target: String,
    areas: Vec<(Entity, bool)>,
    applying: bool,
}

#[derive(Resource, Default)]
struct Mutations {
    previews: HashMap<Entity, InfluenceArea>,
    grants: HashMap<Entity, Grant>,
    pending: HashMap<String, Pending>,
}

pub struct AreaMutationPlugin;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
struct ApplyAreaChanges;

impl Plugin for AreaMutationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Mutations>()
            .add_message::<CellMessage>()
            .add_systems(
                PostUpdate,
                update
                    .in_set(ApplyAreaChanges)
                    .after(crate::actions::ApplyActions),
            )
            .add_systems(
                PostUpdate,
                (
                    crate::area_mutation_panel::autosave
                        .after(bevy::text::EditableTextSystems)
                        .before(crate::actions::ApplyActions),
                    labels.after(ApplyAreaChanges),
                ),
            );
    }
}

pub fn armed(world: &World, entity: Entity) -> bool {
    world
        .get_resource::<Mutations>()
        .is_some_and(|state| state.grants.contains_key(&entity))
}

pub fn previewed(world: &World, entity: Entity) -> bool {
    world
        .get_resource::<Mutations>()
        .and_then(|state| state.previews.get(&entity))
        .is_some_and(|area| world.get::<InfluenceArea>(entity) == Some(area))
}

fn status(world: &mut World, entity: Entity, value: &str) {
    if world.get_entity(entity).is_ok() {
        world
            .entity_mut(entity)
            .insert(MutationStatus(value.into()));
    }
}

pub fn disarm(world: &mut World, entity: Entity, message: &str) {
    let mut affected = HashSet::from([entity]);
    if let Some(mut state) = world.get_resource_mut::<Mutations>() {
        let canceled: Vec<_> = state
            .pending
            .iter()
            .filter(|(_, pending)| pending.areas.iter().any(|(area, _)| *area == entity))
            .map(|(id, _)| id.clone())
            .collect();
        for id in canceled {
            if let Some(pending) = state.pending.remove(&id) {
                for (area, _) in pending.areas {
                    affected.insert(area);
                    state.grants.remove(&area);
                    state.previews.remove(&area);
                }
            }
        }
        state.grants.remove(&entity);
        state.previews.remove(&entity);
    }
    for area in affected {
        status(world, area, message);
    }
}

pub fn disarm_all(world: &mut World, root: Entity) {
    let areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf)>()
        .iter(world)
        .filter(|(_, _, parent)| parent.parent() == root)
        .map(|(entity, _, _)| entity)
        .collect();
    for area in areas {
        disarm(
            world,
            area,
            "Disarmed. Already submitted changes may still finish.",
        );
    }
}

pub fn preview(world: &mut World, root: Entity, entity: Entity) {
    if !crate::area_panel::owns(world, root, entity) {
        return;
    }
    disarm(world, entity, "Disarmed");
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    if !area.validate()
        || area.changes.is_empty()
        || area.rules.is_empty()
        || crate::area_mutation_panel::invalid_fields(world, entity)
    {
        status(
            world,
            entity,
            "Choose matching properties and at least one valid entry or exit change.",
        );
        return;
    }
    world.init_resource::<Mutations>();
    world
        .resource_mut::<Mutations>()
        .previews
        .insert(entity, area);
    status(
        world,
        entity,
        "Preview ready. Arming allows future crossings to change matching Records. Records already inside stay unchanged until they cross a boundary.",
    );
}

pub fn arm(world: &mut World, root: Entity, entity: Entity) {
    if !crate::area_panel::owns(world, root, entity)
        || !previewed(world, entity)
        || armed(world, entity)
    {
        return;
    }
    if world.get_non_send::<CellBridge>().is_none() {
        status(
            world,
            entity,
            "Connect to a Cell before arming Record changes.",
        );
        return;
    }
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    let workspace = world.get::<WorkspaceMember>(entity).unwrap().0;
    let records = records(world);
    let mut grant = Grant {
        root,
        workspace,
        area,
        visits: HashMap::new(),
        remaining: 128,
    };
    baseline(&mut grant, &records);
    world
        .resource_mut::<Mutations>()
        .grants
        .insert(entity, grant);
    status(
        world,
        entity,
        "Armed for future crossings. Stops after 128 requests, an edit, a workspace switch, or an error.",
    );
}

struct Record {
    root: Entity,
    workspace: u64,
    uid: String,
    points: Vec<DVec2>,
    properties: RecordProperties,
}

fn records(world: &mut World) -> Vec<Record> {
    let mut records = BTreeMap::<(Entity, u64, String), Record>::new();
    for (item, properties, parent, member) in world
        .query_filtered::<(&CanvasItem, &RecordProperties, &ChildOf, &WorkspaceMember), (
            Without<Pinned>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        )>()
        .iter(world)
    {
        let Some(uid) = properties
            .0
            .get("uid")
            .and_then(serde_json::Value::as_str)
            .filter(|uid| !uid.is_empty() && uid.len() <= 128)
        else {
            continue;
        };
        if !item.position.is_finite() {
            continue;
        }
        let key = (parent.parent(), member.0, uid.to_string());
        records
            .entry(key)
            .or_insert_with(|| Record {
                root: parent.parent(),
                workspace: member.0,
                uid: uid.into(),
                points: Vec::new(),
                properties: properties.clone(),
            })
            .points
            .push(item.position);
    }
    records.into_values().collect()
}

fn baseline(grant: &mut Grant, records: &[Record]) {
    for record in records
        .iter()
        .filter(|record| record.root == grant.root && record.workspace == grant.workspace)
    {
        let inside = record
            .points
            .iter()
            .any(|point| grant.area.contains(*point));
        grant.visits.insert(
            record.uid.clone(),
            Visit {
                inside,
                eligible: inside && grant.area.matches(&record.properties),
            },
        );
    }
}

fn valid_grant(world: &World, entity: Entity, grant: &Grant) -> bool {
    world.get::<InfluenceArea>(entity) == Some(&grant.area)
        && world
            .get::<ChildOf>(entity)
            .is_some_and(|parent| parent.parent() == grant.root)
        && world
            .get::<WorkspaceMember>(entity)
            .is_some_and(|member| member.0 == grant.workspace)
        && world
            .get::<Workspaces>(grant.root)
            .is_some_and(|spaces| spaces.active == grant.workspace)
}

fn request_id() -> String {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).expect("Area request identity");
    format!(
        "area-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

fn send(world: &World, id: String, action: Action) -> bool {
    world.get_non_send::<CellBridge>().is_some_and(|bridge| {
        bridge
            .outgoing
            .try_send(ClientMessage::Act { id, action })
            .is_ok()
    })
}

fn stop_pending(world: &mut World, pending: &Pending, message: &str) {
    crate::notifications::report(world, "interface::areas", message);
    for (area, _) in &pending.areas {
        disarm(world, *area, message);
    }
}

fn receive(world: &mut World, messages: Vec<ServerMessage>, records: &[Record]) {
    for message in messages {
        if let ServerMessage::Error { ref id, .. } = message
            && id == crate::cell_bridge::CONNECTION
        {
            let areas: Vec<_> = world
                .resource::<Mutations>()
                .grants
                .keys()
                .copied()
                .collect();
            for area in areas {
                disarm(
                    world,
                    area,
                    "Disarmed because the Cell connection stopped. Check Records before arming again.",
                );
            }
            world.resource_mut::<Mutations>().pending.clear();
            continue;
        }
        let (id, result) = match message {
            ServerMessage::ActionOk { id, data, .. } => (id, Ok(data)),
            ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let Some(mut pending) = world.resource_mut::<Mutations>().pending.remove(&id) else {
            continue;
        };
        let data = match result {
            Ok(data) => data,
            Err(error) => {
                stop_pending(
                    world,
                    &pending,
                    &format!("Disarmed. Change was not confirmed: {error}"),
                );
                continue;
            }
        };
        if pending.applying {
            for (area, _) in pending.areas {
                if world
                    .resource::<Mutations>()
                    .grants
                    .get(&area)
                    .is_some_and(|grant| grant.remaining == 0)
                {
                    disarm(
                        world,
                        area,
                        "Disarmed after 128 requests. Review before arming again.",
                    );
                } else if armed(world, area) {
                    status(world, area, "Armed. Last Record change saved.");
                }
            }
            continue;
        }
        let valid = pending.areas.iter().all(|(area, inside)| {
            world
                .resource::<Mutations>()
                .grants
                .get(area)
                .is_some_and(|grant| {
                    valid_grant(world, *area, grant)
                        && records.iter().any(|record| {
                            record.root == grant.root
                                && record.workspace == grant.workspace
                                && record.uid == pending.target
                                && record
                                    .points
                                    .iter()
                                    .any(|point| grant.area.contains(*point))
                                    == *inside
                        })
                })
        });
        if !valid {
            stop_pending(
                world,
                &pending,
                "Disarmed. The Area or Sand moved before its change was submitted.",
            );
            continue;
        }
        let preview = data.and_then(|data| serde_json::from_value::<TransitionPreview>(data).ok());
        let Some(preview) = preview.filter(|preview| preview.target == pending.target) else {
            stop_pending(
                world,
                &pending,
                "Disarmed. The Cell did not return a valid change preview.",
            );
            continue;
        };
        let apply_id = request_id();
        if !send(
            world,
            apply_id.clone(),
            Action::ApplyAreaTransition {
                request_id: apply_id.clone(),
                preview,
            },
        ) {
            stop_pending(
                world,
                &pending,
                "Disarmed. The Cell could not accept this change.",
            );
            continue;
        }
        pending.applying = true;
        world
            .resource_mut::<Mutations>()
            .pending
            .insert(apply_id, pending);
    }
}

fn update(world: &mut World, mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>) {
    let expired: Vec<_> = world
        .resource::<Mutations>()
        .previews
        .iter()
        .filter(|(entity, area)| world.get::<InfluenceArea>(**entity) != Some(*area))
        .map(|(entity, _)| *entity)
        .collect();
    for entity in expired {
        world.resource_mut::<Mutations>().previews.remove(&entity);
    }
    let messages = cursor
        .read(world.resource::<Messages<CellMessage>>())
        .map(|message| message.0.clone())
        .collect();
    if world.resource::<Mutations>().grants.is_empty()
        && world.resource::<Mutations>().pending.is_empty()
    {
        return;
    }
    let records = records(world);
    let invalid: Vec<_> = world
        .resource::<Mutations>()
        .grants
        .iter()
        .filter(|(entity, grant)| !valid_grant(world, **entity, grant))
        .map(|(entity, _)| *entity)
        .collect();
    for area in invalid {
        disarm(
            world,
            area,
            "Disarmed after an Area edit or workspace change. Preview again to arm.",
        );
    }
    receive(world, messages, &records);
    if crate::laboratory::active(world) {
        return;
    }
    let mut state = world.remove_resource::<Mutations>().unwrap();
    let record_keys: HashSet<_> = records
        .iter()
        .map(|record| (record.root, record.workspace, record.uid.as_str()))
        .collect();
    let pending_uids: HashSet<_> = state
        .pending
        .values()
        .map(|pending| pending.target.as_str())
        .collect();
    let mut candidates = BTreeMap::<String, Vec<(Entity, bool, RecordChanges)>>::new();
    for (entity, grant) in &mut state.grants {
        grant
            .visits
            .retain(|uid, _| record_keys.contains(&(grant.root, grant.workspace, uid.as_str())));
        for record in records
            .iter()
            .filter(|record| record.root == grant.root && record.workspace == grant.workspace)
        {
            if pending_uids.contains(record.uid.as_str()) {
                continue;
            }
            let inside = record
                .points
                .iter()
                .any(|point| grant.area.contains(*point));
            let next = Visit {
                inside,
                eligible: inside && grant.area.matches(&record.properties),
            };
            let Some(previous) = grant.visits.get_mut(&record.uid) else {
                grant.visits.insert(record.uid.clone(), next);
                continue;
            };
            if previous.inside == inside {
                continue;
            }
            let eligible = if inside {
                next.eligible
            } else {
                previous.eligible
            };
            *previous = next;
            let changes = if inside {
                &grant.area.changes.enter
            } else {
                &grant.area.changes.leave
            };
            if eligible && !changes.is_empty() {
                candidates.entry(record.uid.clone()).or_default().push((
                    *entity,
                    inside,
                    changes.clone(),
                ));
            }
        }
    }
    let mut stopped = Vec::new();
    for (uid, contributions) in candidates {
        let mut changes = RecordChanges::default();
        let mut conflict = contributions
            .iter()
            .any(|(_, _, change)| !changes.merge(change));
        let mut constraints = changes.clone();
        let mut involved: Vec<_> = contributions.iter().map(|(entity, _, _)| *entity).collect();
        for (entity, grant) in &state.grants {
            if !involved.contains(entity)
                && grant
                    .visits
                    .get(&uid)
                    .is_some_and(|visit| visit.inside && visit.eligible)
            {
                if !constraints.merge(&grant.area.changes.enter) {
                    conflict = true;
                }
                involved.push(*entity);
            }
        }
        let exhausted = involved
            .iter()
            .any(|entity| state.grants[entity].remaining == 0)
            || state.pending.len() >= 64;
        if conflict || exhausted {
            for area in involved {
                stopped.push((
                    area,
                    if conflict {
                        "Disarmed. Overlapping Areas request conflicting Record changes."
                    } else {
                        "Disarmed at the change limit. Review the Records before arming again."
                    },
                ));
            }
            continue;
        }
        let id = request_id();
        if send(
            world,
            id.clone(),
            Action::PreviewAreaTransition {
                target: uid.clone(),
                changes,
                constraints,
            },
        ) {
            for (entity, _, _) in &contributions {
                state.grants.get_mut(entity).unwrap().remaining -= 1;
            }
            state.pending.insert(
                id,
                Pending {
                    target: uid,
                    areas: contributions
                        .into_iter()
                        .map(|(entity, inside, _)| (entity, inside))
                        .collect(),
                    applying: false,
                },
            );
        } else {
            for area in involved {
                stopped.push((
                    area,
                    "Disarmed. The Cell could not accept a preview request.",
                ));
            }
        }
    }
    world.insert_resource(state);
    let mut reported = HashSet::new();
    for (area, message) in stopped {
        disarm(world, area, message);
        if reported.insert(message) {
            crate::notifications::report(world, "interface::areas", message);
        }
    }
}

fn labels(
    mut labels: Query<(&StatusLabel, &mut Text)>,
    statuses: Query<&MutationStatus>,
    state: Res<Mutations>,
    mut controls: Query<(&DisarmControl, &mut Node)>,
) {
    for (label, mut text) in &mut labels {
        let value = statuses
            .get(label.0)
            .map_or("Disarmed", |status| status.0.as_str());
        if text.0 != value {
            text.0 = value.into();
        }
    }
    for (control, mut node) in &mut controls {
        let display = if state.grants.values().any(|grant| grant.root == control.0) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
}
