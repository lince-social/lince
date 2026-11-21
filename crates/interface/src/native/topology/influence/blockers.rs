use super::{contains, spatial};
use crate::native::{
    area::{InfluenceArea, RecordProperties},
    area_effects::Immunity,
    canvas_host, component_push,
    influence_report::{Outcome, activity},
    protein_area::{RecordBinding, filter::Matches},
    topology::{Spatial, position},
    workspace::WorkspaceMember,
};
use bevy::{math::DVec3, prelude::*};
use nucleus::performance::{Span, add};

pub(crate) type BlockerQuery = QueryState<
    (
        Entity,
        &'static InfluenceArea,
        &'static ChildOf,
        &'static WorkspaceMember,
        Option<&'static Matches>,
    ),
    (
        Without<component_push::composition::Generated>,
        Without<canvas_host::composition::Generated>,
    ),
>;

struct Source {
    entity: Entity,
    root: Entity,
    workspace: u64,
    center: DVec3,
    forces: bool,
}

impl Source {
    fn new(
        world: &World,
        root: Entity,
        workspace: u64,
        entity: Entity,
        forces: bool,
    ) -> Option<Self> {
        Some(Self {
            entity,
            root,
            workspace,
            center: position(world, entity)?,
            forces,
        })
    }
}

struct Blocker<'w> {
    area: &'w InfluenceArea,
    filter: Option<&'w Matches>,
    placement: Spatial,
    source_inside: bool,
}

impl<'w> Blocker<'w> {
    fn prepare(
        world: &World,
        source: &Source,
        (entity, area, parent, member, filter): (
            Entity,
            &'w InfluenceArea,
            &ChildOf,
            &WorkspaceMember,
            Option<&'w Matches>,
        ),
    ) -> Option<Self> {
        if entity == source.entity
            || parent.parent() != source.root
            || member.0 != source.workspace
            || area.immunity == Immunity::None
            || (!source.forces && area.immunity == Immunity::Containment)
            || activity(world, entity, area, source.root, source.workspace) != Outcome::Active
        {
            return None;
        }
        let placement = spatial(world, entity);
        Some(Self {
            area,
            filter,
            placement,
            // All immunity depends only on the target position.
            source_inside: area.immunity != Immunity::All
                && contains(area, placement, source.center),
        })
    }

    fn blocks(
        &self,
        point: DVec3,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> bool {
        self.area.immunity.blocks(
            self.source_inside,
            contains(self.area, self.placement, point),
        ) && if self.area.filter.is_some() {
            record.is_some_and(|r| self.filter.is_some_and(|f| f.allows(r, binding)))
        } else {
            self.area.rules.is_empty() || record.is_some_and(|r| self.area.matches(r))
        }
    }
}

pub(crate) struct Blockers<'w>(Vec<Blocker<'w>>);

impl<'w> Blockers<'w> {
    pub(crate) fn prepare(
        world: &'w World,
        query: &mut BlockerQuery,
        root: Entity,
        workspace: u64,
        entity: Entity,
        forces: bool,
    ) -> Self {
        let _preparation = Span::new("area-blocker-preparation");
        let Some(source) = Source::new(world, root, workspace, entity, forces) else {
            return Self(Vec::new());
        };
        let barriers: Vec<_> = query
            .iter(world)
            .filter_map(|entry| Blocker::prepare(world, &source, entry))
            .collect();
        add("area-source-blockers-prepared", barriers.len() as u64);
        Self(barriers)
    }

    pub(crate) fn blocks(
        &self,
        point: DVec3,
        record: Option<&RecordProperties>,
        binding: Option<&RecordBinding>,
    ) -> bool {
        self.0
            .iter()
            .any(|barrier| barrier.blocks(point, record, binding))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "The single-point check shares the prepared check's source and target inputs"
)]
pub(crate) fn blocked(
    world: &mut World,
    root: Entity,
    workspace: u64,
    entity: Entity,
    point: DVec3,
    record: Option<&RecordProperties>,
    binding: Option<&RecordBinding>,
    forces: bool,
) -> bool {
    let Some(source) = Source::new(world, root, workspace, entity, forces) else {
        return false;
    };
    BlockerQuery::new(world)
        .iter(world)
        .filter_map(|entry| Blocker::prepare(world, &source, entry))
        .any(|barrier| barrier.blocks(point, record, binding))
}
