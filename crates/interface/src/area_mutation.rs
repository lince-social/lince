use crate::{
    area::{InfluenceArea, RecordProperties},
    canvas::CanvasItem,
    cell_bridge::{CellBridge, CellMessage},
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{
    math::{DVec2, DVec3},
    prelude::*,
};
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

#[derive(Component)]
pub(crate) struct Preparing;

#[derive(Component)]
struct Suspended(InfluenceArea);

#[derive(Component, Default)]
pub struct MutationStatus(pub String);

#[derive(Message, Clone)]
pub struct TransitionApplied {
    pub area: Entity,
    pub record: String,
    pub inside: bool,
}

#[derive(Component)]
pub(crate) struct HeldPoint(pub DVec3);

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
    entity: Entity,
    root: Entity,
    workspace: u64,
    area: InfluenceArea,
    placement: crate::topology::Spatial,
    visits: HashMap<String, Visit>,
}

struct Pending {
    target: String,
    areas: Vec<(Entity, bool)>,
    applying: bool,
    retry: Option<TransitionPreview>,
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
        app.add_message::<TransitionApplied>();
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

pub(crate) fn pending(world: &World, entity: Entity) -> bool {
    let Some(uid) = world
        .get::<RecordProperties>(entity)
        .and_then(|record| record.0["uid"].as_str())
    else {
        return false;
    };
    world
        .get_resource::<Mutations>()
        .is_some_and(|state| state.pending.values().any(|pending| pending.target == uid))
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
        world
            .get_mut::<InfluenceArea>(area)
            .unwrap()
            .changes_enabled = false;
        disarm(
            world,
            area,
            "Property changes inactive. Already submitted changes may still finish.",
        );
    }
}

pub fn preview(world: &mut World, root: Entity, entity: Entity) {
    if !crate::area_panel::owns(world, root, entity) {
        return;
    }
    disarm(world, entity, "Property changes inactive");
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    if world.get::<Preparing>(entity).is_some()
        || !area.enabled
        || !area.changes_enabled
        || !area.validate()
        || area.changes.is_empty()
        || (area.change_filter.is_none() && area.filter.is_none() && area.rules.is_empty())
        || ((area.change_filter.is_some() || area.filter.is_some())
            && !change_matches(world, entity, &area).is_some_and(|filter| {
                filter.current && filter.source == crate::protein_area::Source::Local
            }))
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
        "Preview ready. Enabled property changes allow future crossings to change matching Records. Records already inside stay unchanged until they cross a boundary.",
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
            "Connect to a Cell to enable property changes.",
        );
        return;
    }
    world.entity_mut(entity).remove::<Suspended>();
    let area = world.get::<InfluenceArea>(entity).unwrap().clone();
    let workspace = world.get::<WorkspaceMember>(entity).unwrap().0;
    let records = records(world);
    let mut grant = Grant {
        entity,
        root,
        workspace,
        area,
        placement: crate::topology::spatial(world, entity),
        visits: HashMap::new(),
    };
    baseline(&mut grant, &records);
    world
        .resource_mut::<Mutations>()
        .grants
        .insert(entity, grant);
    status(
        world,
        entity,
        "Property changes enabled for future crossings. Settings and permissions are checked automatically.",
    );
}

struct Record {
    root: Entity,
    workspace: u64,
    uid: String,
    points: Vec<DVec3>,
    immune: HashSet<Entity>,
    properties: RecordProperties,
    filters: HashSet<Entity>,
}

fn records(world: &mut World) -> Vec<Record> {
    let mut records = BTreeMap::<(Entity, u64, String), Record>::new();
    for (entity, item, properties, parent, member) in world
        .query_filtered::<(
            Entity,
            &CanvasItem,
            &RecordProperties,
            &ChildOf,
            &WorkspaceMember,
        ), (
            Without<Pinned>,
            Without<crate::protein_area::placement::Pending>,
            Without<crate::protein_area::RemoteRecord>,
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
                immune: HashSet::new(),
                properties: properties.clone(),
                filters: HashSet::new(),
            })
            .points
            .push(
                world
                    .get::<HeldPoint>(entity)
                    .map(|held| held.0)
                    .or_else(|| crate::layout::membership(world, entity))
                    .unwrap_or_else(|| {
                        crate::topology::spatial(world, entity).position(item.position)
                    }),
            );
    }
    let filters: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .collect();
    for record in records.values_mut() {
        for (entity, area, parent, member) in &filters {
            if parent.parent() == record.root
                && member.0 == record.workspace
                && change_matches(world, *entity, area)
                    .is_some_and(|filter| filter.allows(&record.properties, None))
            {
                record.filters.insert(*entity);
            }
        }
    }
    let shields = world
        .query::<&InfluenceArea>()
        .iter(world)
        .any(|area| area.immunity != crate::area_effects::Immunity::None);
    let sources: Vec<_> = if shields {
        world
            .query::<(Entity, &InfluenceArea)>()
            .iter(world)
            .filter(|(_, area)| !area.changes.is_empty())
            .map(|(entity, _)| entity)
            .collect()
    } else {
        Vec::new()
    };
    for record in records.values_mut() {
        for source in &sources {
            if record.points.iter().all(|point| {
                crate::topology::influence::blocked(
                    world,
                    record.root,
                    record.workspace,
                    *source,
                    *point,
                    Some(&record.properties),
                    None,
                )
            }) {
                record.immune.insert(*source);
            }
        }
    }
    records.into_values().collect()
}

