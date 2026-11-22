// non-lean-yet
// Scope: mod tests, mod ui, enum ForceMode, enum Immunity.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 332, 335, 346,
//   347, 408, 502, 628, 659, 754, 795, 821, 826, 928.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (265, 865,
//   910).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   444, 447, 452, 453, 498, 565.
// - [review: §5 Panics] Resolve unwrap receivers and propagate or handle failure where reachable.
//   Sites unwrap (305, 622, 741).
// - [Clippy probe: clippy::allow_attributes] Use a reasoned expect when the diagnostic reliably
//   fires. Sites 15.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 906.

use crate::native::{
    area::{AreaForce, AreaForces, InfluenceArea, ReachMode, RecordProperties},
    canvas::CanvasItem,
    protein_area::{RecordBinding, Source, filter::Matches, grouping::GeneratedGroup},
    sand_placement::Pinned,
    workspace::WorkspaceMember,
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

#[allow(
    clippy::unwrap_used,
    reason = "Area effect Laboratory fixtures must fail when the expected systems, materials, or entities are missing"
)]
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

#[derive(Default, PartialEq)]
struct Arrangement {
    targets: HashMap<Entity, DVec2>,
    ranks: HashMap<Entity, usize>,
    members: Vec<(Entity, Vec2, String)>,
}

#[derive(Clone)]
struct Field {
    entity: Entity,
    root: Entity,
    workspace: u64,
    area: Arc<InfluenceArea>,
    activity: crate::native::influence_report::Outcome,
    filter: Option<Arc<Matches>>,
    group: Option<GeneratedGroup>,
    arrangement: Option<Arc<Arrangement>>,
    order: Option<crate::native::protein_area::ordering::Snapshot>,
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity
            && self.root == other.root
            && self.workspace == other.workspace
            && (Arc::ptr_eq(&self.area, &other.area) || self.area == other.area)
            && self.activity == other.activity
            && self.filter == other.filter
            && self.group == other.group
            && self.order == other.order
            && match (&self.arrangement, &other.arrangement) {
                (Some(left), Some(right)) => Arc::ptr_eq(left, right) || left == right,
                (None, None) => true,
                _ => false,
            }
    }
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
            point: if area.direction == crate::native::area::Direction::Attract {
                sorting
                    .filter(|(_, strength)| *strength > 0.0)
                    .map_or_else(|| area.target_position(), |(target, _)| target)
            } else {
                area.target_position()
            },
            strength: if matches { area.strength } else { 0.0 }
                * if area.direction == crate::native::area::Direction::Attract {
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
    trace: Option<Vec<crate::native::influence_report::Evaluation>>,
}

#[derive(Resource, Default)]
pub(crate) struct Influences {
    fields: Vec<Field>,
    indices: HashMap<Entity, usize>,
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
            Without<crate::native::component_push::composition::Generated>,
            Without<crate::native::canvas_host::composition::Generated>,
        )>()
        .iter(world)
        .any(|(entity, shield, parent, member, filter)| {
            entity != source
                && parent.parent() == root
                && member.0 == workspace
                && crate::native::influence_report::activity(world, entity, shield, root, workspace)
                    == crate::native::influence_report::Outcome::Active
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
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeAreaFields);
    crate::native::topology::holes::refresh(world);
    world
        .run_system_cached(refresh_fields)
        .expect("refresh Area inputs");
}

