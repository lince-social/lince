use super::{HeldPoint, Record, change_matches};
use crate::native::{
    area::{InfluenceArea, RecordProperties},
    area_effects::Immunity,
    canvas::CanvasItem,
    layout::membership,
    practice_cells::PracticeRecord,
    protein_area::{RecordBinding, RemoteRecord, Source, filter::Matches, placement::Pending},
    sand_placement::Pinned,
    topology::{
        influence::{BlockerQuery, Blockers},
        participation_point,
    },
    workspace::WorkspaceMember,
};
use bevy::{
    ecs::{entity_disabling::Disabled, query::Allow},
    prelude::*,
};
use nucleus::performance::{Span, add};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
};

type RecordItems = QueryState<
    (
        Entity,
        &'static CanvasItem,
        &'static RecordProperties,
        &'static ChildOf,
        &'static WorkspaceMember,
    ),
    (
        Without<Pinned>,
        Without<Pending>,
        Or<(Without<RemoteRecord>, With<PracticeRecord>)>,
        Allow<Disabled>,
    ),
>;

pub(super) fn prepare(
    world: &mut World,
    items: &mut RecordItems,
    areas: &mut QueryState<(
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
    )>,
    all_areas: &mut QueryState<(Entity, &'static InfluenceArea)>,
    shields: &mut BlockerQuery,
) -> Vec<Record> {
    let _preparation = Span::new("area-mutation-snapshot");
    let local = Source::Local;
    // Source-name keys stay inside the exclusive snapshot system.
    let mut source_names = HashMap::<&Source, Rc<str>>::new();
    let mut records = BTreeMap::new();
    for (entity, item, properties, parent, member) in items.iter(world) {
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
        let source = world
            .get::<RecordBinding>(entity)
            .map_or(&local, |binding| &binding.source);
        let source_name = source_names
            .entry(source)
            .or_insert_with(|| Rc::from(format!("{source:?}")))
            .clone();
        let root = parent.parent();
        let workspace = member.0;
        let record = records
            .entry((root, workspace, uid, source_name))
            .or_insert_with(|| Record {
                root,
                workspace,
                uid: uid.into(),
                points: Vec::new(),
                immune: HashSet::new(),
                properties: properties.clone(),
                filters: HashSet::new(),
                sources: Vec::new(),
            });
        if !record.sources.contains(source) {
            record.sources.push(source.clone());
        }
        record.points.push(
            world
                .get::<HeldPoint>(entity)
                .map(|held| held.0)
                .or_else(|| membership(world, entity))
                .unwrap_or_else(|| participation_point(world, entity, item)),
        );
    }
    add("area-mutation-sources-formatted", source_names.len() as u64);
    if records.is_empty() {
        return Vec::new();
    }
    let mut filters = HashMap::<(Entity, u64), Vec<(Entity, &Matches)>>::new();
    let mut prepared = 0;
    for (entity, area, parent, member) in areas.iter(world) {
        if let Some(filter) = change_matches(world, entity, area).filter(|filter| filter.current) {
            filters
                .entry((parent.parent(), member.0))
                .or_default()
                .push((entity, filter));
            prepared += 1;
        }
    }
    let mut pairs = 0;
    if !filters.is_empty() {
        for record in records.values_mut() {
            let Some(scope) = filters.get(&(record.root, record.workspace)) else {
                continue;
            };
            pairs += scope.len() as u64;
            for (entity, filter) in scope {
                if record.sources.contains(&filter.source) && filter.uids.contains(&record.uid) {
                    record.filters.insert(*entity);
                }
            }
        }
    }
    add("area-mutation-filters-prepared", prepared);
    add("area-mutation-filter-pairs", pairs);
    let has_shields = all_areas
        .iter(world)
        .any(|(_, area)| area.immunity != Immunity::None);
    if has_shields {
        let sources: Vec<_> = all_areas
            .iter(world)
            .filter(|(_, area)| !area.changes.is_empty())
            .map(|(entity, _)| entity)
            .collect();
        let mut blockers = HashMap::new();
        for record in records.values_mut() {
            for source in &sources {
                let barriers = blockers
                    .entry((record.root, record.workspace, *source))
                    .or_insert_with(|| {
                        Blockers::prepare(
                            world,
                            shields,
                            record.root,
                            record.workspace,
                            *source,
                            false,
                        )
                    });
                if record
                    .points
                    .iter()
                    .all(|point| barriers.blocks(*point, Some(&record.properties), None))
                {
                    record.immune.insert(*source);
                }
            }
        }
    }
    records.into_values().collect()
}