fn baseline(grant: &mut Grant, records: &[Record]) {
    for record in records
        .iter()
        .filter(|record| record.root == grant.root && record.workspace == grant.workspace)
    {
        let inside = record.points.iter().any(|point| {
            crate::topology::influence::contains(&grant.area, grant.placement, *point)
        });
        grant.visits.insert(
            record.uid.clone(),
            Visit {
                inside,
                eligible: inside && record.matches(grant),
            },
        );
    }
}

fn valid_grant(world: &World, entity: Entity, grant: &Grant) -> bool {
    world.get::<InfluenceArea>(entity).is_some_and(|area| {
        if area == &grant.area {
            return true;
        }
        let mut expected = grant.area.clone();
        expected.color = area.color;
        expected.opacity = area.opacity;
        area == &expected
    }) && crate::topology::spatial(world, entity) == grant.placement
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
        suspend(world, *area, message);
    }
}

fn receive(world: &mut World, messages: Vec<ServerMessage>, records: &[Record]) {
    let retries: Vec<_> = world
        .resource_mut::<Mutations>()
        .pending
        .iter_mut()
        .filter_map(|(id, pending)| pending.retry.take().map(|preview| (id.clone(), preview)))
        .collect();
    for (id, preview) in retries {
        if let Some(pending) = world.resource_mut::<Mutations>().pending.remove(&id) {
            submit(world, records, id, pending, preview);
        }
    }
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
                    "Property changes inactive because the Cell connection stopped. Check Records before property changes can resume.",
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
        let Some(pending) = world.resource_mut::<Mutations>().pending.remove(&id) else {
            continue;
        };
        let data = match result {
            Ok(data) => data,
            Err(error) => {
                stop_pending(
                    world,
                    &pending,
                    &format!("Property changes inactive. Change was not confirmed: {error}"),
                );
                continue;
            }
        };
        if pending.applying {
            for (area, inside) in pending.areas {
                world.write_message(TransitionApplied { area, record: pending.target.clone(), inside });
                if armed(world, area) {
                    status(
                        world,
                        area,
                        "Property changes enabled. Last Record change saved.",
                    );
                }
            }
            continue;
        }
        let preview = data.and_then(|data| serde_json::from_value::<TransitionPreview>(data).ok());
        let Some(preview) = preview.filter(|preview| preview.target == pending.target) else {
            stop_pending(
                world,
                &pending,
                "Property changes inactive. The Cell did not return a valid change preview.",
            );
            continue;
        };
        submit(world, records, id, pending, preview);
    }
}

