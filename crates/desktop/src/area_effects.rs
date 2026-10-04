use crate::{
    area::{AreaForce, AreaForces, InfluenceArea, ReachMode, RecordProperties},
    canvas::CanvasItem,
    protein_area::{RecordBinding, Source, filter::Matches, grouping::GeneratedGroup},
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub(crate) mod tests;
pub(crate) mod ui;
pub(crate) use ui::controls;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ForceMode {
    #[default]
    Simple,
    Newtonian,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Immunity {
    #[default]
    None,
    External,
    Internal,
    All,
    Containment,
    Isolation,
}

impl Immunity {
    pub(crate) fn blocks(self, source_inside: bool, target_inside: bool) -> bool {
        match self {
            Self::None => false,
            Self::External => target_inside && !source_inside,
            Self::Internal => target_inside && source_inside,
            Self::All => target_inside,
            Self::Containment => source_inside && !target_inside,
            Self::Isolation => source_inside != target_inside,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sorting {
    pub horizontal: bool,
    pub reverse: bool,
    pub strength: f64,
}

impl Default for Sorting {
    fn default() -> Self {
        Self {
            horizontal: false,
            reverse: false,
            strength: 100.0,
        }
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AreaScale(pub f32);

#[derive(Clone, PartialEq)]
struct Field {
    entity: Entity,
    root: Entity,
    workspace: u64,
    area: InfluenceArea,
    activity: crate::influence_report::Outcome,
    filter: Option<Arc<Matches>>,
    filter_tick: Option<u32>,
    group: Option<GeneratedGroup>,
    targets: HashMap<Entity, DVec2>,
    order: Option<(Source, Arc<Vec<String>>)>,
    members: Vec<(Entity, Vec2, String)>,
}

impl Field {
    fn matches(
        &self,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
        all: bool,
    ) -> bool {
        if self.area.filter.is_some() {
            return record.is_some_and(|record| {
                self.filter
                    .as_ref()
                    .is_some_and(|f| f.allows(record, binding))
            });
        }
        if all && self.area.rules.is_empty() {
            return true;
        }
        record.is_some_and(|record| self.area.matches(record))
    }
}

#[derive(Clone, PartialEq)]
struct Identity {
    root: Entity,
    workspace: u64,
    record: Option<RecordProperties>,
    source: Source,
}

struct Destination {
    point: DVec2,
    strength: f64,
    radius: f64,
    sorting: Option<(DVec2, f64)>,
}

impl Destination {
    fn new(area: &InfluenceArea, target: Option<DVec2>, matches: bool) -> Self {
        let sorting = target.zip(area.sorting.as_ref().map(|sort| sort.strength));
        Self {
            point: if area.direction == crate::area::Direction::Attract {
                sorting
                    .filter(|(_, strength)| *strength > 0.0)
                    .map_or_else(|| area.target_position(), |(target, _)| target)
            } else {
                area.target_position()
            },
            strength: if matches { area.strength } else { 0.0 }
                * if area.direction == crate::area::Direction::Attract {
                    1.0
                } else {
                    -1.0
                },
            radius: area.size[0].min(area.size[1]) * 0.5,
            sorting,
        }
    }

    fn force(&self, point: DVec2) -> DVec2 {
        let delta = self.point - point;
        let mut force =
            delta.normalize_or_zero() * simple_strength(delta.length(), self.radius, self.strength);
        if let Some((target, strength)) = self.sorting {
            let delta = target - point;
            force +=
                delta.normalize_or_zero() * simple_strength(delta.length(), self.radius, strength);
        }
        force
    }
}

pub(crate) fn simple_strength(distance: f64, radius: f64, strength: f64) -> f64 {
    if strength <= 0.0 {
        strength
    } else if distance <= 0.5 {
        0.0
    } else {
        strength * (distance / radius.max(1.0)).min(1.0)
    }
}

struct Cached {
    revision: u64,
    identity: Identity,
    point: DVec2,
    simple: BTreeMap<Entity, Destination>,
    forces: AreaForces,
    total: DVec2,
    spatial: bool,
    trace: Option<Vec<crate::influence_report::Evaluation>>,
}

#[derive(Resource, Default)]
pub(crate) struct Influences {
    fields: Vec<Field>,
    revision: u64,
    cache: HashMap<Entity, Cached>,
    evaluations: u64,
    shields: Vec<usize>,
    tracked: std::collections::HashSet<Entity>,
}

pub(crate) fn blocked(
    world: &mut World,
    root: Entity,
    workspace: u64,
    source: Entity,
    point: DVec2,
    record: Option<&RecordProperties>,
    binding: Option<&RecordBinding>,
) -> bool {
    let Some(area) = world.get::<InfluenceArea>(source) else {
        return false;
    };
    let center = DVec2::from_array(area.center);
    world
        .query_filtered::<(
            Entity,
            &InfluenceArea,
            &ChildOf,
            &WorkspaceMember,
            Option<&Matches>,
        ), (
            Without<crate::component_push::composition::Generated>,
            Without<crate::canvas_host::composition::Generated>,
        )>()
        .iter(world)
        .any(|(entity, shield, parent, member, filter)| {
            entity != source
                && parent.parent() == root
                && member.0 == workspace
                && crate::influence_report::activity(world, entity, shield, root, workspace)
                    == crate::influence_report::Outcome::Active
                && shield.validate()
                && shield.immunity != Immunity::Containment
                && shield
                    .immunity
                    .blocks(shield.contains(center), shield.contains(point))
                && if shield.filter.is_some() {
                    record.is_some_and(|r| filter.is_some_and(|f| f.allows(r, binding)))
                } else {
                    shield.rules.is_empty() || record.is_some_and(|r| shield.matches(r))
                }
        })
}

pub(crate) fn refresh(world: &mut World) {
    world.init_resource::<Influences>();
    let mut runtime = world.remove_resource::<Influences>().unwrap();
    let previous: HashMap<_, _> = runtime.fields.iter().map(|f| (f.entity, f)).collect();
    let mut fields: Vec<_> = world
        .query_filtered::<(
            Entity,
            &InfluenceArea,
            &ChildOf,
            &WorkspaceMember,
            Option<Ref<Matches>>,
            Option<&GeneratedGroup>,
        ), (
            Without<crate::component_push::composition::Generated>,
            Without<crate::canvas_host::composition::Generated>,
        )>()
        .iter(world)
        .filter(|(_, area, _, _, _, group)| area.validate() || group.is_some())
        .map(|(entity, area, parent, member, filter, group)| Field {
            entity,
            root: parent.parent(),
            workspace: member.0,
            activity: crate::influence_report::activity(
                world,
                entity,
                area,
                parent.parent(),
                member.0,
            ),
            area: {
                let mut area = area.clone();
                if !area.attraction_enabled {
                    area.strength = 0.0;
                }
                area
            },
            filter_tick: filter.as_ref().map(|f| f.last_changed().get()),
            filter: filter.as_ref().map(|f| {
                previous
                    .get(&entity)
                    .filter(|old| {
                        old.filter_tick == Some(f.last_changed().get())
                            && old.filter.as_deref() == Some(&**f)
                    })
                    .and_then(|old| old.filter.clone())
                    .unwrap_or_else(|| Arc::new((**f).clone()))
            }),
            group: group.cloned(),
            targets: HashMap::new(),
            order: None,
            members: Vec::new(),
        })
        .collect();
    fields.sort_by(|a, b| a.area.id.cmp(&b.area.id).then(a.entity.cmp(&b.entity)));
    for field in &mut fields {
        if field.activity != crate::influence_report::Outcome::Active {
            continue;
        }
        let Some(sort) = &field.area.sorting else {
            continue;
        };
        let ordered = crate::protein_area::ordered_records(world, field.entity);
        let mut sands: Vec<_> = world
            .query_filtered::<(
                Entity,
                &CanvasItem,
                &ChildOf,
                &WorkspaceMember,
                Option<&RecordProperties>,
                Option<&RecordBinding>,
            ), (
                Without<InfluenceArea>,
                Without<Pinned>,
                Without<crate::external_drop::Preview>,
                Without<crate::time_castle::AttachedCard>,
            )>()
            .iter(world)
            .filter(|(_, _, parent, member, record, binding)| {
                parent.parent() == field.root
                    && member.0 == field.workspace
                    && field.matches(*record, *binding, true)
                    && ordered.as_ref().is_none_or(|(source, _)| {
                        binding.map_or(&Source::Local, |b| &b.source) == source
                    })
            })
            .map(|(entity, item, _, _, record, _)| {
                (
                    entity,
                    item.size,
                    record
                        .and_then(|r| r.0["uid"].as_str())
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect();
        sands.sort_by_key(|(entity, _, _)| *entity);
        field.members = sands.clone();
        field.order = ordered;
        if let Some(old) = previous.get(&field.entity)
            && old.area == field.area
            && old.filter == field.filter
            && old.root == field.root
            && old.workspace == field.workspace
            && old.members == field.members
            && old.order == field.order
        {
            field.targets = old.targets.clone();
            continue;
        }
        let ranks: HashMap<_, _> = field
            .order
            .as_ref()
            .map(|(_, ids)| {
                ids.iter()
                    .enumerate()
                    .map(|(i, uid)| (uid.as_str(), i))
                    .collect()
            })
            .unwrap_or_default();
        if field.order.is_some() {
            sands.retain(|(_, _, uid)| ranks.contains_key(uid.as_str()));
        }
        sands.sort_by(|a, b| {
            ranks
                .get(a.2.as_str())
                .cmp(&ranks.get(b.2.as_str()))
                .then(a.2.cmp(&b.2))
                .then(a.0.cmp(&b.0))
        });
        if sort.reverse {
            sands.reverse();
        }
        let axis = usize::from(!sort.horizontal);
        let total: f64 = sands
            .iter()
            .map(|(_, size, _)| f64::from(size[axis]) + 16.0)
            .sum::<f64>()
            - 16.0;
        let extent = field.area.size[axis];
        let ratio = (extent / total.max(1.0)).min(1.0);
        let mut cursor = -total.max(0.0) * ratio * 0.5;
        for (entity, size, _) in sands {
            let mut target = DVec2::from_array(field.area.center);
            target[axis] += cursor + f64::from(size[axis]) * ratio * 0.5;
            cursor += (f64::from(size[axis]) + 16.0) * ratio;
            field.targets.insert(entity, target);
        }
    }
    let living: std::collections::HashSet<_> = world
        .query_filtered::<Entity, With<CanvasItem>>()
        .iter(world)
        .collect();
    if runtime.fields != fields {
        runtime.shields = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.activity == crate::influence_report::Outcome::Active
                    && f.area.immunity != Immunity::None
            })
            .map(|(i, _)| i)
            .collect();
        runtime.fields = fields;
        runtime.revision = runtime.revision.wrapping_add(1);
    }
    runtime.cache.retain(|entity, _| living.contains(entity));
    runtime.tracked = world
        .get_resource::<crate::influence_report::Tracked>()
        .map(|tracked| tracked.0.clone())
        .unwrap_or_default();
    for (entity, cached) in &mut runtime.cache {
        if !runtime.tracked.contains(entity) {
            cached.trace = None;
        }
    }
    world.insert_resource(runtime);
}

impl Influences {
    pub(crate) fn topology_target(&self, area: Entity, sand: Entity) -> Option<DVec2> {
        self.fields
            .iter()
            .find(|field| field.entity == area)?
            .targets
            .get(&sand)
            .copied()
    }
    pub(crate) fn forget(&mut self, sand: Entity) {
        self.cache.remove(&sand);
    }
    fn blocker(
        &self,
        field: &Field,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
        forces: bool,
    ) -> Option<Entity> {
        self.shields
            .iter()
            .map(|i| &self.fields[*i])
            .find(|shield| {
                shield.entity != field.entity
                    && shield.root == field.root
                    && shield.workspace == field.workspace
                    && shield.area.immunity != Immunity::None
                    && (forces || shield.area.immunity != Immunity::Containment)
                    && shield.area.immunity.blocks(
                        shield.area.contains(DVec2::from_array(field.area.center)),
                        shield.area.contains(point),
                    )
                    && shield.matches(record, binding, true)
            })
            .map(|shield| shield.entity)
    }

    fn calculate(
        &mut self,
        sand: Entity,
        root: Entity,
        workspace: u64,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> &Cached {
        if let Some(cache) = self.cache.get_mut(&sand)
            && cache.revision == self.revision
            && cache.identity.root == root
            && cache.identity.workspace == workspace
            && cache.identity.record.as_ref() == record
            && &cache.identity.source == binding.map_or(&Source::Local, |b| &b.source)
            && (!cache.spatial || cache.point == point)
            && (!self.tracked.contains(&sand) || cache.trace.is_some())
        {
            if cache.point != point {
                cache.point = point;
                cache.forces.0.clear();
                for (area, destination) in &cache.simple {
                    let force = destination.force(point);
                    if force.is_finite() && force != DVec2::ZERO {
                        cache.forces.0.push(AreaForce { area: *area, force });
                    }
                }
                cache.total = bounded_total(&cache.forces);
                if let Some(trace) = &mut cache.trace {
                    for entry in trace {
                        entry.force = cache
                            .forces
                            .0
                            .iter()
                            .find(|force| force.area == entry.area)
                            .map_or(bevy::math::DVec3::ZERO, |force| {
                                bevy::math::DVec3::new(force.force.x, 0.0, force.force.y)
                            });
                    }
                }
            }
            return self.cache.get(&sand).unwrap();
        }
        let identity = Identity {
            root,
            workspace,
            record: record.cloned(),
            source: binding.map_or(Source::Local, |b| b.source.clone()),
        };
        let old = self.cache.remove(&sand);
        let mut simple = old
            .filter(|c| c.revision == self.revision && c.identity == identity)
            .map_or_else(BTreeMap::new, |c| c.simple);
        let mut forces = AreaForces::default();
        let mut trace = self.tracked.contains(&sand).then(Vec::new);
        let spatial = self.fields.iter().any(|f| {
            f.root == root
                && f.workspace == workspace
                && (f.area.reach.mode == ReachMode::Limited
                    || f.area.force_mode == ForceMode::Newtonian
                    || f.area.immunity != Immunity::None
                    || f.group.is_some())
        });
        for field in self
            .fields
            .iter()
            .filter(|f| f.root == root && f.workspace == workspace)
        {
            self.evaluations += 1;
            let mut evaluation = crate::influence_report::Evaluation::new(field.entity);
            evaluation.inside = field.area.contains(point);
            evaluation.slot = field
                .targets
                .get(&sand)
                .map(|target| bevy::math::DVec3::new(target.x, 0.0, target.y));
            let reason = if field.activity != crate::influence_report::Outcome::Active {
                Some(field.activity.clone())
            } else if let Some(shield) = self.blocker(field, point, record, binding, true) {
                Some(crate::influence_report::Outcome::Immune(shield))
            } else if !field.area.reaches(point) {
                Some(crate::influence_report::Outcome::Reach)
            } else {
                None
            };
            if let Some(reason) = reason {
                evaluation.motion = reason;
                if let Some(trace) = &mut trace {
                    trace.push(evaluation);
                }
                continue;
            }
            let mut force = DVec2::ZERO;
            if let Some(group) = &field.group {
                force += group.force(&field.area, sand, point);
            } else if field.area.force_mode == ForceMode::Simple {
                let matches = field.matches(record, binding, false);
                if !matches && !field.targets.contains_key(&sand) {
                    evaluation.motion = crate::influence_report::Outcome::Filter;
                    if let Some(trace) = &mut trace {
                        trace.push(evaluation);
                    }
                    continue;
                }
                force += simple
                    .entry(field.entity)
                    .or_insert_with(|| {
                        Destination::new(&field.area, field.targets.get(&sand).copied(), matches)
                    })
                    .force(point);
            } else if field.matches(record, binding, false) {
                let radius = DVec2::from_array(field.area.size).min_element() * 0.5;
                let distance = point.distance(field.area.target_position());
                force += field.area.force_for_match(point, true)
                    * (radius / distance.max(radius)).powi(2);
            } else {
                evaluation.motion = crate::influence_report::Outcome::Filter;
            }
            if let Some(sort) = &field.area.sorting
                && (field.area.force_mode != ForceMode::Simple || field.group.is_some())
                && let Some(target) = field.targets.get(&sand)
            {
                let delta = *target - point;
                force += delta.normalize_or_zero()
                    * simple_strength(
                        delta.length(),
                        field.area.size[0].min(field.area.size[1]) * 0.5,
                        sort.strength,
                    );
                evaluation.motion = crate::influence_report::Outcome::Active;
            }
            if force.is_finite() && force != DVec2::ZERO {
                forces.0.push(AreaForce {
                    area: field.entity,
                    force,
                });
            }
            evaluation.force = if force.is_finite() {
                bevy::math::DVec3::new(force.x, 0.0, force.y)
            } else {
                bevy::math::DVec3::ZERO
            };
            if let Some(trace) = &mut trace {
                trace.push(evaluation);
            }
        }
        forces.0.sort_by_key(|force| force.area);
        let total = bounded_total(&forces);
        self.cache.insert(
            sand,
            Cached {
                revision: self.revision,
                identity,
                point,
                simple,
                forces,
                total,
                spatial,
                trace,
            },
        );
        self.cache.get(&sand).unwrap()
    }

    pub(crate) fn forces(
        &mut self,
        sand: Entity,
        root: Entity,
        workspace: u64,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> (AreaForces, DVec2) {
        let cache = self.calculate(sand, root, workspace, point, record, binding);
        (cache.forces.clone(), cache.total)
    }

    pub(crate) fn total(
        &mut self,
        sand: Entity,
        root: Entity,
        workspace: u64,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> DVec2 {
        self.calculate(sand, root, workspace, point, record, binding)
            .total
    }

    fn scale(
        &self,
        root: Entity,
        workspace: u64,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> f32 {
        self.fields
            .iter()
            .filter(|f| f.root == root && f.workspace == workspace)
            .map(|field| f64::from(self.scale_result(field, point, record, binding).0))
            .product::<f64>()
            .clamp(0.05, 20.0) as f32
    }

    fn scale_result(
        &self,
        field: &Field,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> (f32, crate::influence_report::Outcome) {
        use crate::influence_report::Outcome;
        if field.activity != Outcome::Active {
            return (1.0, field.activity.clone());
        }
        if !field.area.contains(point) {
            return (1.0, Outcome::Outside);
        }
        if !field.matches(record, binding, true) {
            return (1.0, Outcome::Filter);
        }
        if let Some(shield) = self.blocker(field, point, record, binding, false) {
            return (1.0, Outcome::Immune(shield));
        }
        (field.area.scale, Outcome::Active)
    }

    fn report(
        &mut self,
        sand: Entity,
        root: Entity,
        workspace: u64,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
        pinned: bool,
        scale: f32,
    ) -> crate::influence_report::Report {
        let cache = self.calculate(sand, root, workspace, point, record, binding);
        let mut entries = cache.trace.clone().unwrap_or_default();
        let total = cache.total;
        for entry in &mut entries {
            if pinned {
                entry.motion = crate::influence_report::Outcome::Pinned;
                entry.size = entry.motion.clone();
                entry.force = bevy::math::DVec3::ZERO;
            } else if let Some(field) = self.fields.iter().find(|field| field.entity == entry.area)
            {
                (entry.scale, entry.size) = self.scale_result(field, point, record, binding);
                entry.inside = field.area.contains(point);
            }
        }
        crate::influence_report::Report {
            entries,
            scale,
            total: if pinned {
                bevy::math::DVec3::ZERO
            } else {
                bevy::math::DVec3::new(total.x, 0.0, total.y)
            },
        }
    }
}

fn bounded_total(forces: &AreaForces) -> DVec2 {
    let total = forces.total();
    if total.is_finite() {
        total.clamp_length_max(1_000_000.0)
    } else {
        DVec2::ZERO
    }
}

pub(crate) fn update(world: &mut World) {
    refresh(world);
    if world.contains_resource::<crate::topology::physics::Runtime>() {
        crate::topology::influence::update(world);
        return;
    }
    let sands: Vec<_> = world
        .query_filtered::<(
            Entity,
            &CanvasItem,
            &ChildOf,
            &WorkspaceMember,
            Option<&RecordProperties>,
            Option<&RecordBinding>,
            Has<Pinned>,
        ), (
            Without<InfluenceArea>,
            Without<crate::external_drop::Preview>,
        )>()
        .iter(world)
        .map(|(entity, item, parent, member, record, binding, pinned)| {
            (
                entity,
                *item,
                parent.parent(),
                member.0,
                record.cloned(),
                binding.cloned(),
                pinned,
            )
        })
        .collect();
    let retained: std::collections::HashSet<_> = sands.iter().map(|(entity, ..)| *entity).collect();
    let stale: Vec<_> = world
        .query_filtered::<Entity, Or<(With<AreaScale>, With<AreaForces>)>>()
        .iter(world)
        .filter(|entity| !retained.contains(entity))
        .collect();
    for entity in stale {
        world.entity_mut(entity).remove::<(AreaScale, AreaForces)>();
    }
    world.resource_scope(|world, mut runtime: Mut<Influences>| {
        runtime.cache.retain(|entity, _| retained.contains(entity));
        for (entity, item, root, workspace, record, binding, pinned) in sands {
            let active = world
                .get::<Workspaces>(root)
                .is_some_and(|spaces| spaces.active == workspace)
                && !pinned;
            let scale = if active {
                runtime.scale(
                    root,
                    workspace,
                    item.position,
                    record.as_ref(),
                    binding.as_ref(),
                )
            } else {
                1.0
            };
            if scale == 1.0 {
                world.entity_mut(entity).remove::<AreaScale>();
            } else if world.get::<AreaScale>(entity) != Some(&AreaScale(scale)) {
                world.entity_mut(entity).insert(AreaScale(scale));
            }
            let forces = if active {
                runtime
                    .forces(
                        entity,
                        root,
                        workspace,
                        item.position,
                        record.as_ref(),
                        binding.as_ref(),
                    )
                    .0
            } else {
                runtime.forget(entity);
                AreaForces::default()
            };
            if crate::influence_report::tracked(world, entity) {
                let report = runtime.report(
                    entity,
                    root,
                    workspace,
                    item.position,
                    record.as_ref(),
                    binding.as_ref(),
                    pinned,
                    scale,
                );
                crate::influence_report::store(world, entity, report);
            }
            if record.is_none() && forces.0.is_empty() {
                world.entity_mut(entity).remove::<AreaForces>();
                continue;
            }
            if world.get::<AreaForces>(entity) != Some(&forces) {
                world.entity_mut(entity).insert(forces);
            }
        }
    });
}
