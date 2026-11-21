// non-lean-yet
// Scope: struct Captions, struct Slot, struct Preparation, fn update.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 262.
// - [review: §5 Panics] Check expect invariants and diagnostic context. Sites expect (31).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   78, 141, 154.
// - [rustc probe: elided_lifetimes_in_paths] Check API clarity and framework allowances before
//   making lifetimes explicit. Sites 37, 38.

use super::*;
use bevy::ecs::system::InRef;

#[derive(Default)]
pub(super) struct Captions {
    title: String,
    count: Option<usize>,
    count_text: String,
}

#[derive(Default)]
struct Slot {
    part: Option<Entity>,
    duplicate: bool,
    labels: Vec<(Entity, bool)>,
}

#[derive(Default)]
struct Preparation {
    slots: Vec<Slot>,
}

pub(super) fn update(
    world: &mut World,
    owner: Entity,
    columns: &[Option<Entity>],
    counts: &[usize],
) {
    world
        .run_system_cached_with(update_headers, (owner, columns, counts))
        .expect("update Kanban headers");
}

fn update_headers(
    (In(owner), InRef(columns), InRef(counts)): (
        In<Entity>,
        InRef<[Option<Entity>]>,
        InRef<[usize]>,
    ),
    world: &mut World,
    parts: &mut QueryState<(Entity, &'static Part)>,
    mut preparation: Local<Preparation>,
) {
    let _prepare = nucleus::performance::Span::new("kanban-header-preparation");
    if !columns.iter().any(Option::is_some) {
        if let Some(mut view) = world.get_mut::<View>(owner)
            && !view.header_captions.is_empty()
        {
            view.header_captions.clear();
        }
        *preparation = Preparation::default();
        return;
    }
    let Preparation { slots } = &mut *preparation;
    slots.resize_with(columns.len(), Slot::default);
    for slot in slots.iter_mut() {
        slot.part = None;
        slot.duplicate = false;
        slot.labels.clear();
    }
    let mut parts_examined = 0;
    for (entity, part) in parts.iter(world) {
        parts_examined += 1;
        if part.owner == owner
            && let Some(slot) = slots.get_mut(part.column)
        {
            if slot.part.is_some() {
                slot.duplicate = true;
            } else {
                slot.part = Some(entity);
            }
        }
    }
    for (index, column) in columns.iter().enumerate() {
        let Some(column) = *column else {
            continue;
        };
        let slot = &mut slots[index];
        let Some(item) = world.get::<CanvasItem>(column).copied() else {
            slot.part = None;
            continue;
        };
        let part = slot.part.and_then(|entity| {
            if !slot.duplicate
                && world
                    .get::<Part>(entity)
                    .is_some_and(|part| part.owner == owner && part.column == index)
            {
                Some(entity)
            } else {
                parts.iter(world).find_map(|(entity, part)| {
                    parts_examined += 1;
                    (part.owner == owner && part.column == index).then_some(entity)
                })
            }
        });
        let position = item.position + DVec2::new(0.0, -f64::from(item.size.y) * 0.5 - 40.0);
        let Some(part) = part.or_else(|| header(world, owner, index, position)) else {
            slot.part = None;
            continue;
        };
        let Some(mut header_item) = world.get_mut::<CanvasItem>(part) else {
            slot.part = None;
            continue;
        };
        slot.part = Some(part);
        if header_item.size.x != item.size.x {
            header_item.size.x = item.size.x;
        }
        let placement = crate::native::topology::spatial(world, column);
        let offset = position - item.position;
        place(
            world,
            part,
            placement.position(item.position)
                + placement.rotation() * DVec3::new(offset.x, 0.0, offset.y),
        );
        rotate(world, part, placement.rotation);
        if let Some(group) = world
            .get::<crate::native::canvas_selection::SandGroup>(column)
            .copied()
        {
            if world.get::<crate::native::canvas_selection::SandGroup>(part) != Some(&group) {
                world.entity_mut(part).insert(group);
            }
        } else if world
            .get::<crate::native::canvas_selection::SandGroup>(part)
            .is_some()
        {
            world
                .entity_mut(part)
                .remove::<crate::native::canvas_selection::SandGroup>();
        }
        refresh_attachment(world, part, column);
    }
    nucleus::performance::add("kanban-header-parts-examined", parts_examined);
    let live = |part: &Entity| {
        slots
            .iter()
            .enumerate()
            .any(|(index, slot)| columns[index].is_some() && slot.part == Some(*part))
    };
    if let Some(mut view) = world.get_mut::<View>(owner)
        && view.header_captions.keys().any(|part| !live(part))
    {
        view.header_captions.retain(|part, _| live(part));
    }

    let mut candidates = 0;
    for (index, column) in columns.iter().enumerate() {
        let Some(column) = *column else {
            continue;
        };
        let slot = &mut slots[index];
        let Some(part) = slot.part else {
            continue;
        };
        candidates += collect_labels(world, part, &mut slot.labels);
        if !slot.labels.is_empty() {
            let count = counts.get(index).copied().unwrap_or_default();
            prepare_captions(world, owner, part, column, count, &slot.labels);
            for (label, status) in slot.labels.drain(..) {
                publish(world, owner, part, label, status);
            }
        }
    }
    nucleus::performance::add("kanban-header-label-candidates", candidates);
    for slot in slots {
        slot.part = None;
        slot.duplicate = false;
        slot.labels.clear();
    }
}

fn collect_labels(world: &World, part: Entity, labels: &mut Vec<(Entity, bool)>) -> u64 {
    fn admit(world: &World, entity: Entity, labels: &mut Vec<(Entity, bool)>) {
        let status = world.get::<Status>(entity).is_some();
        if (status || world.get::<ColumnTitle>(entity).is_some())
            && world.get::<Text>(entity).is_some()
        {
            labels.push((entity, status));
        }
    }
    let mut candidates = 0;
    if let Some(children) = world.get::<Children>(part) {
        for &child in children {
            candidates += 1;
            admit(world, child, labels);
            if let Some(children) = world.get::<Children>(child) {
                for &child in children {
                    candidates += 1;
                    admit(world, child, labels);
                }
            }
        }
    }
    candidates
}

fn prepare_captions(
    world: &mut World,
    owner: Entity,
    part: Entity,
    column: Entity,
    count: usize,
    labels: &[(Entity, bool)],
) {
    let (Some(area), Some(view)) = (world.get::<InfluenceArea>(column), world.get::<View>(owner))
    else {
        return;
    };
    let quantity = area.changes.enter.quantity.as_deref().unwrap_or("—");
    let captions = view.header_captions.get(&part);
    let title = (labels.iter().any(|(_, status)| !*status)
        && captions.is_none_or(|captions| {
            captions
                .title
                .strip_prefix(area.name.as_str())
                .and_then(|tail| tail.strip_prefix(": "))
                != Some(quantity)
        }))
    .then(|| format!("{}: {}", area.name, quantity));
    let count_text = (labels.iter().any(|(_, status)| *status)
        && captions.is_none_or(|captions| captions.count != Some(count)))
    .then(|| format!("{count} records"));
    if title.is_none() && count_text.is_none() {
        return;
    }
    nucleus::performance::add(
        "kanban-header-captions-prepared",
        u64::from(title.is_some()) + u64::from(count_text.is_some()),
    );
    let Some(mut view) = world.get_mut::<View>(owner) else {
        return;
    };
    let captions = view.header_captions.entry(part).or_default();
    if let Some(title) = title {
        captions.title = title;
    }
    if let Some(text) = count_text {
        captions.count = Some(count);
        captions.count_text = text;
    }
}

fn publish(world: &mut World, owner: Entity, part: Entity, label: Entity, status: bool) {
    let Some(captions) = world
        .get::<View>(owner)
        .and_then(|view| view.header_captions.get(&part))
    else {
        return;
    };
    let value = if status {
        &captions.count_text
    } else {
        &captions.title
    };
    if world
        .get::<Text>(label)
        .is_some_and(|text| &text.0 != value)
    {
        let value = value.clone();
        if let Some(mut text) = world.get_mut::<Text>(label) {
            text.0 = value;
            nucleus::performance::add("kanban-header-labels-written", 1);
        }
    }
}