fn submit(
    world: &mut World,
    records: &[Record],
    id: String,
    mut pending: Pending,
    preview: TransitionPreview,
) {
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
                            && !record.immune.contains(area)
                            && record.points.iter().any(|point| {
                                crate::topology::influence::contains(
                                    &grant.area,
                                    grant.placement,
                                    *point,
                                )
                            }) == *inside
                    })
            })
    });
    if !valid {
        stop_pending(
            world,
            &pending,
            "Property changes inactive. The Area, Sand or immunity changed before submission.",
        );
        return;
    }
    let apply_id = request_id();
    if !send(
        world,
        apply_id.clone(),
        Action::ApplyAreaTransition {
            request_id: apply_id.clone(),
            preview: preview.clone(),
        },
    ) {
        if world
            .get_non_send::<CellBridge>()
            .is_some_and(|bridge| !bridge.outgoing.is_closed())
        {
            pending.retry = Some(preview);
            world
                .resource_mut::<Mutations>()
                .pending
                .insert(id, pending);
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.ring();
            }
        } else {
            stop_pending(
                world,
                &pending,
                "Property changes inactive. The Cell could not accept this change.",
            );
        }
        return;
    }
    pending.applying = true;
    world
        .resource_mut::<Mutations>()
        .pending
        .insert(apply_id, pending);
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
    activate_configured(world);
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
            "Property changes inactive after an Area edit or workspace change. Configured changes resume automatically.",
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
            let inside = record.points.iter().any(|point| {
                crate::topology::influence::contains(&grant.area, grant.placement, *point)
            });
            let next = Visit {
                inside,
                eligible: inside && record.matches(grant),
            };
            if record.immune.contains(entity) {
                grant.visits.insert(
                    record.uid.clone(),
                    Visit {
                        inside,
                        eligible: false,
                    },
                );
                continue;
            }
            let Some(previous) = grant.visits.get_mut(&record.uid) else {
                grant.visits.insert(record.uid.clone(), next);
                continue;
            };
            if previous.inside == inside {
                previous.eligible |= next.eligible;
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
        let entering_quantity = contributions
            .iter()
            .any(|(_, inside, change)| *inside && change.quantity.is_some());
        let mut conflict = contributions.iter().any(|(_, inside, change)| {
            let mut change = change.clone();
            if !inside && entering_quantity {
                change.quantity = None;
            }
            !changes.merge(&change)
        });
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
        let exhausted = state.pending.len() >= 64;
        if exhausted && !conflict {
            for (entity, inside, _) in &contributions {
                if let Some(grant) = state.grants.get_mut(entity) {
                    grant.visits.insert(
                        uid.clone(),
                        Visit {
                            inside: !inside,
                            eligible: !inside,
                        },
                    );
                }
            }
            continue;
        }
        if conflict {
            for area in involved {
                stopped.push((
                    area,
                    "Property changes inactive. Overlapping Areas request conflicting Record changes.",
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
            state.pending.insert(
                id,
                Pending {
                    target: uid,
                    areas: contributions
                        .into_iter()
                        .map(|(entity, inside, _)| (entity, inside))
                        .collect(),
                    applying: false,
                    retry: None,
                },
            );
        } else if world
            .get_non_send::<CellBridge>()
            .is_some_and(|bridge| !bridge.outgoing.is_closed())
        {
            for (entity, inside, _) in &contributions {
                if let Some(grant) = state.grants.get_mut(entity) {
                    grant.visits.insert(
                        uid.clone(),
                        Visit {
                            inside: !inside,
                            eligible: !inside,
                        },
                    );
                }
            }
            if let Some(wake) = world.get_resource::<crate::wake::WakeSignal>() {
                wake.ring();
            }
        } else {
            for area in involved {
                stopped.push((
                    area,
                    "Property changes inactive. The Cell could not accept a preview request.",
                ));
            }
        }
    }
    world.insert_resource(state);
    let mut reported = HashSet::new();
    for (area, message) in stopped {
        suspend(world, area, message);
        if reported.insert(message) {
            crate::notifications::report(world, "interface::areas", message);
        }
    }
}

fn labels(
    mut labels: Query<(&StatusLabel, &mut Text, &mut crate::icons::Tooltip)>,
    statuses: Query<&MutationStatus>,
    state: Res<Mutations>,
    mut controls: Query<(&DisarmControl, &mut Node)>,
) {
    for (label, mut text, mut tooltip) in &mut labels {
        let value = statuses
            .get(label.0)
            .map_or("Property changes inactive", |status| status.0.as_str());
        if tooltip.0 != value {
            tooltip.0 = value.into();
        }
        let title = if state.grants.contains_key(&label.0) {
            "Property changes enabled"
        } else if state.previews.contains_key(&label.0) {
            "Preview"
        } else {
            "Property changes inactive"
        };
        if text.0 != title {
            text.0 = title.into();
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

impl Record {
    fn matches(&self, grant: &Grant) -> bool {
        if self.immune.contains(&grant.entity) {
            return false;
        }
        if grant.area.change_filter.is_some() || grant.area.filter.is_some() {
            self.filters.contains(&grant.entity)
        } else {
            grant.area.matches(&self.properties)
        }
    }
}

fn change_matches<'a>(
    world: &'a World,
    entity: Entity,
    area: &InfluenceArea,
) -> Option<&'a crate::protein_area::filter::Matches> {
    if area.change_filter.is_some() {
        world
            .get::<crate::protein_area::filter::ChangeMatches>(entity)
            .map(|m| &m.0)
    } else {
        world.get::<crate::protein_area::filter::Matches>(entity)
    }
}

fn activate_configured(world: &mut World) {
    if crate::laboratory::active(world) || world.get_non_send::<CellBridge>().is_none() {
        return;
    }
    let areas: Vec<_> = world
        .query::<(Entity, &InfluenceArea, &ChildOf, &WorkspaceMember)>()
        .iter(world)
        .filter(|(entity, area, parent, member)| {
            area.enabled
                && area.changes_enabled
                && !area.changes.is_empty()
                && !armed(world, *entity)
                && world
                    .get::<Suspended>(*entity)
                    .is_none_or(|s| &s.0 != *area)
                && world
                    .get::<Workspaces>(parent.parent())
                    .is_some_and(|w| w.active == member.0)
        })
        .map(|(entity, _, parent, _)| (entity, parent.parent()))
        .collect();
    for (entity, root) in areas {
        preview(world, root, entity);
        arm(world, root, entity);
    }
}

fn suspend(world: &mut World, entity: Entity, message: &str) {
    disarm(world, entity, message);
    if let Some(area) = world.get::<InfluenceArea>(entity).cloned() {
        world.entity_mut(entity).insert(Suspended(area));
    }
}