fn refresh_fields(
    world: &mut World,
    areas: &mut QueryState<
        (
            Entity,
            Ref<'static, InfluenceArea>,
            &'static ChildOf,
            &'static WorkspaceMember,
            Option<Ref<'static, Matches>>,
            Option<&'static GeneratedGroup>,
        ),
        (
            Without<crate::native::component_push::composition::Generated>,
            Without<crate::native::canvas_host::composition::Generated>,
        ),
    >,
    items: &mut QueryState<
        (
            Entity,
            &'static CanvasItem,
            &'static ChildOf,
            &'static WorkspaceMember,
            Option<&'static RecordProperties>,
            Option<&'static RecordBinding>,
        ),
        (
            Without<InfluenceArea>,
            Without<crate::native::topology::terrain::Effect>,
            Without<Pinned>,
            Without<crate::native::canvas_item::Ephemeral>,
            Without<crate::native::time_castle::AttachedCard>,
        ),
    >,
    canvas: &mut QueryState<Entity, With<CanvasItem>>,
    subscriptions: &mut crate::native::protein_area::ordering::Subscriptions,
) {
    world.init_resource::<Influences>();
    let mut runtime = world.remove_resource::<Influences>().unwrap();
    let mut definitions_reused = 0;
    let mut definitions_rebuilt = 0;
    let mut fields: Vec<_> = areas
        .iter(world)
        .filter(|(entity, area, _, _, _, group)| {
            group.is_some()
                || !area.is_changed()
                    && runtime
                        .field(*entity)
                        .is_some_and(|old| old.group.is_none())
                || area.validate()
        })
        .map(|(entity, area, parent, member, filter, group)| Field {
            entity,
            root: parent.parent(),
            workspace: member.0,
            activity: crate::native::influence_report::activity(
                world,
                entity,
                &area,
                parent.parent(),
                member.0,
            ),
            area: {
                if let Some(old) = runtime.field(entity).filter(|_| !area.is_changed()) {
                    definitions_reused += 1;
                    old.area.clone()
                } else {
                    definitions_rebuilt += 1;
                    let mut area = (*area).clone();
                    if !area.attraction_enabled {
                        area.strength = 0.0;
                    }
                    Arc::new(area)
                }
            },
            filter: filter.as_ref().map(|f| {
                runtime
                    .field(entity)
                    .filter(|old| !f.is_changed() || old.filter.as_deref() == Some(&**f))
                    .and_then(|old| old.filter.clone())
                    .unwrap_or_else(|| Arc::new((**f).clone()))
            }),
            group: group.cloned(),
            arrangement: None,
            order: None,
        })
        .collect();
    crate::host::performance::add(
        crate::host::performance::Count::NativeAreaDefinitionsReused,
        definitions_reused,
    );
    crate::host::performance::add(
        crate::host::performance::Count::NativeAreaDefinitionsRebuilt,
        definitions_rebuilt,
    );
    fields.sort_by(|a, b| a.area.id.cmp(&b.area.id).then(a.entity.cmp(&b.entity)));
    let mut orders = crate::native::protein_area::ordering::Lookup::new(world, subscriptions);
    for field in &mut fields {
        if field.activity != crate::native::influence_report::Outcome::Active {
            continue;
        }
        let Some(sort) = &field.area.sorting else {
            continue;
        };
        let ordered = orders.get(field.entity);
        let mut sands: Vec<_> = items
            .iter(world)
            .filter(|(_, _, parent, member, record, binding)| {
                parent.parent() == field.root
                    && member.0 == field.workspace
                    && field.matches(*record, *binding, true)
                    && ordered.as_ref().is_none_or(|order| {
                        binding.map_or(&Source::Local, |b| &b.source) == order.source
                    })
            })
            .map(|(entity, item, _, _, record, _)| {
                (
                    entity,
                    item.size,
                    record.and_then(|r| r.0["uid"].as_str()).unwrap_or_default(),
                )
            })
            .collect();
        sands.sort_by_key(|(entity, _, _)| *entity);
        field.order = ordered.map(|order| {
            order.capture(runtime.field(field.entity).and_then(|old| old.order.as_ref()))
        });
        if let Some(old) = runtime.field(field.entity)
            && (Arc::ptr_eq(&old.area, &field.area) || old.area == field.area)
            && old.filter == field.filter
            && old.root == field.root
            && old.workspace == field.workspace
            && old.order == field.order
            && let Some(arrangement) = &old.arrangement
            && arrangement.members.len() == sands.len()
            && arrangement
                .members
                .iter()
                .zip(&sands)
                .all(|(old, new)| old.0 == new.0 && old.1 == new.1 && old.2 == new.2)
        {
            field.arrangement = Some(arrangement.clone());
            continue;
        }
        let mut arrangement = Arrangement {
            members: sands
                .iter()
                .map(|(entity, size, uid)| (*entity, *size, (*uid).to_owned()))
                .collect(),
            ..default()
        };
        let ranks: HashMap<_, _> = field
            .order
            .as_ref()
            .map(|order| {
                order.1.iter()
                    .enumerate()
                    .map(|(i, uid)| (uid.as_str(), i))
                    .collect()
            })
            .unwrap_or_default();
        if field.order.is_some() {
            sands.retain(|(_, _, uid)| ranks.contains_key(uid));
        }
        sands.sort_by(|a, b| {
            ranks
                .get(a.2)
                .cmp(&ranks.get(b.2))
                .then(a.2.cmp(b.2))
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
        for (rank, (entity, size, _)) in sands.into_iter().enumerate() {
            let mut target = DVec2::from_array(field.area.center);
            target[axis] += cursor + f64::from(size[axis]) * ratio * 0.5;
            cursor += (f64::from(size[axis]) + 16.0) * ratio;
            arrangement.targets.insert(entity, target);
            arrangement.ranks.insert(entity, rank);
        }
        field.arrangement = Some(Arc::new(arrangement));
    }
    drop(orders);
    let living: std::collections::HashSet<_> = canvas.iter(world).collect();
    if runtime.fields != fields {
        runtime.indices = fields
            .iter()
            .enumerate()
            .map(|(index, field)| (field.entity, index))
            .collect();
        runtime.shields = fields
            .iter()
            .enumerate()
            .filter(|(_, f)| {
                f.activity == crate::native::influence_report::Outcome::Active
                    && f.area.immunity != Immunity::None
            })
            .map(|(i, _)| i)
            .collect();
        runtime.revision = runtime.revision.wrapping_add(1);
    }
    runtime.fields = fields;
    runtime.cache.retain(|entity, _| living.contains(entity));
    if let Some(tracked) = world.get_resource::<crate::native::influence_report::Tracked>() {
        if runtime.tracked != tracked.0 {
            runtime.tracked.clone_from(&tracked.0);
            nucleus::performance::add("influence-tracked-copies", 1);
        }
    } else {
        runtime.tracked.clear();
    }
    for (entity, cached) in &mut runtime.cache {
        if !runtime.tracked.contains(entity) {
            cached.trace = None;
        }
    }
    world.insert_resource(runtime);
}

impl Influences {
    pub(crate) fn has_fields(&self) -> bool {
        !self.fields.is_empty()
    }

    fn field(&self, entity: Entity) -> Option<&Field> {
        self.indices.get(&entity).map(|index| &self.fields[*index])
    }

    pub(crate) fn definition(&self, entity: Entity) -> Option<Arc<InfluenceArea>> {
        self.field(entity).map(|field| field.area.clone())
    }

    pub(crate) fn layout_input(
        &self,
        sand: Entity,
        root: Entity,
        workspace: u64,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> crate::layout::desktop::Influence {
        let order = self
            .fields
            .iter()
            .filter(|field| field.root == root && field.workspace == workspace)
            .find_map(|field| {
                if field.activity != crate::native::influence_report::Outcome::Active
                    || !field.area.reaches(point)
                    || !field.matches(record, binding, true)
                    || self.blocker(field, point, record, binding, true).is_some()
                    || field
                        .area
                        .sorting
                        .as_ref()
                        .is_none_or(|sorting| sorting.strength <= 0.0)
                {
                    return None;
                }
                field
                    .arrangement
                    .as_ref()?
                    .ranks
                    .get(&sand)
                    .and_then(|rank| i64::try_from(*rank).ok())
            });
        crate::layout::desktop::Influence {
            scale: f64::from(self.scale(root, workspace, point, record, binding)),
            order,
        }
    }

    pub(crate) fn topology_target(&self, area: Entity, sand: Entity) -> Option<DVec2> {
        self.field(area)?
            .arrangement
            .as_ref()?
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
            let mut evaluation = crate::native::influence_report::Evaluation::new(field.entity);
            evaluation.inside = field.area.contains(point);
            let target = field
                .arrangement
                .as_ref()
                .and_then(|arrangement| arrangement.targets.get(&sand))
                .copied();
            evaluation.slot = target.map(|target| bevy::math::DVec3::new(target.x, 0.0, target.y));
            let reason = if field.activity != crate::native::influence_report::Outcome::Active {
                Some(field.activity.clone())
            } else if let Some(shield) = self.blocker(field, point, record, binding, true) {
                Some(crate::native::influence_report::Outcome::Immune(shield))
            } else if !field.area.reaches(point) {
                Some(crate::native::influence_report::Outcome::Reach)
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
                if !matches && target.is_none() {
                    evaluation.motion = crate::native::influence_report::Outcome::Filter;
                    if let Some(trace) = &mut trace {
                        trace.push(evaluation);
                    }
                    continue;
                }
                force += simple
                    .entry(field.entity)
                    .or_insert_with(|| Destination::new(&field.area, target, matches))
                    .force(point);
            } else if field.matches(record, binding, false) {
                let radius = DVec2::from_array(field.area.size).min_element() * 0.5;
                let distance = point.distance(field.area.target_position());
                force += field.area.force_for_match(point, true)
                    * (radius / distance.max(radius)).powi(2);
            } else {
                evaluation.motion = crate::native::influence_report::Outcome::Filter;
            }
            if let Some(sort) = &field.area.sorting
                && (field.area.force_mode != ForceMode::Simple || field.group.is_some())
                && let Some(target) = target
            {
                let delta = target - point;
                force += delta.normalize_or_zero()
                    * simple_strength(
                        delta.length(),
                        field.area.size[0].min(field.area.size[1]) * 0.5,
                        sort.strength,
                    );
                evaluation.motion = crate::native::influence_report::Outcome::Active;
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
    ) -> (f32, crate::native::influence_report::Outcome) {
        use crate::native::influence_report::Outcome;
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
    ) -> crate::native::influence_report::Report {
        let cache = self.calculate(sand, root, workspace, point, record, binding);
        let mut entries = cache.trace.clone().unwrap_or_default();
        let total = cache.total;
        for entry in &mut entries {
            if pinned {
                entry.motion = crate::native::influence_report::Outcome::Pinned;
                entry.size = entry.motion.clone();
                entry.force = bevy::math::DVec3::ZERO;
            } else if let Some(field) = self.fields.iter().find(|field| field.entity == entry.area)
            {
                (entry.scale, entry.size) = self.scale_result(field, point, record, binding);
                entry.inside = field.area.contains(point);
            }
        }
        crate::native::influence_report::Report {
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
    let _timing =
        crate::host::performance::Span::new(crate::host::performance::Stage::NativeAreaEffects);
    if world.contains_resource::<crate::native::topology::physics::Runtime>() {
        crate::native::topology::influence::update(world);
        return;
    }
    refresh(world);
    world
        .run_system_cached(update_sands)
        .expect("update Area effects");
}

#[derive(Default)]
struct SandPreparation {
    entities: Vec<Entity>,
    retained: std::collections::HashSet<Entity>,
    stale: Vec<Entity>,
}

fn update_sands(
    world: &mut World,
    items: &mut QueryState<
        (
            Entity,
            &'static CanvasItem,
            &'static ChildOf,
            &'static WorkspaceMember,
            Option<&'static RecordProperties>,
            Option<&'static RecordBinding>,
            Has<Pinned>,
        ),
        (
            Without<InfluenceArea>,
            Without<crate::native::canvas_item::Ephemeral>,
        ),
    >,
    effects: &mut QueryState<Entity, Or<(With<AreaScale>, With<AreaForces>)>>,
    mut preparation: Local<SandPreparation>,
) {
    let _effects = nucleus::performance::Span::new("native-area-sand-effects");
    let SandPreparation { entities, retained, stale } = &mut *preparation;
    entities.extend(items.iter(world).map(|(entity, ..)| entity));
    retained.extend(entities.iter().copied());
    stale.extend(effects.iter(world).filter(|entity| !retained.contains(entity)));
    for entity in stale.drain(..) {
        world.entity_mut(entity).remove::<(AreaScale, AreaForces)>();
    }
    let count = entities.len();
    let mut published = 0;
    let mut removed = 0;
    world.resource_scope(|world, mut runtime: Mut<Influences>| {
        runtime.cache.retain(|entity, _| retained.contains(entity));
        for entity in entities.drain(..) {
            let (_, item, parent, member, record, binding, pinned) =
                items.get(world, entity).expect("current Area effect item");
            let root = parent.parent();
            let workspace = member.0;
            let active = crate::native::topology::holes::live(world, root, workspace) && !pinned;
            let scale = if active {
                runtime.scale(root, workspace, item.position, record, binding)
            } else {
                1.0
            };
            let empty = AreaForces::default();
            let forces = if active {
                &runtime.calculate(entity, root, workspace, item.position, record, binding).forces
            } else {
                runtime.forget(entity);
                &empty
            };
            let remove_forces = record.is_none() && forces.0.is_empty();
            let replacement = (!remove_forces && world.get::<AreaForces>(entity) != Some(forces))
                .then(|| forces.clone());
            let report = crate::native::influence_report::tracked(world, entity).then(|| {
                runtime.report(
                    entity,
                    root,
                    workspace,
                    item.position,
                    record,
                    binding,
                    pinned,
                    scale,
                )
            });
            if scale == 1.0 {
                if world.get::<AreaScale>(entity).is_some() {
                    world.entity_mut(entity).remove::<AreaScale>();
                }
            } else if world.get::<AreaScale>(entity) != Some(&AreaScale(scale)) {
                world.entity_mut(entity).insert(AreaScale(scale));
            }
            if let Some(report) = report {
                crate::native::influence_report::store(world, entity, report);
            }
            if remove_forces {
                if world.get::<AreaForces>(entity).is_some() {
                    world.entity_mut(entity).remove::<AreaForces>();
                    removed += 1;
                }
            } else if let Some(forces) = replacement {
                world.entity_mut(entity).insert(forces);
                published += 1;
            }
        }
    });
    nucleus::performance::add("native-area-sands-prepared", count as u64);
    nucleus::performance::add("native-area-forces-published", published);
    nucleus::performance::add("native-area-forces-removed", removed);
    retained.clear();
    if count == 0 {
        *preparation = SandPreparation::default();
    }
}
