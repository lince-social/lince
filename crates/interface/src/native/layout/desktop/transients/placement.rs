use super::*;
use crate::layout::desktop::SolvedItem;

struct Pending {
    id: PlacementId,
    parent: PlacementId,
    offset: bevy::math::DVec2,
}

pub(super) fn place_with(
    world: &World,
    entities: &BTreeMap<PlacementId, Entity>,
    model: &mut Workspace,
    solution: &mut Solution,
    usable: Rectangle,
    preview: Option<(Entity, Placement)>,
) {
    let _placement = engine::performance::Span::new("desktop-transient-placement");
    let bounded = model.mode() != WorkspaceLayout::Free
        || model.fullscreen().is_some()
        || model
            .items()
            .iter()
            .any(|item| model.maximized(item.id) && !model.minimized(item.id));
    // Visibility depends on ownership/hidden state, not on the free geometry
    // written below. Observe each mapped item once, before any placement.
    let mut hidden = BTreeSet::new();
    let mut remaining = Vec::new();
    for (&id, &entity) in entities {
        if minimized(world, entity, model) {
            hidden.insert(id);
            continue;
        }
        let Some(parent) = world.get::<TransientParent>(entity).map(|parent| parent.0) else {
            continue;
        };
        let Some(parent_id) = world
            .get::<LayoutBox>(parent)
            .map(|layout| PlacementId(layout.id))
        else {
            continue;
        };
        let offset = preview
            .filter(|(target, _)| *target == entity)
            .map(|(_, placement)| placement)
            .or_else(|| world.get::<Placement>(entity).copied())
            .filter(|placement| placement.parent == parent)
            .map(|placement| placement.offset)
            .unwrap_or_default();
        remaining.push(Pending {
            id,
            parent: parent_id,
            offset,
        });
    }
    let previous = solution.items.len();
    if !hidden.is_empty() {
        solution.items.retain(|item| !hidden.contains(&item.id));
    }
    engine::performance::add("desktop-transient-ancestry-checks", entities.len() as u64);
    engine::performance::add(
        "desktop-transient-hidden-rows-removed",
        (previous - solution.items.len()) as u64,
    );
    if remaining.is_empty() {
        return;
    }
    // Index only the identities this placement pass will ask for. No rows are
    // removed after this point. Keep each ID's first occurrence (the previous
    // find() behavior) and index newly appended rows at once. In particular,
    // do not topologically reorder already-solved descendants.
    let mut positions = BTreeMap::new();
    for pending in &remaining {
        positions.entry(pending.id).or_insert(None);
        positions.entry(pending.parent).or_insert(None);
    }
    let mut missing = positions.len();
    let mut examined = 0;
    for (index, item) in solution.items.iter().enumerate() {
        examined += 1;
        if let Some(position) = positions.get_mut(&item.id)
            && position.is_none()
        {
            *position = Some(index);
            missing -= 1;
            if missing == 0 {
                break;
            }
        }
    }
    engine::performance::add("desktop-transient-solution-rows-examined", examined);
    engine::performance::add(
        "desktop-transient-solution-lookup-ids",
        positions.len() as u64,
    );
    let mut attempts = 0;
    let mut placed = 0;
    for _ in 0..32 {
        let previous = remaining.len();
        remaining.retain(|pending| {
            attempts += 1;
            let Some(parent_index) = positions.get(&pending.parent).copied().flatten() else {
                return true;
            };
            let bounds = solution.items[parent_index].rectangle;
            let id = &pending.id;
            let Some(item) = model.items().iter().find(|item| item.id == *id) else {
                return false;
            };
            let size = std::array::from_fn(|axis| {
                item.free.size[axis].clamp(
                    f64::from(item.rules.axes[axis].min),
                    f64::from(item.rules.axes[axis].max),
                )
            });
            let origin = std::array::from_fn(|axis| {
                let origin = bounds.origin()[axis]
                    + (bounds.size()[axis] - size[axis]) * 0.5
                    + pending.offset[axis];
                if !bounded {
                    return origin;
                }
                let start = usable.origin()[axis];
                let end = start + usable.size()[axis] - size[axis];
                origin.clamp(start.min(end), start.max(end))
            });
            let Some(rectangle) = Rectangle::new(origin, size) else {
                return false;
            };
            let _ = model.set_free_geometry(*id, Geometry { origin, size });
            if let Some(index) = positions.get(id).copied().flatten() {
                let item = &mut solution.items[index];
                item.rectangle = rectangle;
                item.floating = true;
            } else {
                positions.insert(*id, Some(solution.items.len()));
                solution.items.push(SolvedItem {
                    id: *id,
                    rectangle,
                    floating: true,
                });
            }
            placed += 1;
            if model.focused() == Some(*id) {
                solution.focused = Some(*id);
            }
            false
        });
        if remaining.len() == previous || remaining.is_empty() {
            break;
        }
    }
    engine::performance::add("desktop-transient-placement-attempts", attempts);
    engine::performance::add("desktop-transient-items-placed", placed);
}
