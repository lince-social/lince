// non-lean-yet
// Scope: struct Baseline, struct Sources.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 48.
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   74, 125, 126, 127, 156, 157, 158.

use super::*;
use std::cell::OnceCell;

struct Baseline {
    model: Workspace,
    solution: Solution,
}

pub(super) struct Sources<'a> {
    world: &'a World,
    session: &'a Session,
    pub(super) current: &'a Workspace,
    entities: OnceCell<BTreeMap<PlacementId, Entity>>,
    baseline: OnceCell<Option<Baseline>>,
}

impl<'a> Sources<'a> {
    pub(super) fn new(world: &'a World, session: &'a Session) -> Option<Self> {
        if !valid(world, session) {
            return None;
        }
        Some(Self {
            world,
            session,
            current: model(world, session.root, session.workspace)?,
            entities: OnceCell::new(),
            baseline: OnceCell::new(),
        })
    }

    fn entities(&self) -> &BTreeMap<PlacementId, Entity> {
        self.entities
            .get_or_init(|| entities(self.world, self.session))
    }

    pub(super) fn into_entities(self) -> BTreeMap<PlacementId, Entity> {
        let (world, session) = (self.world, self.session);
        self.entities
            .into_inner()
            .unwrap_or_else(|| entities(world, session))
    }

    fn baseline(&self) -> Option<&Baseline> {
        self.baseline
            .get_or_init(|| {
                let _baseline = engine::performance::Span::new("desktop-manipulation-baseline");
                engine::performance::add("desktop-manipulation-baseline-attempts", 1);
                let mut model = self.current.clone();
                let mut solution = model.solve(self.session.bounds, self.session.bounds).ok()?;
                transients::place(
                    self.world,
                    self.entities(),
                    &mut model,
                    &mut solution,
                    self.session.bounds,
                );
                Some(Baseline { model, solution })
            })
            .as_ref()
    }

    pub(super) fn proposal(&self) -> Option<Proposal> {
        let _proposal = engine::performance::Span::new("desktop-manipulation-proposal");
        let (world, session, current) = (self.world, self.session, self.current);
        let (mut next, mut geometry) = session
            .gesture
            .propose(current, session.delta, session.slot)
            .ok()?;
        let mut spatial = session.spatial;
        if !session.gesture.tiled() && next != *current {
            let original = session.gesture.original();
            let center = |geometry: Geometry| {
                bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                    geometry.origin[axis] + geometry.size[axis] * 0.5
                }))
            };
            let base = center(original);
            let local = center(geometry) - base;
            let resize = if session.operation == Manipulation::Resize {
                spatial.rotation()
                    * bevy::math::DVec3::new(local.x * session.scale, 0.0, local.y * session.scale)
                    - bevy::math::DVec3::new(local.x, 0.0, local.y)
            } else {
                bevy::math::DVec3::ZERO
            };
            let restore = if session.maximized {
                let offset = |size: [f64; 2]| {
                    spatial.rotation()
                        * bevy::math::DVec3::new(
                            (session.uv[0] - 0.5) * size[0] * session.scale,
                            0.0,
                            (session.uv[1] - 0.5) * size[1] * session.scale,
                        )
                };
                spatial.position(session.displayed.position)
                    + offset(session.displayed.size.as_dvec2().to_array())
                    - spatial.position(base)
                    - offset(original.size)
            } else {
                bevy::math::DVec3::ZERO
            };
            let shift = resize + restore;
            geometry.origin[0] += shift.x;
            geometry.origin[1] += shift.z;
            spatial.elevation += shift.y;
            if !spatial.valid() {
                return None;
            }
            next.set_free_geometry(session.gesture.id(), geometry)
                .ok()?;
        }
        let entities = self.entities();
        let offset = if next != *current {
            session.parent.and_then(|parent| {
                let parent_id = world.get::<Managed>(parent.0)?.id;
                let solution = &self.baseline()?.solution;
                let parent_bounds = solution
                    .items
                    .iter()
                    .find(|item| item.id == parent_id)?
                    .rectangle;
                Some(transients::Placement {
                    parent: parent.0,
                    offset: bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                        geometry.origin[axis] + geometry.size[axis] * 0.5
                            - parent_bounds.origin()[axis]
                            - parent_bounds.size()[axis] * 0.5
                    })),
                })
            })
        } else {
            None
        };
        let mut solution = next.solve(session.bounds, session.bounds).ok()?;
        transients::place_with(
            world,
            entities,
            &mut next,
            &mut solution,
            session.bounds,
            offset.map(|offset| (session.target, offset)),
        );
        let offset = offset.and_then(|mut placement| {
            let parent_id = world.get::<Managed>(placement.parent)?.id;
            let parent = solution
                .items
                .iter()
                .find(|item| item.id == parent_id)?
                .rectangle;
            let child = solution
                .items
                .iter()
                .find(|item| item.id == session.gesture.id())?
                .rectangle;
            placement.offset = bevy::math::DVec2::from_array(std::array::from_fn(|axis| {
                child.origin()[axis] + child.size()[axis] * 0.5
                    - parent.origin()[axis]
                    - parent.size()[axis] * 0.5
            }));
            Some(placement)
        });
        Some(Proposal {
            model: next,
            solution,
            placement: offset,
            spatial,
        })
    }

    pub(super) fn changed(
        &self,
        proposed: &Workspace,
        solution: &Solution,
    ) -> BTreeSet<PlacementId> {
        let _comparison = engine::performance::Span::new("desktop-manipulation-comparison");
        let Some(baseline) = self.baseline() else {
            return BTreeSet::new();
        };
        let mut previous = BTreeMap::new();
        for item in &baseline.solution.items {
            previous.entry(item.id).or_insert(item);
        }
        engine::performance::add(
            "desktop-manipulation-baseline-rows-indexed",
            baseline.solution.items.len() as u64,
        );
        engine::performance::add(
            "desktop-manipulation-proposed-rows-compared",
            solution.items.len() as u64,
        );
        solution
            .items
            .iter()
            .filter(|item| {
                previous
                    .get(&item.id)
                    .is_none_or(|other| other.rectangle != item.rectangle)
                    || baseline.model.compact_viewport(
                        item.id,
                        self.session.bounds,
                        &baseline.solution,
                    ) != proposed.compact_viewport(item.id, self.session.bounds, solution)
            })
            .map(|item| item.id)
            .collect()
    }
}
