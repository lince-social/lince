// non-lean-yet
// Scope: struct Preparation, fn refresh.
// BeautifulRust: No site flagged by this pass. Check contracts and feature coverage.

use super::*;

#[derive(Default)]
pub(super) struct Preparation {
    roots: Vec<Entity>,
    stale: Vec<Entity>,
    tracked: HashSet<Entity>,
}

pub(super) fn refresh(
    world: &mut World,
    roots_query: &mut QueryState<(Entity, &'static Workspaces)>,
    reports: &mut QueryState<Entity, With<Report>>,
    mut preparation: Local<Preparation>,
) {
    let _tracking = nucleus::performance::Span::new("influence-inspection-tracking");
    let Preparation {
        roots,
        stale,
        tracked,
    } = &mut *preparation;
    roots.clear();
    roots.extend(roots_query.iter(world).map(|(entity, _)| entity));
    let root_count = roots.len();
    tracked.clear();
    let mut published = 0;
    for root in roots.drain(..) {
        let chosen = crate::native::canvas_selection::selected_iter(world, root)
            .find(|sand| world.get::<InfluenceArea>(*sand).is_none())
            .or_else(|| {
                world
                    .get::<crate::native::inspection::Inspection>(root)
                    .and_then(|inspection| inspection.selected)
            });
        if let Some(sand) = chosen.filter(|sand| {
            world.get::<InfluenceArea>(*sand).is_none()
                && crate::native::canvas_selection::eligible(world, root, *sand)
        }) && world.get::<Inspected>(root).is_none_or(|old| old.0 != sand)
        {
            world.entity_mut(root).insert(Inspected(sand));
            published += 1;
        }
        if let Some(sand) = world.get::<Inspected>(root).map(|sand| sand.0) {
            if crate::native::canvas_selection::eligible(world, root, sand) {
                tracked.insert(sand);
            } else {
                world.entity_mut(root).remove::<Inspected>();
                published += 1;
            }
        }
    }
    stale.clear();
    stale.extend(reports.iter(world).filter(|sand| !tracked.contains(sand)));
    let retired = stale.len();
    for sand in stale.drain(..) {
        world.entity_mut(sand).remove::<Report>();
    }
    if world
        .get_resource::<Tracked>()
        .is_none_or(|old| old.0 != *tracked)
    {
        if let Some(mut current) = world.get_resource_mut::<Tracked>() {
            std::mem::swap(&mut current.0, tracked);
        } else {
            world.insert_resource(Tracked(std::mem::take(tracked)));
        }
        nucleus::performance::add("influence-tracked-sets-published", 1);
    }
    nucleus::performance::add("influence-inspected-roots-published", published);
    nucleus::performance::add("influence-stale-reports-retired", retired as u64);
    if root_count == 0 {
        *roots = Vec::new();
        *stale = Vec::new();
        *tracked = HashSet::new();
    }
}
