use crate::native::{
    area::{InfluenceArea, RecordProperties},
    canvas::CanvasItem,
    cell_bridge::{CellMessage, replies},
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

mod admission;
mod cycles;
mod snapshot;
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

#[derive(Clone, Copy)]
struct Visit {
    inside: bool,
    eligible: bool,
    immune: bool,
    deferred: bool,
}

struct Grant {
    binding: Option<nucleus::terrain::SpatialBinding>,
    entity: Entity,
    root: Entity,
    workspace: u64,
    area: InfluenceArea,
    placement: crate::native::topology::Spatial,
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
    cycles: cycles::Cycles,
}

pub struct AreaMutationPlugin;

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ApplyAreaChanges;

impl Plugin for AreaMutationPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<TransitionApplied>();
        app.init_resource::<Mutations>()
            .add_message::<CellMessage>()
            .add_systems(
                PostUpdate,
                update
                    .in_set(ApplyAreaChanges)
                    .after(crate::native::actions::ApplyActions),
            )
            .add_systems(
                PostUpdate,
                (
                    crate::native::area_mutation_panel::autosave
                        .after(bevy::text::EditableTextSystems)
                        .before(crate::native::actions::ApplyActions),
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
    if world.get_entity(entity).is_ok()
        && world
            .get::<MutationStatus>(entity)
            .is_none_or(|status| status.0 != value)
    {
        world
            .entity_mut(entity)
            .insert(MutationStatus(value.into()));
    }
}

pub(crate) fn tracks(world: &World, entity: Entity, uid: &str) -> bool {
    world
        .get_resource::<Mutations>()
        .and_then(|state| state.grants.get(&entity))
        .is_some_and(|grant| grant.visits.contains_key(uid))
}

pub fn disarm(world: &mut World, entity: Entity, message: &str) {
    let mut affected = HashSet::from([entity]);
    if let Some(mut state) = world.get_resource_mut::<Mutations>() {
        state.cycles.forget(entity);
        let canceled: Vec<_> = state
            .pending
            .iter()
            .filter(|(_, pending)| pending.areas.iter().any(|(area, _)| *area == entity))
            .map(|(id, _)| id.clone())
            .collect();
        for id in canceled {
            if let Some(pending) = state.pending.get(&id) {
                let areas = pending.areas.clone();
                if !pending.applying {
                    state.pending.remove(&id);
                }
                for (area, _) in areas {
                    affected.insert(area);
                    state.cycles.forget(area);
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

pub fn preview(world: &mut World, root: Entity, entity: Entity) {
    if !crate::native::area_panel::owns(world, root, entity) {
        return;
    }
    disarm(world, entity, "Property changes inactive");
    let area = world.get::<InfluenceArea>(entity).unwrap();
    let rejected = world.get::<Preparing>(entity).is_some()
        || crate::native::influence_report::activity(
            world,
            entity,
            area,
            root,
            world.get::<WorkspaceMember>(entity).map_or(1, |m| m.0),
        ) != crate::native::influence_report::Outcome::Active
        || !area.changes_enabled
        || !area.validate()
        || area.changes.is_empty()
        || (area.change_filter.is_none() && area.filter.is_none() && area.rules.is_empty())
        || ((area.change_filter.is_some() || area.filter.is_some())
            && !change_matches(world, entity, area).is_some_and(|filter| {
                filter.current
                    && (filter.source == crate::native::protein_area::Source::Local
                        || crate::native::practice_cells::owns_source(world, &filter.source)
                            && world
                                .get::<crate::native::practice_cells::PracticeArea>(entity)
                                .is_some_and(|practice| {
                                    matches!(
                                        &filter.source,
                                        crate::native::protein_area::Source::Organ(source)
                                            if source == &practice.0
                                    )
                                }))
            }));
    let area = (!rejected).then(|| area.clone());
    if area.is_none() || crate::native::area_mutation_panel::invalid_fields(world, entity) {
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
        .insert(entity, area.unwrap());
    status(
        world,
        entity,
        "Preview ready. Enabled property changes allow future crossings to change matching Records. Records already inside stay unchanged until they cross a boundary.",
    );
}

pub fn arm(world: &mut World, root: Entity, entity: Entity) {
    if !crate::native::area_panel::owns(world, root, entity)
        || !previewed(world, entity)
        || armed(world, entity)
        || world.get::<InfluenceArea>(entity).is_none_or(|area| {
            crate::native::influence_report::activity(
                world,
                entity,
                area,
                root,
                world.get::<WorkspaceMember>(entity).map_or(1, |m| m.0),
            ) != crate::native::influence_report::Outcome::Active
        })
    {
        return;
    }
    if crate::native::practice_cells::sender(world, entity).is_none() {
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
        binding: world
            .get::<crate::native::topology::terrain::Bound>(entity)
            .map(|b| b.0.clone()),
        entity,
        root,
        workspace,
        area,
        placement: crate::native::topology::spatial(world, entity),
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
    sources: Vec<crate::native::protein_area::Source>,
}

fn records(world: &mut World) -> Vec<Record> {
    world
        .run_system_cached(snapshot::prepare)
        .expect("prepare current Area mutation Records")
}

fn baseline(grant: &mut Grant, records: &[Record]) {
    for record in records
        .iter()
        .filter(|record| record.root == grant.root && record.workspace == grant.workspace)
    {
        let inside = record.points.iter().any(|point| {
            crate::native::topology::influence::contains(&grant.area, grant.placement, *point)
        });
        grant.visits.insert(
            record.uid.clone(),
            Visit {
                inside,
                eligible: inside && record.matches(grant),
                immune: record.immune.contains(&grant.entity),
                deferred: false,
            },
        );
    }
}

fn valid_grant(world: &World, entity: Entity, grant: &Grant) -> bool {
    nucleus::performance::add("area-mutation-grants-checked", 1);
    if crate::native::influence_report::activity(
        world,
        entity,
        &grant.area,
        grant.root,
        grant.workspace,
    ) != crate::native::influence_report::Outcome::Active
    {
        return false;
    }
    let follows = grant.binding.as_ref().is_some_and(|binding| {
        binding.anchor != nucleus::terrain::Anchor::Fixed
            && world
                .get::<crate::native::topology::terrain::Bound>(entity)
                .is_some_and(|current| binding.same_attachment(&current.0))
    });
    let current_binding = world
        .get::<crate::native::topology::terrain::Bound>(entity)
        .map(|b| &b.0);
    if grant.binding.as_ref().is_some() != current_binding.is_some()
        || grant
            .binding
            .as_ref()
            .zip(current_binding)
            .is_some_and(|(a, b)| !a.same_attachment(b))
    {
        return false;
    }
    let mut placement = crate::native::topology::spatial(world, entity);
    if follows {
        placement.elevation = grant.placement.elevation;
        if grant.binding.as_ref().is_some_and(|b| b.follow_orientation) {
            placement.rotation = grant.placement.rotation;
        }
    }
    world
        .get::<InfluenceArea>(entity)
        .is_some_and(|area| admission::same_configuration(area, &grant.area, follows))
        && placement == grant.placement
        && world
            .get::<ChildOf>(entity)
            .is_some_and(|parent| parent.parent() == grant.root)
        && world
            .get::<WorkspaceMember>(entity)
            .is_some_and(|member| member.0 == grant.workspace)
        && crate::native::topology::holes::live(world, grant.root, grant.workspace)
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

fn send(world: &World, owner: Entity, id: String, action: Action) -> bool {
    crate::native::practice_cells::send(world, owner, ClientMessage::Act { id, action }).is_ok()
}

fn stop_pending(world: &mut World, pending: &Pending, message: &str) {
    crate::native::notifications::report(world, "interface::areas", message);
    for (area, _) in &pending.areas {
        suspend(world, *area, message);
    }
}

fn receive(world: &mut World, wave: &mut replies::Wave, records: &[Record]) {
    let _receive = nucleus::performance::Span::new("area-mutation-reply-processing");
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
    for sequence in wave.drain() {
        let message = replies::message(world.resource::<Messages<CellMessage>>(), sequence);
        if let ServerMessage::Error { id, .. } = message.as_ref()
            && id == crate::native::cell_bridge::CONNECTION
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
        let Some(id) = replies::action_id(message) else {
            continue;
        };
        if !world.resource::<Mutations>().pending.contains_key(id) {
            continue;
        }
        let message = message.clone();
        nucleus::performance::add("area-mutation-replies-retained", 1);
        let (id, result) = match message.as_ref() {
            ServerMessage::ActionOk { id, data, .. } => (id, Ok(data)),
            ServerMessage::Error { id, message, .. } => (id, Err(message)),
            _ => continue,
        };
        let Some(pending) = world.resource_mut::<Mutations>().pending.remove(id) else {
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
                world.write_message(TransitionApplied {
                    area,
                    record: pending.target.clone(),
                    inside,
                });
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
        let preview = data
            .as_ref()
            .and_then(|data| TransitionPreview::deserialize(data).ok());
        let Some(preview) = preview.filter(|preview| preview.target == pending.target) else {
            stop_pending(
                world,
                &pending,
                "Property changes inactive. The Cell did not return a valid change preview.",
            );
            continue;
        };
        submit(world, records, id.clone(), pending, preview);
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
                                crate::native::topology::influence::contains(
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
        pending.areas[0].0,
        apply_id.clone(),
        Action::ApplyAreaTransition {
            request_id: apply_id.clone(),
            preview: preview.clone(),
        },
    ) {
        if crate::native::practice_cells::sender(world, pending.areas[0].0)
            .is_some_and(|sender| !sender.is_closed())
        {
            pending.retry = Some(preview);
            world
                .resource_mut::<Mutations>()
                .pending
                .insert(id, pending);
            if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
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

fn update(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<CellMessage>>,
    mut wave: Local<replies::Wave>,
) {
    let _update = nucleus::performance::Span::new("area-mutation-maintenance");
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
    wave.capture(
        &mut cursor,
        world.resource::<Messages<CellMessage>>(),
        |message| replies::action_id(message).is_some(),
    );
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
    let moved: Vec<_> = world
        .resource::<Mutations>()
        .grants
        .keys()
        .filter_map(|entity| {
            world
                .get::<crate::native::topology::terrain::Bound>(*entity)
                .filter(|b| b.0.anchor != nucleus::terrain::Anchor::Fixed)?;
            Some((
                *entity,
                world.get::<InfluenceArea>(*entity)?.center,
                crate::native::topology::spatial(world, *entity),
            ))
        })
        .collect();
    for (entity, center, placement) in moved {
        if let Some(grant) = world.resource_mut::<Mutations>().grants.get_mut(&entity) {
            grant.area.center = center;
            grant.placement = placement;
        }
    }
    receive(world, &mut wave, &records);
    if !crate::native::laboratory::normal(world) {
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
    let mut candidates =
        BTreeMap::<(Entity, u64, String, Option<String>), Vec<(Entity, bool, RecordChanges)>>::new(
        );
    for (entity, grant) in &mut state.grants {
        if crate::native::laboratory::blocked(world, *entity) {
            continue;
        }
        grant
            .visits
            .retain(|uid, _| record_keys.contains(&(grant.root, grant.workspace, uid.as_str())));
        let source = crate::native::practice_cells::source(world, *entity).map_or(
            crate::native::protein_area::Source::Local,
            crate::native::protein_area::Source::Organ,
        );
        for record in records
            .iter()
            .filter(|record| record.root == grant.root && record.workspace == grant.workspace)
        {
            if !record.sources.contains(&source) {
                continue;
            }
            if !crate::native::practice_cells::permits_area(world, *entity, &record.uid) {
                continue;
            }
            if pending_uids.contains(record.uid.as_str()) {
                continue;
            }
            let inside = record.points.iter().any(|point| {
                crate::native::topology::influence::contains(&grant.area, grant.placement, *point)
            });
            let next = Visit {
                inside,
                eligible: inside && record.matches(grant),
                immune: record.immune.contains(entity),
                deferred: false,
            };
            if record.immune.contains(entity) {
                grant.visits.insert(
                    record.uid.clone(),
                    Visit {
                        inside,
                        eligible: false,
                        immune: true,
                        deferred: false,
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
                previous.immune = next.immune;
                previous.deferred = false;
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
                candidates
                    .entry((
                        grant.root,
                        grant.workspace,
                        record.uid.clone(),
                        match &source {
                            crate::native::protein_area::Source::Local => None,
                            crate::native::protein_area::Source::Organ(uid) => Some(uid.clone()),
                        },
                    ))
                    .or_default()
                    .push((*entity, inside, changes.clone()));
            }
        }
    }
    let mut stopped = Vec::new();
    for ((root, workspace, uid, source), mut contributions) in candidates {
        let mut changes = RecordChanges::default();
        let entering_quantity = contributions
            .iter()
            .any(|(_, inside, change)| *inside && change.quantity.is_some());
        let mut conflict = contributions.iter_mut().any(|(_, inside, change)| {
            if !*inside && entering_quantity {
                change.quantity = None;
            }
            !changes.merge(change)
        });
        let mut constraints = changes.clone();
        let mut involved: Vec<_> = contributions.iter().map(|(entity, _, _)| *entity).collect();
        for (entity, grant) in &state.grants {
            if !involved.contains(entity)
                && grant.root == root
                && grant.workspace == workspace
                && crate::native::practice_cells::source(world, *entity) == source
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
        let settings = involved
            .first()
            .and_then(|area| state.grants.get(area))
            .map(|grant| crate::native::workspace_config::rules(world, grant.root, grant.workspace))
            .unwrap_or_default();
        let exhausted = state.pending.len() >= usize::from(settings.max_pending);
        if exhausted && !conflict {
            for (entity, inside, _) in &contributions {
                if let Some(grant) = state.grants.get_mut(entity) {
                    grant.visits.insert(
                        uid.clone(),
                        Visit {
                            inside: !inside,
                            eligible: !inside,
                            immune: false,
                            deferred: true,
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
        let cycle = contributions.iter().find_map(|(area, inside, _)| {
            let grant = state.grants.get(area)?;
            state
                .cycles
                .check(
                    grant.root,
                    grant.workspace,
                    *area,
                    &uid,
                    *inside,
                    settings.max_repeats,
                )
                .err()
        }).or_else(|| {
            (contributions.iter().filter(|(area, _, _)| !state.cycles.has(*area, &uid)).count() > state.cycles.available())
                .then_some("Area paused: the bounded crossing history is full. Resume after reviewing the rules.")
        });
        if let Some(message) = cycle {
            for area in involved {
                if let Some(mut source) = world.get_mut::<InfluenceArea>(area) {
                    source.paused = true;
                }
                stopped.push((area, message));
            }
            continue;
        }
        let id = request_id();
        if send(
            world,
            contributions[0].0,
            id.clone(),
            Action::PreviewAreaTransition {
                target: uid.clone(),
                changes,
                constraints,
            },
        ) {
            for (area, inside, _) in &contributions {
                if let Some(grant) = state.grants.get(area) {
                    state
                        .cycles
                        .record(grant.root, grant.workspace, *area, &uid, *inside);
                }
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
                    retry: None,
                },
            );
        } else if crate::native::practice_cells::sender(world, contributions[0].0)
            .is_some_and(|sender| !sender.is_closed())
        {
            for (entity, inside, _) in &contributions {
                if let Some(grant) = state.grants.get_mut(entity) {
                    grant.visits.insert(
                        uid.clone(),
                        Visit {
                            inside: !inside,
                            eligible: !inside,
                            immune: false,
                            deferred: true,
                        },
                    );
                }
            }
            if let Some(wake) = world.get_resource::<crate::native::wake::WakeSignal>() {
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
            crate::native::notifications::report(world, "interface::areas", message);
        }
    }
}

fn labels(
    mut labels: Query<(&StatusLabel, &mut Text, &mut crate::native::icons::Tooltip)>,
    statuses: Query<&MutationStatus>,
    state: Res<Mutations>,
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
) -> Option<&'a crate::native::protein_area::filter::Matches> {
    if area.change_filter.is_some() {
        world
            .get::<crate::native::protein_area::filter::ChangeMatches>(entity)
            .map(|m| &m.0)
    } else {
        world.get::<crate::native::protein_area::filter::Matches>(entity)
    }
}

fn activate_configured(world: &mut World) {
    if !crate::native::laboratory::normal(world) {
        return;
    }
    let areas = world
        .run_system_cached(admission::candidates)
        .expect("collect current Area activation candidates");
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

pub(crate) fn explanation(world: &World, area: Entity, sand: Entity) -> &str {
    let Some(source) = world.get::<InfluenceArea>(area) else {
        return "Area removed.";
    };
    if world.get::<Pinned>(sand).is_some() {
        return "Boundary changes suppressed: Sand pinned to the screen.";
    }
    if world
        .get::<crate::native::protein_area::RemoteRecord>(sand)
        .is_some()
    {
        return "Boundary changes suppressed: this Record belongs to another Organ.";
    }
    if !source.changes_enabled {
        return "Boundary changes switched off.";
    }
    if let Some(state) = world.get_resource::<Mutations>() {
        let uid = world
            .get::<RecordProperties>(sand)
            .and_then(|record| record.0["uid"].as_str());
        if let Some(uid) = uid {
            if state.pending.values().any(|pending| {
                pending.target == uid && pending.areas.iter().any(|(entity, _)| *entity == area)
            }) {
                return "Boundary change pending; submitted changes may still finish when paused.";
            }
            if let Some(visit) = state
                .grants
                .get(&area)
                .and_then(|grant| grant.visits.get(uid))
            {
                return if visit.deferred {
                    "Boundary crossing waiting for request capacity."
                } else if visit.immune {
                    "Boundary changes suppressed by immunity."
                } else if visit.inside && visit.eligible {
                    "At least one copy of this Record is inside and eligible for future exit changes."
                } else if visit.inside {
                    "At least one copy of this Record is inside; it does not match the change filter."
                } else {
                    "All copies of this Record are outside; an eligible entry can trigger configured changes."
                };
            }
        } else {
            return "Boundary changes require a local Record.";
        }
    }
    world
        .get::<MutationStatus>(area)
        .map_or("Boundary changes inactive.", |status| status.0.as_str())
}
