use super::*;
use crate::native::protein_area::{RecordBinding, Source, filter::Matches};
use std::sync::Arc;

pub(super) fn prepare(
    world: &mut World,
    items: &mut QueryState<
        (
            Entity,
            &'static CanvasItem,
            &'static RecordProperties,
            &'static ChildOf,
            &'static WorkspaceMember,
        ),
        (
            Without<Pinned>,
            Without<crate::native::protein_area::placement::Pending>,
            Or<(
                Without<crate::native::protein_area::RemoteRecord>,
                With<crate::native::practice_cells::PracticeRecord>,
            )>,
            bevy::ecs::query::Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
    areas: &mut QueryState<(
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
    )>,
    all_areas: &mut QueryState<(Entity, &'static InfluenceArea)>,
    shields: &mut crate::native::topology::influence::BlockerQuery,
) -> Vec<Record> {
    let _preparation = nucleus::performance::Span::new("area-mutation-snapshot");
    let local = Source::Local;
    let mut source_names = HashMap::<&Source, Arc<str>>::new();
    let mut records = BTreeMap::<(Entity, u64, &str, Arc<str>), Record>::new();
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
            .or_insert_with(|| Arc::from(format!("{source:?}")))
            .clone();
        let key = (parent.parent(), member.0, uid, source_name);
        let record = records.entry(key).or_insert_with(|| Record {
            root: parent.parent(),
            workspace: member.0,
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
                .or_else(|| crate::native::layout::membership(world, entity))
                .unwrap_or_else(|| {
                    crate::native::topology::participation_point(world, entity, item)
                }),
        );
    }
    nucleus::performance::add("area-mutation-sources-formatted", source_names.len() as u64);
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
    for record in records.values_mut() {
        let Some(scope) = filters.get(&(record.root, record.workspace)) else {
            continue;
        };
        for (entity, filter) in scope {
            pairs += 1;
            if record.sources.contains(&filter.source) && filter.uids.contains(&record.uid) {
                record.filters.insert(*entity);
            }
        }
    }
    nucleus::performance::add("area-mutation-filters-prepared", prepared);
    nucleus::performance::add("area-mutation-filter-pairs", pairs);
    let has_shields = all_areas
        .iter(world)
        .any(|(_, area)| area.immunity != crate::native::area_effects::Immunity::None);
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
                        crate::native::topology::influence::Blockers::prepare(
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
