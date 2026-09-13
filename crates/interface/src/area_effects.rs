use crate::{
    area::{AreaForce, AreaForces, InfluenceArea, ReachMode, RecordProperties},
    canvas::CanvasItem,
    protein_area::{RecordBinding, Source, filter::Matches, grouping::GeneratedGroup},
    sand_placement::Pinned,
    workspace::{WorkspaceMember, Workspaces},
};
use bevy::{math::DVec2, prelude::*};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

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

struct Cached {
    revision: u64,
    identity: Identity,
    point: DVec2,
    simple: HashMap<Entity, DVec2>,
    forces: AreaForces,
    total: DVec2,
    spatial: bool,
}

#[derive(Resource, Default)]
pub(crate) struct Influences {
    fields: Vec<Field>,
    revision: u64,
    cache: HashMap<Entity, Cached>,
    evaluations: u64,
    shields: Vec<usize>,
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
        .query::<(
            Entity,
            &InfluenceArea,
            &ChildOf,
            &WorkspaceMember,
            Option<&Matches>,
        )>()
        .iter(world)
        .any(|(entity, shield, parent, member, filter)| {
            entity != source
                && parent.parent() == root
                && member.0 == workspace
                && shield.validate()
                && shield.contains(point)
                && immunity_blocks(shield, center)
                && if shield.filter.is_some() {
                    record.is_some_and(|r| filter.is_some_and(|f| f.allows(r, binding)))
                } else {
                    shield.rules.is_empty() || record.is_some_and(|r| shield.matches(r))
                }
        })
}

fn immunity_blocks(shield: &InfluenceArea, center: DVec2) -> bool {
    match shield.immunity {
        Immunity::None => false,
        Immunity::All => true,
        Immunity::Internal => shield.contains(center),
        Immunity::External => !shield.contains(center),
    }
}

pub(crate) fn refresh(world: &mut World) {
    world.init_resource::<Influences>();
    let mut runtime = world.remove_resource::<Influences>().unwrap();
    let previous: HashMap<_, _> = runtime.fields.iter().map(|f| (f.entity, f)).collect();
    let mut fields: Vec<_> = world
        .query::<(
            Entity,
            &InfluenceArea,
            &ChildOf,
            &WorkspaceMember,
            Option<Ref<Matches>>,
            Option<&GeneratedGroup>,
        )>()
        .iter(world)
        .filter(|(_, area, _, _, _, group)| area.validate() || group.is_some())
        .map(|(entity, area, parent, member, filter, group)| Field {
            entity,
            root: parent.parent(),
            workspace: member.0,
            area: area.clone(),
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
            ), (Without<InfluenceArea>, Without<Pinned>)>()
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
            .filter(|(_, f)| f.area.immunity != Immunity::None)
            .map(|(i, _)| i)
            .collect();
        runtime.fields = fields;
        runtime.revision = runtime.revision.wrapping_add(1);
    }
    runtime.cache.retain(|entity, _| living.contains(entity));
    world.insert_resource(runtime);
}

impl Influences {
    pub(crate) fn forget(&mut self, sand: Entity) {
        self.cache.remove(&sand);
    }
    fn immune(
        &self,
        field: &Field,
        point: DVec2,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> bool {
        self.shields.iter().map(|i| &self.fields[*i]).any(|shield| {
            shield.entity != field.entity
                && shield.root == field.root
                && shield.workspace == field.workspace
                && shield.area.immunity != Immunity::None
                && shield.area.contains(point)
                && immunity_blocks(&shield.area, DVec2::from_array(field.area.center))
                && shield.matches(record, binding, true)
        })
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
        if let Some(cache) = self.cache.get(&sand)
            && cache.revision == self.revision
            && cache.identity.root == root
            && cache.identity.workspace == workspace
            && cache.identity.record.as_ref() == record
            && &cache.identity.source == binding.map_or(&Source::Local, |b| &b.source)
            && (!cache.spatial || cache.point == point)
        {
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
            .map_or_else(HashMap::new, |c| c.simple);
        let mut forces = AreaForces::default();
        let spatial = self.fields.iter().any(|f| {
            f.root == root
                && f.workspace == workspace
                && (f.area.reach.mode == ReachMode::Limited
                    || f.area.force_mode == ForceMode::Newtonian
                    || f.area.immunity != Immunity::None
                    || f.area.sorting.is_some()
                    || f.group.is_some())
        });
        for field in self
            .fields
            .iter()
            .filter(|f| f.root == root && f.workspace == workspace)
        {
            self.evaluations += 1;
            if self.immune(field, point, record, binding) || !field.area.reaches(point) {
                continue;
            }
            let mut force = DVec2::ZERO;
            if let Some(group) = &field.group {
                force += group.force(&field.area, sand, point);
            } else if field.matches(record, binding, false) {
                force += match field.area.force_mode {
                    ForceMode::Simple => *simple
                        .entry(field.entity)
                        .or_insert_with(|| field.area.force_for_match(point, true)),
                    ForceMode::Newtonian => {
                        let radius = DVec2::from_array(field.area.size).min_element() * 0.5;
                        let distance = point.distance(field.area.target_position());
                        field.area.force_for_match(point, true)
                            * (radius / distance.max(radius)).powi(2)
                    }
                };
            }
            if let Some(sort) = &field.area.sorting
                && let Some(target) = field.targets.get(&sand)
            {
                let delta = *target - point;
                if delta.length_squared() > 0.25 {
                    force += delta.clamp_length_max(1.0) * sort.strength;
                }
            }
            if force.is_finite() && force != DVec2::ZERO {
                forces.0.push(AreaForce {
                    area: field.entity,
                    force,
                });
            }
        }
        let total = forces.total();
        let total = if total.is_finite() {
            total.clamp_length_max(1_000_000.0)
        } else {
            DVec2::ZERO
        };
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
            .filter(|f| {
                f.root == root
                    && f.workspace == workspace
                    && f.area.scale != 1.0
                    && f.area.contains(point)
                    && f.matches(record, binding, true)
                    && !self.immune(f, point, record, binding)
            })
            .map(|f| f64::from(f.area.scale))
            .product::<f64>()
            .clamp(0.05, 20.0) as f32
    }
}

pub(crate) fn update(world: &mut World) {
    refresh(world);
    let sands: Vec<_> = world
        .query_filtered::<(
            Entity,
            &CanvasItem,
            &ChildOf,
            &WorkspaceMember,
            Option<&RecordProperties>,
            Option<&RecordBinding>,
            Has<Pinned>,
        ), Without<InfluenceArea>>()
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
